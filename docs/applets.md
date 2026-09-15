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

Winarchy ships `weather`, `wifi`, `calendar`, `volume` and `_template`; they
are installed with the other defaults and never overwritten. `calendar` uses
`attach = "clock"`: it has no icon and opens when the clock is clicked;
`volume` attaches to the volume module the same way and takes keyboard focus
so the arrows adjust the level. `wifi` lists the nearby networks: `j`/`k` or
the arrows move, Enter connects (asking for the key of an unknown secured
network, stored as a WPA2 profile), `d` disconnects, `f` forgets the saved
profile and `r` rescans. The key travels only as the provider's argument.

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
| `wifi_traffic` | `false` | Merge native Wi-Fi traffic counters using the script's `connected` and `interface_guid` fields; independent of script cadence |
| `provider` | none | `builtin:clock`, `builtin:system` or `builtin:volume` instead of a process |
| `focusable` | `false` | Let the popup take keyboard focus |
| `attach` | none | Built-in module (`clock`, `battery`, `cpu`, `memory`, `volume`, `window-title`) whose click opens this applet; it then has no icon and is loaded whenever that module is in a section |
| `[settings]` | empty | Passed to the provider as `WINARCHY_APPLET_<KEY>` variables |

### The provider

Any executable works; PowerShell needs nothing installed. It runs in the applet
folder, may take up to 20 seconds, and may print up to 64 KiB. Its first
argument, when present, is the action requested by the view. A non-zero exit
turns the bar label into `!` and the popup shows the error.

Providers run off the UI thread, one at a time per applet, on the configured
interval. Actions arriving during a run are queued in order (up to eight); periodic
refreshes do not start overlapping processes. Results from a previous configuration
load are discarded. They are ordinary commands from your configuration directory: treat
them with the same trust as `apps.toml`.

Built-in providers avoid a process for fast cadences:

- `builtin:clock`: `year`, `month`, `day`, `weekday`, `hour`, `minute`,
  `second`, `month_name`, `weekday_name`, `time`, and `weeks`, a Monday-first
  grid of `{ day, current, today }` cells.
- `builtin:system`: `cpu`, `memory_available_gb`, `memory_total_gb`,
  `memory_load`, `battery` (-1 without one), `plugged`, `processors`.
- `builtin:volume`: `volume` (percent) and `muted` of the default output
  device. It is the only built-in provider that acts on the view's action:
  `set <percent>`, `up`, `down` (5% steps) and `toggle-mute`. Setting a
  level above zero also unmutes.

### Wi-Fi traffic sampling

With `wifi_traffic = true`, the script remains responsible for discovery and actions.
Winarchy reads counters for exactly its connected interface GUID through `GetIfEntry2`
on a background thread, at most once per second with the popup open and once every
30 seconds otherwise. Ethernet/VPN interfaces are rejected. No additional PowerShell
process or network probe is needed for those samples.

Added string fields: `receiving`, `sending`, `downloaded`, `uploaded`, `traffic_period`.
Rates use elapsed monotonic time and decimal byte units; they are `—` until two recent
samples are available. Totals cover the current interface's monitoring period, not
Windows' historical usage or a persistent daily counter. Reloading applets, changing
interfaces, counter resets or unavailable counters restart that period. The counters
include all traffic on that adapter, not just Internet/application payload.

### The view

`view.slint` exports a component, `View` by preference, that inherits `Window`
with `no-frame: true`. Winarchy sets these properties when they exist:

- `busy`: boolean, true while the external provider is running.
- `provider-error`: string, the latest provider/JSON error (empty after success).
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
on a click anywhere else, on the module again, or with Escape. By default it does not take
keyboard focus, so global Alt chords keep working while it is open. Errors in
the manifest, provider or view appear in a plain popup under the icon and in
the daemon log.

## Writing your own

1. Copy `applets/_template` to `applets/<name>` and rename `_template.ps1` to
   `<name>.ps1` (or set `script`).
2. Print the JSON you need from the script; declare the matching `Data` struct
   in `view.slint` and lay it out.
3. Add `<name>` to a section of `bar.toml`. The daemon reloads when a file
   under the configuration home or an applet folder changes; the view is
   compiled when you first open it, so markup errors show in the popup.

The first provider run happens right after loading; the bar shows the icon
alone until data arrives.
