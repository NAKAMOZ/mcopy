# Platform validation checklist — 0.3.0

Automated tests cover the state machine, the escaping rules, the window
configuration and the shell-integration side effects. What they cannot cover is
anything that needs a real desktop session: whether a window actually appears in
the Dock, whether a click actually lands, whether a theme switch actually
repaints. Those are listed here.

Run each column on a clean user profile. `[CI]` marks rows already enforced by
an automated test; they are listed for completeness, not for re-testing by hand.

## Legend

| Mark   | Meaning                                              |
| ------ | ---------------------------------------------------- |
| `auto` | Covered by an automated test; no manual check needed |
| `☐`    | Needs a manual check on that platform                |
| `—`    | Not applicable                                       |

---

## Installation and launch

| #   | Check                                                                                                | Windows | macOS | Linux |
| --- | ---------------------------------------------------------------------------------------------------- | ------- | ----- | ----- |
| 1   | The installer completes without an administrator or root prompt                                      | ☐       | ☐     | ☐     |
| 2   | The app appears in the Start menu / Launchpad / applications menu                                    | ☐       | ☐     | ☐     |
| 3   | **Delete the installer, eject the `.dmg`, delete the `.deb`, then launch the app** — it still starts | ☐       | ☐     | ☐     |
| 4   | Right-click menu entries still work after step 3                                                     | ☐       | ☐     | ☐     |
| 5   | Running from the download folder refuses to register menus and says why                              | auto    | auto  | auto  |
| 6   | Uninstall via Settings ▸ Apps / Trash / `apt remove` completes cleanly                               | ☐       | ☐     | ☐     |
| 7   | After uninstall: no shortcuts, no menu entries, no running processes                                 | ☐       | ☐     | ☐     |
| 8   | Reinstalling over an existing install works                                                          | ☐       | ☐     | ☐     |

Notes:

- On Windows, the installer is per-user and installs to
  `%LOCALAPPDATA%\Programs\mcopy`. Expect a SmartScreen warning (unsigned).
- On macOS, expect a Gatekeeper warning; use right-click ▸ Open the first time.
- On Linux, the `.deb` installs the binary and launcher; the _menu_ entries are
  per-user and are registered by pressing Install in the app once.

## Theme

| #   | Check                                                          | Windows | macOS | Linux |
| --- | -------------------------------------------------------------- | ------- | ----- | ----- |
| 9   | Light mode: the four logo bars are black                       | ☐       | ☐     | ☐     |
| 10  | Dark mode: the four logo bars are white                        | ☐       | ☐     | ☐     |
| 11  | **The green bar is `#22c55e` in both modes**                   | auto    | auto  | auto  |
| 12  | Switching the system theme with a window open repaints it live | ☐       | ☐     | ☐     |
| 13  | Text and buttons stay legible in both modes                    | auto    | auto  | auto  |

On Linux, the theme is read from the XDG desktop portal `color-scheme` setting;
a session without a portal will report light mode and stay there.

## Copy progress window

| #   | Check                                                               | Windows | macOS | Linux |
| --- | ------------------------------------------------------------------- | ------- | ----- | ----- |
| 14  | Appears in the taskbar / Dock / window switcher during a copy       | ☐       | ☐     | ☐     |
| 15  | Can be minimized mid-copy and restored                              | ☐       | ☐     | ☐     |
| 16  | Alt-Tab / Cmd-Tab reaches it                                        | ☐       | ☐     | ☐     |
| 17  | Clicking it in the taskbar focuses it                               | ☐       | ☐     | ☐     |
| 18  | It is configured as a normal, minimizable window                    | auto    | auto  | auto  |
| 19  | Pause and Resume actually stop and restart the queue                | ☐       | ☐     | ☐     |
| 20  | Cancel stops the queue and the window reports "Cancelled"           | ☐       | ☐     | ☐     |
| 21  | A clean copy closes the window automatically                        | ☐       | ☐     | ☐     |
| 22  | A copy with failures leaves the window open with the reason visible | auto    | auto  | auto  |

Use a large enough source tree that the copy lasts several seconds; a copy that
finishes instantly cannot demonstrate rows 14–20.

## Setup window shutdown

