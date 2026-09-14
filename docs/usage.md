# Usage

## Windows already open at startup

Starting Winarchy automatically enrolls eligible, already-visible application windows and tiles them in the current workspace (workspace 1 initially). You do not need to close and reopen your applications. Window rules still apply: ignored windows are untouched, dialogs generally float, and workspace assignments are respected. System/helper windows and applications that cannot be managed with your standard user token remain excluded.

Minimized windows are not forcibly restored at startup. Restoring one enrolls it automatically. Minimizing an already-managed window removes it from the tiling calculation until it is restored, without losing its workspace membership.

Hooks are registered before the initial enumeration, so windows created or restored during startup are queued for management rather than falling between the snapshot and event subscription. The log records the initial enrolled-window count without recording window titles.

## Applications and launcher

Alt+Shift+Space opens a two-level menu in the launcher surface, filtered with the same fuzzy search. `Apps` lists the companion applications of `winarchy-apps.exe` (Files, Tasks, Screenshot), which the launcher also indexes by name. `System` holds Hibernate, Lock, Restart, Shut down and Quit Winarchy; power actions run the system `shutdown.exe`, and hibernation requires it to be enabled on the machine. `Theme` lists the files of `themes/`, the current one marked, and applies the selected one. Enter descends into a submenu, Backspace on an empty query returns to the root, Escape closes. Win+Shift+S runs `winarchy-apps.exe shot` from the daemon's directory (`app shot` in `keybindings.toml`): the screen freezes dimmed, drag a rectangle to copy it to the clipboard as a bitmap, Escape or right click cancels. Alt+E opens the file manager (`app files`) and Alt+Shift+Escape the task manager (`app tasks`); see [apps.md](apps.md) for their keys. Alt+Enter executes the `terminal` alias. Alt+Space toggles the launcher. Type a subsequence of an application's name, use Up/Down, Enter to launch, Escape to dismiss. The index combines `apps.toml` aliases, `.lnk` files under the current-user and common Start Menu Programs directories, and packaged (Store/MSIX) applications from the Applications shell folder such as Microsoft Teams or Windows Terminal. Reload to refresh the index. Shortcuts are launched through ShellExecute, without requiring an Explorer process.

`spawn` takes an alias, not an arbitrary shell expression:

```powershell
winarchyctl spawn terminal
winarchyctl spawn browser
```

## Workspaces

Alt+1…9 switches between nine global Winarchy workspaces. These are unrelated to Windows Virtual Desktops. Inactive clients are hidden rather than minimized. Alt+Shift+number moves the focused client and follows it. Alt+S visits the next occupied workspace; Alt+D toggles the two most recently selected workspaces. Selecting the current workspace does not overwrite history.

```powershell
winarchyctl workspace 4
winarchyctl window move-workspace 2          # do not follow
winarchyctl window move-workspace 2 --follow
winarchyctl workspace next-active
winarchyctl workspace recent
```

The bar lists the occupied workspaces plus the active one and highlights the active one; empty workspaces are not shown. Workspaces have a monitor association; focusing a client on a monitor updates that association. Switching workspaces is global, not independently per monitor.

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
