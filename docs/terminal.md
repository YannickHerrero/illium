# Native WSL terminal (experimental)

`winarchy-terminal.exe` is a separate companion executable in this workspace.
It opens **WSL only**, never Herdr. There are no tabs, splits, plugins or SSH
profiles. Winarchy remains responsible for windows; Herdr can be started
manually inside WSL if desired.

## Try it without replacing WezTerm

Build on Windows with `cargo build --workspace --release --locked`, or use the
packaged executable beside `winarchy.exe`:

```powershell
.\winarchy-terminal.exe              # open a new WSL window, keep a resident
.\winarchy-terminal.exe --serve      # prewarm, no visible window and NO WSL
.\winarchy-terminal.exe --standalone # isolated process, exits with its window
.\winarchy-terminal.exe --status     # also records status in terminal.log
.\winarchy-terminal.exe --quit       # refuses while terminal windows are open
```

Closing a window terminates its own WSL child; it does not shut down the WSL
VM or intentionally terminate unrelated Linux sessions. Exiting the shell
leaves its final output visible until the window is closed. The resident keeps
one empty hidden window and shared graphics/font resources, not a hidden shell.
There is at most one resident per Windows user/session and 16 windows including
the spare. `--standalone` can use a separate `WINARCHY_CONFIG_HOME` for testing.

To enable the fastest path, **upgrade Winarchy too**, then set in `apps.toml`:

```toml
[apps]
terminal = "winarchy-terminal.exe"
```

Winarchy prewarms the bundled executable at startup or when this alias changes.
`Alt+Enter`, `winarchyctl spawn terminal` and launcher entries pointing to the
bundled executable use direct IPC, without launching another process. An
absolute path to the executable beside the daemon also works. Explicit
arguments or another executable path retain normal Windows process launching.
A missing resident falls back to launching the bundled terminal. Quitting
Winarchy stops only an idle resident; existing terminal windows keep working.

The shipped alias and existing user configuration still use WezTerm: this is
an **opt-in candidate**, not an automatic replacement. Restore
`terminal = "wezterm.exe"` to switch back. No WezTerm configuration is changed.

## Preferences and theme

`%USERPROFILE%/.config/winarchy/terminal.toml`, respecting
`WINARCHY_CONFIG_HOME` in the **Windows** process environment:

```toml
font_family = "JetBrainsMono Nerd Font Mono"
font_size = 14.0 # points, 6..72
padding = 4     # logical pixels, 0..64
scrollback = 2000 # lines, 0..100000
distribution = "Debian" # new windows only
```

Use an installed monospace font. Missing preferences use these defaults.
Unknown keys, nonfinite sizes and oversized files (>64 KiB) are rejected.
Font, padding, history and theme edits update existing windows; distribution
changes affect only new WSL children. Editing the file resets a window's zoom
when the configured font changes. There is no settings UI.

Colors come directly from `winarchy.toml` and `themes/<name>.toml`, using
`background`, `text`, `accent` (cursor), `overlay` (selection), `ansi` and
`brights`. Legacy themes without ANSI arrays fall back to Catppuccin Mocha or
Latte according to `mode`. Application truecolor and OSC palette overrides
remain application-controlled.

In the **theme**, not in terminal preferences:

```toml
terminal_background_opacity = 0.85
```

Default `0.85` (85% opacity); valid range `0.0..1.0`. The default background receives alpha;
glyphs, cursor, selection and explicit application background cells stay
opaque. No blur/acrylic. Upgrade all Winarchy binaries before adding this field:
older schema readers reject it. See [themes](themes.md).

OS directory notifications trigger a bounded, debounced background reload.
Invalid edits preserve the last valid settings/palette. At initial startup,
missing/invalid files fall back to readable defaults. Theme loading does not
require a running Winarchy daemon and does not respawn WSL. Wallpaper/preview
edits and log writes do not trigger terminal reloads.

## Input

- **Ctrl+Shift+C / Ctrl+Shift+V**: copy selection / paste.
- **Shift+Insert**: paste. Maximum one input/paste item: 64 KiB UTF-8.
- **Ctrl++ / Ctrl+- / Ctrl+0**: zoom in / out / reset.
- **Shift+PageUp / Shift+PageDown**, mouse wheel: scrollback.
- Drag to select; hold **Shift** to select/scroll when an application owns mouse
  reporting. ANSI legacy and SGR mouse reporting are supported.
- **Ctrl+1..9** sends CSI-u, matching the existing Herdr workaround.
- **Alt+Enter** is left to Winarchy. AltGr/dead-key text goes through Windows
  text composition, not Ctrl+digit shortcut encoding.

