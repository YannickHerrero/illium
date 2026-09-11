# Corrections and Windows validation follow-up

**Later investigation:** the controller has since been separated from the GUI dependencies. It became substantially smaller but was still blocked at launch. See [controller investigation](controller-investigation.md) for the evidence and independent toolchain control.

This follow-up supersedes the open-item status in the initial [security audit](security-audit.md), but **does not approve the application for everyday use or establish an antivirus false positive**.

## Source corrections

- IPC reads/writes use overlapped Win32 I/O with cancellation completion before buffers are released. Deadlines: request read 3 s, queued command 5 s, reply write 2 s, acknowledgement 1 s, client exchange 12 s. The acknowledgement replaces an unbounded server `FlushFileBuffers` call.
- Queued requests can be cancelled atomically. A request already executing reports an unknown outcome on timeout; the client must not retry blindly.
- IPC endpoint and mutex names derive from the Windows token SID and session ID, not `USERNAME`. The client checks the server process's user/session and requests identification-level token access only.
- Hook event queues are capped at 1,024 items and never block the hook. Overflow triggers bounded reconciliation. Client management is capped at 512 windows.
- TOML reads are capped at 64 KiB per file, configuration snapshots at 256 directory entries. Start Menu discovery examines at most 8,192 entries and 16 levels per root, without following links/junctions.
- Recovery uses a GUID-scoped session, validates the daemon's process creation time, and tags each managed HWND with a per-window generation. Stale numeric HWND values are pruned before actions; pruning triggers a relayout. These checks mitigate reuse races, not hostile same-user process control.
- Existing eligible application windows are enrolled at startup. Hooks are installed before enumeration so the snapshot/subscription gap cannot lose new windows. Minimized windows stay minimized and are enrolled on restoration. Already-managed minimized windows retain membership but leave the tiling/navigation calculations until restored.
- Following a stronger keyboard test failure, modifier tracking was moved from `GetAsyncKeyState` inside the keyboard callback to an ordered left/right modifier state machine. The latest source has three portable regression tests for this change; its Windows desktop effect has **not** been validated yet.

The earlier standard-token requirement, absolute paths for Windows executables, session-limited Explorer stop, log redaction and pinned CI actions remain in place.

## Results by build

### Candidate `7b8e406`

Defender custom scan reported no threats, with realtime protection enabled. Windows execution passed:

- 45 native/non-desktop tests, including timeout/cancellation and window-generation tests;
- IPC desktop smoke test;
- the then-current interactive smoke test;
- Alt+Enter opening a new real WezTerm GUI window;
- launcher search, Down/Up and Enter launching the terminal alias;
- launcher search and Enter opening the installed WezTerm Start Menu `.lnk`;
- ordinary-session crash recovery.

The old interactive smoke test did not assert each directional movement outcome precisely enough. Its success must not be presented as conclusive validation of all directional shortcuts.

### Candidate `c10d931`

The corrected startup behavior was tested by **creating windows before launching the daemon**. Three visible fixtures were enrolled, assigned and tiled without overlap. A fourth minimized fixture stayed minimized, then was enrolled after restoration. Minimizing an enrolled fixture followed by configuration reload did not forcibly restore it. This test passed.

The 45 native/non-desktop tests and IPC desktop smoke test also passed.

However:

- The strengthened interactive test failed an assertion comparing the expected and actual ordering after a directional movement shortcut.
- A subsequent terminal-launch shortcut test failed to observe a new managed WezTerm window; the remainder of that run was interrupted. No definitive cause has been established.
- The current modifier-state source correction is newer than this candidate and was not deployed or retried against Defender.

Current source checks: `cargo fmt --check`, Linux Clippy, Windows-target Clippy and **44 Linux desktop-independent tests** pass. This is not a substitute for rerunning Windows interactive tests.

## Antivirus: static success did not predict runtime acceptance

For `c10d931`, standard Defender scans of both the installation directory and native tests reported **“found no threats”**, exit code 0. Protections were not disabled or excluded. The scanner executable was from platform `4.18.26080.3-0`. At the initial scan preflight, Windows reported signatures `1.459.156.0`, normal mode, and antivirus/realtime protection enabled; no claim is made that signatures stayed unchanged throughout the later tests.

Later, attempting to execute the installed `winarchyctl.exe` was blocked. Defender recorded:

```text
Trojan:Win32/Wacatac.F!ml
ThreatID: 2147749375
ActionSuccess: true
File: %LOCALAPPDATA%\Programs\Winarchy\winarchyctl.exe
```

The file was removed from the installation directory. The daemon was stopped, and Explorer was left running. There was no quarantine restoration, antivirus exclusion, policy change or manual binary submission. No additional build was generated to try to defeat that classification.

The `c10d931` executable hashes are:

```text
64ac67f938c57b6f443da769413e3e8a73b1f2a87bb586fedf5801e142e398b8  winarchy.exe
2cfdd016df3744a0de000e34737ffb0086725e51ea8a6111df00f43f12a62069  winarchyctl.exe
```

The earlier `Trojan:Win32/Bearfoos.B!ml` daemon detection remains a separate historical result. Neither detection has been established to be a false positive. **A clean on-demand scan alone is not evidence that future launch or behavior monitoring will accept a binary.** The partial test installation is not ready for use.

## Still outstanding

- Windows rerun of the latest modifier tracking and stronger directional assertions in an approved test environment.
- Resolution/review of the antivirus detections; no promise of Defender acceptance.
- Negative cross-account/elevated-token tests beyond the same-user identity checks.
- Exhaustive 100%/mixed-DPI and monitor-hotplug validation, visual inspection/screenshots.
- The four upstream non-maintenance notices recorded by RustSec; no known vulnerability was reported in the locked set during the audit.
- Synchronous configuration/filesystem work may still be slow on unavailable/network-backed storage; size/depth limits are not filesystem latency guarantees.

The old `dist/` candidate archive is historical, not a package of the current corrected sources. Build sources and tests remain available without administrator privileges; do not disable protection to run a blocked executable.

To reproduce the startup regression in an approved Windows test environment, stop any existing daemon, set `WINARCHY_CONFIG_HOME` to a disposable configuration and `WINARCHY_TEST_DAEMON` to the test daemon's absolute path, then run:

```powershell
cargo test --test startup -- --ignored --exact enrolls_preexisting_windows_and_tracks_restoration --nocapture
```

Stop testing if any component is blocked by endpoint protection; do not continue simply because an earlier scan was clear.
