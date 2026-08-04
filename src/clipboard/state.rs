//! The owned copy/paste state model.
//!
//! Version 0.2 spread this across three uncoordinated things — `payload.tmp`,
//! `session.tmp`, and the system clipboard — with no single owner. The
//! consequences were all visible to users:
//!
//! - Nothing ever cleared the payload after a successful paste, so the Paste
//!   verb stayed armed forever (and, on Windows and macOS, across reboots,
//!   because the payload lived in the local-data directory).
//! - Sources that had since been deleted were silently dropped, so Paste could
//!   appear to work and then do nothing at all.
//! - Two quick Paste clicks started two processes copying into the same folder.
//! - The session window was computed with an unsigned subtraction of epoch
//!   seconds, which underflows if the clock steps backwards.
//!
//! This module owns all of it: one atomically-written file, validated on read,
//! consumed only on success, and guarded by a real lock.

use crate::util::path::normalize_path;
use crate::{log_debug, log_info, log_warn};
use std::fs::{DirBuilder, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

/// How long a copy session stays open for additional `--append` invocations.
///
/// Explorer and Finder invoke the copy verb once per selected item, so a
/// multi-item selection arrives as a burst of separate processes. Anything
/// arriving inside this window joins the same session.
pub const SESSION_WINDOW: Duration = Duration::from_secs(2);

/// A stale lock older than this is assumed to belong to a process that died
/// without cleaning up.
const LOCK_STALE_AFTER: Duration = Duration::from_secs(60 * 60);

const PAYLOAD_FILE: &str = "payload.v3";
const LOCK_FILE: &str = "paste.lock";
/// 0.2 artifacts, removed on first run so a stale payload cannot be revived.
const LEGACY_FILES: &[&str] = &["payload.tmp", "session.tmp"];

/// Identifies one copy session, so appends can only extend the session that
/// created them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionId(u64);

impl SessionId {
    fn new() -> Self {
        // Process id plus wall-clock nanoseconds: unique enough to tell two
        // copy bursts apart, which is all this needs to do.
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos() as u64;
        Self((u64::from(std::process::id()) << 32) | nanos)
    }
}

/// What has been copied, if anything.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardState {
    /// Nothing to paste. The Paste verb is hidden in this state.
    Empty,
    /// A validated set of sources is ready to paste.
    Copied {
        items: Vec<PathBuf>,
        created: SystemTime,
        session: SessionId,
    },
}

impl ClipboardState {
    pub fn items(&self) -> &[PathBuf] {
        match self {
            Self::Empty => &[],
            Self::Copied { items, .. } => items,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items().is_empty()
    }

    /// Whether an `--append` arriving now should extend this session rather
    /// than starting a new one.
    ///
    /// Uses [`SystemTime::elapsed`], whose `Err` case carries a clock that
    /// moved backwards. That is treated as "the session is over", which is the
    /// safe reading — and, unlike 0.2's `now - then` on `u64` seconds, it
    /// cannot underflow.
    pub fn accepts_append(&self, window: Duration) -> bool {
        match self {
            Self::Empty => false,
            Self::Copied { created, .. } => {
                created.elapsed().is_ok_and(|age| age <= window)
            },
        }
    }

    fn session(&self) -> Option<SessionId> {
        match self {
            Self::Empty => None,
            Self::Copied { session, .. } => Some(*session),
        }
    }
}

/// Reads and writes the copy state in a per-user private directory.
///
/// The directory is deliberately not the shared system temp dir: the payload
/// drives a real file copy, so a well-known path in `/tmp` would let another
/// local user pre-plant a symlink or seed attacker-chosen source paths. On Linux
/// this resolves to `$XDG_RUNTIME_DIR` (already 0700 and per-user); elsewhere to
/// a per-user local data directory, and only as a last resort the temp dir.
pub struct ClipboardStore {
    dir: PathBuf,
}

impl Default for ClipboardStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardStore {
    pub fn new() -> Self {
        let base = dirs::runtime_dir()
            .or_else(dirs::data_local_dir)
            .unwrap_or_else(std::env::temp_dir);
        Self::at(base.join("mcopy"))
    }

    /// Build a store rooted at an explicit directory (used by tests).
    pub fn at(dir: PathBuf) -> Self {
        let mut builder = DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        builder.mode(0o700);
        let _ = builder.create(&dir);

        let store = Self { dir };
        store.remove_legacy_files();
        store
    }

