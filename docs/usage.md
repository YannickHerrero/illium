# Usage

## Applications and launcher

Alt+Enter executes the `terminal` alias. Alt+Space toggles the launcher. Type a subsequence of an application's name, use Up/Down, Enter to launch, Escape to dismiss. The index combines `apps.toml` aliases and `.lnk` files under the current-user and common Start Menu Programs directories. Reload to refresh the index. Shortcuts are launched through ShellExecute, without requiring an Explorer process.

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

The bar highlights the current workspace and distinguishes occupied from empty workspaces. Workspaces have a monitor association; focusing a client on a monitor updates that association. Switching workspaces is global, not independently per monitor.

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
