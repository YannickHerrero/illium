# Illium Mono

`IlliumMono-Regular.ttf` and `IlliumMono-Bold.ttf` are JetBrains Mono
(`JetBrainsMono-*.ttf`, see `JetBrainsMono-SOURCE.md`) merged with the Japanese
repertoire of Noto Sans JP: JIS X 0208 (kana, 6355 kanji, punctuation, full-width
forms) plus half-width katakana, taken from the variable font
`NotoSansJP[wght].ttf` of https://github.com/google/fonts/tree/main/ofl/notosansjp
(downloaded 2026-10-19) instantiated at weights 400 and 700. Both sources are
under the SIL Open Font License 1.1 with reserved font names, hence the new
name; see `JetBrainsMono-OFL.txt` and `NotoSansJP-OFL.txt`.

Rebuild with `scripts/build-illium-mono.py 'NotoSansJP[wght].ttf' ui/fonts`
(needs fonttools). The files are embedded by `illium_theme::fonts`, written to
`%LOCALAPPDATA%\Illium\fonts` at startup and made Slint's default font through
`SLINT_DEFAULT_FONT`, because the software renderer has no glyph fallback: the
Japanese glyphs must be in the font every text uses, applet views included.
