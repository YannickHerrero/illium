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