Bracketed paste strips ESC so clipboard content cannot close its bracket and
inject escape-key sequences. OSC52 clipboard reads/writes are disabled; only
explicit user copy/paste accesses the Windows clipboard. The terminal does not
advertise full Kitty keyboard support. It handles alternate screen, 256 colors,
truecolor and synchronized updates with a timeout for missing end markers.

## Performance and limits

Rust + Alacritty's VT core + portable-pty/ConPTY + `wsl.exe --distribution ...
--cd ~`. DirectWrite layouts are cached (4096 entries); Direct2D/Direct3D11 and
DirectComposition render the grid. Hardware rendering falls back to WARP if
hardware device creation fails. No Slint/WebView/Lua in this executable.

A themed GPU frame and native window are prepared **before** WSL startup.
PTY creation, parsing, writing, resize and teardown run off the UI thread.
Output is parsed in 16 KiB chunks; input is bounded to 32 items and resize has
one latest-value slot. Frames are scheduled on notifications, not an idle
animation loop. Hidden/minimized windows do not render. The grid is bounded
to 512 columns by 256 rows. More scrollback consumes more RAM.

Diagnostics are in `terminal.log` with one rotated backup, approximately 1 MiB
each. They contain opening times and errors, not terminal output or typed input.
`show_ms` measures the resident's show operation, **not** key-to-photon latency.

Initial measurements on the development Windows machine, WSL already running:

| Probe | Samples | Median | p95 |
|---|---:|---:|---:|
| Existing WezTerm GUI, forced new process | 5 | 1023 ms | 1416 ms |
| Existing `wezterm.exe` Scoop alias, normal invocation | 10 | 1935 ms | 4113 ms |
| Native resident, direct IPC (same protocol as Winarchy) | 20 | **33 ms** | **48 ms** |

These are **window discovery** measurements, not actual presentation or shell
readiness, and were sampled at different times rather than in a randomized
benchmark. The IPC result exercises the same request path, not a simulated
physical hotkey. Native launches through a new client executable were roughly
75–140 ms once warm. First execution of newly built native binaries was about
2.5 s in the initial probes; a resident avoids paying this on every window but
does not remove cold startup. No attribution of that delay to Defender or WSL
has been established by profiling.

After 20 open/close cycles: about **68 MiB private / 67 MiB working set** for the
idle resident, and **0 CPU seconds recorded over a 5-second idle sample**. These
numbers exclude WSL/ConPTY helper processes and GPU memory. They are not a
promise of zero resource use or proof of lower end-to-end input latency.

Remaining validation/limitations:

- Sustained real Herdr/agent output, high-refresh typing latency and multi-monitor
  mixed-DPI use still need a longer interactive trial.
- Full Kitty keyboard negotiation, ligatures/complex cross-cell shaping, IME
  candidate positioning, search, URL launching and double-click word selection
  are not implemented.
- A GPU device loss currently requires closing windows and restarting the
  resident; automatic device recovery is not implemented.
- File-manager "terminal here" currently still starts in Linux `~`; Windows CWD
  translation is not part of this WSL-only initial version.

## Reproduce validation

```powershell
cargo test -p winarchy-terminal
# Hidden GPU test: requires a desktop compositor, never shows/focuses a window.
cargo test -p winarchy-terminal --lib hidden_gpu -- --ignored --nocapture
# Starts one disposable Debian shell, checks actual CSI-u input roundtrip.
cargo test -p winarchy-terminal --lib wsl_preserves -- --ignored --nocapture

# Opens/closes visible test windows: do not interact with them during probes.
.\target\release\winarchy-terminal.exe --serve
cargo run -p winarchy-terminal --release --example measure_open -- 20
.\scripts\measure-terminal.ps1 -Executable .\target\release\winarchy-terminal.exe -WindowClass WinarchyTerminal
.\scripts\test-terminal-desktop.ps1 -Executable .\target\release\winarchy-terminal.exe
```

The desktop script uses a disposable configuration and verifies actual palette
pixels, invalid-opacity retention and unchanged HWND/WSL PID through reloads.
It never writes the personal theme or types into another application.
Workspace Linux tests, Windows native tests and Windows/Linux Clippy are also
part of validation; see [testing](testing.md).

Third-party components retain their upstream licenses: [Alacritty terminal
core](https://github.com/alacritty/alacritty) (Apache-2.0) and
[portable-pty](https://github.com/wezterm/wezterm/tree/main/pty) (MIT). Reusing the
PTY library does not start WezTerm or its multiplexing services.
