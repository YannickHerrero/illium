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

## Native terminal

See [terminal validation and performance probes](terminal.md#reproduce-validation).
Regular tests cover settings, palette fallbacks, VT/Unicode/alternate-screen
behavior, selection, CSI-u Ctrl+digits, AltGr routing, bracketed paste, mouse
encoding and synchronized-update timeout. Windows adds the ConPTY error path.
Opt-in hidden GPU tests verify resize/DPI/font changes and read back actual
background/glyph alpha. A separate WSL roundtrip test validates CSI-u through
ConPTY. A visible desktop probe uses a temporary theme and asserts that live
reload retains both the native HWND and WSL PID. Do not run visible probes
while interacting with their test windows.

## Status bar opacity

The Windows-only offscreen test renders the production Slint bar at 0%, 50%,
85% and 100% opacity. It checks pixel alpha for the base, inactive workspaces
(no double layer), the opaque active indicator, and the full-transparency toggle.
It also verifies the shared temporary override and reset without changing the
running desktop or personal configuration:

```powershell
cargo test -p winarchy --lib bar_fades_only_its_background_and_honors_shared_override -- --nocapture
```

This test passes on Windows. After deployment, `winarchyctl status` exposes
`bar_background_opacity` (null when no bar exists) alongside `bar_transparent`.
Opacity shortcuts and external override edits update bars in place, without
restarting applets or recreating HWNDs. The opt-in live regression also covers a
quick command/reset whose file notifications can coalesce:

```powershell
.\scripts\test-bar-opacity-desktop.ps1 -Bin "$env:LOCALAPPDATA\Programs\Winarchy"
```

It briefly decreases opacity, restores only its own temporary override, and
checks that the status bar follows both updates without changing workspaces.

## Status bar design

The bar uses embedded JetBrains Mono, rounded workspace markers, vertical separators,
and configurable time/date labels (see [configuration](configuration.md#bartoml)).
Time and date have independent click targets, popup anchors and keyboard hints.
The timezone provider's DST/date-rollover checks run with
`powershell -NoProfile -ExecutionPolicy Bypass -File tests/timezones-provider.ps1`.

```powershell
cargo test -p winarchy --lib bar_ -- --test-threads=1
```

The offscreen checks cover workspace/module order, click anchors, clock/date clicks,
top/bottom border placement and transparency, background alpha and hint geometry.
The eight matching tests passed on Windows after the redesign. The two portable
layout tests also passed in a temporary Linux harness compiling the production Slint.
Still check actual DWM composition, hover feedback and different monitor DPIs on Windows.
The independently centered groups do not implement overflow menus: very long custom
labels or narrow displays may require reducing configured modules or label lengths.

## Status bar keyboard hints

Portable tests cover the hint vocabulary (1–9, A–Z, numpad), navigation wrapping,
command parsing and the default configurable shortcut:

```sh
cargo test -p winarchy -p winarchy-ipc --lib --tests
```

The production Slint layout also has an offscreen test for module-center reports,
reopening, non-actionable exclusions and transparent strip/opaque badge pixels.
Windows adds a session test for stale generations, incomplete geometry, invalid
selections and cancellation before the strip is ready:

```powershell
cargo test -p winarchy --lib bar_hints
cargo test -p winarchy --lib escape
```

Implementation validation: the portable suite passed (105 tests); the Slint
layout/pixel test passed in a temporary Linux headless harness compiling the same
`ui/shell.slint` and importing `platform/shell/bar_hints_tests.rs`. Windows code
and tests passed `cargo check -p winarchy --tests --target x86_64-pc-windows-gnu`.
After merging the workspace-order fix, the four hint tests and both bar tests also
passed natively on Windows with the MSVC build. The hint layout test covers modules
both before and after the workspace buttons. Native Escape regressions also pass:
the hook is called directly without injecting desktop input to verify Escape
queued immediately after selection and suppression of held-key repeats; a
headless applet whose `cancel` callback returns true still closes in one press
and receives exactly one `dismissed` callback.
A focused Windows regression now covers an Escape-intercepting client hook installed
*after* Winarchy's hook. Opening a passive popup must renew Winarchy's capture
priority without changing the foreground HWND. Escape closes it, repeats/key-up
stay consumed, and the next press goes back to the client. This opt-in test uses
real hooks and tagged Escape injections (no desktop clicks); run it alone:

```powershell
cargo test -p winarchy --lib passive_popup_escape -- --ignored --test-threads=1
```

It fails with the old popup flag-only publication and passes with capture renewal.
The regular `timezone_columns_align_despite_different_label_widths` test renders
the timezone applet at two widths and compares column edges across all rows,
including midnight and half-hour labels.
**Physical-key interaction with third-party hooks and mixed-DPI behavior still need desktop validation.**

For a failure that cannot be reproduced with injected input, start the daemon
with `WINARCHY_TRACE_ESCAPE=1`. `winarchy.log` then records Escape-only input
metadata (`source=hook` or `source=raw`, down/up, source-specific flags, consumed
state, active capture modes) plus
the foreground HWND and open applet at dispatch time. No other keys or typed
text are recorded, and the hook only enqueues metadata; logging stays on the UI
thread. Popup capture transitions are also logged. Restart without this variable
to disable the diagnostic. Reproduce with the physical key, without running an
input-injection probe between the opening and the failed dismissal.

The input thread also receives keyboard raw-input notifications with `INPUTSINK`
on its hidden event window. Only Escape can produce an action, only while a popup
or bar hints are open, and never during chord recording. The raw fallback handles
a physical Escape missing from the hook without focusing the popup; repeats are
one-shot and ordinary input remains enabled (no `NOLEGACY`). Winit's mouse raw
registration is unchanged; Slint keyboard events continue using regular window
messages. Raw input is observational: if another program intercepts a physical
key, this fallback can dismiss the popup but cannot undo that program's action.
The consuming hook remains the normal path. Native unit tests under `input::raw`
cover modal gating, held-before-opening keys, repeats and hook-consumed presses.

On an unlocked Windows desktop, with the updated default binding installed:

1. Focus an editor. Press Ctrl+Alt+B; check badges align with each actionable
   applet. Press an invalid key, then Escape: no text should appear in the editor.
2. Reopen and select volume/Wi-Fi by digit, and by Left/Right + Enter. Check the
   applet responds to its normal keys and Escape restores the editor's focus.
   Escape must close the entire applet even with a password/confirmation dialog open.
3. Hold the selection key or Escape: repeats must neither type into the editor
   nor reach the previously focused application after the applet closes.
4. Toggle twice, click outside, reload configuration and change display geometry
   while hints are open; check that hints and key capture disappear. Check Ctrl,
   Alt, Shift and Windows keys still work normally after exiting.
5. Try an AZERTY number row without Shift, numpad with Num Lock, more than nine
   modules, duplicate modules, no actionable modules and a disabled bar. Wait
   through provider refreshes: labels must not move or change during selection.
6. Repeat with a bottom bar and monitors at different DPIs, focusing a client on
   each monitor first. Only its bar should show hints; popups must stay anchored.

`winarchyctl status` exposes `bar_hints` and `bar_applet` (name or null) for probes.

## Theme demo scene

Regular tests cover `demo` parsing/serialization, the three-tile geometry,
static terminal fixtures and independent temporary browser libraries/filter lists.
Windows adds a no-shell terminal session test and a hidden WebView home regression:

```powershell
cargo test -p winarchy-terminal demo_ignores_input_and_redraws_without_a_shell
cargo test -p winarchy-browser --bin winarchy-browser demo_home_never_loads_the_normal_library -- --ignored --test-threads=1 --nocapture
```

The browser regression deliberately provides an unreadable normal library,
opens the synthetic home in an isolated profile, checks its five fixture entries,
then verifies cleanup and that the normal file remains unchanged. Both of these
Windows tests passed for the demo implementation; neither starts a user's shell.

For the complete menu/daemon scene, use matching rebuilt binaries and an empty
workspace on an unlocked Windows desktop:

```powershell
.\scripts\test-demo-desktop.ps1 -Bin .\target\debug -KeepScene
```

The opt-in probe checks startup readiness, ordering, 50/50 geometry, focus and
occupied-workspace rejection. Without `-KeepScene` it closes only the demo HWNDs
whose process ownership and ready marker still match. It never restarts the
normal daemon. Full desktop geometry, menu selection, live theme switching,
startup cancellation/failure, and mixed-DPI appearance still need this desktop
check/manual validation. See [demo privacy limits](themes.md#demo-scene-for-theme-screenshots).

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

`wallpapers_follow_selection_and_directory_changes` temporarily installs a tiny fixture theme and checks image discovery, cycling, solid backgrounds, remembered choices, corrupt/deleted image fallback, rapid requests, cancellation and asynchronous errors through IPC. It restores the original theme and wallpaper-selection files. Run it only with an upgraded daemon and matching `WINARCHY_CONFIG_HOME`.

Performance probes (release builds): `cargo run -p winarchy-theme --release --example profile_wallpaper -- <wallpaper-directory> 2560 1600` measures decoding and fitting without changing files or the desktop. On Windows, `cargo run --release --example profile_theme -- pissarro akane dracula` deliberately changes the live theme/wallpapers, measures acknowledgement, preparation completion and status latency, then restores the original selection files. The daemon log also records worker preparation time, cache-hit application latency and in-place theme changes.

Do not run the ignored tests concurrently: they share the current user's daemon and desktop. From WSL, compile with `cargo xwin test --target x86_64-pc-windows-msvc --no-run`, copy the reported executables to Windows and run them there.

`ipc_desktop_smoke` creates disposable native windows and checks discovery, directional moves, floating and fullscreen geometry, workspace membership and visibility, launcher toggling, theme changes and reload behaviour through IPC. `crash_restores_hidden_windows` and `replacement_crash_restores_explorer` kill a daemon and check that the watchdog restores hidden windows and Explorer. `desktop_smoke` asserts real foreground changes and application launches and needs an unlocked, interactive desktop.

## Theme and wallpaper picker checks

Linux tests cover preview installation/discovery, filtering and labels, circular
navigation, rectangular hit areas, geometry, antialiased raster masks, DPI,
bounded caches and cancellation of superseded worker results.

On Windows, the regular (non-ignored) library test
`headless_picker_renders_filters_and_never_applies_while_browsing` uses the actual
picker and Slint software renderer with **no native window**. It creates its own
temporary themes, drives the FocusScope callbacks, checks scrim alpha, cached
image reuse, filtering, confirmation, unreadable assets and stale input, and
asserts that browsing never writes the selected theme or wallpaper preferences.
The same test also exercises wallpaper mode: active filename selection, original labels, filtering, deferred confirmation, directory deletion, empty catalogs, cancellation on theme changes and reopening in theme mode. It is safe while your normal Winarchy session is running.

Optional PNG snapshots of that test (not theme assets):

```powershell
$env:WINARCHY_PICKER_RENDER_DIR = "$env:TEMP\winarchy-picker-renders"
cargo test -p winarchy --lib headless_picker -- --test-threads=1
Remove-Item Env:WINARCHY_PICKER_RENDER_DIR
```

Snapshots cover the center/previous card, one/multiple/no matches, long labels,
light/dark palettes and 100/125/150/200% DPI. The pinned visual contract and
comparison procedure are in [theme-picker-reference.md](theme-picker-reference.md).
The intermediate image crop is 1536×864 like Omarchy; the memory cache retains
lossless pixels rather than recompressing JPEG thumbnails. Qt and Slint also
have different font/edge rasterizers: do not infer pixel-identical output merely
from passing logic or headless tests.

The interactive test below is **not** safe to run casually: it types into the
foreground window, changes focus and briefly applies a fixture theme. Use an
unlocked desktop and a running upgraded daemon with a disposable configuration,
as for the other desktop tests. It checks native focus/restoration, Escape's
two stages, no application on browse, asset disappearance/reappearance through
the watcher, and Enter confirmation; original files are restored on completion.

```powershell
cargo test --test theme_picker -- --ignored --exact picker_focus_filter_cancel_confirm_and_asset_refresh --nocapture
```

Still verify manually on Windows: DWM compositing over live windows (including
transparent corners), no native frame/Alt+Tab entry, multiple monitors, monitor
removal and DPI changes, no stray key/click delivery, and reference screenshots
with identical assets and fonts. Headless alpha checks cannot establish native
DWM composition or foreground behavior. These interactive checks are not run
against an already-running personal desktop automatically.

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
