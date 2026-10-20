#!/usr/bin/env python3
"""Builds ui/fonts/IlliumMono-{Regular,Bold}.ttf: JetBrains Mono plus the
Japanese glyphs of Noto Sans JP (JIS X 0208 repertoire), renamed because both
OFL licenses reserve their font names. Slint's software renderer has no glyph
fallback, so the Japanese glyphs must live in the font every text uses.

Usage: build-illium-mono.py 'NotoSansJP[wght].ttf' [ui/fonts]
Needs fonttools (pip install fonttools). Noto Sans JP variable TrueType:
https://github.com/google/fonts/tree/main/ofl/notosansjp
"""
import sys
from pathlib import Path

from fontTools.merge import Merger
from fontTools.subset import Options, Subsetter
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont

FAMILY = "Illium Mono"
WEIGHTS = {"Regular": 400, "Bold": 700}


def japanese_repertoire(already):
    """Code points of JIS X 0208 (what strict Shift_JIS encodes) not in `already`."""
    chars = set()
    for cp in range(0x80, 0x10000):
        ch = chr(cp)
        try:
            ch.encode("shift_jis")
        except UnicodeEncodeError:
            continue
        chars.add(cp)
    # Half-width katakana are JIS X 0201 but common in network names.
    chars.update(range(0xFF61, 0xFFA0))
    return sorted(chars - already)


def main():
    noto_path = Path(sys.argv[1])
    out_dir = Path(sys.argv[2] if len(sys.argv) > 2 else "ui/fonts")
    for style, weight in WEIGHTS.items():
        base_path = out_dir / f"JetBrainsMono-{style}.ttf"
        base = TTFont(base_path)
        covered = set(base.getBestCmap())

        noto = instantiateVariableFont(TTFont(noto_path), {"wght": weight}, inplace=False)
        options = Options()
        options.layout_features = []  # no OpenType layout in the merged font
        options.name_IDs = []
        options.notdef_outline = False
        options.hinting = False
        options.recalc_bounds = True
        subsetter = Subsetter(options)
        subsetter.populate(unicodes=japanese_repertoire(covered))
        subsetter.subset(noto)
        # Only the outlines, metrics and cmap are merged; vertical metrics,
        # layout, hinting and variation leftovers would not merge.
        keep = {"glyf", "loca", "hmtx", "hhea", "head", "maxp", "cmap", "name", "post", "OS/2"}
        for tag in [t for t in noto.keys() if t not in keep and t != "GlyphOrder"]:
            del noto[tag]
        noto_path_tmp = out_dir / f".noto-{style}.ttf"
        noto.save(noto_path_tmp)

        merger = Merger()
        merged = merger.merge([str(base_path), str(noto_path_tmp)])
        noto_path_tmp.unlink()

        name = merged["name"]
        full = f"{FAMILY} {style}"
        postscript = f"IlliumMono-{style}"
        for record in list(name.names):
            if record.nameID in (16, 17):
                name.removeNames(nameID=record.nameID)
        for platform, encoding, language in ((3, 1, 0x409), (1, 0, 0)):
            name.setName(FAMILY, 1, platform, encoding, language)
            name.setName(style, 2, platform, encoding, language)
            name.setName(f"{FAMILY}:{style}", 3, platform, encoding, language)
            name.setName(full, 4, platform, encoding, language)
            name.setName(postscript, 6, platform, encoding, language)
        merged["OS/2"].usWeightClass = weight
        merged["OS/2"].fsSelection = (merged["OS/2"].fsSelection & ~0x61) | (0x20 if style == "Bold" else 0x40)
        merged["head"].macStyle = 1 if style == "Bold" else 0
        # Keep JetBrains Mono's line metrics; the CJK glyphs fit inside them.
        out = out_dir / f"IlliumMono-{style}.ttf"
        merged.save(out)
        check = TTFont(out)
        print(f"{out}: {len(check.getGlyphOrder())} glyphs, {out.stat().st_size // 1024} KiB")


if __name__ == "__main__":
    main()
