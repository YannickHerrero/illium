# Instant experience work log

Branch: `feat/instant-experience`, in Illium and the applet collection.
Baselines: `048f56d` / `75a3641`. Commits retain
YannickHerrero <yannick.herrero@proton.me> as author.

**Status:** implementation and automated validation below are complete; active
Windows desktop acceptance and key-to-pixel benchmarks are not. Illium's
`master` and the collection's `main` remain unchanged. A user-authorized test
deployment is recorded below; do not fast-forward until the desktop gate passes.

## Contract

- Acknowledge input at the next available frame. Keep slow disk reads, COM
  enumeration and process waits off interactive paths.
- Prefer cached useful content to skeletons. Never present another folder's files
  as actionable data or an unloaded provider's defaults as authoritative state.
- Reject stale asynchronous results. Preserve focus, selection and scroll.
- Reconcile optimistic controls; destructive actions never claim unconfirmed success.
- Bound background work and separate cold/warm measurements. This contract is a
  target, not a claim that every interaction has been measured below 16 ms.

## Implemented lots

### 1. Responsiveness foundations

- Coalesced event-loop wakeups actually drain the queue, rather than posting an
  empty callback. Drains yield after 128 events or a 4 ms budget; remaining work
  is rescheduled. Existing overflow reconciliation and maintenance stay intact.
- Single-flight launcher indexing runs off the UI thread with generation checks,
  retained results and balanced worker COM initialization.
- Files listings, previews and operations run on workers. Request identity rejects
  obsolete results; operation completion does not override subsequent navigation.
  Quit cannot interrupt a running file operation.
- Audio COM work is off-thread. Cheap levels and expensive device/session snapshots
  are separate. Queued user volume intent coalesces without poll starvation.

### 2. Immediate openings

- Files shows a retained snapshot or stable loading placeholders before reading;
  Tasks wakes its sampler on show and publishes via event-loop callbacks.
- Native shell surfaces are prewarmed offscreen. Preparation waits until a HWND
  really exists before hiding; a real opening supersedes the pending hide.
  Popup geometry is applied before showing an existing native window.
- Referenced applet definitions/views are prepared before desktop readiness.
  Initial audio and calendar snapshots have loading placeholders, not false zeros.
  Cached applet content remains accessible after provider errors.
- Browser residents retain their WebView2 environment, filter engine and unchanged
  library snapshot while closing controllers/pages. Error cleanup releases views
  before COM teardown. Unnecessary blank navigation/theme reads were removed.

### 3. Immediate interactions

- Audio sliders/key controls update locally and reconcile after the last queued
  action, including failure. Generic providers retain per-result completion callbacks.
- Calendar snapshots include all 42 visible days within the provider's 64 KiB
  output budget. Day/Today selection is local even while busy; dismiss persists
  selection without overwriting a pending month change. Visibility preferences
  remain persisted. Month/provider/filter changes are still provider-backed.
- Files sorting and hidden-file filtering reuse current/parent listings without
  disk reads. Pending operations keep navigation usable and reconcile errors and
  cut/copy state. Tasks acknowledges termination requests and immediately resamples;
  it does not pretend the process has already exited.

### 4. Visual continuity

- Files retains row models and changes only differing rows.
- Unchanged bars/backgrounds are retained across non-geometric reloads. Bounded
  recursive applet fingerprints preserve unchanged instances, in-flight providers,
  action queues and per-entry generations. Superseded view callbacks are rejected.
- Border colors/regions are cached without skipping visibility or stacking repair.
- Workspace outgoing parking and incoming final placement use batches, without an
  intermediate tiled reveal; batch failure falls back to individual placement.

### 5. Anticipation and fine tuning

- Terminal isolated output paints on the leading edge; sustained output remains
  coalesced. Borrowed DirectWrite cache lookups avoid hit-time allocations; ordinary
  Unicode scalars are stored inline, retaining combining sequences and clipping.
- Launcher names are normalized once per index; result sorting no longer clones
  names for comparator keys.
- Release builds use thin LTO and one codegen unit. The full cross-build passed.
  Observed executable sizes fell approximately 1–14% versus the earlier branch
  build, but those were not controlled same-source latency benchmarks. No runtime
  speedup is inferred from size or build time.
- Opt-in `--trace-latency` logs queue wait, dispatch and placement durations.
  `scripts/summarize-latency.py` reports median/p95/max and has a self-test.
  These measurements are **not first-visible-pixel latency**.

## Validation actually run

