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

Use exactly six hexadecimal RGB digits. Background is the desktop and launcher base; surface is the bar; overlay is selection/border; text is normal and occupied-workspace text; subtext is inactive text; accent marks the active workspace and launcher selection. Green/yellow/red are available semantic colors. No wallpaper or application theming is performed.

Select with `winarchyctl theme set my-theme`. Names cannot contain slashes, backslashes or dots. Editing the active palette triggers the directory watcher.
