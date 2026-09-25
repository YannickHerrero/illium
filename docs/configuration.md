# Configuration

All files live under `%USERPROFILE%\.config\illium`, or `ILLIUM_CONFIG_HOME`. Missing defaults are installed using create-new semantics: existing files are never overwritten. TOML files are UTF-8 regular files (no symlinks/reparse points), limited to 64 KiB each. Configuration and theme directory snapshots examine at most 256 entries.

Directory notifications trigger a debounced, validated reload. Changes to logs and recovery markers do not cause reloads. Reload is transactional across the configuration set: if any file is invalid, the last valid configuration remains active. Fix the file or run `illiumctl config reload` to see a readable error. Startup requires valid files.

## illium.toml

Only global settings:

```toml
theme = "catppuccin-mocha"
background_blur = false
```

`background_blur` (optional, default `false`) blurs what lies behind the translucent backgrounds of the native terminal, Files and Tasks, for a frosted-glass look with [`background_opacity`](themes.md). It applies live to open and hidden windows and survives theme switches; `illiumctl blur toggle` flips it. It uses the Windows accent blur, which stays visible on unfocused windows, and needs Windows' "Transparency effects" setting (Settings > Personalization > Colors) to be on. The browser, the status bar and external applications are not blurred.

## wm.toml

```toml
workspaces = 9
layout = "fibonacci"
gap = 16
outer_gap = 16
focus_follows_mouse = true
square_corners = true
border_width = 2
conceal = "park"
```

V1 requires nine workspaces and Fibonacci. Gaps are 0–100 logical pixels. Pointer focus focuses eligible clients on pointer entry. In the other direction, focus changed by a directional shortcut or a workspace switch centers the pointer on the newly focused client. Dimensions are scaled to physical monitor coordinates. Gaps are measured between the visible frames of windows: Illium compensates for the invisible resize borders Windows adds around top-level windows. `square_corners` asks the Desktop Window Manager not to round the corners of managed windows and shell surfaces; the preference is reset when a window leaves Illium. `border_width` is the total visible border around each visible client in logical pixels, including the one-pixel edge Windows draws itself, in the theme's `accent` for the focused one and `overlay` for the others; 0 disables it. Border widths are 0 to 32. `conceal` selects how the clients of inactive workspaces leave the screen: `park` (the default) moves them entirely off screen, to x = -32000 like minimized windows, so the window stays visible to Win32, keeps painting and comes back to its place without repainting, and the [exposé](usage.md#exposé) can still capture it; `hide` uses plain ShowWindow, as a fallback for an application that misbehaves off screen (its exposé card then shows only the icon and title). Either way the recovery helper restores the windows if the daemon dies.

## keybindings.toml

```toml
[keybindings]
"Alt+Space" = "launcher toggle"
"Alt+Shift+Space" = "meta toggle"
"Alt+Tab" = "expose toggle"
"Ctrl+Alt+Shift+Space" = "theme picker"
"Ctrl+Alt+Shift+W" = "wallpaper picker"
"Ctrl+Alt+Shift+Y" = "opacity decrease"
"Ctrl+Alt+Shift+U" = "opacity increase"
"Alt+Enter" = "spawn terminal"
"Ctrl+Alt+R" = "config reload"
"Alt+Shift+3" = "window move-workspace 3 --follow"
```

Modifiers: Alt, Ctrl, Shift, Super. Keys: letters, digits, Space, Enter, arrows, Escape, Tab, F1 to F12, and a single punctuation character such as `?` or `,`, resolved to the physical key that types it on the active keyboard layout. A function key may be bound alone. The `dictate` command is the one hold-to-talk action: the daemon records through [illium-dictate.exe](dictate.md) while its key is down, swallowing the key's repeats and release; over `illiumctl dictate` it toggles. Matching uses virtual keys, with exact modifier sets. Remove a binding to release that shortcut to applications. Only configured combinations are consumed; Ctrl+Alt+Delete is not supported. Default bindings are listed in the README and shipped `config/defaults/keybindings.toml`.

