# Lock screensaver: Windows validation

Automated tests cover config compatibility/validation, idle thresholds, activity taking precedence over timeout, resetting after loss of focus, cycle selection (including a single effect), bounded animation frames and effect progression. Cross-target checks compile the Slint UI and native Windows input integration. They do **not** replace these interactive checks on Windows 11.

Use a disposable session with a known Winarchy password and a recovery console available. Winarchy's lock surface is not the Windows security boundary; use Win+L for a real unattended lock.

## Timing and configuration

- Lock with Ctrl+Alt+L. At 29 seconds the password UI remains; at about 30 seconds animation covers every monitor without a desktop flash.
- Before timeout, type, move the mouse, click, and scroll. Each restarts the idle interval, including movement within one window and on a secondary monitor.
- Verify all four effects using one-element `effects` lists, reloading before each new lock. Check the 12-second cycle and no adjacent repeats with multiple effects.
- Test `enabled = false`, timeout 1, omitted table, and invalid timeout/unknown/empty effects. Invalid reloads must leave the previous valid config intact.

## Wake and password isolation

- Wake with a letter, Enter, Escape, a modifier, mouse movement, each mouse button, vertical and horizontal wheel. Only the wake action is consumed; the session stays locked with an empty password field.
- Hold the waking letter/Enter through the transition: auto-repeat must not type or submit. Release it and then type the actual password normally (including AltGr characters).
- Hold a waking mouse button and release it over the field. No stray click/release should reach the desktop or submit anything.
- Enter half a password, wait for animation, wake: the field is empty. A correct full password still unlocks.
- Enter four wrong passwords, allow animation and wake, then submit the fifth wrong password. Windows must take over; the saver must not reset attempts or obscure fallback.
- Repeatedly lock/unlock: no stale frame, retained password, hidden cursor or delayed activation in the unlocked session.

## Displays, session and rendering

- Test different monitor resolutions, 100/150/200% DPI, negative monitor coordinates, and a primary password monitor other than the Windows primary monitor.
- Unplug/reconnect a display during animation. Existing lock behavior must continue to cover all attached monitors or fall back to Windows, with no exposed desktop. Wake from the secondary monitor.
- Check a stationary pointer disappears at activation and reappears at wake, fallback and unlock.
- Try Win+L, Ctrl+Alt+Del, focus takeover, UAC, suspend/resume, and Windows lock/unlock. No focus stealing from the secure desktop; no animation should run when the lock's password window lacks foreground focus. Upon regaining focus, the idle delay starts again.
- Observe CPU/GPU before, during and after animation on one and several monitors. Animation has a fixed 64 x 23 character canvas, 23 text rows per monitor and a 33 ms minimum frame interval (the shell polls at 10 ms). No extra animation timer/process should survive exit.
- Confirm all effects remain readable on light and dark themes. No external terminal, Python process, image download or network access is used.

## V1 validation record

- Linux: `cargo test -p winarchy -p winarchy-config -- --test-threads=1` passed: 163 tests, one existing opt-in test ignored.
- The parallel run hit an existing theme-picker render test's wall-clock timeout (`parallel_render_preserves_paint_order_and_reuses_frames`). It passed in the serial run; no unrelated test was changed.
- Formatting, Clippy with warnings denied on Linux, and Windows GNU cross-target Clippy for all targets passed (including Slint compilation).
- The interactive Windows checklist above remains **unverified** on this Linux host. In particular, compilation alone does not prove cursor, input-hook, multi-monitor or secure-desktop behavior.

## Keyboard wake regression

The focused saver owns a Slint `FocusScope` as a fallback if Windows removes the low-level keyboard hook. Any delivered key press requests an epoch-tagged wake, without submitting a password. The fallback retains keyboard focus until the waking key is released to consume auto-repeat; only then does the password field accept text again.

`focused_key_wakes_without_hook_and_cannot_type_or_submit` runs the real lock view with a headless software window. It verifies letters, Enter and Escape, held-key repeats, stale wake rejection, retention of the lock, and password-field focus after release. This test was executed successfully as an MSVC Windows test executable without altering the live desktop. The merged Linux suite (168 tests) and MSVC Clippy with warnings denied also passed.

## Commands

```sh
cargo fmt --all --check
cargo test -p winarchy -p winarchy-config
cargo clippy -p winarchy -p winarchy-config --all-targets -- -D warnings
# From a Linux host with the Windows GNU target installed:
cargo check -p winarchy --target x86_64-pc-windows-gnu
cargo clippy -p winarchy --target x86_64-pc-windows-gnu --all-targets -- -D warnings
```