    fn payload_path(&self) -> PathBuf {
        self.dir.join(PAYLOAD_FILE)
    }

    fn lock_path(&self) -> PathBuf {
        self.dir.join(LOCK_FILE)
    }

    /// Drop 0.2's payload rather than migrating it.
    ///
    /// The old format carried no session or timestamp, so a payload written
    /// before the upgrade cannot be validated the way 0.3 requires. Discarding
    /// costs the user one re-copy; importing it would resurrect exactly the
    /// stale state this release exists to eliminate.
    fn remove_legacy_files(&self) {
        for name in LEGACY_FILES {
            let path = self.dir.join(name);
            if path.exists() && fs_remove(&path) {
                log_info!("discarded mcopy 0.2 clipboard file {name}");
            }
        }
    }

    /// Load the current state, dropping sources that no longer exist.
    ///
    /// Validation happens here rather than at paste time so that the Paste verb
    /// and the paste command always agree about whether there is anything to do.
    pub fn load(&self) -> ClipboardState {
        let Ok(raw) = std::fs::read_to_string(self.payload_path()) else {
            return ClipboardState::Empty;
        };

        let Some(record) = PayloadRecord::parse(&raw) else {
            log_warn!("clipboard payload was unreadable; discarding it");
            self.clear();
            return ClipboardState::Empty;
        };

        let total = record.items.len();
        let items: Vec<PathBuf> = record
            .items
            .into_iter()
            .filter(|path| path.exists())
            .collect();

        if items.len() != total {
            log_warn!(
                "{} of {total} copied sources no longer exist",
                total - items.len()
            );
        }

        if items.is_empty() {
            // Every source is gone: there is nothing to paste, and leaving the
            // file behind would keep the Paste verb visible for nothing.
            self.clear();
            return ClipboardState::Empty;
        }

        ClipboardState::Copied {
            items,
            created: record.created,
            session: record.session,
        }
    }

    /// Replace the state with a fresh copy session.
    pub fn store(&self, items: &[PathBuf]) -> anyhow::Result<ClipboardState> {
        self.write_state(items, SessionId::new(), SystemTime::now())
    }

    /// Add to the current session, or start a new one if it has expired.
    ///
    /// Returns the resulting state so callers do not have to re-read it.
    pub fn append(
        &self,
        items: &[PathBuf],
        window: Duration,
    ) -> anyhow::Result<ClipboardState> {
        let current = self.load();

        let (mut merged, session, created) = if current.accepts_append(window) {
            let created = match &current {
                ClipboardState::Copied { created, .. } => *created,
                ClipboardState::Empty => SystemTime::now(),
            };
            (
                current.items().to_vec(),
                current.session().unwrap_or_else(SessionId::new),
                created,
            )
        } else {
            (Vec::new(), SessionId::new(), SystemTime::now())
        };

        // Preserve selection order while dropping duplicates. The list is a
        // user's selection, so it stays small enough that a linear contains
        // check is cheaper than building a hash set.
        for item in items {
            if !merged.contains(item) {
                merged.push(item.clone());
            }
        }

        self.write_state(&merged, session, created)
    }

    fn write_state(
        &self,
        items: &[PathBuf],
        session: SessionId,
        created: SystemTime,
    ) -> anyhow::Result<ClipboardState> {
        let items: Vec<PathBuf> =
            items.iter().cloned().map(normalize_path).collect();

        if items.is_empty() {
            anyhow::bail!("No valid file paths were found to copy");
        }

        let record = PayloadRecord {
            items: items.clone(),
            created,
            session,
        };
        self.write_atomic(&record.render())?;

        log_debug!("stored {} copied item(s)", items.len());
        Ok(ClipboardState::Copied {
            items,
            created,
            session,
        })
    }

