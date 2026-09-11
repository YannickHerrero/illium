# Controller dependency and Defender investigation

## Scope and findings

This follow-up to [corrections.md](corrections.md) does not establish a false positive or approve a release. No Defender exclusions, quarantine restoration, elevation, external submission or protection-policy changes were made.

The previously quarantined controller depended on the complete `winarchy` library. PE inspection found unnecessary graphical imports, including `SetWindowsHookExW`, `CreateWindowExW`, `GetAsyncKeyState` and `ShellExecuteW`, in a roughly 12 MiB controller. Imports alone do not establish execution of those functions, and do not establish the cause of detection.

Commit `62ead81` separates:

- `winarchy-ipc`: the existing canonical commands, framing, identity validation and bounded pipe I/O;
- `winarchyctl`: a small controller depending only on `winarchy-ipc`;
- the graphical daemon, which uses the same shared protocol and retains its existing features.

The CLI no longer depends on Slint, winit or the daemon. No access checks, deadlines, cancellation completion or reply bounds were removed. Existing public daemon-library command/client paths are reexported for compatibility.

The resulting controller is **1,039,872 bytes**. PE inspection finds no user32 import and none of the four graphical functions listed above. Ordinary Rust runtime imports, including process/network-related system APIs, can remain even when the application does not call them; this is not a claim of complete binary minimality. High-entropy ASLR, dynamic-base relocation and NX compatibility flags remain present.

CI now checks the dependency boundary and builds/tests all workspace packages. `scripts/check-cli-dependencies.py` passes for both Windows GNU and MSVC dependency graphs; graph inspection is not an MSVC compilation.

## Validation

- Formatting and Linux/Windows-GNU Clippy pass for the workspace.
- All **44 Linux desktop-independent tests** pass after extraction.
- Windows-GNU release binaries and native test executables cross-compile.
- Windows runtime tests of this revision were **not** run after the controller was blocked.

The installed test binaries built from `62ead81` had these SHA-256 values:

```text
0f427ab8438a4f00c7cf7366c93fdaba3c072f2b1dcdb1951085b5403db8818b  winarchy.exe
a8be20ed9266e2e331f024c650b702b3bdc155a6fa18c56ad058a1ff1ad2c2c2  winarchyctl.exe
```

They were placed in the planned user-local installation directory, `%LOCALAPPDATA%\Programs\Winarchy`, without restoring the quarantined controller. Defender's on-demand directory scan again returned no threats. On the **single attempted controller launch**, Windows rejected process execution. The test used terminating PowerShell errors and stopped immediately; the daemon and desktop tests were not launched.

Defender Operational event 1116, at host-local time 2026-09-11 18:25:23, records:

- `Trojan:Win32/Wacatac.F!ml`, ThreatID `2147749375`;
- detection source: realtime protection; detection type: fast path;
- the installed controller as the file resource;
- PowerShell as the process attempting access/execution;
- signature version `1.459.156.0`, engine `1.1.26080.3`.

Event 1117 records successful quarantine at 18:25:54. These records do not disclose the classifier's features or prove that the controller reached `main`. In particular, this is **not evidence of a particular IPC operation being detected during execution**.

Thus removing unrelated GUI dependencies fixed a genuine architectural defect, **but did not resolve Defender's classification**. No further Winarchy build variants were tried after that failure.

## Independent toolchain control

`scripts/diagnostics/toolchain-hello.rs` is a separate print-only program, not a substitute controller. It performs no Winarchy operations. It was compiled using the same Rust 1.96.0 / Windows-GNU target with optimization and symbol stripping, scanned normally, then executed once from the development diagnostics directory. It printed its expected message and no block was observed.

```text
26332f8a6095cec59ae01deaa08555ffa6c2b9f6b4c45f83d58704a3e595b1cf  winarchy-toolchain-hello.exe
```

This shows that this toolchain can produce an executable that runs on this machine. It does not certify the toolchain, exclude path/context effects, or prove Winarchy benign. The control was not a renamed Winarchy binary and was not used to perform an operation through a blocked component.

## Read-only correlation of other security controls

A subsequent investigation inspected the raw XML fields (not just localized messages) for Defender events 1116/1117, and correlated Code Integrity, AppLocker EXE/DLL and Defender ASR/CFA events between **15:42 and 16:28 UTC on 2026-09-11**. It did not run or modify any executable. A minimized, profile-redacted evidence extract is in [detection-correlation.json](audit/2026-09-11-detection-correlation.json).

The controller's antivirus detection and quarantine share detection ID `{4545C864-C567-43A8-8284-F5879A1524EC}` (records 70440 and 70441). They identify realtime source 3, fast-path type 8 and successful quarantine. They provide **no classifier feature, code location, API trace or confidence score**.

