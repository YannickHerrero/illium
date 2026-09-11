# Recovery — read before stopping Explorer

**Ctrl+Shift+Esc → Task Manager → Run new task → `explorer.exe`.**

This is the primary escape hatch. It does not depend on Winarchy's keyboard hook, bar, launcher or IPC. Winarchy never changes the permanent Windows shell registry.

## Normal exit

Run `winarchyctl quit`. Managed windows are shown, fullscreen floating geometry is restored, and Explorer is restarted if Winarchy stopped it. Existing tiled applications retain their most recent tiled sizes.

## Crash or frozen shell

1. Open Task Manager with Ctrl+Shift+Esc.
2. End the Winarchy daemon if it remains running. A recovery helper with `--watch-session` may also appear under the same executable name; it waits for the daemon.
3. Use Run new task to run `explorer.exe`.
4. The helper restores hidden windows tagged with that daemon's unique session identity. If both processes were killed, relaunch the affected applications or sign out after saving recoverable work. Do not assume restarting Explorer alone unhides WM-hidden application windows.

The helper starts for both ordinary and replacement sessions. It is not a service and renders no UI. Explorer-restoration intent is recorded before `taskkill` runs. Recovery remains best effort: process termination, machine shutdown, application privilege boundaries or policy restrictions can defeat it.

## Keyboard shortcuts stop working

Try `winarchyctl status`, then `winarchyctl config reload`. Inspect `keybindings.toml` and `winarchy.log`. Windows may remove a low-level hook whose callback stalls; restart the daemon if reloading does not help. No WM shortcut is expected to work on the lock screen, UAC secure desktop or Ctrl+Alt+Delete screen.

## Bar or background disappears

Try `winarchyctl config reload`. Check `bar.toml` (`enabled = true`) and the selected theme file. If IPC also fails, use Task Manager to stop and restart Winarchy. Display changes should recreate surfaces, but mixed-DPI/hotplug paths still need broader hardware validation.

## Invalid configuration

During a session the last valid configuration is retained and the log records the parse error. On startup, invalid configuration prevents launch. Rename the invalid file, then relaunch to install that missing default. To reset everything without deleting personal settings:

```powershell
Rename-Item "$env:USERPROFILE\.config\winarchy" "winarchy-backup"
winarchy.exe
```

Adapt the path if `WINARCHY_CONFIG_HOME` is set. Never delete your only backup.

## Explorer stop/start

`winarchyctl explorer stop` is a session-scoped experiment; `winarchyctl explorer start` restores it. Stopping Explorer terminates its File Explorer windows. Prefer `--replace-explorer`, which checks that Winarchy surfaces and the watchdog have initialized first.
