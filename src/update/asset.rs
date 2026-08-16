//! Picking the right file out of a release, and proving it arrived intact.
//!
//! The names matched here are produced by the packaging scripts, so the two
//! must stay in step:
//! - `scripts/package-windows.ps1` → `mcopy-setup-<version>-x86_64.exe`
//! - `scripts/package-macos.sh`    → `mcopy-<version>.pkg`
//! - `scripts/package-linux.sh`    → `mcopy-<version>-x86_64.AppImage`

use crate::update::github::Asset;

/// The checksum manifest attached to every release from 0.3.1 on.
pub const CHECKSUM_ASSET: &str = "SHA256SUMS";

/// How the running mcopy can be updated in place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateStyle {
    /// A downloadable artifact this build knows how to install.
    Automatic,
    /// Updatable, but not by us — the user is sent to the release page.
    ///
    /// A tarball install spreads files across a prefix, so there is no single
    /// file to swap; unpacking a new one over `~/.local` is the installer's
    /// job, not the running binary's.
    Manual,
}

/// Whether this build can install its own update, and which file it needs.
pub fn update_style() -> UpdateStyle {
    #[cfg(target_os = "linux")]
    {
        // Only an AppImage is a single self-contained file we can replace.
        if std::env::var_os("APPIMAGE").is_none() {
            return UpdateStyle::Manual;
        }
    }

    UpdateStyle::Automatic
}

/// Find the artifact for the current platform.
pub fn pick_for_this_platform(assets: &[Asset]) -> Option<&Asset> {
    assets
        .iter()
        .find(|asset| matches_this_platform(&asset.name))
}

/// Whether `name` is the artifact this OS installs.
///
/// macOS deliberately takes the `.pkg` and never the `.dmg`: a `.pkg` can be
/// handed to `Installer.app` and runs the postinstall script that re-registers
/// the Finder Services, whereas a `.dmg` needs a drag gesture no program can
/// perform for the user.
fn matches_this_platform(name: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        name.starts_with("mcopy-setup-") && name.ends_with("-x86_64.exe")
    }
    #[cfg(target_os = "macos")]
    {
        name.starts_with("mcopy-") && name.ends_with(".pkg")
    }
    #[cfg(target_os = "linux")]
    {
        name.starts_with("mcopy-") && name.ends_with("-x86_64.AppImage")
    }
    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "linux"
    )))]
    {
        let _ = name;
        false
    }
}

/// Look up one file's expected digest in a `sha256sum` manifest.
///
/// The format is `<64 hex chars><two spaces><filename>` per line; the second
/// space is a `*` for files written in binary mode, which is why the name is
/// matched with the separator trimmed rather than by an exact split.
pub fn expected_digest(manifest: &str, file_name: &str) -> Option<String> {
    manifest.lines().find_map(|line| {
        let (digest, name) =
            line.split_once("  ").or_else(|| line.split_once(" *"))?;
        let digest = digest.trim();
        if name.trim() != file_name || digest.len() != 64 {
            return None;
        }
        if !digest.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        Some(digest.to_ascii_lowercase())
    })
}

/// Hash a downloaded file.
pub fn digest_of(path: &std::path::Path) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};

    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> Asset {
        Asset {
            name: name.to_string(),
            url: format!("https://example.invalid/{name}"),
        }
    }

    /// The full asset list of a real release: each platform must take its own
    /// file and nothing else.
    #[test]
    fn each_platform_picks_its_own_artifact() {
        let assets = vec![
            asset("mcopy-setup-0.4.0-x86_64.exe"),
            asset("mcopy-0.4.0.pkg"),
            asset("mcopy-0.4.0.dmg"),
            asset("mcopy-0.4.0-x86_64.AppImage"),
            asset("mcopy-0.4.0-x86_64.tar.gz"),
            asset(CHECKSUM_ASSET),
        ];

        let picked = pick_for_this_platform(&assets).expect("an artifact");

        #[cfg(target_os = "windows")]
        assert_eq!(picked.name, "mcopy-setup-0.4.0-x86_64.exe");
        #[cfg(target_os = "macos")]
        assert_eq!(picked.name, "mcopy-0.4.0.pkg");
        #[cfg(target_os = "linux")]
        assert_eq!(picked.name, "mcopy-0.4.0-x86_64.AppImage");
    }

    /// macOS must never pick the `.dmg`: it cannot be installed unattended.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_disk_image_is_never_chosen() {
        let assets = vec![asset("mcopy-0.4.0.dmg")];
        assert!(pick_for_this_platform(&assets).is_none());
    }

    /// The tarball is not a single-file swap, so Linux ignores it.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_tarball_is_never_chosen() {
        let assets = vec![asset("mcopy-0.4.0-x86_64.tar.gz")];
        assert!(pick_for_this_platform(&assets).is_none());
    }

    #[test]
    fn a_release_without_our_artifact_yields_nothing() {
        let assets = vec![asset("source-code.zip"), asset(CHECKSUM_ASSET)];
        assert!(pick_for_this_platform(&assets).is_none());
    }

    #[test]
    fn a_digest_is_found_by_file_name() {
        let manifest = "\
0000000000000000000000000000000000000000000000000000000000000001  mcopy-0.4.0.pkg
0000000000000000000000000000000000000000000000000000000000000002  mcopy-0.4.0-x86_64.AppImage
";
        assert_eq!(
            expected_digest(manifest, "mcopy-0.4.0-x86_64.AppImage").as_deref(),
            Some(
                "0000000000000000000000000000000000000000000000000000000000000002"
            )
        );
    }

    /// `sha256sum --binary` writes ` *` instead of two spaces.
    #[test]
    fn the_binary_mode_separator_is_understood() {
        let manifest = "0000000000000000000000000000000000000000000000000000000000000003 *mcopy-setup-0.4.0-x86_64.exe";
        assert_eq!(
            expected_digest(manifest, "mcopy-setup-0.4.0-x86_64.exe")
                .as_deref(),
            Some(
                "0000000000000000000000000000000000000000000000000000000000000003"
            )
        );
    }

    #[test]
    fn an_absent_file_has_no_digest() {
        let manifest = "0000000000000000000000000000000000000000000000000000000000000001  other.bin";
        assert!(expected_digest(manifest, "mcopy-0.4.0.pkg").is_none());
    }

    /// A truncated or non-hex digest must not be accepted as a match; treating
    /// it as valid would let a malformed manifest wave a bad download through.
    #[test]
    fn a_malformed_digest_is_rejected() {
        let short = "abc  mcopy-0.4.0.pkg";
        assert!(expected_digest(short, "mcopy-0.4.0.pkg").is_none());

        let not_hex = "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz  mcopy-0.4.0.pkg";
        assert!(expected_digest(not_hex, "mcopy-0.4.0.pkg").is_none());
    }

    #[test]
    fn a_known_file_hashes_to_its_digest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("payload");
        std::fs::write(&path, b"abc").unwrap();

        // The published SHA-256 of "abc".
        assert_eq!(
            digest_of(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
