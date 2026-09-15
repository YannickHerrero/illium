# Theme picker reference

The picker ports Omarchy's `shell/plugins/image-picker/ImagePicker.qml` and
`ImagePickerModel.js` at revision `86a2e5830eae4d660a66df8cf37f0a35bf4fe8a2`
(PR https://github.com/omacom/omarchy/pull/6231). See
`docs/licenses/omarchy.md` for the upstream notice. This is a visual port, not a
redesign; the wallpaper menu is outside its scope.

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
  nearer slices drawn over farther ones. Only relative positions −16…16 shown.
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
card; no previews are shipped in this change (the owner supplies them later).

Decode/resize/masking happen outside the UI thread. No downloads, shell
scripts, videos, configuration schema extension or new UI dependencies in CLI.
A bounded cache and generation checks prevent stale loads from changing the
current view. Asset edits must not restart providers or reload application lists.

## Visual acceptance

Compare the pinned QML and the Windows picker with identical images, labels,
font, colors and logical monitor size. Capture center/start/end selections,
multiple/one/zero filter matches, long labels, light/dark colors, 100/125/150/200%
DPI and small monitors. Overlay screenshots and inspect image crop, vertices,
Z-order, alpha, border width, text metrics and hit areas. Only explainable text
and edge rasterization differences may remain; record any unverified Windows
checks rather than claiming pixel identity from a successful build.
