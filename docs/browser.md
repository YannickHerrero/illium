# Winarchy Browser — native-blocking prototype

An experimental, single-window Windows browser in `crates/winarchy-browser`, alongside Terminal and Dictate. Rust/Win32 hosts WebView2 Evergreen; `adblock-rust` evaluates requests in the host, not in an extension. This is a feasibility prototype, **not a validated replacement for a full browser**.

## Build and launch (Windows)

Install the current stable Microsoft WebView2 Evergreen Runtime and the Rust MSVC toolchain. Older runtimes without `ICoreWebView2_22` are not supported (the prototype requires request interception including worker sources).

```powershell
cargo build --release -p winarchy-browser --locked
powershell -ExecutionPolicy Bypass -File scripts/update-browser-filters.ps1
.\target\release\winarchy-browser.exe https://example.com
```

In a release archive, use `winarchy-browser.exe` next to the other binaries and the included `scripts/update-browser-filters.ps1`. The updater is an explicit user operation; neither the browser nor Winarchy downloads lists on startup. Restart after updating. The first launch compiles the lists; subsequent launches use a content-keyed, engine-versioned disk cache. Text lists are still read to validate the cache. Missing lists cause an explicit startup error rather than silently browsing without protection.

The browser uses:

- `$WINARCHY_CONFIG_HOME/browser`, or `$HOME/.config/winarchy/browser`: `easylist.txt`, `easyprivacy.txt`, optional `custom.txt`, compiled filters and `exceptions.json`;
- `%LOCALAPPDATA%/Winarchy/browser/profile`: persistent WebView2 profile, cookies and disk cache;
- the current `winarchy-theme` at launch: window/editor background, editor text, WebView initial background and page dark/light preference. Live theme switching is not implemented.

## Home, history and bookmarks

Launching without an argument (including `Alt+B`), or submitting an empty address / `about:blank`, shows a native home surface. A solid Winarchy theme color fills the window, with the URL/search field centered and focused. The home window uses 85% opacity (alpha 217/255, also applying to its native controls). Web pages return to full opacity. An explicit web URL on the command line bypasses home.

The field suggests up to eight local results, matching case-insensitive ordered subsequences against titles and URLs. Contiguous matches rank higher; equally ranked favorites precede recent history. With empty input, favorites come first, then recent history. No remote suggestions, page prefetches or second WebView are used. Typing and pressing Enter submits the input as typed; select a suggestion with arrows first to open it, or double-click it.

`Ctrl+D` adds the current page to favorites without opening the search field or moving focus. Repeating it does not remove or duplicate the bookmark. Favorites and history share the same fuzzy results list, with each URL shown only once. An already-open home refreshes its data when it regains focus. Successful top-level web navigations record the title and URL; history is deduplicated and limited to 500 entries. Data is stored as plain JSON in `browser/library.json`; embedded URL username/password credentials are removed, but paths and query strings remain. There is no private browsing mode yet. Close the browser and remove this file to erase history and bookmarks. Updates are merged under a file lock across windows; no idle polling/indexer is added.

## Controls

| Input | Action |
| --- | --- |
| `Ctrl+L` | Reveal native address editor; select current URL |
| URL/domain, then Enter | Navigate (bare domains use HTTPS) |
| Other text, then Enter | DuckDuckGo search; no remote autocomplete |
| Up / Down, then Enter | Select and open a history/bookmark suggestion |
| `Ctrl+D` | Add the current page to favorites (safe to repeat) |
| Escape | Close editor and restore page focus; on home, clear input |
| `:block` in editor, then Enter | Toggle blocking for the current exact hostname, persist, reload |
| `Alt+Left` / `Alt+Right` | Back / forward |
| `Ctrl+R`, `Ctrl+F`, `Ctrl++/-/0` | WebView2 built-in reload, find and zoom shortcuts |
| `Alt+F4` | Close |

