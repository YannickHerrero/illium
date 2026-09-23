# Usage

## Windows already open at startup

Starting Winarchy automatically enrolls eligible, already-visible application windows and tiles them in the current workspace (workspace 1 initially). You do not need to close and reopen your applications. Window rules still apply: ignored windows are untouched, dialogs generally float, and workspace assignments are respected. System/helper windows and applications that cannot be managed with your standard user token remain excluded.

Minimized windows are not forcibly restored at startup. Restoring one enrolls it automatically. Minimizing an already-managed window removes it from the tiling calculation until it is restored, without losing its workspace membership.

Hooks are registered before the initial enumeration, so windows created or restored during startup are queued for management rather than falling between the snapshot and event subscription. The log records the initial enrolled-window count without recording window titles.

## Applications and launcher

Alt+Shift+Space opens a two-level menu in the launcher surface, filtered with the same fuzzy search. `Apps` lists the companion applications of `winarchy-apps.exe` (Files, Tasks, Screenshot), which the launcher also indexes by name. `System` holds Hibernate, Lock, Restart, Shut down, Stop Explorer or Start Explorer (whichever applies to the current shell state, see [recovery](recovery.md) before stopping it) and Quit Winarchy; power actions run the system `shutdown.exe`, and hibernation requires it to be enabled on the machine. `Appearance` groups the theme settings. Its `Theme` entry opens the visual theme carousel (also Ctrl+Alt+Shift+Space or `winarchyctl theme picker`): Left/Right or Tab/Shift+Tab browse, typing filters, Enter or a click on the selected preview applies, and Escape clears the filter before closing. Browsing does not apply a theme. Add preview images as described in [themes](themes.md). `Wallpaper` opens the same carousel for the active theme's images (`winarchyctl wallpaper picker`), with filename filtering and explicit confirmation; browsing never changes the wallpaper. `Solid background` switches to the theme's plain color. `Increase opacity` and `Decrease opacity` show the current value and step it by five points, `Reset opacity` returns to the theme's value, and `Enable blur` / `Disable blur` flips [`background_blur`](configuration.md#winarchytoml); these four leave the menu open so they can be repeated. `Keybindings` (also Alt+Shift+? or `winarchyctl keybindings toggle`) lists every binding of `keybindings.toml` in the shipped order with a description; typing filters by description or chord, Up/Down select, Enter or the `Change` button opens a capture dialog. Press the new combination (at least one modifier), then Enter to apply; Backspace clears, Escape cancels. While the dialog is open no key reaches the desktop, so any chord can be recorded, configured ones included. A combination already bound to another action shows a warning: Enter replaces it and leaves the other action unbound, Escape keeps things as they are. A binding whose chord differs from the shipped default is marked `changed` and gets a `Reset` button; an unbound default stays listed as `Unbound` so it can be restored. Ctrl+Alt+Shift+W opens the wallpaper picker directly; `winarchyctl wallpaper next` remains available for cycling. Wallpaper choices are remembered per theme (see [themes](themes.md)). Enter descends into a submenu, Backspace on an empty query returns to the root, Escape closes. Win+Shift+S runs `winarchy-apps.exe shot` from the daemon's directory (`app shot` in `keybindings.toml`): the screen freezes dimmed, drag a rectangle to copy it to the clipboard as a bitmap, Escape or right click cancels. Alt+E opens the file manager (`app files`) and Alt+Shift+Escape the task manager (`app tasks`); see [apps.md](apps.md) for their keys. Alt+Enter executes the `terminal` alias. Alt+Space toggles the launcher. Type a subsequence of an application's name, use Up/Down, Enter to launch, Escape to dismiss. The index combines `apps.toml` aliases, `.lnk` files under the current-user and common Start Menu Programs directories, and packaged (Store/MSIX) applications from the Applications shell folder such as Microsoft Teams or Windows Terminal. Reload to refresh the index. Shortcuts are launched through ShellExecute, without requiring an Explorer process.

`spawn` takes an alias, not an arbitrary shell expression:

```powershell
winarchyctl spawn terminal
winarchyctl spawn browser
```

## Workspaces

