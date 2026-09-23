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

Built-in module names are reserved (`separator` and `drawer` among them; see
[configuration](configuration.md#bartoml)); any other name must match an applet
folder holding an `applet.toml`, or the configuration is rejected. Applets may
also be listed in the `drawer` list, folded behind a chevron until the bar is
hovered or the chevron clicked (see [drawer](configuration.md#drawer)).

Winarchy ships `weather`, `wifi`, `calendar`, `timezones`, `volume`, `battery` and `_template`; they
are installed with the other defaults and never overwritten. `calendar` uses
`attach = "clock"`: it has no icon and opens when the date is clicked;
`timezones` uses `attach = "time"` and opens from the separate time target. Both
are loaded by the bar's `clock` entry, without changing existing calendar files;
`volume` attaches to the volume module the same way: an audio panel with the
output level and devices, the input level, a live input meter and devices, and
one slider per application playing sound. Clicking a device makes it the
default; the switch in the header mutes the output, as do `m` and the arrows
adjust the level while the popup has focus. `wifi` lists the nearby networks: `j`/`k` or
the arrows move, Enter connects (asking for the key of an unknown secured
network, stored as a WPA2 profile), `d` disconnects, `f` forgets the saved
profile and `r` rescans. The key is passed as the provider's argument (never logged by Winarchy). Profile
creation briefly uses a randomly named, current-user-only temporary directory,
removed in a `finally` block; Windows then stores the saved WLAN profile.
The Wi-Fi provider reads English/French `netsh` output, chooses one interface
(the first connected one, otherwise the first available one), and reports its IP,
gateway, DNS and link speed separately from traffic. “Connected” means associated
with Wi-Fi, not verified Internet access. New secured profiles use WPA2-Personal;
use Windows Settings for WPA3-only, enterprise or specially named managed profiles.

## Anatomy

```
applets/weather/
  applet.toml    manifest
  icon.svg       monochrome bar icon, tinted with the theme's subtext color
  view.slint     popup content, prepared before desktop readiness
  weather.ps1    data provider: prints one JSON object on stdout
```

### applet.toml

| Key | Default | Meaning |
|---|---|---|
| `icon` | `icon.svg` | Bar icon, relative to the folder; `{field}` placeholders let the provider choose the file (a plain name in the folder), so a state can change the icon |
| `sprite` | none | Optional sprite-pack TOML filename; replaces the icon when valid, otherwise falls back to `icon` |
| `interval` | `1m` | Provider cadence: `30s`, `10m`, `2h` |
| `label` | none | Bar text; `{field}` and `{a.b}` read the JSON |
| `popup` | `{ width = 360, height = 240 }` | Popup size in logical pixels |
| `script` | `<name>.ps1` | PowerShell script run hidden with `-NoProfile -ExecutionPolicy Bypass` |
| `command` | none | Full command line instead of `script` |
| `wifi_traffic` | `false` | Merge native Wi-Fi traffic counters using the script's `connected` and `interface_guid` fields; independent of script cadence |
| `provider` | none | `builtin:clock`, `builtin:system`, `builtin:volume` or `builtin:battery` instead of a process |
| `focusable` | `false` | Let the popup take keyboard focus |
| `attach` | none | Built-in module (`clock`, `time`, `battery`, `cpu`, `memory`, `volume`, `window-title`) whose click opens this applet; it then has no icon and is loaded whenever that module is in a section |
| `[settings]` | empty | Passed to the provider as `WINARCHY_APPLET_<KEY>` variables |

### Animated bar icons

Set `sprite = "glitchcat.toml"` in `applet.toml`. Packs are separate data files,
so multiple packs can be installed side by side and selected by changing this
one filename (hot reload). No pet names, state names beyond the `idle` fallback,
or fixed atlas dimensions are built into Winarchy. A picker is not required.
For example, a pack for an 8 × 9 atlas:

```toml
sheet = "glitchcat.png"
frame_width = 192
frame_height = 208
columns = 8
rows = 9
interval_ms = 140
display_height = 28
state = "{pet_state}"
[states]
idle = { row = 0, frames = 6 }
working = { row = 7, frames = 6 }
waiting = { row = 6, frames = 6 }
success = { row = 8, frames = 6 }
error = { row = 5, frames = 8 }
```

Rows are zero-based; each animation starts at column zero. Unknown/missing
provider states use `idle`. Images retain their colors and use nearest-neighbor
sampling. The bar animates locally, independently of provider polling, and keeps
the usual label and popup click behavior. Changing rows resets the frame.
Use PNG for portable raster loading. Pack and sheet must be plain filenames in
the applet folder. Invalid/missing packs or mismatched atlas sizes log a warning
and use the ordinary icon. Frames are 1–512 pixels per dimension, the grid is
1–32 rows/columns, the atlas at most 4096 pixels per dimension, cadence
60–2000 ms, and display height 16–48 logical pixels (clamped to bar height).
Existing static applets require no changes. Older Winarchy builds reject the
new manifest key: upgrade Winarchy before installing animated applets.

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
- `builtin:volume`: `volume` (percent), `muted` and `output_name` of the
  default output, `outputs` and `inputs` as `{ id, name, default }` lists of
  the active devices, `input_volume`, `input_muted`, `input_level` (peak of the
  default input at call time, in percent) and `sessions`, up to 16
  `{ id, name, volume, muted }` entries for the applications using the default
  output. It acts on the view's action:
  `set <percent>`, `up`, `down` (5% steps), `toggle-mute`, `input-set <percent>`,
  `input-toggle-mute`, `output <id>`, `input <id>` (default device for every
  role, through the same undocumented COM interface the third-party switchers
  use) and `session <percent> <id>`. Setting a level above zero also unmutes;
  `refresh` only reads. The bar reads the same endpoint for its speaker icon,
  crossed out while muted.
- `builtin:battery`: `present` (false without a battery), `percent`, `plugged`,
  `state` (`Charging`, `Full`, `Not charging`, `On battery`), `time` (to full or
  left, empty when unknown), `stats`, a `{ label, value }` list of the figures
  the battery reports (design capacity, full charge, health, cycles, power),
  `mode` (`saver`, `balanced`, `performance`, empty when unavailable),
  `saver_active`, `saver_threshold` (-1 when unavailable), `saver_forced`,
  `brightness` (-1 without an adjustable built-in display), `travel`,
  `travel_summary` (empty when travel mode can change nothing) and `error`, the
  failure of the last action. Actions: `mode <saver|balanced|performance>`,
  `brightness <percent>`, `saver <on|off>`, `travel <on|off>` and `refresh`.
  It runs on a worker thread, like `builtin:volume`.

### Battery panel

`battery` attaches to the battery module and uses only generic Windows
interfaces, so it works on any laptop; a control the machine lacks is hidden.

- Charge, state and remaining time come from `GetSystemPowerStatus`; capacities,
  cycles and power from the battery class driver (the source of
  `powercfg /batteryreport`). Batteries reporting relative capacities show no
  Wh figures, and several batteries are summed. "Not charging" while plugged in
  is how firmware charge limits appear; Winarchy does not set such limits,
  which are vendor specific.
- The power mode is the Windows setting of the same name (the power mode
  functions of `powrprof.dll`, undocumented but used by the Settings app). It
  applies to the current power source, and like Windows it is only offered
  while the Balanced plan is active.
- Brightness uses the WMI brightness classes of built-in displays; external
  monitors are not adjusted.
- The battery saver switch keeps it on whenever the machine runs on battery,
  by setting its threshold to 100% in the active plan; switching it off
  restores the previous threshold (20%, the Windows default, if unknown).
- Travel mode turns on Power saver mode and the battery saver and lowers the
  brightness to 40% at most, then restores the previous values when switched
  off. The values to restore are kept in `battery.json` in the configuration
  home, so they survive a daemon restart.

No setting needs administrator rights. The panel refreshes every 5 seconds
while open.

### Timezone viewer

The time popup compares Local, London, Paris and Tokyo across 24 shared hourly
columns. Accent shading marks daytime (08–18), muted cells mark night, the current
hour is highlighted and a vertical accent line indicates the current minute.
Midnight cells show the new date. All colors come from the active Winarchy theme.
The view refreshes on opening and every minute; Windows time zone rules handle
summer/winter time, including skipped or repeated hours.

Edit `[settings].zones` in `applets/timezones/applet.toml` to change cities:
`City|Windows time zone identifier`, separated by semicolons. For example,
`London|GMT Standard Time;Paris|Romance Standard Time;Tokyo|Tokyo Standard Time`.
Adjust the popup height if adding more rows. The new folder is installed alongside
existing applets without overwriting the calendar or bar configuration.

Provider regression checks (including DST and date rollover):
`powershell -NoProfile -ExecutionPolicy Bypass -File tests/timezones-provider.ps1`.

### Wi-Fi panel

The panel uses English labels, status messages and decimal byte units (`KB/s`,
`MB`, `GB`). SSIDs retain their original Unicode characters; diagnostics returned
by Windows itself may follow the Windows display language. System fonts are used
at runtime rather than a bitmap subset of the shell's static text.

The 480×600 logical-pixel panel separates the connected-network header, traffic,
interface details and scrollable saved/other networks. A lock means secured, not
necessarily WPA2. Saved networks that are not visible remain listed as out of range.
The active connection has a filled background; the keyboard selection has an outline.
Mouse actions and the existing keyboard shortcuts are both available. Forgetting a
profile requires confirmation. Passwords stay bound to the chosen SSID across data
refreshes and are cleared on cancellation, submission and popup dismissal.

To update an existing installation, back up its `applets/wifi` directory and copy the
new manifest, provider, view and SVG files together **after** updating the daemon.
Default configuration installation never overwrites existing applet files.

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

- `busy`: boolean, true while an external or audio provider is running.
- `has-data`: optional boolean, true after a snapshot has been applied. Use it
  for an initial skeleton rather than presenting default values as real data.
  Cached data stays available when a later refresh fails.
- `open`: boolean, true while the popup is shown; the volume view runs a
  200 ms `Timer` on it to poll its built-in provider for the input meter.
- `provider-error`: string, the latest provider/JSON error (empty after success).
- `data`: your own `struct`; JSON keys map to fields (`month_name` also matches
  `month-name`). Keys the struct does not declare are ignored; type mismatches
  are reported in the popup.
- `popup-width`, `popup-height`: bind `width`/`height` to them.
- `bg`, `surface`, `overlay`, `fg`, `muted`, `accent`, `green`, `yellow`, `red`: theme colors.

Declaring `callback action(string)` and calling it re-runs the provider with
the argument, then updates `data`. A callback with multiple string arguments sends
one JSON array instead, preserving delimiters, quotes and Unicode without a custom
separator protocol. The Wi-Fi view uses `action(verb, ssid, key)`; its provider also
accepts the older single-string actions for compatibility.

Read-only Wi-Fi parser/action-encoding fixtures can be run on Windows with:
`powershell -NoProfile -ExecutionPolicy Bypass -File tests/wifi-provider.ps1`.

Images in the view load relative to the folder: `@image-url("sun.svg")`.
SVG and the usual raster formats are supported at run time; `colorize` tints
monochrome icons with a theme color.

Optional view callbacks let the applet clean up transient UI state:

- `completed()`: invoked after applying a provider result, including failures.
  The builtin volume provider waits until queued user actions finish before
  invoking it, so optimistic controls reconcile with the latest intent.
- `dismissed()`: invoked before hiding the popup (clear password input here).

Global Escape always closes the entire popup, including any inner dialog. The
`dismissed()` callback handles cleanup. A legacy `cancel() -> bool` callback may
remain in a view for its own controls, but no longer intercepts global Escape.

The opt-in native unit test `wifi_view_renders_and_pins_password_target` compiles and
renders the real Slint view headlessly and exercises connection, password-target
pinning, dismissal and forget confirmation without executing any network actions.
Set `WINARCHY_WIFI_TEST_DIR` to the Windows-visible applet folder and optionally
`WINARCHY_WIFI_RENDER_DIR` to an existing output folder for PPM snapshots, then run
that test with `--ignored`. It includes light/dark, 150% DPI and long-list fixtures.

## Popup behaviour

One popup at a time, anchored under the module and kept on screen. It closes
on a click anywhere else, on the module again, or with Escape. By default it does not take
keyboard focus, so global Alt chords keep working while it is open. Errors in
the manifest, provider or view appear in a plain popup under the icon and in
the daemon log.

## Explicit activation state

`plugins.toml` in the configuration home may contain `disabled = ["weather",
"calendar-agenda"]`. This suppresses both direct bar references and automatic
attachments without editing the package manifests or losing bar placement.
Missing state preserves existing discovery. Invalid state rejects configuration
reload. See the experimental [local plugin manager](plugins.md) for CLI commands,
packages and preservation rules. Disabling an applet does not stop external
services it controls.

## Writing your own

1. Copy `applets/_template` to `applets/<name>` and rename `_template.ps1` to
   `<name>.ps1` (or set `script`).
2. Print the JSON you need from the script; declare the matching `Data` struct
   in `view.slint` and lay it out.
3. Add `<name>` to a section of `bar.toml`. The daemon reloads when a file
   under the configuration home or an applet folder changes; the view is
   prepared during startup and retained across unchanged reloads. New or changed
   views are compiled on their next opening; compilation errors are logged.

The first provider run happens right after loading; the bar shows the icon
alone until data arrives.
