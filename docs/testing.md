# Validation status

**Historical baseline below.** The [latest follow-up](corrections.md) records 44 passing Linux tests, 45 passing native Windows tests on `c10d931`, the successful preexisting-window enrollment/restoration test, stronger keyboard assertions that failed, and the later Defender CLI block. Earlier smoke-test success does not override those later failures.

This is an **integration candidate, not a completed/verified V1 release**. The source and release binaries are available, but the interactive definition of done has not all been demonstrated.

The desktop results below describe the pre-audit candidate. The [security audit](security-audit.md) subsequently added source corrections and three framing tests (32 desktop-independent tests now pass). Those Win32 changes are cross-checked, not yet runtime-tested; the earlier desktop results must not be treated as their validation.

## Executed on the development host

Development: WSL2 on Windows 11 x64. Windows target: `x86_64-pc-windows-gnu`, cross-built with MinGW and executed on the Windows host. Host display reported 2560×1600 with 125% effective scaling.

- `cargo fmt --check`: passed.
- `cargo clippy --all-targets -- -D warnings`: passed on Linux and for the Windows target.
- `cargo test`: passed, including 29 non-desktop tests for grammar, configuration, invalid reloads, palettes, rule matching, keyboard chords, Fibonacci geometry/non-overlap/area, navigation, ordering, workspaces and IPC envelopes.
- The 29 tests were also compiled to Windows executables and **run successfully on Windows**, not merely cross-checked.
- Release Windows binaries built successfully. CI also defines an MSVC build/test/package job; that remote workflow has not been executed from this session.

### Real HWND integration: passed

`ipc_desktop_smoke` creates four disposable native application windows and verifies:

- discovery and non-identical actual window rectangles;
- directional order movement changes HWND geometry;
- floating, tiling and reversible floating fullscreen geometry;
- workspace membership, actual visibility changes and move-and-follow;
- recent-workspace transitions;
- directional command selection in the manager state;
- launcher visible state toggling, theme changes;
- valid hot reload, invalid reload retaining the prior configuration, explicit reload errors;
- normal WM_CLOSE destruction of a fixture.

`crash_restores_hidden_windows` starts a disposable daemon, creates and hides fixtures, forcibly terminates the daemon, and verifies the watchdog restores the real HWNDs. It passed.

`replacement_crash_restores_explorer` additionally verifies Explorer is stopped, forcibly terminates the replacement daemon, and asserts both hidden-client restoration and Explorer's taskbar returning. It passed on the release binary. Explorer was left running afterward.

Additional host checks:

- WezTerm processes launched through IPC, including while Explorer was absent. Actual terminal interaction remains unverified.
- Alongside-Explorer session ran without stopping Explorer automatically.
- Controlled `--replace-explorer` session: Explorer was confirmed absent after readiness; `quit` restored Explorer with a new process ID.
- IPC status and theme persistence worked. The native-handle startup failure discovered during testing was fixed by deferring HWND positioning until Slint created the windows.

### Blocked / not verified

After the successful release integration tests, Defender quarantined the Windows-host copy of the release daemon as `Trojan:Win32/Bearfoos.B!ml`. Further launch attempts correctly failed because the file was removed. No exclusion, security disablement or quarantine restoration was attempted. This is a separate unresolved distribution/runtime blocker; see [security validation](security-validation.md). The final terminal-spawn recheck was blocked by that quarantine.

The foreground window on the host was **“Écran de verrouillage par défaut de Windows”** (Windows default lock screen). The interactive `desktop_smoke` test failed at foreground selection: its target HWND was not made foreground. No attempt was made to bypass the lock screen.

Consequently, these remain **unverified**, not passing:

- actual foreground H/J/K/L and arrow focus;
- global shortcut interception and Alt menu suppression;
- typing, Up/Down/Enter/Escape and shortcut launching in the launcher;
- visual inspection of bar/background, clipping, selection and themes;
- keyboard move/follow/close/fullscreen/reload actions;
- 100% and mixed-DPI visual paths, real monitor hotplug;

Do not interpret IPC state assertions as proof that keyboard input or visual rendering is correct. The remaining interactive checks require an unlocked desktop **and resolution of the antivirus detection through proper review**, not a security bypass.

## Reproducing tests

Run pure tests normally. Desktop tests are opt-in and deliberately ignored in CI. **Save your work; these tests rearrange windows.** Use a disposable configuration:

```powershell
$env:WINARCHY_CONFIG_HOME = "$env:USERPROFILE\winarchy-test"
.\target\debug\winarchy.exe
cargo test --test desktop -- --ignored --exact ipc_desktop_smoke --nocapture
# Only on an unlocked desktop:
cargo test --test desktop -- --ignored --exact desktop_smoke --nocapture
.\target\debug\winarchyctl.exe quit
# No existing daemon may be running for this one:
$env:WINARCHY_TEST_DAEMON = (Resolve-Path .\target\debug\winarchy.exe).Path
cargo test --test desktop -- --ignored --exact crash_restores_hidden_windows --nocapture
# Destructive Explorer-session test: close File Explorer windows and save work first.
cargo test --test desktop -- --ignored --exact replacement_crash_restores_explorer --nocapture
```

Do not run all ignored tests concurrently: they share the current user's daemon and desktop. With WSL, compile using `cargo test --target x86_64-pc-windows-gnu --no-run`, copy the reported test executables to Windows, and invoke them there.

## Known scope and implementation constraints

- One globally active workspace; no independent per-monitor workspace switching UI.
- Client minimum-size constraints can prevent exact Fibonacci rectangles at high window counts.
- Minimized clients are excluded from initial enumeration. Minimization/restore behavior needs broader application coverage.
- Rule changes apply to newly managed windows, not retroactively.
- Complete-config transactional reload, rather than independent partial-subsystem commits.
- Start Menu discovery is refreshed during reload, not watched independently.
- Workspaces are a left-bar module; the clock supports `%H`, `%M`, `%S` only.
- No notifications, tray, shell registry changes, custom decorations or compositor.
- IPC serves one client at a time; a misbehaving same-user pipe client can delay other clients. The CLI has a timeout, but the synchronous server is not hardened against same-user denial of service.
- Startup/hot reload scanning runs on the UI thread. Broader responsiveness and long-session memory testing remain to be done.
