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

`background_opacity` is optional (default `0.85`), a finite number from `0.0`
to `1.0`. It controls the native terminal, Files and Tasks window backgrounds
and the status bar's base surface without fading their text or icons. Terminal cursor, selection and explicit application
background cells, and Files/Tasks selection and status surfaces remain opaque.
The browser's native home uses the same value (including its native controls);
**web pages always remain opaque**. The bar's active workspace indicator also
remains opaque; inactive workspaces reveal the same translucent base, without
stacking additional opacity layers. Clicking empty bar space still toggles a
fully transparent base; clicking again restores the shared opacity. Other shell
surfaces and external applications such as WezTerm are unchanged. For example:

```toml
background_opacity = 0.85
```

The old `terminal_background_opacity` spelling remains accepted as an alias,
now with the same shared scope. Do not specify both names. Upgrade all Winarchy
binaries before using the new spelling: older readers reject unknown fields.

### Adjust opacity live

- **Ctrl+Alt+Shift+Y**: decrease by 5 percentage points.
- **Ctrl+Alt+Shift+U**: increase by 5 percentage points.
- CLI equivalents: `winarchyctl opacity decrease` / `winarchyctl opacity increase`.

Interactive adjustments are bounded to **5–100%** and apply to all open, hidden
and subsequently opened companion windows, without restarting sessions or moving
focus. The daemon writes a temporary `background-opacity.state` snapshot, never
the installed theme. A different selected theme, daemon restart or clean shutdown
clears it. Ordinary config reloads keep it. Invalid palette edits retain the last
valid appearance in running applications.