    /// Write the payload atomically.
    ///
    /// A crash or a concurrent reader must never observe a half-written list of
    /// paths, because each line becomes a real filesystem operation. Writing to
    /// a process-unique temporary file and renaming makes the swap atomic on
    /// every supported platform.
    fn write_atomic(&self, contents: &str) -> anyhow::Result<()> {
        let final_path = self.payload_path();
        let staging_path = self
            .dir
            .join(format!("{PAYLOAD_FILE}.{}", std::process::id()));

        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        options.mode(0o600);

        {
            let mut file = options.open(&staging_path).map_err(|e| {
                anyhow::anyhow!("could not write the clipboard payload: {e}")
            })?;
            file.write_all(contents.as_bytes())?;
            // Durability before the rename, so a crash cannot leave the new
            // name pointing at an empty file.
            file.sync_all()?;
        }

        // On Windows `rename` fails if the destination exists, so replace it.
        #[cfg(windows)]
        let renamed = {
            let _ = std::fs::remove_file(&final_path);
            std::fs::rename(&staging_path, &final_path)
        };
        #[cfg(not(windows))]
        let renamed = std::fs::rename(&staging_path, &final_path);

        if let Err(error) = renamed {
            let _ = std::fs::remove_file(&staging_path);
            anyhow::bail!("could not commit the clipboard payload: {error}");
        }

        Ok(())
    }

    /// Drop the state unconditionally.
    pub fn clear(&self) {
        if fs_remove(&self.payload_path()) {
            log_debug!("cleared the clipboard payload");
        }
    }

    /// Consume the state after a paste that fully succeeded.
    ///
    /// The counterpart to [`ClipboardStore::clear`] in intent: only a completed,
    /// uncancelled paste reaches this, so a cancelled or partially failed paste
    /// keeps its state and the user can simply try again.
    pub fn consume(&self, session: SessionId) {
        match self.load() {
            ClipboardState::Copied {
                session: current, ..
            } if current == session => {
                self.clear();
                log_info!("copy state consumed after a successful paste");
            },
            ClipboardState::Copied { .. } => {
                // A newer copy happened while this paste ran; keeping it is
                // correct, since the user's most recent intent wins.
                log_info!(
                    "a newer copy replaced the pasted session; keeping it"
                );
            },
            ClipboardState::Empty => {},
        }
    }

    /// Take the single-flight paste lock.
    ///
    /// Returns `None` when another paste already holds it. Prevents two
    /// right-click Paste invocations from racing into the same destination.
    pub fn try_lock_paste(&self) -> Option<PasteLock> {
        let path = self.lock_path();

        if let Some(age) = lock_age(&path)
            && age > LOCK_STALE_AFTER
        {
            log_warn!("reclaiming a stale paste lock");
            let _ = std::fs::remove_file(&path);
        }

        let mut options = OpenOptions::new();
        // `create_new` is the atomic test-and-set: exactly one caller can win,
        // even across processes.
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);

        match options.open(&path) {
            Ok(mut file) => {
                let _ = write!(file, "{}", std::process::id());
                Some(PasteLock { path })
            },
            Err(error) => {
                log_debug!("paste lock is held: {error}");
                None
            },
        }
    }
}

/// Age of the lock file, or `None` if there is no lock.
fn lock_age(path: &Path) -> Option<Duration> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    // A lock whose timestamp is in the future (clock step) is treated as fresh
    // rather than stale, which errs toward refusing a concurrent paste.
    modified.elapsed().ok()
}

fn fs_remove(path: &Path) -> bool {
    std::fs::remove_file(path).is_ok()
}

/// RAII guard releasing the paste lock on drop, including on panic or on an
/// early `?` return.
pub struct PasteLock {
    path: PathBuf,
}

impl Drop for PasteLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// On-disk representation.
///
/// A tiny line-oriented format rather than JSON, to avoid a serialization
/// dependency for three fields. Paths cannot contain a newline on any supported
/// platform, so one path per line is unambiguous.
struct PayloadRecord {
    items: Vec<PathBuf>,
    created: SystemTime,
    session: SessionId,
}

impl PayloadRecord {
    const HEADER: &'static str = "mcopy-clipboard-v3";

    fn render(&self) -> String {
        let created = self
            .created
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut out = format!(
            "{}\nsession={}\ncreated={}\n",
            Self::HEADER,
            self.session.0,
            created
        );
        for item in &self.items {
            out.push_str(&item.to_string_lossy());
            out.push('\n');
        }
        out
    }

