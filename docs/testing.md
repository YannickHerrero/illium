# Testing

Winarchy is an integration candidate, not a verified V1 release. Automated checks cover the pure logic; the interactive behaviour is exercised by opt-in desktop tests that rearrange real windows.

## Automated checks

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings
cargo test --workspace --locked
python3 scripts/check-cli-dependencies.py
```

The Linux test suite covers the command grammar, configuration loading and invalid reloads, palettes, rule matching, keyboard chords and modifier tracking, Fibonacci geometry and non-overlap, directional navigation, workspace ordering, IPC framing and timeouts, and the models of the companion applications (process sorting and filtering, file listing, selection, clipboard and prompts against a temporary tree). The dependency check keeps `winarchyctl` free of the daemon and UI toolkit. CI runs the same checks on `windows-latest` and packages the release binaries.

## Desktop tests

Desktop tests are `#[ignore]`d and never run in CI. **Save your work first: they rearrange, hide and close windows.** Use a disposable configuration directory:

```powershell
$env:WINARCHY_CONFIG_HOME = "$env:USERPROFILE\winarchy-test"
.\target\debug\winarchy.exe
cargo test --test desktop -- --ignored --exact ipc_desktop_smoke --nocapture
# Only on an unlocked desktop:
cargo test --test desktop -- --ignored --exact desktop_smoke --nocapture
.\target\debug\winarchyctl.exe quit
# No existing daemon may be running for these:
$env:WINARCHY_TEST_DAEMON = (Resolve-Path .\target\debug\winarchy.exe).Path
cargo test --test startup -- --ignored --exact enrolls_preexisting_windows_and_tracks_restoration --nocapture
cargo test --test desktop -- --ignored --exact crash_restores_hidden_windows --nocapture
# Destructive Explorer-session test: close File Explorer windows first.
cargo test --test desktop -- --ignored --exact replacement_crash_restores_explorer --nocapture
```

Do not run the ignored tests concurrently: they share the current user's daemon and desktop. From WSL, compile with `cargo xwin test --target x86_64-pc-windows-msvc --no-run`, copy the reported executables to Windows and run them there.

`ipc_desktop_smoke` creates disposable native windows and checks discovery, directional moves, floating and fullscreen geometry, workspace membership and visibility, launcher toggling, theme changes and reload behaviour through IPC. `crash_restores_hidden_windows` and `replacement_crash_restores_explorer` kill a daemon and check that the watchdog restores hidden windows and Explorer. `desktop_smoke` asserts real foreground changes and application launches and needs an unlocked, interactive desktop.

## Known scope and implementation constraints

- One globally active workspace; no independent per-monitor workspace switching UI.
- Client minimum-size constraints can prevent exact Fibonacci rectangles at high window counts.
- Rule changes apply to newly managed windows, not retroactively.
- Complete-config transactional reload, rather than independent partial-subsystem commits.
- Start Menu discovery is refreshed during reload, not watched independently.
- Workspaces are a left-bar module; the clock supports `%H`, `%M`, `%S` only.
- No notifications, tray, shell registry changes, custom decorations or compositor.
- IPC serves one client at a time with bounded deadlines; a misbehaving same-user pipe client can delay other clients.
- While the launcher has keyboard focus, global Alt chords are not intercepted: the low-level hook is not run for input aimed at the daemon's own windows.
- Startup and hot-reload scanning run on the UI thread.
