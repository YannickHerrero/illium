# Architecture

## Boundaries

- `crates/winarchy-ipc`: canonical command grammar, JSON envelope, bounded pipe I/O and token/session identity. Shared by daemon and CLI without graphical dependencies.
- `crates/winarchy-theme`: theme file types, validation and lookup of the selected theme, without graphics or Windows dependencies; shared by the daemon and the applications.
- `crates/winarchy-config`: configuration files (`config.rs`: subsystem TOML types, first-run defaults, validation, rules, themes), applet manifests, local plugin packages, bounded file reads, clock formats and keybinding parsing, without the desktop toolkit. The daemon reexports its modules under their former paths.
- `crates/winarchyctl`: standalone CLI depending on `winarchy-ipc`, `winarchy-theme` and `winarchy-config`, never on the daemon or Slint; `scripts/check-cli-dependencies.py` enforces it in CI.
- `crates/winarchy-apps`: one executable, one subcommand per companion application (`shot`, `tasks`, `files`). Each application keeps its state and key handling in a `model.rs` without Win32 calls, tested on Linux, and its Win32 or shell calls in a `win.rs`; `ui/common.slint` holds the shared palette, scrolling column, status line and key help, compiled at build time. The daemon launches it with `app <name>` and manages its windows like any client; `serve` is the resident mode that keeps the file and task manager windows hidden and shows them on request over its own owner-only pipe (`platform/apps.rs` on the daemon side, with a plain spawn as fallback).
- `layout.rs`: pure Fibonacci rectangles and deterministic geometric neighbor scoring.
- `model.rs`: ordered client list, nine workspaces, monitor associations and recent history, and the named spaces. Each client belongs to one space; the current space's workspace fields live on the model and an inactive space keeps its own copy, so a switch swaps them and layout parks everything outside the current space.
- `space_picker.rs`: pure space picker state (rows, selection, name prompt, delete confirmation); `platform/shell/space_picker.rs` and `ui/space-picker.slint` draw it.
- `platform/native.rs`: HWND filtering, enumeration, title/process lookup, batching, focus, off-screen parking and process launch.
- `expose.rs`: pure exposé state, card grid geometry, filtering and navigation. `platform/thumbnails.rs` registers live DWM thumbnails of the windows into the surface (parked windows keep composing, so every workspace is live); `platform/icons.rs` draws shell icons on a worker thread; `platform/shell/expose.rs` and `ui/expose.slint` manage the full-monitor surface, generation-tagged input and the Slint images.
- `platform/input.rs`: keyboard/mouse hooks and WinEvent hooks on a dedicated Win32 message thread; hidden broadcast window receives display changes.
- `platform/mod.rs`: serialized manager state, command execution and event dispatch.
- `platform/shell.rs`, `ui/shell.slint`: same-process Slint surfaces and fuzzy launcher.
- `theme_picker.rs`: pure Omarchy carousel state and geometry; `theme_picker/render.rs` prepares antialiased oblique RGBA cards with tiny-skia. `theme_picker/loader.rs` owns a bounded latest-request worker, separate from wallpaper loading. `platform/shell/theme_picker.rs` and `ui/theme-picker.slint` manage the full-monitor surface, foreground focus, cached Slint images and generation-tagged input. Only confirmation dispatches `Command::Theme`; the CLI has no picker rendering dependency.
- `platform/ipc.rs`: owner-only local named-pipe server and UI-thread command dispatch; reexports the shared client for compatibility.
- `platform/session.rs`: window recovery tags and opt-in Explorer lifecycle.
- `platform/dpi.rs`, `status.rs`: physical-coordinate conversion and native status modules.

No async runtime, database, network server or web UI is involved. Windows builds use the `windows` crate and Slint's winit/software renderer. Linux builds run pure logic tests only.

## Event flow

Hook callbacks only capture small events/commands and try to enqueue them in a 1,024-item bounded queue, without blocking. A 10 ms Slint timer drains a bounded batch on the UI thread. Discovery is event-driven; an overflow schedules reconciliation of the current desktop and configuration. Initial hooks precede startup enumeration. Modifier masks follow ordered key events, with resynchronization outside keyboard callbacks. Directory change notifications are debounced and compare only TOML contents. A separate one-second timer updates battery/volume/time. Display broadcasts trigger monitor re-enumeration.

All regular keyboard bindings and IPC commands reach the same `Manager::execute` implementation. Bar workspace clicks emit the same commands. Launcher application selections share the same native spawn/shortcut helpers; Escape and search are UI-only events.

Slint defers native window creation. Surface positioning waits until valid HWNDs exist. Backgrounds are placed at HWND_BOTTOM, bars at HWND_TOPMOST, and all shell windows become tool windows. Per-monitor physical rectangles come from EnumDisplayMonitors, not Explorer work areas. The daemon excludes its entire process from management.

## State and layout

Each HWND has one workspace and one position in the global ordered vector. Layout filters that order by active workspace and excludes floating/fullscreen/minimized clients. Each client has a per-window generation property scoped to the current session; numeric handle reuse invalidates the old record. The managed list is capped at 512 windows. Fibonacci alternates horizontal/vertical bisection; once a split is physically impossible, remaining clients stack. Directions use window centers with squared forward distance plus four times squared perpendicular distance, with HWND tie-breaking.

Workspace switches park managed clients off screen (left edge at x = -32000, where Windows puts minimized windows; DWM cloaking is refused to other processes), or show/hide them with `conceal = "hide"`: a parked window stays visible for Win32 and keeps painting, so revealing it needs no repaint and the DWM keeps composing it for the exposé while it is off screen. The client remembers its rectangle from before parking. A client carries its floating state and saved fullscreen geometry. Monitor associations are workspace-local; only one global workspace is active. This deliberately is not an independent-workspace-per-monitor system.

## IPC and recovery

`\\.\pipe\winarchy-SID-SESSION` accepts a UTF-8 command line and returns a newline-terminated JSON `{ok,message}` reply, followed by a client acknowledgement byte (`0x06`). Commands are capped at 8191 bytes. Overlapped reads/writes use cancellation deadlines; the client exchange has a 12-second deadline. Unstarted queued commands can be cancelled atomically; already-started operations cannot be rolled back and may report an unknown outcome. The pipe rejects remote clients, has an owner-only DACL and retains its single handle between clients. The client verifies the server's token SID and session and requests identification-level access only. A named local mutex covers the daemon lifetime.

Companion applications are separate processes: file, process and screen-capture code never runs in the daemon, which holds the keyboard hook. A second invocation of the same executable in watchdog mode has no UI. It verifies the daemon's PID plus process creation time, waits on that process, restores windows tagged with the unique session GUID, and restarts Explorer if a GUID-scoped session marker requests it. Its handshake completes before client windows are touched; Explorer is stopped only after the native UI surfaces are ready. Apart from the applications the user opens, this is the only extra process; all visible shell surfaces remain in the daemon.
