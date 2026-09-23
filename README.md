# Winarchy

[![Made with Slint](https://raw.githubusercontent.com/slint-ui/slint/master/logo/MadeWithSlint-logo-whitebg.png)](https://slint.dev)

A minimal, keyboard-first Windows 11 x64 tiling window manager and experimental shell, written in Rust. One process owns the Slint background, per-monitor bars and fuzzy application launcher; a second executable holds the companion applications (file manager, task manager, screenshot). No web runtime, desktop icons, tray or shell-registry replacement.

**Status: V1 implementation under integration validation — not yet a verified V1 release.** See [test results and limitations](docs/testing.md). Do not use as your unattended primary shell yet.

**Build target:** `x86_64-pc-windows-msvc`, from Windows or cross-compiled from WSL with cargo-xwin (see [installation](docs/installation.md)). The executables embed version information and an `asInvoker` manifest and are unsigned.

> **Recovery:** press **Ctrl+Shift+Esc**, choose **Run new task**, and run `explorer.exe`. To end Winarchy normally, run `winarchyctl quit`. A watchdog also makes a best-effort recovery after crashes. Never rely on it as your only recovery mechanism.

## Quick start

The default terminal alias uses [WezTerm](https://wezterm.org/) (`wezterm.exe` on PATH).
An experimental, bundled [native WSL terminal](docs/terminal.md) is also available:
`winarchy-terminal.exe`, with resident fast opening and live Winarchy themes, as is
[hold-to-talk dictation](docs/dictate.md) through `winarchy-dictate.exe`.
The experimental [minimal browser](docs/browser.md), `winarchy-browser.exe`, hosts WebView2
with native ad filtering and a themed `Ctrl+L` address editor (filter-list setup required).
Build on Windows with Rust and Visual Studio's **Desktop development with C++** tools:

```powershell
cargo build --release
.\target\release\winarchy.exe
```

This does **not** stop Explorer. Defaults are installed without overwriting existing files in `%USERPROFILE%\.config\winarchy`. Override that location with `WINARCHY_CONFIG_HOME`.

```powershell
.\target\release\winarchyctl.exe theme set catppuccin-latte
.\target\release\winarchyctl.exe workspace 2
.\target\release\winarchyctl.exe quit
```

## Features

- Automatic enrollment of already-open eligible windows; enrollment on restoration for minimized windows
- Fibonacci tiling, geometrical focus and directional order swaps
- Nine owned workspaces, move-and-follow, occupied/recent navigation
- Floating clients, reversible fullscreen and normal application close requests
- Configured low-level keyboard shortcuts, with **Alt** as the default modifier
- Named-pipe commands with owner-only permissions and readable errors
- Catppuccin Mocha/Latte and wallpaper-driven Dynamic Dark/Light; modular TOML configuration and directory-change reloads
- Start Menu shortcut discovery, fuzzy search, native process launching
- Theme background, bar with workspaces, clock, battery, CPU and memory modules and detail popups
- Applets: icon + Slint popup fed by a PowerShell script or a built-in provider, examples included (weather, wifi network chooser, calendar, volume)
- Companion applications sharing the theme: keyboard-first file manager after yazi, task manager, region screenshot
- Monitor enumeration, display-change handling and physical-pixel layout
- Opt-in Explorer stop/restore session, with no permanent registry changes

## Default shortcuts

| Keys | Action |
|---|---|
| Alt+Enter | Configured terminal (WezTerm by default) |
| Alt+B | Configured browser (Winarchy Browser by default; filter-list setup required) |
| Alt+Space | Launcher |
| Alt+Shift+Space | Menu: Apps, System (hibernate, lock, restart, shut down, stop or start Explorer, quit), Keybindings, Theme and Wallpaper |
| Alt+Shift+? | Keybindings viewer and editor |
| Alt+Tab | [Exposé](docs/usage.md#exposé): every window of every workspace as a card; type to filter, Enter to focus |
| Ctrl+Alt+B | [Keyboard hints for bar applets](docs/usage.md#status-bar-applets) |
| Ctrl+Alt+L | [Lock screen](docs/usage.md#lock-screen) with its own password |
| Ctrl+Alt+Shift+Space | Visual theme picker (requires previews or wallpapers; see [Themes](docs/themes.md)) |
| Ctrl+Alt+Shift+W | Visual wallpaper picker for the active theme |
| Ctrl+Alt+Shift+Y / U | Decrease / increase application background opacity by 5 points (web pages stay opaque) |
| Alt+E | File manager |
| Alt+Shift+Escape | Task manager |
| Alt+H/J/K/L or arrows | Focus left/down/up/right |
| Alt+Shift+H/J/K/L or arrows | Swap tiled windows directionally |
| Alt+U / Alt+P | Reduce / increase tiled window width by 5% of its split (hold to repeat) |
| Alt+I / Alt+O | Reduce / increase tiled window height by 5% of its split (hold to repeat) |
| Alt+1…9 | Workspace |
| Alt+Shift+1…9 | Move to workspace and follow |
| Alt+S / Alt+D | Next occupied / recent workspace |
| Alt+T / Alt+Shift+T | Set tiling / toggle floating |
| Alt+F / Alt+Q | Toggle fullscreen / close normally |
| Alt+Shift+R | Reload configuration |
| Win+Shift+S | Region screenshot to clipboard |

All bindings come from `keybindings.toml`, not hard-coded actions in the keyboard hook.

## Documentation

[Installation](docs/installation.md) · [Usage](docs/usage.md) · [Configuration](docs/configuration.md) · [Applets](docs/applets.md) · [Applications](docs/apps.md) · [Themes](docs/themes.md) · [Architecture](docs/architecture.md) · [Recovery](docs/recovery.md) · [Testing](docs/testing.md)

## Screenshots

Pending capture on an unlocked interactive Windows desktop. No mockup is presented as a screenshot.

## Licensing

Winarchy source is MIT-licensed. Slint is used under its [Royalty-free Desktop, Mobile, and Web Applications License](docs/licenses/slint-royalty-free.md); retain the Slint attribution when distributing this desktop application. Dependencies retain their own licenses.
