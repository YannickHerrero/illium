# Unresolved Windows security detection

Follow-up: [source/dependency audit and remaining issues](security-audit.md). The source now contains security corrections; this report's executable hashes and prior candidate archive identify the **old build**, not those corrections. No corrected daemon has been run to bypass this detection.

The locally cross-built release daemon ran the recorded IPC/window/recovery tests, then Microsoft Defender quarantined its Windows-host copy. The product reported:

- Daemon: `Trojan:Win32/Bearfoos.B!ml` (ThreatID `2147731849`).
- Defender action succeeded; the host executable was removed.
- Explorer remains running. No Winarchy daemon/watchdog was left running.
- Defender was not disabled, exclusions were not added, and quarantine was not reversed.

The detection is **unresolved**. The fact that this executable was built from the repository does not justify silently bypassing endpoint protection. It also does not establish which behavior caused the classification. This is a release blocker, not a passing security test.

## Build identification

Runtime source corresponds to commit `ebdca83c04c587e565fde27042e0dd9efd299f15` (subsequent changes at the time of this report are tests/documentation/license metadata).

Target: `x86_64-pc-windows-gnu`, release profile, stripped symbols, unsigned.

SHA-256:

```text
340999bc6351f277bcc8067d7f30141de5e306772a96773fcd195c8c9882afbd  winarchy.exe
5b8985f7f149d6ddd505fb119928d3cbe70e56bd6243faefbf8d1a0c6acb2a36  winarchyctl.exe
```

The candidate artifacts retained under WSL's ignored `dist/` directory are for local review, not an endorsed public release. The quarantined executable was not recopied to Windows after detection.

## Review before proceeding

1. Review Defender Protection History and your organization's application-control policy.
2. Review the source and reproduce a clean build from the locked dependency set.
3. If the classification is believed to be incorrect, submit the exact hash/binary to Microsoft's official file-submission service for analysis. No submission was made automatically.
4. For a public release, complete dependency/license auditing, normal publisher signing and endpoint-protection validation.
5. Only after the detection is resolved, resume interactive tests on an unlocked desktop.

The intended high-impact operations are explicit and documented: configured keyboard hooks, window show/hide/position calls, owner-only command IPC, and an opt-in Explorer stop with a recovery helper. There is no persistence installation, shell registry replacement, network listener or process-killing window-close command.

A temporary PowerShell screenshot helper was separately blocked as `HackTool:PowerShell/ScreenCapture.G`. It was not bypassed or retained in the repository. No screenshot was obtained.