There is no caption or tab strip. Use Winarchy's window management or Windows' system menu (`Alt+Space`) to move the window. The native editor and suggestions temporarily reserve space above the page instead of allocating another WebView. `target=_blank` currently navigates the same window; popup-based OAuth flows may not work. A second executable launch is not forwarded to a shared host yet: use **one instance** for prototype testing.

## Blocking scope and security

Implemented:

- EasyList/EasyPrivacy network rules, resource types, request methods, engine exceptions and exact-host user exceptions;
- document/frame/worker interception via WebView2, returning an empty 403 for blocked requests;
- top-level navigation is not subjected to strict document blocking;
- source context uses the request Referer, falling back to the current top-level destination; reduced/missing Referer and concurrent navigation limit attribution accuracy;
- site-specific CSS hiding after top-level navigation completes.

Not implemented: generic DOM/class-based cosmetic lookup, subframe cosmetic injection, procedural filters, scriptlets, redirect resources or rewritten URLs. Cosmetic rules may briefly flash before navigation completes, and SPA changes are not monitored. Do not assume parity with uBlock or reliable blocking of in-stream video ads. Worker attribution and real-site breakage still need Windows testing.

The page has no host-object/IPC bridge; web messaging is disabled. GPU, sandbox, site isolation and TLS error handling keep runtime defaults. Permissions are explicitly denied pending native permission UI (microphone/camera/geolocation will not work). Downloads and script dialogs retain WebView2's default UI and require desktop validation. There is no Proton Pass integration or extension loading yet.

## Deterministic Windows acceptance test

Use a disposable `WINARCHY_CONFIG_HOME` and copy `tests/browser/custom.txt` to its `browser/custom.txt` (the custom list alone is sufficient). This changes configuration only, not the WebView profile path.

```powershell
python -m http.server 8765 --bind 127.0.0.1 --directory tests/browser
# In another terminal:
.\target\release\winarchy-browser.exe http://127.0.0.1:8765/index.html
```

Check:

1. The page reports the ad blocked and the exception allowed; the red placeholder disappears.
2. DevTools Network and the server log confirm blocked requests do not reach the server.
3. `Ctrl+L` works while a page input is focused and during page loading; Escape returns focus.
4. `:block` disables both network and cosmetic filtering on reload; repeat to re-enable.
5. Close/reopen and confirm the exception persists. Check cookies/login persistence separately.
6. Check back/forward, built-in shortcuts, resize, minimize, restore, DPI changes, light/dark themes and closing during engine initialization.
7. Test nested cross-origin frames, worker fetches, sites with no Referer, redirects and a few real ad-heavy sites. These are **not covered by the local fixture**.
8. Test downloads, TLS failures and denied permissions. Check popup policy with a real authentication flow.

An automated UI smoke test is available after starting the fixture server:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/test-browser-desktop.ps1 `
  -Exe "$PWD/target/release/winarchy-browser.exe"
```

It uses disposable configuration and profile directories, checks home opacity, fuzzy matching, navigation, opaque page rendering mode, history and bookmark persistence, and closes only its own process. It invokes the queued bookmark action directly, without injecting global keystrokes; manually verify the actual `Ctrl+D` accelerator as well. It retains its temporary logs for diagnosis.

## Performance protocol

```powershell
powershell -ExecutionPolicy Bypass -File scripts/measure-browser.ps1 `
  -Url http://127.0.0.1:8765/index.html -Output browser-local.csv