    fn parse(raw: &str) -> Option<Self> {
        let mut lines = raw.lines();
        if lines.next()? != Self::HEADER {
            return None;
        }

        let session = lines.next()?.strip_prefix("session=")?.parse().ok()?;
        let created_secs: u64 =
            lines.next()?.strip_prefix("created=")?.parse().ok()?;

        let items: Vec<PathBuf> = lines
            .filter(|line| !line.trim().is_empty())
            .map(PathBuf::from)
            .collect();

        Some(Self {
            items,
            created: UNIX_EPOCH + Duration::from_secs(created_secs),
            session: SessionId(session),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        store: ClipboardStore,
        _dir: tempfile::TempDir,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = ClipboardStore::at(dir.path().join("state"));
        Fixture { store, _dir: dir }
    }

    fn make_file(fixture: &Fixture, name: &str) -> PathBuf {
        let path = fixture._dir.path().join(name);
        std::fs::write(&path, b"contents").expect("write source file");
        path
    }

    #[test]
    fn a_fresh_store_is_empty() {
        let fixture = fixture();
        assert_eq!(fixture.store.load(), ClipboardState::Empty);
        assert!(fixture.store.load().is_empty());
    }

    #[test]
    fn stored_items_round_trip() {
        let fixture = fixture();
        let a = make_file(&fixture, "a.txt");
        let b = make_file(&fixture, "b.txt");

        fixture.store.store(&[a.clone(), b.clone()]).unwrap();

        let loaded = fixture.store.load();
        assert_eq!(loaded.items(), &[a, b]);
    }

    #[test]
    fn storing_nothing_is_an_error_rather_than_an_empty_session() {
        let fixture = fixture();
        assert!(fixture.store.store(&[]).is_err());
        assert_eq!(fixture.store.load(), ClipboardState::Empty);
    }

    /// The headline bug: Paste stayed armed forever after a successful paste.
    #[test]
    fn consume_clears_the_state() {
        let fixture = fixture();
        let a = make_file(&fixture, "a.txt");
        let state = fixture.store.store(&[a]).unwrap();

        fixture.store.consume(state.session().unwrap());
        assert_eq!(fixture.store.load(), ClipboardState::Empty);
    }

    /// A restart must not resurrect a consumed session.
    #[test]
    fn a_consumed_session_stays_gone_across_reloads() {
        let fixture = fixture();
        let a = make_file(&fixture, "a.txt");
        let state = fixture.store.store(&[a]).unwrap();
        fixture.store.consume(state.session().unwrap());

        // A second store instance models a fresh process.
        let reopened = ClipboardStore::at(fixture.store.dir.clone());
        assert_eq!(reopened.load(), ClipboardState::Empty);
    }

    /// A copy made while a paste was running represents newer intent and must
    /// survive that paste's completion.
    #[test]
    fn consume_keeps_a_newer_session() {
        let fixture = fixture();
        let a = make_file(&fixture, "a.txt");
        let b = make_file(&fixture, "b.txt");

        let old = fixture.store.store(&[a]).unwrap();
        let new = fixture.store.store(std::slice::from_ref(&b)).unwrap();
        assert_ne!(old.session(), new.session());

        fixture.store.consume(old.session().unwrap());

        assert_eq!(fixture.store.load().items(), &[b]);
    }

    #[test]
    fn load_drops_sources_that_no_longer_exist() {
        let fixture = fixture();
        let kept = make_file(&fixture, "kept.txt");
        let removed = make_file(&fixture, "removed.txt");

        fixture
            .store
            .store(&[kept.clone(), removed.clone()])
            .unwrap();
        std::fs::remove_file(&removed).unwrap();

        assert_eq!(fixture.store.load().items(), &[kept]);
    }

    #[test]
    fn load_becomes_empty_when_every_source_is_gone() {
        let fixture = fixture();
        let only = make_file(&fixture, "only.txt");
        fixture.store.store(std::slice::from_ref(&only)).unwrap();
        std::fs::remove_file(&only).unwrap();

        assert_eq!(fixture.store.load(), ClipboardState::Empty);
        // The stale payload is removed so the Paste verb does not linger.
        assert!(!fixture.store.payload_path().exists());
    }

    #[test]
    fn a_corrupt_payload_is_discarded_rather_than_trusted() {
        let fixture = fixture();
        std::fs::write(fixture.store.payload_path(), "garbage\nlines\n")
            .unwrap();

        assert_eq!(fixture.store.load(), ClipboardState::Empty);
        assert!(!fixture.store.payload_path().exists());
    }

    #[test]
    fn append_extends_a_live_session() {
        let fixture = fixture();
        let a = make_file(&fixture, "a.txt");
        let b = make_file(&fixture, "b.txt");

        let first = fixture
            .store
            .append(std::slice::from_ref(&a), SESSION_WINDOW)
            .unwrap();
        let second = fixture
            .store
            .append(std::slice::from_ref(&b), SESSION_WINDOW)
            .unwrap();

        assert_eq!(second.items(), &[a, b]);
        assert_eq!(first.session(), second.session());
    }

    #[test]
    fn append_starts_a_new_session_once_the_window_closes() {
        let fixture = fixture();
        let a = make_file(&fixture, "a.txt");
        let b = make_file(&fixture, "b.txt");

        let first = fixture.store.append(&[a], SESSION_WINDOW).unwrap();
        // A zero-length window makes any prior session expired.
        let second = fixture
            .store
            .append(std::slice::from_ref(&b), Duration::from_secs(0))
            .unwrap();

        assert_eq!(second.items(), &[b]);
        assert_ne!(first.session(), second.session());
    }

    #[test]
    fn append_does_not_duplicate_an_item() {
        let fixture = fixture();
        let a = make_file(&fixture, "a.txt");

        fixture
            .store
            .append(std::slice::from_ref(&a), SESSION_WINDOW)
            .unwrap();
        let state = fixture
            .store
            .append(std::slice::from_ref(&a), SESSION_WINDOW)
            .unwrap();

        assert_eq!(state.items(), &[a]);
    }

    /// 0.2 computed the session window as `now - then` on `u64` seconds, which
    /// underflows when the clock steps backwards.
    #[test]
    fn a_future_timestamp_closes_the_session_instead_of_underflowing() {
        let state = ClipboardState::Copied {
            items: vec![PathBuf::from("/tmp/x")],
            created: SystemTime::now() + Duration::from_secs(3600),
            session: SessionId::new(),
        };

        assert!(!state.accepts_append(SESSION_WINDOW));
    }

    #[test]
    fn an_empty_state_never_accepts_an_append() {
        assert!(!ClipboardState::Empty.accepts_append(SESSION_WINDOW));
    }

    #[test]
    fn the_paste_lock_admits_one_holder() {
        let fixture = fixture();

        let first = fixture.store.try_lock_paste();
        assert!(first.is_some(), "the first paste should acquire the lock");
        assert!(
            fixture.store.try_lock_paste().is_none(),
            "a concurrent paste must be rejected"
        );

        drop(first);
        assert!(
            fixture.store.try_lock_paste().is_some(),
            "the lock must be released when the guard drops"
        );
    }

    #[test]
    fn the_paste_lock_is_released_on_unwind() {
        let fixture = fixture();
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _guard = fixture.store.try_lock_paste().unwrap();
                panic!("simulated failure during paste");
            }));

