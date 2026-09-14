# Themes

Built-in themes use the official Catppuccin Mocha and Latte palettes. Mocha is the first-run default. Every visible color is supplied by theme properties, including the launcher's input and selection.

```powershell
winarchyctl theme set catppuccin-latte
winarchyctl theme set catppuccin-mocha
```

The command validates the new configuration, refreshes the shell, and writes the selected name to `winarchy.toml`. Failure restores the previous global file.

To add a theme, copy an existing TOML file to `themes/my-theme.toml` and edit its colors:

```toml
name = "My Theme"
background = "#1e1e2e"
surface = "#313244"
overlay = "#45475a"
text = "#cdd6f4"
subtext = "#a6adc8"
accent = "#cba6f7"
green = "#a6e3a1"
yellow = "#f9e2af"
red = "#f38ba8"
```

`mode` is optional: `"dark"` or `"light"` switches the Windows color mode for apps and system surfaces when the theme is applied (a per-user registry preference, followed by the usual settings-change broadcast so open applications follow). Use exactly six hexadecimal RGB digits. Background is the desktop and launcher base; surface is the bar; overlay is selection/border; text is normal and occupied-workspace text; subtext is inactive text; accent marks the active workspace, the launcher selection and the focused window border; overlay frames the unfocused windows. Green/yellow/red are available semantic colors. Application theming is left to external integrations; wallpapers are described below.

Themes may also provide `ansi` and `brights`, each an array of exactly eight `"#rrggbb"` colors in this order: black, red, green, yellow, blue, magenta, cyan, white. Both fields are optional and independent; older themes remain valid. They describe terminal colors for external consumers and do not change Winarchy's shell colors or automatically configure applications. Consumers choose their own fallback when either array is absent.

The built-in themes include the [official Catppuccin terminal palettes](https://github.com/catppuccin/wezterm/tree/main/dist). Existing installed files are not overwritten: upgrade Winarchy before adding these fields to local themes, as older versions reject unknown fields.

Select with `winarchyctl theme set my-theme`. Names cannot contain slashes, backslashes or dots. Editing the active palette triggers the directory watcher.

## External theme packs

Themes do not need to be compiled into Winarchy. A local pack is a folder named with 1–64 lowercase ASCII letters, digits, hyphens or underscores:

```text
my-theme/
  theme.toml        # the palette format above
  wallpapers/      # optional JPEG/PNG images, flat directory
  README.md        # optional description and attribution
  LICENSE          # optional upstream license
  SOURCES.md       # optional source URLs and revisions
```

Install without a running daemon, then select:

```powershell
winarchyctl theme install "C:\Downloads\my-theme"
winarchyctl theme set my-theme
```

The installer validates the palette and decodes the images, installs assets under `themes/my-theme/`, then publishes `themes/my-theme.toml`. Existing palettes or asset directories are never overwritten. No scripts are executed and no network access is performed. Keep external packs outside the Winarchy repository; adding a pack requires no rebuild. A theme can also be installed manually using the same layout.

## Wallpapers

For an installed `my-theme.toml`, place images in `themes/my-theme/wallpapers/`. No manifest entries are needed. The first readable JPEG/PNG in alphabetical order is used on first activation. The menu **Alt+Shift+Space → Wallpaper** lists the active theme's images, marks the current one, and offers **Solid background**. **Ctrl+Alt+Shift+W** cycles through the same theme's images, wrapping around and skipping unreadable files. From a solid background it starts with the first image; without images it does nothing.

```powershell
winarchyctl wallpaper next
winarchyctl wallpaper set "A painting.jpg"
winarchyctl wallpaper clear
```

The last explicit choice is saved per theme in `wallpapers.json`, independently of the palette and window-placement state. Switching away and back or restarting restores that choice, including an explicit solid background. If the chosen image disappears or cannot be decoded, Winarchy tries the other images and ultimately the theme's solid `background`. An unknown `wallpaper set` file is rejected immediately. For an existing file, the command acknowledges the request without waiting for image decoding. If decoding fails, the current image and saved choice are preserved; `winarchyctl status` exposes `wallpaper_pending` and `wallpaper_error` for completion/error tracking.

Images fill Winarchy's own desktop surfaces on every monitor with centered, aspect-preserving cropping. The same image is used on all monitors. The Windows wallpaper preference is not modified, so leaving Winarchy restores the underlying desktop as before. Image additions, edits, removals and saved-selection edits are watched separately from configuration: they do not restart applets or change WezTerm's palette. Decoding and resizing run in one background worker: the previous background remains visible until the latest request is ready. Rapid cycling advances from the pending target and discards obsolete results. A 128 MiB LRU cache keeps prepared images, keyed by file path, size, modification time and monitor resolutions; the next wallpaper is preloaded. Prepared buffers for one request are limited to 256 MiB, separately from the decode limits and currently displayed Slint images. Center-cropping and bilinear resampling use SIMD acceleration when supported by the CPU, with alpha-aware filtering for transparent PNGs. No persistent cache or modified originals are created.

Theme-only changes recolor existing shell and applet views without reindexing applications or restarting providers. The subsequent file-watcher notification is ignored when that exact configuration has already been applied. Windows light/dark registry broadcasts are serialized in a separate latest-value worker, since other applications may respond slowly. `winarchyctl config reload` remains an explicit full reload.

Existing keybinding files are not overwritten by an upgrade. Add this entry under `[keybindings]` if needed:

```toml
"Ctrl+Alt+Shift+W" = "wallpaper next"
```

Limits: 64 wallpapers per theme, 32 MiB and 64 megapixels per image (maximum dimension 16384), 512 MiB of images per pack, 256 entries per wallpaper directory. Only regular files/directories are used, not symlinks or Windows reparse points. Atomic palette publication requires hard-link support on the configuration volume (NTFS on Windows). Failed installations remove their own staged files, not existing themes. Source folders are never modified.
