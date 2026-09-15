# catppuccin-latte — Omarchy preview and wallpapers

Source: https://github.com/omacom/omarchy/tree/86a2e5830eae4d660a66df8cf37f0a35bf4fe8a2/themes/catppuccin-latte

Pinned revision: `86a2e5830eae4d660a66df8cf37f0a35bf4fe8a2` (the same revision as Winarchy's picker reference).
Winarchy palette/ANSI colors are unchanged; only image assets are imported.
`catppuccin` in Omarchy corresponds to Winarchy's `catppuccin-mocha`.

The preview is an unchanged screenshot of **Omarchy**, not a screenshot or a
live rendering of Winarchy. Lock-screen previews and unlock icons are excluded.
All files from the theme's `backgrounds/` folder are included, including the
full-resolution Omarchy-branded wallpaper (not a standalone UI icon).

WebP wallpapers are converted to PNG because Winarchy decodes PNG/JPEG only.
Pillow 11.1.0 was used to decode to RGBA and save PNG, with no resizing, cropping
or additional lossy compression. Dimensions and decoded RGBA pixels were
verified against the WebP originals. Existing losses in a lossy WebP cannot be
reversed. The preview PNG is copied byte-for-byte.

## Attribution and rights

The Omarchy repository provides the MIT notice reproduced in `LICENSE`
(copyright David Heinemeier Hansson). Catppuccin is the upstream palette project:
https://github.com/catppuccin/catppuccin. These images retain their original
content and branding. No separate artwork-specific credits/license files were
present in these theme directories at this revision. The repository's license
is not a claim that Winarchy owns third-party artwork or brands depicted in
wallpapers/screenshots (including Totoro in the dark theme). Do not infer a new
license for those underlying works from Winarchy's own MIT license; obtain any
additional permissions needed for redistribution of third-party imagery.

Upstream LICENSE SHA-256: `717ba1949502290f8e47688ae2e323acd06c8ca47aec9f7596b15f678c1af4a2`.

## Image provenance

### `preview.png`

- Upstream: `themes/catppuccin-latte/preview.png`
- Dimensions: 1800 × 1012
- Source SHA-256: `82bd7506b370214407cdfef1f324555b122d290249a03e80462dda0d7ca69360`
- Installed PNG SHA-256: `82bd7506b370214407cdfef1f324555b122d290249a03e80462dda0d7ca69360`

### `wallpapers/1-color-fade.png`

- Upstream: `themes/catppuccin-latte/backgrounds/1-color-fade.webp`
- Dimensions: 1536 × 1024
- Source SHA-256: `c4cb211770713984e630ef6eebfb6cfb03169124c98765fd52f82ee0bfd36603`
- Installed PNG SHA-256: `a476592bb5bdfe994275ddfd72a9b22a0d08df9f472bd20d7fd945176ab5f046`

### `wallpapers/omarchy.png`

- Upstream: `themes/catppuccin-latte/backgrounds/omarchy.webp`
- Dimensions: 3840 × 2160
- Source SHA-256: `8482e189755138084295e90f353b8efe8d3e575d7bc3fc3ed756d1e890242d32`
- Installed PNG SHA-256: `b8994507b9fadf62906ce37e55cccac101115547c89d2cd7743d1eebae94f49d`