Existing `keybindings.toml` files are not overwritten during upgrades: add the
two bindings from [configuration](configuration.md#keybindingstoml) if missing.

Select with `winarchyctl theme set my-theme`. Names cannot contain slashes, backslashes or dots. Editing the active palette triggers the directory watcher.

## Visual theme picker

Open **Alt+Shift+Space → Theme**, **Ctrl+Alt+Shift+Space**, or:

```powershell
winarchyctl theme picker
```

The picker follows [Omarchy's carousel](theme-picker-reference.md): a large
oblique preview, dimmed side slices, the selected name, and an initially hidden
text filter over the current desktop. Left/Right or Tab/Shift+Tab cycle through
themes; type to filter by a case-insensitive substring (not fuzzy search).
Backspace deletes a character, Ctrl+Backspace a word, and Ctrl+U clears the
filter. Escape clears a nonempty filter, then closes on the next press. Clicking
a side card selects it; clicking the selected card or pressing Enter applies it.
Clicking outside the carousel container cancels. Browsing never changes the
active theme, wallpaper, Windows color mode or saved preferences.

Add your own **static image** at:

```text
themes/
  my-theme.toml
  my-theme/
    preview.png           # alternatively preview.jpg or preview.jpeg
    wallpapers/           # existing wallpapers, unchanged
```

The built-in Catppuccin Mocha and Latte themes include Omarchy's preview images
and respectively four and two wallpapers. Their PNG previews are unchanged;
WebP wallpapers are converted to PNG without resizing or additional pixel loss.
Sources, hashes, conversion details and the upstream license are installed in
each theme's `SOURCES.md` and `LICENSE`. These are Omarchy screenshots, not
representations of Winarchy-specific applications. Asset and branding rights
remain with their respective owners; see the source notes before redistribution.

Missing built-in assets are added when the upgraded daemon starts; existing
files, palettes, keybindings and wallpaper choices are never overwritten. An
explicit solid-background choice remains solid. Without a saved choice, the
usual first-wallpaper behavior applies. Other themes can add previews directly
to their installed configuration directory; no rebuild is needed.
Names are matched case-insensitively, with PNG → JPG → JPEG priority.
Without a dedicated preview, the first alphabetically sorted wallpaper is used
(regardless of the remembered wallpaper choice). Themes without any image, with
an invalid palette, or with an unreadable preview are omitted. If none have an
image, the picker shows only the dimmed desktop: Escape or a click cancels;
`theme set <name>` remains available. No placeholder cards are generated.

Preview images are screenshots supplied by the theme author, **not live renders
of your windows**. Asset additions/edits/removals are picked up automatically,
including for inactive themes. PNG/JPEG decoding, cropping and masking run in a
separate worker, with 64 MiB each for intermediate thumbnails and prepared cards,
and a separate 64 MiB Slint image cache. A prepared view is limited to 256 MiB,
in addition to temporary decode buffers and currently displayed images. Nothing
is downloaded or written to a persistent image cache. Rapid navigation discards
superseded work; an Enter during loading waits for the requested preview.

`winarchyctl status` exposes `theme_picker`, `theme_picker_selected`,
`theme_picker_filter`, `theme_picker_loading` and `theme_picker_error` for
diagnostics. Opening through IPC acknowledges without waiting for image loading.
An application failure is reported in diagnostics, not as extra picker UI.

Existing keybinding files are not overwritten on upgrade. Add under `[keybindings]`:

```toml
"Ctrl+Alt+Shift+Space" = "theme picker"
```

## Demo scene for theme screenshots

On an **empty workspace**, choose **Alt+Shift+Space → Demo**, or run:

```powershell
winarchyctl demo
```

Winarchy opens three dedicated windows using the active theme and opacity:

- **Left half:** native terminal with a fixed, colored fictional Rust build/test log.
- **Top right:** native terminal with a built-in demo fetch, fictional machine data
  and all sixteen ANSI colors (not the user's installed `fetch` command).
- **Bottom right:** browser home with synthetic public bookmarks/history.

The terminal fixtures never start WSL, a shell, or a real build. Keyboard/paste
input cannot execute commands. Their output is static, with no blinking cursor.
The browser runs separately from the normal resident, with a fresh temporary
WebView profile, library and filter directory. It does not load personal
bookmarks, history, cookies or sessions; the demo home does not navigate to the
listed sites automatically. Following a link still performs a real navigation
inside the isolated profile. Normal browser windows/data remain unchanged.

Switch themes while the scene stays open to take several previews. The existing
live theme/opacity watchers remain active. Close the three windows normally when
finished; the browser attempts to remove its temporary data on exit. Forced
termination/crashes can leave temporary files under the system temp directory.

`demo` acknowledges startup, not completion. `winarchyctl status` exposes
`demo_pending` and `demo_error`; wait until pending is false and error is null
before capturing. The windows are identified by their owned processes and ready
markers, not titles or fixed sleep delays. The final scene uses 50/50 splits and
focuses the left terminal. Existing gaps/borders and the workspace's monitor are
preserved. Startup overrides placement rules for these three windows only.

Occupied workspaces (including minimized windows) and concurrent startups are
rejected. Changing workspace or adding an unrelated window during startup cancels
it. Missing/failed applications or a 30-second readiness timeout close only the
new demo processes, leaving existing windows alone. Install matching daemon,
CLI, terminal and browser binaries before using this command.

**Screenshot privacy is limited to these demo windows:** the bar, notifications,
other monitors, wallpaper and custom theme/font names may still reveal personal
information. Check the capture before publishing it. No automatic screenshot or
preview-file replacement is performed.

## External theme packs

Themes do not need to be compiled into Winarchy. A local pack is a folder named with 1–64 lowercase ASCII letters, digits, hyphens or underscores:

```text
my-theme/
  theme.toml        # the palette format above
  preview.png       # optional screenshot (PNG/JPG/JPEG), see picker above
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

For an installed `my-theme.toml`, place images in `themes/my-theme/wallpapers/`. No manifest entries are needed. The first readable JPEG/PNG in alphabetical order is used on first activation. The menu **Alt+Shift+Space → Wallpaper** opens the same visual carousel as the theme picker, starting on the current (or pending) image. Left/Right or Tab/Shift+Tab browse, typing filters filenames, and Enter or clicking the selected card applies that exact image through the existing wallpaper loader. Escape clears the filter, then cancels; clicking outside cancels. Browsing never changes the desktop or saved choices. **Alt+Shift+Space → Solid background** retains the explicit solid-color choice without adding an artificial image card. Without readable wallpapers, the carousel stays empty and can be cancelled. If the active theme changes while browsing wallpapers, the picker cancels rather than applying an old filename to the new theme. **Ctrl+Alt+Shift+W** opens the wallpaper picker directly. The `winarchyctl wallpaper next` command still cycles through the same theme's images, wrapping around and skipping unreadable files. From a solid background it starts with the first image; without images it does nothing.

```powershell
winarchyctl wallpaper picker
winarchyctl wallpaper next
winarchyctl wallpaper set "A painting.jpg"
winarchyctl wallpaper clear
```

The wallpaper carousel shares the theme picker's surface, geometry, preview worker and bounded caches; it does not share or cancel the real wallpaper worker. Directory edits refresh its cards. Labels display original filenames. `status` exposes `wallpaper_picker`, `wallpaper_picker_theme`, `wallpaper_picker_selected`, `wallpaper_picker_filter`, `wallpaper_picker_loading` and `wallpaper_picker_error` separately from theme-picker diagnostics. An older running daemon must be upgraded/restarted before it recognizes `wallpaper picker`.

The last explicit choice is saved per theme in `wallpapers.json`, independently of the palette and window-placement state. Switching away and back or restarting restores that choice, including an explicit solid background. If the chosen image disappears or cannot be decoded, Winarchy tries the other images and ultimately the theme's solid `background`. An unknown `wallpaper set` file is rejected immediately. For an existing file, the command acknowledges the request without waiting for image decoding. If decoding fails, the current image and saved choice are preserved; `winarchyctl status` exposes `wallpaper_pending` and `wallpaper_error` for completion/error tracking.

Images fill Winarchy's own desktop surfaces on every monitor with centered, aspect-preserving cropping. The same image is used on all monitors. The Windows wallpaper preference is not modified, so leaving Winarchy restores the underlying desktop as before. Image additions, edits, removals and saved-selection edits are watched separately from configuration: they do not restart applets or change WezTerm's palette. Decoding and resizing run in one background worker: the previous background remains visible until the latest request is ready. Rapid cycling advances from the pending target and discards obsolete results. A 128 MiB LRU cache keeps prepared images, keyed by file path, size, modification time and monitor resolutions; the next wallpaper is preloaded. Prepared buffers for one request are limited to 256 MiB, separately from the decode limits and currently displayed Slint images. Center-cropping and bilinear resampling use SIMD acceleration when supported by the CPU, with alpha-aware filtering for transparent PNGs. No persistent cache or modified originals are created.

Theme-only changes recolor existing shell and applet views without reindexing applications or restarting providers. The subsequent file-watcher notification is ignored when that exact configuration has already been applied. Windows light/dark registry broadcasts are serialized in a separate latest-value worker, since other applications may respond slowly. `winarchyctl config reload` remains an explicit full reload.

Existing keybinding files are not overwritten by an upgrade. Add or replace this entry under `[keybindings]` (older defaults used `wallpaper next`), then reload with **Alt+Shift+R** or `winarchyctl config reload`:

```toml
"Ctrl+Alt+Shift+W" = "wallpaper picker"
```

Limits: 64 wallpapers per theme, 32 MiB and 64 megapixels per image (maximum dimension 16384), 512 MiB of images per pack (including previews), 256 entries per wallpaper directory. Preview scans allow at most 1,024 directory entries and 256 theme identifiers; the existing configuration snapshot limits still apply. Only regular files/directories are used, not symlinks or Windows reparse points. Atomic palette publication requires hard-link support on the configuration volume (NTFS on Windows). Failed installations remove their own staged files, not existing themes. Source folders are never modified.
