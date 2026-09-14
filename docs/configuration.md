# Configuration

All files live under `%USERPROFILE%\.config\winarchy`, or `WINARCHY_CONFIG_HOME`. Missing defaults are installed using create-new semantics: existing files are never overwritten. TOML files are UTF-8 regular files (no symlinks/reparse points), limited to 64 KiB each. Configuration and theme directory snapshots examine at most 256 entries.

Directory notifications trigger a debounced, validated reload. Changes to logs and recovery markers do not cause reloads. Reload is transactional across the configuration set: if any file is invalid, the last valid configuration remains active. Fix the file or run `winarchyctl config reload` to see a readable error. Startup requires valid files.

## winarchy.toml

Only global settings:

```toml
theme = "catppuccin-mocha"
```

## wm.toml

```toml
workspaces = 9
layout = "fibonacci"
gap = 16
outer_gap = 16
focus_follows_mouse = true
square_corners = true
border_width = 2
```

V1 requires nine workspaces and Fibonacci. Gaps are 0–100 logical pixels. Pointer focus focuses eligible clients on pointer entry. In the other direction, focus changed by a directional shortcut or a workspace switch centers the pointer on the newly focused client. Dimensions are scaled to physical monitor coordinates. Gaps are measured between the visible frames of windows: Winarchy compensates for the invisible resize borders Windows adds around top-level windows. `square_corners` asks the Desktop Window Manager not to round the corners of managed windows and shell surfaces; the preference is reset when a window leaves Winarchy. `border_width` is the total visible border around each visible client in logical pixels, including the one-pixel edge Windows draws itself, in the theme's `accent` for the focused one and `overlay` for the others; 0 disables it. Border widths are 0 to 32.

## keybindings.toml

```toml
[keybindings]
"Alt+Space" = "launcher toggle"
"Alt+Shift+Space" = "meta toggle"
"Alt+Enter" = "spawn terminal"
"Ctrl+Alt+R" = "config reload"
"Alt+Shift+3" = "window move-workspace 3 --follow"
```

Modifiers: Alt, Ctrl, Shift, Super. Keys: letters, digits, Space, Enter, arrows, Escape, Tab. Matching uses virtual keys, with exact modifier sets. Remove a binding to release that shortcut to applications. Only configured combinations are consumed; Ctrl+Alt+Delete is not supported. Default bindings are listed in the README and shipped `config/defaults/keybindings.toml`.

## apps.toml

```toml
[apps]
terminal = "wezterm.exe"
browser = "msedge.exe"
editor = '"C:\Program Files\Editor\editor.exe" --new-window'
```

Values are native Windows command lines passed to CreateProcessW, not shell scripts. Quote paths with spaces. TOML single-quoted literal strings avoid backslash escaping. For shell syntax explicitly configure `cmd.exe /c ...` or a PowerShell invocation. PATH is inherited when Winarchy starts.

## bar.toml

```toml
enabled = true
position = "top"
height = 28
left = ["workspaces"]
center = ["window-title"]
right = ["volume", "battery", "clock"]
clock_format = "%A %d %b - %H:%M"
```

Positions: top/bottom. Height: 16–100 logical pixels. Text modules: window-title, volume, battery, clock. Workspaces are supported on the left. Battery disappears when unavailable; audio is read from the default render endpoint. Clock substitutions, in English: `%A` weekday, `%a` short weekday, `%d` day, `%B` month, `%b` short month (Jan, Feb, Mar, Apr, May, June, July, Aug, Sept, Oct, Nov, Dec), `%H`, `%M`, `%S`. A bar is created on every monitor; its reservation is calculated directly, never from Explorer's taskbar work area.

## launcher.toml

```toml
width = 620
max_results = 8
show_descriptions = false
```

Width: 200–2000 logical pixels; results: 1–30. Launcher dimensions are clamped to the active monitor. The V1 index is rebuilt on configuration reload. Each Start Menu root is limited to 8,192 examined entries and 16 nested levels; links and junctions are not followed.

## rules.toml

```toml
[[rules]]
executable = "notepad.exe"
workspace = 3

[[rules]]
class = "#32770"
floating = true

[[rules]]
title = "Picture-in-Picture"
ignore = true
```

Fields are optional. Supplied executable (full path), class and title fields must all match case-insensitive substrings. Empty match fields match all eligible windows. Rules run in file order: ignore wins immediately, floating accumulates, and the last matching workspace assignment wins. Workspace numbers must be 1–9. Rules apply on initial management, not retroactively to existing clients. Child/invisible/cloaked/tool/shell/Winarchy windows are filtered before rules; dialogs and owned windows float automatically.

## themes/*.toml

See [themes](themes.md). Adding a theme requires no rebuild.

## Logs

`winarchy.log` is written inside the configuration directory. Start with `--debug` for command and positioning diagnostics. New window lifecycle messages omit application titles, and launch command arguments are omitted even in debug mode. Historical logs and configuration parse errors can still contain private data; review logs before sharing. Logs currently require manual rotation while the daemon is stopped.