There are additional, separate findings:

- Winarchy-related Code Integrity events were **3076**, under `DefaultWindowsAudit`. These are audit-mode findings, not enforced 3077 blocks. The successfully executed print-only control also has a 3076 event.
- ASR events referencing Winarchy were **1122**, for rule `01443614-CD74-433A-B99E-2ECDC07BFC25`: “Block executable files from running unless they meet a prevalence, age, or trusted list criterion.” These particular events are **audit**, not block events. The print-only control also triggered this audit at 16:27:52 UTC and still printed its message.
- AppLocker EXE/DLL returned no events in the queried interval. No Winarchy-related enforced Code Integrity or ASR/CFA block was found there. This is a scoped observation, not proof that the machine has no other policies.

Thus a reputation/trust audit signal is confirmed, but **must not be conflated with the separate `Wacatac.F!ml` antivirus quarantine**. Its presence on the successful control means it does not by itself explain the observed difference. A missing signature, the compiler, an IPC API, or PowerShell being listed as the accessing process cannot be declared the exact cause on this evidence.

Microsoft references used to interpret event IDs and the rule GUID:

- [ASR rule reference](https://learn.microsoft.com/en-us/defender-endpoint/attack-surface-reduction-rules-reference#block-executable-files-from-running-unless-they-meet-a-prevalence-age-or-trusted-list-criterion)
- [ASR audit testing / event 1122](https://learn.microsoft.com/en-us/defender-endpoint/attack-surface-reduction-rules-deployment-test)
- [App Control event 3076 versus 3077](https://learn.microsoft.com/en-us/windows/security/application-security/application-control/app-control-for-business/operations/event-id-explanations)

The exact antivirus classifier trigger remains unknown. Iterating renamed, stripped or selectively altered binaries until one is accepted would not establish that trigger or certify safety; it is not the diagnostic method used here.

## Current limits

Visual Studio is installed, but `vswhere` finds no `Microsoft.VisualStudio.Component.VC.Tools.x86.x64`, and the standard Windows SDK library directory is absent. No MSVC toolchain/SDK was installed and no native MSVC build is claimed.

The test daemon was never launched during this follow-up; Explorer remains running. The user-local installation is incomplete because the controller was quarantined. Defender remains in normal mode with antivirus and realtime protection enabled.

Source/dependency cleanup and a passing scan cannot guarantee runtime acceptance. The exact cause of the classification remains unknown. Native MSVC validation in an appropriately equipped environment and an independent security review remain legitimate avenues, not promises that a different build will be accepted. Do not retry the quarantined executable through exclusions, renaming, or a different launcher.

## MSVC build with embedded resources

All binaries above were MinGW cross-builds (`x86_64-pc-windows-gnu`): they imported `msvcrt.dll`, carried no `.rsrc` section (no version information, no manifest) and were unsigned with zero prevalence. No MSVC build had ever been produced: the repository has no remote, so the Windows CI job never ran, and the Windows host has the `stable-x86_64-pc-windows-msvc` toolchain but neither the MSVC compiler nor the Windows SDK, and no administrator rights to install them.

The build now targets `x86_64-pc-windows-msvc` from WSL through cargo-xwin, with clang-cl/lld-link/llvm-rc extracted from Debian packages into a user prefix. `build.rs` and `crates/winarchyctl/build.rs` embed VERSIONINFO and the `asInvoker` manifest from `winarchy.manifest` through `winresource`. Results on 2026-09-11, from the working tree after `804f14e`:

```text
f85882b84f5a99c2fd4efec90eb907b6d38d7cffeacd5127809dedd415c71a12  winarchy.exe   (5,100,544 bytes)
4fc57c4daa665b1839c119b84d3e7aede5199ad1728e530d535016cbec77ea7b  winarchyctl.exe  (262,144 bytes)
```

Both have a `.rsrc` section. The controller imports only kernel32, ntdll, advapi32, oleaut32, the synch API set, `VCRUNTIME140.dll` and UCRT API sets; `ws2_32`, `userenv` and `msvcrt` are gone. They were installed in `%LOCALAPPDATA%\Programs\Winarchy`, an on-demand Defender scan of that directory found no threats, `winarchyctl status` ran (reporting the daemon unavailable, as expected), then the daemon was started from PowerShell and `winarchyctl status` returned the managed-window state. No Defender detection, quarantine or block event was recorded in the interval. Realtime protection stayed enabled and no exclusion was added.

This is a single host with one signature version and does not establish a false positive or predict acceptance elsewhere. It does show that the classification was tied to the MinGW build artifacts rather than to an observed runtime behavior, since the same source now runs. Submission of the earlier hashes to Microsoft for review remains a separate, user-approved step.