```

The script samples the host and descendants, not just the small Rust process. It records private commit, summed working set, CPU time of live processes and process count. Working-set sums **double-count shared pages**; these are not unique physical-RAM totals. PID ancestry and CPU sums are diagnostic approximations, not ETW traces. Close other instances first so an existing shared runtime does not distort accounting.

The startup log records elapsed time from Rust `run()` entry to filter readiness, visible window, WebView readiness and navigation completion. This excludes process creation overhead and does **not** measure first paint or interactivity. Measure those separately with Windows tracing/page instrumentation. There are no performance claims based solely on a blank native window.

Run at least ten repetitions and report median/p95, Windows/runtime version, CPU/RAM and page URL:

- first profile creation and filter compilation (separate category);
- cold launch after reboot versus warm relaunch;
- native home (no URL or `about:blank`), local fixture, simple real page, heavy web app;
- real lists with blocking enabled versus exact-host disabled. This isolates filtering effects but is **not a no-engine baseline**; a plain WebView2 harness is still needed for total blocker overhead;
- stable idle at 5/15/60 seconds, minimized/restored, and ten open/close cycles;
- all associated runtime processes eventually exit after normal closure. Report residual processes rather than forcibly killing them to hide the result.

No preloader, startup resident, polling timer, preloaded site, software-rendering override or manual working-set trimming is introduced. The single WebView is hidden while the native home surface is shown. One WebView per host. No suspension or reduced-memory mode until measurements demonstrate a benefit without breaking calls/audio/background work.

## Validation performed

- `cargo test -p winarchy-browser --locked`: six tests pass (DuckDuckGo/address parsing, network/cosmetic rules, fixture rules, cache invalidation, persisted exceptions, fuzzy matching and merged/bounded history/bookmark persistence).
- Clippy with warnings denied for the browser's Linux, Windows GNU and Windows MSVC targets passes; Windows GNU release linking succeeds. GNU builds additionally require `WebView2Loader.dll` from the matching `webview2-com-sys` package next to the executable; the documented MSVC/release-CI build uses the static loader.
- Workspace formatting and the CLI dependency-boundary check pass.
- Both PowerShell scripts parse; the updater successfully downloads upstream lists into a temporary config. Loading these lists, restoring their cache and matching a known blocked/allowed URL were smoke-tested on Linux.
- The full workspace test run stops on the unchanged `theme_picker::loader::tests::parallel_render_preserves_paint_order_and_reuses_frames` timeout, including with one test thread. No unrelated theme-picker code was changed.
- The home/history/bookmark desktop smoke script passes on Windows via WSL interop. The native frame inset was also verified to be zero on Windows. The broader desktop acceptance matrix, the measurement script's process sampling and meaningful startup/RAM comparisons remain **unverified**.

## Status / remaining gates

Implemented: native host, translucent home, DuckDuckGo/address editor, local fuzzy suggestions, persistent history/bookmarks/profile, startup theme, native filtering, basic cosmetics, persistent exceptions, filter provisioning, fixtures, unit tests and measurement script.

Still required before declaring the V1 validated:

- execute the desktop acceptance matrix on Windows and measure real memory/startup;
- compare to a bare WebView2 host, quantify interception latency and palette responsiveness;
- decide whether interception/source-context limitations meet the blocking requirement;
- add user-facing permission controls and a visible blocking-state indicator;
- shared single-host multi-window lifecycle, only if the V1 actually needs multiple windows;
- optional generic cosmetics and memory/suspension experiments after profiling;
- evaluate Proton Pass separately, later.

## Upstream attribution

`adblock-rust` 0.13.3 is maintained by Brave and licensed MPL-2.0. Winarchy uses the unmodified crate. Source and license: <https://github.com/brave/adblock-rust/tree/v0.13.3> and <https://crates.io/crates/adblock/0.13.3>. Other Rust dependencies retain their own licenses; the root MIT license does not replace them.

EasyList and EasyPrivacy are maintained by the EasyList community. They are downloaded unmodified from <https://easylist.to/easylist/easylist.txt> and <https://easylist.to/easylist/easyprivacy.txt>, with upstream headers retained. Licensing information: <https://easylist.to/pages/licence.html> (GPLv3-or-later or CC BY-SA 3.0). No upstream list is embedded in the executable or checked into this repository. Review these terms when redistributing lists or packaged binaries.