        assert!(result.is_err());
        assert!(
            fixture.store.try_lock_paste().is_some(),
            "a panicking paste must not wedge the lock forever"
        );
    }

    #[test]
    fn legacy_files_are_discarded_on_open() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("state");
        std::fs::create_dir_all(&root).unwrap();
        for name in LEGACY_FILES {
            std::fs::write(root.join(name), "/some/old/path\n").unwrap();
        }

        let store = ClipboardStore::at(root.clone());

        for name in LEGACY_FILES {
            assert!(
                !root.join(name).exists(),
                "{name} from 0.2 should not survive the upgrade"
            );
        }
        assert_eq!(store.load(), ClipboardState::Empty);
    }

    #[test]
    fn writing_leaves_no_staging_file_behind() {
        let fixture = fixture();
        let a = make_file(&fixture, "a.txt");
        fixture.store.store(&[a]).unwrap();

        let staging: Vec<_> = std::fs::read_dir(&fixture.store.dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| {
                name.starts_with(PAYLOAD_FILE) && name != PAYLOAD_FILE
            })
            .collect();

        assert!(
            staging.is_empty(),
            "temporary payload files leaked: {staging:?}"
        );
    }

    #[test]
    fn payload_records_round_trip_through_the_file_format() {
        let record = PayloadRecord {
            items: vec![PathBuf::from("/tmp/a b"), PathBuf::from("/tmp/c")],
            created: UNIX_EPOCH + Duration::from_secs(1_700_000_000),
            session: SessionId(42),
        };

        let parsed = PayloadRecord::parse(&record.render()).expect("parses");
        assert_eq!(parsed.items, record.items);
        assert_eq!(parsed.created, record.created);
        assert_eq!(parsed.session, record.session);
    }

    #[test]
    fn a_payload_from_an_unknown_version_is_rejected() {
        assert!(PayloadRecord::parse("mcopy-clipboard-v2\n/tmp/a\n").is_none());
        assert!(PayloadRecord::parse("").is_none());
    }
}
