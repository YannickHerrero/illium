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

`mode` is optional: `"dark"` or `"light"` switches the Windows color mode for apps and system surfaces when the theme is applied (a per-user registry preference, followed by the usual settings-change broadcast so open applications follow). Use exactly six hexadecimal RGB digits. Background is the desktop and launcher base; surface is the bar; overlay is selection/border; text is normal and occupied-workspace text; subtext is inactive text; accent marks the active workspace, the launcher selection and the focused window border; overlay frames the unfocused windows. Green/yellow/red are available semantic colors. No wallpaper or application theming is performed.

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

Limits: 64 wallpapers per theme, 32 MiB and 32 megapixels per image (maximum dimension 16384), 512 MiB of images per pack, 256 entries per wallpaper directory. Only regular files/directories are used, not symlinks or Windows reparse points. Atomic palette publication requires hard-link support on the configuration volume (NTFS on Windows). Failed installations remove their own staged files, not existing themes. Source folders are never modified.
