# Architecture

## Boundaries

- `command.rs`: canonical textual grammar and typed commands; JSON reply envelope.
- `config.rs`: subsystem TOML types, first-run defaults, validation, rules, themes.
- `layout.rs`: pure Fibonacci rectangles and deterministic geometric neighbor scoring.
- `model.rs`: ordered client list, nine workspaces, monitor associations and recent history.
- `platform/native.rs`: HWND filtering, enumeration, title/process lookup, batching, focus and process launch.
- `platform/input.rs`: keyboard/mouse hooks and WinEvent hooks on a dedicated Win32 message thread; hidden broadcast window receives display changes.
- `platform/mod.rs`: serialized manager state, command execution and event dispatch.
- `platform/shell.rs`, `ui/shell.slint`: same-process Slint surfaces and fuzzy launcher.
- `platform/ipc.rs`: owner-only local named pipe and CLI transport.
- `platform/session.rs`: window recovery tags and opt-in Explorer lifecycle.
- `platform/dpi.rs`, `status.rs`: physical-coordinate conversion and native status modules.

No async runtime, database, network server or web UI is involved. Windows builds use the `windows` crate and Slint's winit/software renderer. Linux builds run pure logic tests only.

## Event flow

Hook callbacks only capture small events/commands and enqueue them. A 10 ms Slint timer drains a bounded batch on the UI thread; window discovery is event-driven, not repeated desktop enumeration. Directory change notifications are debounced and compare only TOML contents. A separate one-second timer updates battery/volume/time. Display broadcasts trigger monitor re-enumeration.

All regular keyboard bindings and IPC commands reach the same `Manager::execute` implementation. Bar workspace clicks emit the same commands. Launcher application selections share the same native spawn/shortcut helpers; Escape and search are UI-only events.

Slint defers native window creation. Surface positioning waits until valid HWNDs exist. Backgrounds are placed at HWND_BOTTOM, bars at HWND_TOPMOST, and all shell windows become tool windows. Per-monitor physical rectangles come from EnumDisplayMonitors, not Explorer work areas. The daemon excludes its entire process from management.

## State and layout

Each HWND has one workspace and one position in the global ordered vector. Layout filters that order by active workspace and excludes floating/fullscreen clients. Fibonacci alternates horizontal/vertical bisection; once a split is physically impossible, remaining clients stack. Directions use window centers with squared forward distance plus four times squared perpendicular distance, with HWND tie-breaking.

Workspace switches show/hide managed clients. A client carries its floating state and saved fullscreen geometry. Monitor associations are workspace-local; only one global workspace is active. This deliberately is not an independent-workspace-per-monitor system.

## IPC and recovery

`\\.\pipe\winarchy-USERNAME` accepts a UTF-8 command line and returns a newline-terminated JSON `{ok,message}` reply. Commands are capped at 8191 bytes; the CLI times out after ten seconds. The pipe rejects remote clients and has an owner-only DACL. A named local mutex covers the entire daemon lifetime.

A second invocation of the same executable in watchdog mode has no UI. It waits on the daemon process, restores tagged hidden windows, and restarts Explorer if a session marker requests it. This is the only extra process; all visible shell surfaces remain in the daemon.
