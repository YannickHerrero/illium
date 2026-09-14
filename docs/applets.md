# Applets

An applet is a small application living in the bar: an icon, an optional
label, and a popup opened by clicking the icon. Applets are folders under the
configuration home, `%USERPROFILE%\.config\winarchy\applets\<name>\`, and are
placed in the bar by name in any section of `bar.toml`:

```toml
left = ["workspaces"]
center = ["clock", "weather"]
right = ["battery", "cpu", "memory", "wifi"]
```

Built-in module names are reserved; any other name must match an applet
folder holding an `applet.toml`, or the configuration is rejected.

Winarchy ships `weather`, `wifi`, `calendar` and `_template`; they are
installed with the other defaults and never overwritten.

## Anatomy

```
applets/weather/
  applet.toml    manifest
  icon.svg       monochrome bar icon, tinted with the theme's subtext color
  view.slint     popup content, compiled when the popup first opens
  weather.ps1    data provider: prints one JSON object on stdout
```

### applet.toml

| Key | Default | Meaning |
|---|---|---|
| `icon` | `icon.svg` | Bar icon, relative to the folder |
| `interval` | `1m` | Provider cadence: `30s`, `10m`, `2h` |
| `label` | none | Bar text; `{field}` and `{a.b}` read the JSON |
| `popup` | `{ width = 360, height = 240 }` | Popup size in logical pixels |
| `script` | `<name>.ps1` | PowerShell script run hidden with `-NoProfile -ExecutionPolicy Bypass` |
| `command` | none | Full command line instead of `script` |
| `provider` | none | `builtin:clock` or `builtin:system` instead of a process |
| `focusable` | `false` | Let the popup take keyboard focus |
| `[settings]` | empty | Passed to the provider as `WINARCHY_APPLET_<KEY>` variables |

### The provider

Any executable works; PowerShell needs nothing installed. It runs in the applet
folder, may take up to 20 seconds, and may print up to 64 KiB. Its first
argument, when present, is the action requested by the view. A non-zero exit
turns the bar label into `!` and the popup shows the error.

Providers run off the UI thread, one at a time per applet, on the configured
interval. They are ordinary commands from your configuration directory: treat
them with the same trust as `apps.toml`.

Built-in providers avoid a process for fast cadences:

- `builtin:clock`: `year`, `month`, `day`, `weekday`, `hour`, `minute`,
  `second`, `month_name`, `weekday_name`, `time`, and `weeks`, a Monday-first
  grid of `{ day, current, today }` cells.
- `builtin:system`: `cpu`, `memory_available_gb`, `memory_total_gb`,
  `memory_load`, `battery` (-1 without one), `plugged`, `processors`.

### The view

`view.slint` exports a component, `View` by preference, that inherits `Window`
with `no-frame: true`. Winarchy sets these properties when they exist:

- `data`: your own `struct`; JSON keys map to fields (`month_name` also matches
  `month-name`). Keys the struct does not declare are ignored; type mismatches
  are reported in the popup.
- `popup-width`, `popup-height`: bind `width`/`height` to them.
- `bg`, `surface`, `overlay`, `fg`, `muted`, `accent`: theme colors.

Declaring `callback action(string)` and calling it re-runs the provider with
the argument, then updates `data`.

Images in the view load relative to the folder: `@image-url("sun.svg")`.
SVG and the usual raster formats are supported at run time; `colorize` tints
monochrome icons with a theme color.

## Popup behaviour

One popup at a time, anchored under the module and kept on screen. It closes
on a click anywhere else or on the module again. By default it does not take
keyboard focus, so global Alt chords keep working while it is open. Errors in
the manifest, provider or view appear in a plain popup under the icon and in
the daemon log.

## Writing your own

1. Copy `applets/_template` to `applets/<name>` and rename `_template.ps1` to
   `<name>.ps1` (or set `script`).
2. Print the JSON you need from the script; declare the matching `Data` struct
   in `view.slint` and lay it out.
3. Add `<name>` to a section of `bar.toml`. The daemon reloads on save; the
   view is compiled when you first open it, so markup errors show in the popup.

The first provider run happens right after loading; the bar shows the icon
alone until data arrives.
