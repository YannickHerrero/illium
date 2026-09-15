# catppuccin-mocha — Omarchy preview and wallpapers

Source: https://github.com/omacom/omarchy/tree/86a2e5830eae4d660a66df8cf37f0a35bf4fe8a2/themes/catppuccin

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

- Upstream: `themes/catppuccin/preview.png`
- Dimensions: 1800 × 1012
- Source SHA-256: `0ee9898c900f40ef83ac785afdb9997e893be22dcd1b70595c77316ee5275792`
- Installed PNG SHA-256: `0ee9898c900f40ef83ac785afdb9997e893be22dcd1b70595c77316ee5275792`

### `wallpapers/1-totoro.png`

- Upstream: `themes/catppuccin/backgrounds/1-totoro.webp`
- Dimensions: 3840 × 2160
- Source SHA-256: `4896da620ed3016b7e52eede8db8169591a9fa4856064db4039521be96f724b6`
- Installed PNG SHA-256: `e8152083df0a7ea64e029012ea9f01b6d43d419e6cfe719d59610ef2d52b2be3`

### `wallpapers/2-waves.png`

- Upstream: `themes/catppuccin/backgrounds/2-waves.webp`
- Dimensions: 3840 × 2160
- Source SHA-256: `5fe347ef9814dbf1d5bc73a96381b8fe4908ed0f7e4293d2c3c8d860b46041ce`
- Installed PNG SHA-256: `d67f03a60f15b323b3cfc665d47aa12690f1cf0358048dcdbc926566f6cd42a1`

### `wallpapers/3-blue-eye.png`

- Upstream: `themes/catppuccin/backgrounds/3-blue-eye.webp`
- Dimensions: 3840 × 2160
- Source SHA-256: `70743c244575253b72f17978060f87239e0125575195f64e288479666129954a`
- Installed PNG SHA-256: `72116b0f65b2bfd68ec35c2a92a374e9ef5556f014ee73c459bdef535060d429`

### `wallpapers/omarchy.png`

- Upstream: `themes/catppuccin/backgrounds/omarchy.webp`
- Dimensions: 3840 × 2160
- Source SHA-256: `24cb9211e2191edefadd093976526d3606167343a636eb9ff123b40d4e594329`
- Installed PNG SHA-256: `95816dd883dea7dfaf8b91d5fdd7e8293f64c09f342a862f6c37c82b36db1fab`

