# mcopy — Optimization Opportunities

A step-by-step review of code structures that can be optimized, with the reasoning and the concrete change for each. Ordered roughly by impact-to-effort.

---

## 1. Drop the redundant `metadata` syscall in the copy loop

**Where:** [src/lib.rs:222-243](src/lib.rs#L222-L243)

**Now:** Each file does two filesystem calls — `fs::metadata(&src)` to learn the size, then `fs::copy(&src, &dst)`.

```rust
let file_size = match fs::metadata(&src).await { ... };
match fs::copy(&src, &dst).await { ... }
```

**Why it matters:** `fs::copy` already returns the number of bytes copied (`u64`). The separate `metadata` call is one extra `stat` syscall per file. On a tree of N files that is N wasted syscalls — measurable on large copies and on network/slow disks.

**Do:** Use the return value of `fs::copy` for `file_bytes`. Emit the `Started` update with `file_bytes: 0` (already the case), and on success read the size from `Ok(bytes)`.

```rust
match fs::copy(&src, &dst).await {
    Ok(bytes) => {
        let processed = files_processed.fetch_add(1, Ordering::Relaxed) + 1;
        // use `bytes` instead of a pre-fetched file_size
    }
    Err(e) => { ... }
}
```

**Trade-off:** On a copy *failure* you no longer have the size (currently reported as `file_bytes: file_size`). The UI only sums bytes for completed files, so reporting `0` on failure is fine. Keep the `metadata` path only if failed-byte accounting is a real requirement.

---

## 2. Parallelize directory traversal in `collect_files`

**Where:** [src/lib.rs:99-136](src/lib.rs#L99-L136)

**Now:** A single-threaded stack walk. Each `read_dir` and every `next_entry().await` runs serially, one directory at a time.

**Why it matters:** Directory enumeration on a deep/wide tree is latency-bound. While one directory is being `stat`-ed, no other directory is being read. On spinning disks and network shares this dominates the "preparing" phase.

**Do:** Fan out subdirectory reads with bounded concurrency, mirroring the copy stage. Use `futures::stream` with `buffer_unordered`, or process the stack in concurrent batches: pop a level of directories, read them all concurrently, collect files, push child dirs.

**Trade-off:** Unbounded fan-out can exhaust file descriptors on huge trees — bound it (reuse `calculate_concurrency`).

---

## 3. Collect sources concurrently in Paste

**Where:** [src/main.rs:124-129](src/main.rs#L124-L129)

**Now:**

```rust
let mut all_files = Vec::new();
for src in &sources {
    let files = collect_files(src, &target).await?;
    all_files.extend(files);
}
```

Each source root is walked one after another.

**Do:** Run the `collect_files` calls concurrently with `futures::future::try_join_all` (or a bounded stream), then flatten. When a user pastes many independent folders this overlaps their traversal.

**Trade-off:** Combine with #2 so total open-FD count stays bounded across both layers.

---

## 4. Replace the O(n²) dedup in clipboard append

**Where:** [src/clipboard.rs:79-85](src/clipboard.rs#L79-L85)

**Now:**

```rust
for path in paths {
    if let Ok(abs_path) = path.canonicalize().map(normalize_path)
        && !existing.contains(&abs_path)   // linear scan every iteration
    {
        existing.push(abs_path);
    }
}
```

`existing.contains` is O(n); inside the loop that is O(n²).

**Do:** Keep a `HashSet<PathBuf>` of seen paths alongside the ordered `Vec` (to preserve insertion order), or seed the set from the existing payload once. For the small selections Explorer sends this is minor, but it is a free correctness-preserving win and clarifies intent.

---

## 5. De-duplicate the two `AssetSource` implementations

**Where:** [src/ui/window.rs:15-32](src/ui/window.rs#L15-L32) and [src/ui/install.rs:25-42](src/ui/install.rs#L25-L42)

**Now:** `ProgressAssets` and `InstallAssets` are byte-for-byte identical — both serve `logo.svg`.

**Do:** Define one `LogoAssets` (or `McopyAssets`) struct in `src/ui/mod.rs` and use it from both windows. Removes ~18 duplicated lines and a future drift hazard (e.g. adding a second asset in one place only).

---

## 6. Collapse the repetitive Windows registry installers

**Where:** [src/context_menu.rs:96-165](src/context_menu.rs#L96-L165)

**Now:** Five near-identical functions (`install_for_files`, `install_for_directories`, `install_paste_background`, `install_paste_directory`, `install_paste_drive`). Each creates a key, sets a label, writes metadata, optionally sets `MultiSelectModel`, then writes a command.

**Do:** Drive it from a table of entries:

```rust
struct MenuEntry {
    path: &'static str,
    label: &'static str,
    command_template: &'static str, // e.g. "{exe} copy --append \"%1\""
    multi_select: bool,
}
```

Loop over a `const` slice and call one `install_entry(hklm, exe, &entry)`. The same table can feed `uninstall_context_menu` and `MENU_PATHS`/`PRIMARY_MENU_PATH`, removing the three places these paths are currently repeated and kept in sync by hand.

**Trade-off:** Pure maintainability/consistency, not runtime speed — but it kills a real bug class (a path added to install but forgotten in uninstall).

---

## 7. Make the UI refresh loops event-driven instead of fixed-poll

**Where:** [src/ui/window.rs:59-80](src/ui/window.rs#L59-L80) and [src/ui/install.rs:87-99](src/ui/install.rs#L87-L99)

**Now:** Both windows wake every 120 ms forever and call `window.refresh()` whether or not anything changed.

**Why it matters:** Constant repaints burn CPU/GPU and prevent the app from idling — noticeable on battery, and the install window keeps polling even when fully idle.

**Do:** Notify the render context when state actually changes (the progress `apply`/`mark_terminal` path and the install worker thread) via a GPUI handle / `cx.notify`, rather than a blind timer. If a timer is kept for the auto-close countdown, stop the loop once terminal + closed.

**Trade-off:** Slightly more wiring (passing an async notifier into `CopyProgress`). The 120 ms timer is simple; this is a polish/efficiency item, not a correctness fix.

---

## 8. Relax atomic orderings on the hot counter

**Where:** [src/lib.rs](src/lib.rs) (`files_processed`, the `CopyController` flags)

**Now:** Everything uses `Ordering::SeqCst`.

**Why it matters:** `files_processed` is a plain progress counter with no other memory it must synchronize; `SeqCst` is the strongest (and slowest) ordering. The pause/cancel flags only need acquire/release semantics, not a global total order.

**Do:** Use `Ordering::Relaxed` for the `fetch_add`/`load` on `files_processed`, and `Acquire`/`Release` for the controller flags. Effect is small but free, and signals intent correctly.

**Trade-off:** Negligible real-world speedup; mostly correctness-of-intent. Safe because no ordering relationship depends on these.

---

## 9. Reduce per-file lock contention in progress state

**Where:** [src/ui/progress.rs:55-75](src/ui/progress.rs#L55-L75)

**Now:** Every `Started`/`Finished`/`Failed` update takes a `Mutex` lock to bump counters and store `current_file`.

**Why it matters:** With high concurrency (up to 128 tasks) and many small files, all workers contend on one mutex on every event.

**Do:** Move the pure counters (`completed_files`, `failed_files`, `active_files`) to `AtomicUsize` and keep the `Mutex` only for `current_file: String`. The snapshot then reads atomics lock-free and locks only briefly for the filename. For a copy of thousands of tiny files this cuts contention noticeably.

**Trade-off:** `current_file` and the counters are no longer captured under one lock, so a snapshot can show a filename one tick out of step with the counts — cosmetic only for a progress UI.

---

## 10. Avoid the `pause` busy-wait poll

**Where:** [src/lib.rs:48-58](src/lib.rs#L48-L58)

**Now:** `wait_until_resumed` sleeps 80 ms in a loop while paused, per blocked task.

**Why it matters:** Up to `concurrency` tasks each wake every 80 ms doing nothing while paused. Wasteful, though low-severity.

**Do:** Replace the polled `AtomicBool` with a `tokio::sync::Notify` (or watch channel) so paused tasks park until `resume()` wakes them. Cancellation can share the same notify to break the wait immediately.

**Trade-off:** More machinery than two atomics; only worth it if pause is used often or task counts are high.

---

## 11. Smaller cleanups

- **`normalize_path` allocation** — [src/lib.rs:169-176](src/lib.rs#L169-L176) calls `to_string_lossy()` then rebuilds a `PathBuf` even when there is no `\\?\` prefix. Fast-path: return the input unchanged when the prefix is absent (the current `else` already does, but the `to_string_lossy` runs first — check bytes before converting).
- **Panic on clock skew** — [src/clipboard.rs:20-23](src/clipboard.rs#L20-L23) `duration_since(UNIX_EPOCH).unwrap()` panics if the system clock is before epoch. Use `unwrap_or_default()`.
- **Layout magic numbers** — [src/ui/install.rs](src/ui/install.rs) positions every element with absolute `px()` offsets (108, 128, 150, 197, 206, 252…). Brittle: one font/size change shifts everything. Prefer fl/column layout like `window.rs` already uses, or derive offsets from named constants.

---

## Suggested order of work

1. **#1** (drop redundant `metadata`) — highest impact, lowest risk, isolated.
2. **#4, #11** — quick correctness/clarity wins.
3. **#5, #6** — de-duplication; reduces maintenance and bug surface.
4. **#2, #3** — traversal concurrency; real speedup, needs FD-bound care.
5. **#9, #8, #10** — concurrency/lock refinements; measure first.
6. **#7** — event-driven UI; polish.

Validate each step with `cargo build` and `cargo clippy`, and benchmark #1–#3 against a large directory (many small files vs. few large files) before and after.