Alt+1…9 switches between nine global Winarchy workspaces. These are unrelated to Windows Virtual Desktops. Inactive clients are parked off screen (or hidden, see `conceal` in [configuration](configuration.md#wmtoml)) rather than minimized. Alt+Shift+number moves the focused client and follows it. Alt+S visits the next occupied workspace; Alt+D toggles the two most recently selected workspaces. Selecting the current workspace does not overwrite history.

```powershell
winarchyctl workspace 4
winarchyctl window move-workspace 2          # do not follow
winarchyctl window move-workspace 2 --follow
winarchyctl workspace next-active
winarchyctl workspace recent
```

The bar lists the occupied workspaces plus the active one and highlights the active one; empty workspaces are not shown. Workspaces have a monitor association; focusing a client on a monitor updates that association. Switching workspaces is global, not independently per monitor.

## Exposé

**Alt+Tab** (or `winarchyctl expose toggle`) shows every managed window of every workspace as a card over the blurred wallpaper of the active monitor, grouped by workspace in tiling order, with a workspace badge, the application's icon and the window title. Each card is a live preview composed by the Desktop Window Manager: video keeps playing and terminals keep scrolling, on every workspace, because inactive windows are parked off screen rather than hidden. Minimized windows show their last frame dimmed; with `conceal = "hide"` the windows of inactive workspaces have no surface and their cards show icon and title only. The focused window is selected on opening and outlined in the accent color.

Typing filters by title or application name with the launcher's subsequence matching; Backspace edits, Ctrl+U clears. **Left/Right** (or Alt+Tab / Alt+Shift+Tab while the exposé is open), **Up/Down** and hovering move the selection. **Enter** or a click switches to the window's workspace if needed and focuses it. A middle click or the configured `window close` chord (Alt+Q by default) closes the selected window and removes its card; a window closed by other means disappears too. **Escape** clears the filter first, then closes the exposé and restores the previous focus. Any other Winarchy command (a workspace switch, the launcher) closes it as well.

Existing keybinding files are not overwritten on upgrade. Add this line under `[keybindings]`, then reload with Alt+Shift+R:

```toml
"Alt+Tab" = "expose toggle"
```

## Status bar applets

**Ctrl+Alt+B** (or `winarchyctl bar hints`) toggles keyboard hints on the active
workspace's monitor. Release Ctrl/Alt, then press a displayed **1–9** or **A–Z**
to open that applet. **Left/Right** cycle the highlighted hint; **Enter** opens it.
The top number row works without Shift on AZERTY (`&`, `é`, etc.); numpad 1–9
works with Num Lock. Letters do not require Shift.

Badges float below a top bar, or above a bottom bar, in the current theme.
They label actionable modules from left to right (up to 35); separators,
unattached window titles and workspace buttons do not receive hints. Workspaces
retain Alt+1…9. Labels and bar geometry stay frozen during selection, even when
providers refresh. There is no timeout. Invalid keys are consumed, not typed into
the previous application.

Selecting hides the hints, opens the same popup as a click, and gives it keyboard
focus. Existing applet controls remain available (for example arrows for volume,
and Up/Down then Enter for Wi-Fi). **Escape** closes it and restores the previous
window in one press, even if the applet has an inner dialog open. While hints are shown,
Escape or the toggle shortcut cancels. Clicking outside, reconfiguring the bar,
or changing displays also cancels selection. Custom applets still need to provide
their own keyboard controls.

Existing keybinding files are not overwritten on upgrade. Add this line under
`[keybindings]`, then reload with Alt+Shift+R:

```toml
"Ctrl+Alt+B" = "bar hints"
```

## Lock screen

**Ctrl+Alt+L** (or `winarchyctl lock`) covers every monitor with the blurred wallpaper, a clock and, on the active monitor, a password field. The password is Winarchy's own, not the Windows one. Set it once, from a console:

```
winarchyctl lock set-password
```

Only its Argon2 hash is stored, in `lock-password` in the configuration folder; it is read at each lock, so no reload is needed. Without it, Ctrl+Alt+L locks Windows instead.

While locked, no Winarchy binding or command runs (except `status`), and the Windows key, Alt+Tab, Alt+Esc, Ctrl+Esc and other Ctrl or Alt chords are swallowed. Ctrl+Alt still types AltGr characters. Win+L keeps the regular Windows lock.

This screen is a window over the open session, not a Windows security boundary: Ctrl+Alt+Del cannot be intercepted. Winarchy therefore calls the Windows lock as soon as it may be bypassed: another application takes the foreground (for example Task Manager opened from Ctrl+Alt+Del), five wrong passwords, or the daemon exits while locked (the recovery watchdog locks Windows). For a long absence, Win+L remains the safer choice.

Existing keybinding files are not overwritten on upgrade. Add this line under `[keybindings]`, then reload with Alt+Shift+R:

```toml
"Ctrl+Alt+L" = "lock"
```

## Windows

The first tiled client gets the left half; subsequent clients split the remainder alternately horizontally and vertically. Gaps are configurable. Very small remaining rectangles stack rather than producing negative dimensions; applications can still enforce their own minimum size.

Alt+H/J/K/L and arrows choose the geometrically nearest neighbor in that direction, penalizing perpendicular distance. Shift swaps tiled order with that neighbor. Floating clients are not layout participants.

- Alt+T: return to tiling.
- Alt+Shift+T: toggle floating; first float is centered at two-thirds of the work area.
- Alt+F: toggle fullscreen within the monitor's bar-excluded usable area; restore prior floating geometry on exit.
- Alt+Q: post WM_CLOSE; save dialogs are the application's responsibility.

```powershell
winarchyctl window focus left
winarchyctl window move down
winarchyctl window set-tiling
winarchyctl window toggle-float
winarchyctl window toggle-fullscreen
winarchyctl window close
winarchyctl launcher toggle
winarchyctl config reload
winarchyctl status
winarchyctl quit
```

`status` returns JSON for diagnostics, including actual client rectangles. CLI exit codes: 0 success, 1 daemon/operation/transport failure, 2 invalid command syntax. Elevated applications and secure Windows desktops cannot reliably be controlled by a non-elevated WM. Do not run Winarchy elevated merely to work around this boundary.