Opacity shortcuts adjust all bundled applications by five percentage points (5–100%),
without changing the installed theme; browser pages remain opaque. See
[shared background opacity](themes.md#adjust-opacity-live). Existing binding files
are preserved on upgrade, so add these two lines if they are missing.

The keybindings editor (Alt+Shift+? or the `Keybindings` menu entry, see [usage](usage.md)) rewrites this file in place: the changed line is replaced and the other lines, comments and order are kept. A chord that the resulting configuration rejects is rolled back and the reason is shown in the editor.

### Resize tiled windows

Add these bindings under `[keybindings]` in existing installations (upgrades do
not overwrite your file), then reload with **Alt+Shift+R**:

```toml
"Alt+U" = "window resize --width -5%"
"Alt+P" = "window resize --width +5%"
"Alt+I" = "window resize --height -5%"
"Alt+O" = "window resize --height +5%"
```

The same commands work over IPC, for example
`illiumctl window resize --width +5%`. One axis and a signed integer percentage
from -100% to +100% (excluding zero) are required. Hold a resize shortcut to
repeat it; other ordinary shortcuts remain one-shot.

Resize changes the nearest Fibonacci split controlling the focused tile's
requested dimension, by percentage points of that split's space excluding its
gap. Sibling tiles share the remaining space; focus, gaps and outer margins are
preserved. A parent split may resize several neighboring tiles. No action is
taken for floating, fullscreen or minimized windows, a single tile, or an axis
without an applicable split (for example, height for two side-by-side tiles).

Split ratios are limited to 10–90%. A step is ignored if it would shrink any
affected tile below 32 logical pixels in either dimension; already smaller tiles
cannot shrink further. These are layout safeguards, not application-specific
minimum window sizes.

Ratios are kept in memory per workspace through focus/workspace changes,
configuration reloads and display changes. They belong to layout positions, so
swapping windows keeps the proportions. Changing tiled membership resets that
workspace to equal splits: opening/closing, moving between workspaces,
minimizing/restoring, or toggling floating/fullscreen. Restarting Illium also
resets ratios. No new layout type or persistent configuration is needed.

Desktop validation: try 2, 3 and 5 tiled windows, each shortcut and a held key;
check that focus stays put and neighbors fill the space. Repeat at different
DPI/resolutions, switch workspaces and reload configuration, then test closing,
minimizing/restoring and toggling floating/fullscreen. Check that unsupported
axes and repeated presses at the size limits do nothing.

## apps.toml

```toml
[apps]
terminal = "wezterm.exe"
browser = "msedge.exe"
editor = '"C:\Program Files\Editor\editor.exe" --new-window'
```

Values are native Windows command lines passed to CreateProcessW, not shell scripts. Quote paths with spaces. TOML single-quoted literal strings avoid backslash escaping. For shell syntax explicitly configure `cmd.exe /c ...` or a PowerShell invocation. PATH is inherited when Illium starts.

## terminal.toml

Preferences for the optional bundled `illium-terminal.exe`: font family/size,
padding, bounded scrollback and WSL distribution. Colors and background opacity
belong to the selected theme, not this file. See [native terminal](terminal.md)
for settings, activation and the resident fast path. Editing terminal preferences
does not restart shell applets.

## bar.toml

```toml
enabled = true
japanese_workspace_numbers = true
position = "top"
height = 28
left = ["workspaces", "space"]
center = ["clock"]
right = ["battery", "cpu", "memory", "wifi"]
clock_format = "%H:%M"
```

`japanese_workspace_numbers = true` displays workspace numbers as Japanese kanji
(`一 二 三 四 五 六 七 八 九`). Set it to `false` for Arabic numerals (`1`–`9`);
omitting it also keeps Arabic numerals. Only the labels change: clicks and shortcuts
still use the same workspace numbers.

Workspace typography can be changed in `bar.toml` without rebuilding or restarting
the daemon; saving the file triggers the existing configuration watcher:

```toml
workspace_font_family = "Yu Gothic UI"
workspace_font_size = 13
workspace_font_weight = 700
```

The family must be installed on Windows and contain the desired characters.
An empty or omitted family automatically selects Yu Gothic UI for kanji and
JetBrains Mono otherwise. Size is in logical pixels (6–48, default 13).
Weight accepts 100–900 (`400` regular, `700` bold) and applies to all workspace
labels. When omitted, kanji and the active workspace remain bold, other numerals
regular. Invalid sizes/weights reject the reload and retain the last valid config.
You can also reload explicitly with `illiumctl config reload`.

Positions: top/bottom. Height: 16–100 logical pixels. Each section lists built-in modules and applet names in display order; an applet name must match a folder under `applets/` with an `applet.toml` (see [applets](applets.md)). Built-in modules: space (name of the current space, hidden while there is only one; a click opens the [space picker](usage.md#spaces)), window-title, volume, battery, clock, cpu (overall load in percent since the previous refresh), memory (available RAM in GB with one decimal), drawer (see [below](#drawer)) and separator, a fine vertical divider in the subtext color that widens the gap between its neighbours, has no module action when clicked and may be listed as many times as wanted. Workspaces are supported on the left, at their listed position: for example, `left = ["winagotchi", "workspaces"]` places an installed `winagotchi` applet before the workspace buttons. The center is centered on the screen regardless of the side groups' widths. battery, cpu and memory show a monochrome icon in the theme's subtext color next to their value. Battery disappears when unavailable; audio is read from the default render endpoint. Clock substitutions, in English: `%A` weekday, `%a` short weekday, `%d` day, `%B` month, `%b` short month (Jan, Feb, Mar, Apr, May, June, July, Aug, Sept, Oct, Nov, Dec), `%H`, `%M`, `%S`. A bar is created on every monitor; its reservation is calculated directly, never from Explorer's taskbar work area. The bar's surface uses the theme's shared `background_opacity`, including live opacity shortcuts, while text, icons and the active workspace indicator stay opaque. Clicking an empty area of the bar toggles its background between that translucent surface color and fully transparent (the wallpaper shows through); a click that closes an open popup does not toggle. The toggle is not persisted across daemon restarts. The bar uses bundled JetBrains Mono,
rounded workspace indicators and subtle hover feedback. A fine border faces the desktop
(bottom edge for a top bar, top edge for a bottom bar) and disappears with the background.
For the airier design, use `height = 38`; existing heights remain supported.

The `clock` entry renders a bold time and a muted date as independent click targets.
Their labels can be customized:

```toml
clock_format = "%H:%M"
clock_date_format = "%a %d %b"
```

`clock_date_format` accepts the same tokens; empty uses `%a %d %b`.
The old default combined `clock_format` is treated as `%H:%M`; other custom formats
are preserved. The date opens the existing calendar (`attach = "clock"`), while
the time opens the timezone viewer (`attach = "time"`). Each has its own keyboard
hint and popup anchor. The reserved `time` module can also be listed alone.

### Drawer

Modules that are useful but not worth permanent space can be folded behind a
chevron, in the spirit of Bartender on macOS:

```toml
right = ["drawer", "battery", "volume"]
drawer = ["cpu", "memory", "wifi"]
```

`drawer` is a reserved module name and may appear once, in any section; the
`drawer` list holds the folded modules (built-in modules or applets, everything
but `workspaces` and `drawer` itself). Both must be present together or absent
together. Collapsed, the drawer is a single chevron pointing left. Expanded, the
folded modules appear to the left of the chevron in their listed order. Since
the right section is anchored to the screen edge, listing `drawer` first there
lets it grow towards the center without moving any other module. It expands, instantly and without delay, while the pointer is over
a bar, while one of its popups is open, and during a keyboard hint session so
every module keeps a hint. Clicking the chevron pins it open until the next
click. Neither state is persisted across daemon restarts: it starts collapsed.

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

Fields are optional. Supplied executable (full path), class and title fields must all match case-insensitive substrings. Empty match fields match all eligible windows. Rules run in file order: ignore wins immediately, floating accumulates, and the last matching workspace assignment wins. Workspace numbers must be 1–9. Rules apply on initial management, not retroactively to existing clients. Child/invisible/cloaked/tool/shell/Illium windows are filtered before rules; dialogs and owned windows float automatically.

## themes/*.toml

See [themes](themes.md). Adding a theme requires no rebuild.

## Logs

`illium.log` is written inside the configuration directory. Start with `--debug` for command and positioning diagnostics. New window lifecycle messages omit application titles, and launch command arguments are omitted even in debug mode. Historical logs and configuration parse errors can still contain private data; review logs before sharing. Logs currently require manual rotation while the daemon is stopped.
