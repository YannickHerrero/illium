# Instant experience work log

Dedicated branch: `feat/instant-experience` (daemon and applet collection).
Do not merge or deploy over the active desktop without completing Windows validation.

## Contract

- Acknowledge input at the next available frame; keep disk, COM enumeration and
  process waits off interactive paths.
- Prefer cached useful content to skeletons; use explicit loading state when no
  snapshot exists. Never present a different folder's files as actionable data.
- Ignore stale asynchronous results. Preserve focus, selection and scroll on refresh.
- Optimistic controls reconcile with real results; destructive actions never claim
  unconfirmed success.
- Background work must not monopolize the UI. Measure cold and warm paths separately.

## Lots

1. Foundations: coalesced event-loop wakeups, bounded draining, latency traces,
   asynchronous application indexing with generation checks.
2. Openings: show Files before reading, retained snapshots and loading state;
   position existing popup windows before showing them.
3. Interactions: cheap audio levels vs device/session enumeration; optimistic
   controls; local calendar day selection where feasible.
4. Continuity: differential file models, border drawing cache without losing
   visibility/z-order repair; workspace placement without intermediate tiled reveal.
5. Fine tuning: terminal output leading-edge rendering, measured build tuning.

Each logical change gets an independent commit and focused tests. This is a
tracking document, not a claim that a lot or desktop validation has completed.

## Validation

- Baseline: clean `048f56d` / collection `75a3641`; authorship verified as
  YannickHerrero <yannick.herrero@proton.me>.
- Run portable unit/integration tests and Windows cross-checks throughout.
- Desktop acceptance: cold/warm applets, launcher, terminal, browser, Files and
  Tasks; rapid repeated input; provider errors; reload; workspace switching with
  tiled/floating/fullscreen/minimized clients; mixed DPI and display changes.
- Record what was actually run and unresolved limitations below before handoff.