- Linux `cargo test --workspace --locked`: **189 passed**.
- Windows workspace tests cross-compiled with `cargo xwin test --workspace --target
  x86_64-pc-windows-msvc --no-run --locked`, then executed on Windows:
  **228 passed**, 23 opt-in tests skipped by the regular run.
- Five opt-in non-destructive Windows regressions additionally passed:
  - real event-loop delivery without polling, prewarmed HWND reuse, geometry,
    unchanged foreground window and opening/prewarm race;
  - cached border visibility/z-order repair;
  - batched parking/restoration and invalid-HWND fallback;
  - hidden DirectComposition/DirectWrite GPU readback, Unicode/styles/resize;
  - two hidden WebView2 openings with the same environment/filter cache and an
    isolated temporary profile, leaving the normal browser library untouched.
- Linux Clippy: `--workspace --all-targets -- -D warnings` passed.
- Windows Clippy passed with `-D warnings -A clippy::items-after-test-module`;
  the allowance is for pre-existing test-module placement in `platform/input.rs`.
- Full Windows release workspace cross-build passed (13m54s after changing the
  release profile); the final-source incremental rebuild passed in 3m21s.
- Calendar collection: six installer tests, 13 Xvfb Slint fixtures/interactions,
  and Windows `provider.ps1`, `outlook.ps1`, `orchestration.ps1`, `model.ps1` passed.
  Outlook tests use fixtures; no private agenda/Outlook data was accessed.
- CLI dependency guard and latency-summary self-test passed.

Two unrelated baseline test defects were fixed in independent commits: reliance
on shared `/tmp` contents and a picker losing valid confirmation because another
preview was corrupt. Wallpaper test assets are embedded for copied Windows tests.

## Remaining acceptance gate

Complete these checks on the target desktop with the deployed test build:

1. Cold/warm opening of applets, launcher, terminal, browser, Files and Tasks;
   record first useful pixels, not only IPC replies. Collect median/p95 over
   repeated runs, along with idle memory and startup/prewarm cost.
2. Type, navigate and adjust volume while providers/indexing/refreshes are busy;
   test rapid reversals, provider failures, disconnected/slow folders and reloads.
3. Exercise tiled/floating/fullscreen/minimized clients, workspace focus order,
   batch fallback, mixed DPI, monitor changes and DWM thumbnail compatibility.
4. Verify no flashes, focus theft or selection/scroll loss during those journeys.

Known boundaries: a currently executing filesystem syscall is not interrupted;
new Files requests wait for its worker while the UI remains responsive. Changed
applet views after reload can still compile on their next opening; startup prewarm
is not a general background compiler. Win32 placement/focus calls and slow external
applications still need real-desktop traces. Calendar visibility filters are not
optimistically rewritten locally. Persistent launcher caches and speculative
folder prefetch were not added without workload evidence.

No active-desktop end-to-end suite or master fast-forward was performed.
Automated correctness checks alone do not establish that the full desktop now
feels instantaneous.

## User-authorized test deployment — 2026-09-22

- Deployed the six release executables from `b8988f0`, volume's view and the three
  changed calendar runtime files from collection `c0b157e`. Installed binaries
  were verified against the release artifacts using SHA-256.
- Backed up replaced binaries and applet files outside the watched configuration:
  `%LOCALAPPDATA%\Illium\backups\instant-20260922-141129`.
  `deployment.json` records the affected paths. User settings were not replaced.
- Gracefully restarted the daemon and checked IPC readiness: one bar, six clients,
  workspace 4 retained, no wallpaper error. New Apps/browser residents started.
- Preserved the existing terminal session and standalone browser window by renaming
  their loaded executable images rather than terminating them. Those processes
  still run the old code. The terminal resident needs a graceful quit after all
  its windows close before its new renderer can be tested.
- This is a deployment smoke check, not completed desktop/perceived-latency acceptance.

### Companion restart and multi-tab launch follow-up

At the user's request, the old terminal session was closed gracefully and a new
terminal resident/window started. The browser's three original URLs were saved
outside the repository. Keyboard-based restoration was unreliable, so the user
requested proper multi-target launching instead.

`76c9907` / `0cbced3` add ordered CLI targets and backward-compatible resident IPC.
Validation: 21 browser tests passed on Linux and Windows, Windows Clippy passed,
and a hidden WebView2 test verified three controllers/tabs with the first selected
across two host openings. The release workspace build passed.

The updated browser was deployed with backup at
`%LOCALAPPDATA%\Illium\backups\browser-multi-20260922-143130`.
A fresh resident accepted all three saved URLs in one CLI invocation. The resulting
tab list was checked: **all three original URLs restored in their original order**.
Both browser and terminal now run updated binaries. No master/main merge occurred.
