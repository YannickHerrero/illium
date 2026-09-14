# Architecture

## Boundaries

- `crates/winarchy-ipc`: canonical command grammar, JSON envelope, bounded pipe I/O and token/session identity. Shared by daemon and CLI without graphical dependencies.
- `crates/winarchy-theme`: theme file types, validation and lookup of the selected theme, without graphics or Windows dependencies; shared by the daemon and the applications.
- `crates/winarchyctl`: standalone CLI depending only on `winarchy-ipc`, not on the daemon or Slint.
- `crates/winarchy-apps`: one executable, one subcommand per companion application (`shot`, `tasks`, `files`). Each application keeps its state and key handling in a `model.rs` without Win32 calls, tested on Linux, and its Win32 or shell calls in a `win.rs`; `ui/common.slint` holds the shared palette, scrolling column, status line and key help, compiled at build time. The daemon launches it with `app <name>` and manages its windows like any client; `serve` is the resident mode that keeps the file and task manager windows hidden and shows them on request over its own owner-only pipe (`platform/apps.rs` on the daemon side, with a plain spawn as fallback).
- `config.rs`: subsystem TOML types, first-run defaults, validation, rules, themes.
- `layout.rs`: pure Fibonacci rectangles and deterministic geometric neighbor scoring.
- `model.rs`: ordered client list, nine workspaces, monitor associations and recent history.
- `platform/native.rs`: HWND filtering, enumeration, title/process lookup, batching, focus and process launch.
- `platform/input.rs`: keyboard/mouse hooks and WinEvent hooks on a dedicated Win32 message thread; hidden broadcast window receives display changes.
- `platform/mod.rs`: serialized manager state, command execution and event dispatch.
- `platform/shell.rs`, `ui/shell.slint`: same-process Slint surfaces and fuzzy launcher.
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

Workspace switches show/hide managed clients. A client carries its floating state and saved fullscreen geometry. Monitor associations are workspace-local; only one global workspace is active. This deliberately is not an independent-workspace-per-monitor system.

## IPC and recovery

`\\.\pipe\winarchy-SID-SESSION` accepts a UTF-8 command line and returns a newline-terminated JSON `{ok,message}` reply, followed by a client acknowledgement byte (`0x06`). Commands are capped at 8191 bytes. Overlapped reads/writes use cancellation deadlines; the client exchange has a 12-second deadline. Unstarted queued commands can be cancelled atomically; already-started operations cannot be rolled back and may report an unknown outcome. The pipe rejects remote clients, has an owner-only DACL and retains its single handle between clients. The client verifies the server's token SID and session and requests identification-level access only. A named local mutex covers the daemon lifetime.

Companion applications are separate processes: file, process and screen-capture code never runs in the daemon, which holds the keyboard hook. A second invocation of the same executable in watchdog mode has no UI. It verifies the daemon's PID plus process creation time, waits on that process, restores windows tagged with the unique session GUID, and restarts Explorer if a GUID-scoped session marker requests it. Its handshake completes before client windows are touched; Explorer is stopped only after the native UI surfaces are ready. Apart from the applications the user opens, this is the only extra process; all visible shell surfaces remain in the daemon.