| #   | Check                                                                                   | Windows | macOS | Linux |
| --- | --------------------------------------------------------------------------------------- | ------- | ----- | ----- |
| 23  | **One click on the close button closes it**                                             | ☐       | ☐     | ☐     |
| 24  | The OS close button / Alt-F4 / Cmd-W also closes it in one action                       | ☐       | ☐     | ☐     |
| 25  | Closing while an install is running cancels and closes                                  | ☐       | ☐     | ☐     |
| 26  | No `mcopy` process remains afterwards (Task Manager / Activity Monitor / `pgrep mcopy`) | ☐       | ☐     | ☐     |
| 27  | Repeated close requests collapse into one shutdown                                      | auto    | auto  | auto  |
| 28  | The install worker is joined, not abandoned                                             | auto    | auto  | auto  |

## Permissions

| #   | Check                                                                                   | Windows | macOS | Linux |
| --- | --------------------------------------------------------------------------------------- | ------- | ----- | ----- |
| 29  | **Registering the menu never prompts for administrator or root**                        | auto    | auto  | auto  |
| 30  | Menu entries are written under the current user only                                    | auto    | —     | auto  |
| 31  | Pasting into a protected folder shows a named reason, not a bare count                  | ☐       | ☐     | ☐     |
| 32  | macOS: pasting into `~/Desktop` or `~/Documents` either works or names Full Disk Access | —       | ☐     | —     |
| 33  | Pasting into a read-only volume fails before starting, with a reason                    | ☐       | ☐     | ☐     |
| 34  | Pasting a folder into itself is refused                                                 | auto    | auto  | auto  |
| 35  | Upgrading from 0.2 removes or reports the old machine-wide entries                      | ☐       | —     | —     |

For row 31 on Windows, use a folder protected by Controlled Folder Access, or
any directory whose ACL denies write.

## Copy and paste state

| #   | Check                                                        | Windows | macOS | Linux |
| --- | ------------------------------------------------------------ | ------- | ----- | ----- |
| 36  | Fresh profile: **no Paste entry before anything is copied**  | ☐       | n/a   | ☐     |
| 37  | After Copy: the Paste entry appears                          | ☐       | n/a   | ☐     |
| 38  | After a successful Paste: the entry disappears               | ☐       | n/a   | ☐     |
| 39  | Delete the copied source, reopen the menu: the entry is gone | ☐       | n/a   | ☐     |
| 40  | A cancelled paste keeps the entry, so it can be retried      | auto    | auto  | auto  |
| 41  | A failed paste keeps the entry                               | auto    | auto  | auto  |
| 42  | Reboot after a successful paste: the entry stays hidden      | ☐       | n/a   | ☐     |
| 43  | Only a new copy brings the entry back                        | auto    | auto  | auto  |
| 44  | Two rapid pastes: the second reports "already in progress"   | auto    | auto  | auto  |
| 45  | Multi-select copy of several files produces one session      | auto    | auto  | auto  |
| 46  | Pasting into a drive root (`D:\`) works                      | ☐       | —     | —     |
| 47  | macOS: Paste with nothing copied shows "Nothing to paste"    | —       | ☐     | —     |

Rows 36–39 and 42 are marked `n/a` for macOS: `NSServices` visibility is static,
so the Paste service stays listed there by design. Row 47 is its replacement.

## Linux desktop environments

Row 14 and rows 36–39 depend on the desktop environment. Check at least:

| Environment            | Taskbar presence | Nautilus menu | Dolphin menu |
| ---------------------- | ---------------- | ------------- | ------------ |
| GNOME / Wayland        | ☐                | ☐             | —            |
| GNOME / X11            | ☐                | ☐             | —            |
| KDE Plasma / Wayland   | ☐                | —             | ☐            |
| KDE Plasma / X11       | ☐                | —             | ☐            |
| A tiling WM (sway, i3) | ☐                | —             | —            |

Client-side decorations and window transparency render inconsistently across
compositors, and some tiling window managers ignore taskbar hints entirely.
Record what you observe rather than treating a difference as a failure.

---

## Reproducing the automated coverage

```bash
cargo test --locked          # unit + integration
cargo clippy --all-targets --locked -- -D warnings
cargo fmt --all --check
```

The shell-integration tests write to the real per-user integration points and
restore the previous state afterwards. They need a desktop user account but no
elevation — which is itself the assertion.
