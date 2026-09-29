# Lock screen: Windows validation

Use a disposable Windows session with an Illium password configured. These
checks require a real desktop; cross-compilation does not validate focus or
Windows session transitions.

## Opening and foreground

- Open the lock using Ctrl+Alt+L, `illiumctl lock`, and the system menu's
  **Lock** entry. All three must show Illium, not immediately lock Windows.
- Repeat with the launcher, an applet, and a picker open. Closing those surfaces
  must not cause a spurious handover.
- Repeat on multiple monitors and after changing display scale or connecting a
  monitor. Verify every display is covered and the password field has focus.
- Queue a foreground notification for an application, then return focus to
  Illium before it is processed. The obsolete event must not trigger handover.
- If initial focus acquisition fails and an external application remains in
  front after placement, Windows must still be locked. Opening is not an
  unlimited exemption from the foreground check.
- Open Task Manager through Ctrl+Alt+Del while Illium is locked. If Task Manager
  takes the foreground, Illium must hand over to Windows.

## Windows handover

- Enter five wrong passwords. Illium must request the Windows lock and retain
  its surfaces until Windows reports the session locked.
- Verify handover does not restore the previously focused application and logs
  `Windows lock confirmed; Illium lock surfaces closed`, not `screen unlocked`.
- Change display configuration during handover: Illium must not try to bring
  its password window back to the foreground.
- If Windows session-state queries fail or the lock request is accepted but
  never confirmed, Illium must keep its surfaces/input protection. No elapsed
  timeout may unlock Illium. Ctrl+Alt+Del remains available for recovery.
- Unlock Windows afterwards and verify the Illium surfaces are gone.
- Verify a correct Illium password still restores the original application.
- Verify missing/invalid password configuration still falls back to Windows.

## Diagnostics

`handing over to the Windows lock` records the reason. A foreground-triggered
handover is preceded by `external foreground while locked`, including the
reported and actual HWND, owning PID, executable path (if accessible), and
placement readiness. Window titles and password input are not logged.

At debug level, `ignoring transitional lock foreground event` identifies
obsolete events or incomplete placement, and
`Windows lock not confirmed; keeping lock surfaces` identifies session-query
errors. The screensaver timeout itself does not request a Windows lock.
