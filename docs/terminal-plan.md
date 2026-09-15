# Native terminal implementation plan

Initial milestones below are implemented as an opt-in candidate. See
[terminal usage, measured results and remaining validation](terminal.md).
The user's WezTerm alias/configuration has not been replaced.

Primary metric: request to visible window, independently of WSL readiness.
A separate `winarchy-terminal.exe` in this workspace opens **WSL only**, never
Herdr. A resident process retains graphics/font resources but no idle WSL
session. Every new window owns a fresh PTY; closing it terminates that child.
No tabs, splits, plugins, SSH client or session persistence.

Small implementation milestones (each committed independently):

1. Record the design and add a reproducible Windows opening-time probe.
2. Add optional theme-owned background opacity; keep legacy themes valid.
3. Add terminal settings, palette mapping and tested VT/input models.
4. Add asynchronous ConPTY/WSL lifecycle with bounded output/input queues.
5. Add the native, demand-driven renderer and window/input integration.
6. Add owner/session-scoped resident IPC and opening-time diagnostics.
7. Add live theme reload, packaging, usage and regression checks.

Reuse `alacritty_terminal` rather than implement VT emulation. Evaluate native
DirectWrite/Direct2D with DirectComposition (GPU, background-only alpha).
Do not put the grid in the shell's Slint software renderer. Keep PTY creation,
reading, writing and teardown off the UI thread. Render only on damage, at most
once per frame; do not animate the cursor or poll while idle.

Configuration: `%USERPROFILE%/.config/winarchy/terminal.toml` (respecting
`WINARCHY_CONFIG_HOME`) for font, padding, scrollback and WSL distribution.
Colors come from the existing selected palette; opacity is an optional theme
field, defaulting to 85% opacity. Preserve Ctrl+1..9 CSI-u and leave Alt+Enter to the
window manager. Test Unicode, alternate screen, bracketed paste, mouse modes,
resize and selection. Explicit application truecolor is not recolored.

Validation: Linux model tests, Windows cross-checks and native Windows tests;
release opening-time probe with WSL already running, followed by interactive
checks where possible. Never shut down the user's WSL to benchmark. Record
window discovery separately from first paint and shell readiness: discovery
alone does not prove pixels have reached the display. Report p50/p95 and
resident private memory, without claiming an unmeasured latency improvement.

Do not replace the user's existing terminal alias until the candidate is
validated. Keep WezTerm available as a fallback. Alacritty is not currently
installed on the inspected Windows machine; no third-party installation is
required for this prototype.
