# Theme picker reference

The picker ports Omarchy's `shell/plugins/image-picker/ImagePicker.qml` and
`ImagePickerModel.js` at revision `86a2e5830eae4d660a66df8cf37f0a35bf4fe8a2`
(PR https://github.com/omacom/omarchy/pull/6231). See
`docs/licenses/omarchy.md` for the upstream notice. This is a visual port, not a
redesign. Illium also reuses this labeled/filterable carousel for the active
theme's wallpapers, displaying exact filenames and confirming through the
existing wallpaper command. This deliberately uses the theme selector's variant,
not Omarchy's label-free background-switcher invocation. No duplicate Slint
surface, renderer or live wallpaper preview is introduced.

## Contract (default Omarchy style, logical pixels)

- Transparent full-monitor overlay; background-colored scrim, alpha 0.5.
- Selected image 768×475; side slices 108×432; spacing −30 (step 78).
- Parallelogram vertices `(28,0), (width,0), (width−28,height), (0,height)`.
  Center-cropped images, **not** sheared images or 3D transforms.
- Selected outline: accent, 3px. Other outlines: foreground, alpha .28, 1px.
  Other images are tinted with background at alpha .42.
- Carousel width 1782. Card width min(screen width−80, 1822), height 609.
  Card centered on screen, carousel starts 30px below its top. Selected card
  centered horizontally. Side cards centered vertically; selected drawn last,
  nearer slices drawn over farther ones. Only relative positions −16…16 shown;
  cards entirely outside the monitor (including their border) are not prepared.
  Carousel can overflow the card and is clipped only by the monitor.
- Label: centered, 768px wide, 16px below carousel, 24px semibold; right elision.
  Filter: 8px below label, 14px, .85 opacity, hidden when empty. Both have a
  background-colored .7-alpha outline. No permanent input box or caret.
  The QML does not set a font family. Omarchy's GTK Qt platform theme and
  `default/fontconfig/conf.avail/50-omarchy.conf` resolve the default sans family
  to Liberation Sans. The picker embeds unmodified Liberation Sans 2.1.5 Regular
  and Bold from Debian `fonts-liberation` 1:2.1.5-3 (SIL OFL 1.1; notice in
  `docs/licenses/liberation-fonts.md`). Only picker labels explicitly use this
  family. Record the resolved reference font when comparing custom installations.
- No animation, arrows, counter, badges, help, buttons, hover selection, wheel
  navigation, drag navigation, shadows, rounded corners or empty placeholders.

## Interaction

Open at the applied theme. Left/right and Shift+Tab/Tab cycle through matches.
Type to filter by case-insensitive substring of the identifier or title-cased
identifier (hyphens/underscores become spaces); do not use launcher's fuzzy
search. Retain a matching selection, otherwise select the first match.
Backspace deletes a character, Ctrl+Backspace a whitespace-delimited word,
Ctrl+U clears. Escape clears a nonempty filter before closing. No matches shows
`No matches`; Enter without a valid choice cancels. Enter or clicking the
selected card confirms; clicking another selects. Rectangular card hit areas
follow upstream, including transparent corners. A click outside both those
areas and the centered card container cancels; container whitespace does not.
Navigation never applies or saves a theme. Confirmation uses `Command::Theme`.

## Windows/data adaptations

Same-process dedicated Slint surface, tool window, full active monitor (not work
area), foreground keyboard focus with restoration. Logical coordinates map to
physical DPI; no responsive redesign. PNG/JPEG `themes/<id>/preview.*` takes
priority over the first alphabetically sorted wallpaper. No asset means no
card. At the owner's subsequent request, the two built-in themes include the
corresponding Omarchy previews and wallpapers with pinned provenance; other
packs can supply their own images without rebuilding Illium.

Decode/resize/masking happen outside the UI thread. No downloads, shell
scripts, videos, configuration schema extension or new UI dependencies in CLI.
Bounded caches and generation checks prevent superseded loads from changing the
current view. Asset edits must not restart providers or reload application lists.

## Opening performance

- Retain up to two initial views (theme picker and active-theme wallpapers),
  bounded together to 64 MiB of CPU frames. A matching view is populated before
  showing the window. Its key includes configuration location, picker mode,
  applied selection, logical dimensions, physical width/DPI and palette colors.
- Revalidate the catalog asynchronously on every opening. Cached pixels are not
  authorization to apply an old selection: Enter/click confirmation waits for
  validation, and a removed or unreadable target cannot confirm its replacement.
  Asset notifications invalidate closed/preloaded views too.
- After a two-second startup grace period, idle checks every 500 ms prewarm both
  initial views, once per context. No prewarm starts while a picker/launcher is
  open or the desktop wallpaper is loading. Prewarming never shows/focuses the
  window or changes preferences. Interactive requests supersede background work;
  an in-flight image decode is not interruptible, but cancellation is checked
  before decoding and before raster preparation.
- Prepare the selected image first, then neighboring pairs with at most two
  simultaneous image decodes. Paint order remains unchanged. Cumulative partial
  results allow the center to appear before all neighbors are ready; cards with
  no pixels yet are not clickable. Completion/confirmation remains generation
  checked. Cache hits do not spawn extra threads.
- Persist lossless 1536×864 RGBA thumbnails in
  `%LOCALAPPDATA%\illium\picker-thumbnails-v1` on Windows (under
  `$XDG_CACHE_HOME/illium/`, or `~/.cache/illium/`, for Linux CPU tests).
  Cache identities include the absolute source path, source size/mtime and
  thumbnail format version. Full identities and exact file lengths are checked
  on read; writes use temporary files. Missing, truncated or unwritable cache
  files fall back to the original PNG/JPEG. Originals are never modified.
  The disk cache is pruned by oldest write time to 256 MiB after writes and may
  be deleted while Illium is stopped. It is outside the watched config tree.
- Existing RAM thumbnail, raster and Slint-image caches each remain bounded to
  64 MiB; initial-view retention is separately bounded to 64 MiB, with shared
  frames rather than duplicated CPU pixels. An in-flight view is capped at
  256 MiB. Original image decode buffers are additional transient memory, now
  limited to two concurrent decodes.

### Measuring

Build/run the CPU benchmark in release mode:

```text
cargo run -p illium --release --example profile_picker -- <config-home>
cargo run -p illium --release --example profile_picker -- <config-home> <wallpaper-theme>
```

It reports catalog time, first available CPU pixels and all-card preparation
for a 1920×1080, 96-DPI view centered on the middle catalog entry. Each invocation
compares empty RAM caches with a second, RAM-warm render. The disk cache is used
normally; use a disposable `LOCALAPPDATA`/`XDG_CACHE_HOME` directory to compare
empty-disk and disk-warm runs without clearing your real cache. Only cache files
are written; no theme or wallpaper is applied.

These timings exclude Slint/native window creation, UI polling, buffer uploads
and Windows compositing. Validate real shortcut-to-visible latency on Windows
in a release build, both immediately after startup and after idle prewarming,
then on repeated openings. Also check progressive clicks/Enter, closing during
preload, edits while closed, theme changes, and monitor/DPI changes. Linux unit
tests and Windows cross-compilation do not establish native display latency.

## Visual acceptance

Compare the pinned QML and the Windows picker with identical images, labels,
font, colors and logical monitor size. Capture center/start/end selections,
multiple/one/zero filter matches, long labels, light/dark colors, 100/125/150/200%
DPI and small monitors. Overlay screenshots and inspect image crop, vertices,
Z-order, alpha, border width, text metrics and hit areas. Only explainable text
and edge rasterization differences may remain; record any unverified Windows
checks rather than claiming pixel identity from a successful build.
