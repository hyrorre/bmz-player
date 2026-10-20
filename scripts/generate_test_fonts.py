#!/usr/bin/env python3
"""Generate the tiny fonts used by bmz-render text tests.

The glyphs are simple polygons drawn by this script, so the outputs contain no
third-party font data. Run with fontTools 4.x:

    python3 -m venv .local/fonttools && .local/fonttools/bin/pip install fonttools
    .local/fonttools/bin/python scripts/generate_test_fonts.py

Outputs are written to crates/bmz-render/testdata/fonts/.
"""

from __future__ import annotations

import struct
import zlib
from pathlib import Path

from fontTools.colorLib.builder import buildCOLR
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import newTable
from fontTools.ttLib.tables import otTables as ot
from fontTools.ttLib.tables import _s_b_i_x, sbixGlyph, sbixStrike

OUT = Path(__file__).resolve().parent.parent / "crates/bmz-render/testdata/fonts"

Point = tuple[float, float]


def polygon_glyph(contours: list[list[Point]]):
    pen = TTGlyphPen(None)
    for contour in contours:
        pen.moveTo(contour[0])
        for point in contour[1:]:
            pen.lineTo(point)
        pen.closePath()
    return pen.glyph()


def rect(x0: float, y0: float, x1: float, y1: float) -> list[Point]:
    return [(x0, y0), (x0, y1), (x1, y1), (x1, y0)]


def octagon(cx: float, cy: float, r: float) -> list[Point]:
    k = r * 0.4142
    return [
        (cx - k, cy - r),
        (cx - r, cy - k),
        (cx - r, cy + k),
        (cx - k, cy + r),
        (cx + k, cy + r),
        (cx + r, cy + k),
        (cx + r, cy - k),
        (cx + k, cy - r),
    ]


def star_of_david(cx: float, cy: float, r: float) -> list[list[Point]]:
    return [
        [(cx, cy + r), (cx + r * 0.866, cy - r * 0.5), (cx - r * 0.866, cy - r * 0.5)],
        [(cx, cy - r), (cx - r * 0.866, cy + r * 0.5), (cx + r * 0.866, cy + r * 0.5)],
    ]


def empty_glyph():
    return TTGlyphPen(None).glyph()


def build_font(
    path: Path,
    family: str,
    upem: int,
    ascent: int,
    descent: int,
    glyphs: dict[str, tuple[list[list[Point]], int]],
    cmap: dict[int, str],
    features: str | None = None,
    setup=None,
) -> None:
    order = [".notdef"] + [name for name in glyphs if name != ".notdef"]
    fb = FontBuilder(upem, isTTF=True)
    fb.setupGlyphOrder(order)
    fb.setupCharacterMap(cmap)
    glyf = {}
    metrics = {}
    for name in order:
        contours, advance = glyphs.get(name, ([], upem // 2))
        glyf[name] = polygon_glyph(contours) if contours else empty_glyph()
        lsb = min((x for contour in contours for x, _ in contour), default=0)
        metrics[name] = (advance, int(lsb))
    fb.setupGlyf(glyf)
    fb.setupHorizontalMetrics(metrics)
    fb.setupHorizontalHeader(ascent=ascent, descent=descent)
    fb.setupNameTable({"familyName": family, "styleName": "Regular"})
    fb.setupOS2(
        sTypoAscender=ascent,
        sTypoDescender=descent,
        usWinAscent=ascent,
        usWinDescent=-descent,
        sCapHeight=int(ascent * 0.85),
    )
    fb.setupPost()
    if features:
        fb.addOpenTypeFeatures(features)
    if setup:
        setup(fb)
    path.parent.mkdir(parents=True, exist_ok=True)
    fb.save(str(path))


def latin_glyphs() -> tuple[dict, dict]:
    glyphs = {
        "space": ([], 300),
        "A": ([rect(50, 0, 550, 700)], 600),
        "B": ([rect(60, 0, 540, 700), rect(200, 300, 400, 400)], 600),
        "S": ([rect(50, 0, 550, 120), rect(50, 290, 550, 410), rect(50, 580, 550, 700)], 600),
        "W": ([rect(40, 0, 760, 700)], 800),
        "o": ([rect(60, 0, 440, 480)], 500),
        "l": ([rect(100, 0, 200, 720)], 300),
        "m": ([rect(50, 0, 750, 480)], 800),
        "n": ([rect(50, 0, 450, 480)], 500),
        "one": ([rect(200, 0, 300, 700)], 500),
        "two": ([rect(50, 0, 450, 700)], 500),
    }
    cmap = {
        0x20: "space",
        ord("A"): "A",
        ord("B"): "B",
        ord("S"): "S",
        ord("W"): "W",
        ord("o"): "o",
        ord("l"): "l",
        ord("m"): "m",
        ord("n"): "n",
        ord("1"): "one",
        ord("2"): "two",
    }
    return glyphs, cmap


def png_rgba(width: int, height: int, pixel) -> bytes:
    rows = b"".join(
        b"\x00" + b"".join(bytes(pixel(x, y)) for x in range(width)) for y in range(height)
    )

    def chunk(kind: bytes, data: bytes) -> bytes:
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(rows, 9))
        + chunk(b"IEND", b"")
    )


def main() -> None:
    # Latin subset without symbols, "?" or box glyphs; .notdef is empty.
    glyphs, cmap = latin_glyphs()
    build_font(OUT / "bmz-test-latin.ttf", "BMZ Test Latin", 1000, 800, -200, glyphs, cmap)

    # "?" only, to check the missing-glyph candidate order.
    glyphs, cmap = latin_glyphs()
    glyphs["question"] = ([rect(150, 0, 350, 120), rect(150, 250, 350, 700)], 500)
    cmap[ord("?")] = "question"
    build_font(OUT / "bmz-test-question.ttf", "BMZ Test Question", 1000, 800, -200, glyphs, cmap)

    # Monochrome symbols with a different em size and line metrics.
    symbols = {
        "u2721": (star_of_david(1024, 700, 800), 2048),
        "u25A1": (
            [rect(200, 0, 1600, 1400), list(reversed(rect(350, 150, 1450, 1250)))],
            1800,
        ),
        "u2660": ([octagon(900, 700, 600)], 1800),
        "u2661": ([octagon(900, 700, 600), list(reversed(octagon(900, 700, 400)))], 1800),
        "u266B": ([rect(300, 0, 600, 1400), rect(1100, 0, 1400, 1400)], 1800),
        "u221E": ([octagon(600, 700, 400), octagon(1300, 700, 400)], 1900),
        "uFFFD": ([[(900, 0), (1700, 700), (900, 1400), (100, 700)]], 1800),
    }
    build_font(
        OUT / "bmz-test-symbols.ttf",
        "BMZ Test Symbols",
        2048,
        1900,
        -500,
        symbols,
        {int(name[1:], 16): name for name in symbols},
    )

    # COLR v0/v1 color glyphs, a monochrome outline for U+2721 and U+2764, and
    # GSUB ligatures for ZWJ, Regional Indicator and skin tone sequences.
    face = octagon(500, 400, 450)
    color = {
        "u1F600": ([face], 1000),
        "eyes": ([rect(300, 450, 400, 600), rect(600, 450, 700, 600)], 1000),
        "u1F3B5": ([rect(300, 0, 450, 750), rect(450, 600, 750, 750)], 1000),
        "u2764": ([octagon(500, 400, 400)], 1000),
        "u2721": (star_of_david(500, 400, 400), 1000),
        "u1F468": ([rect(100, 0, 900, 800)], 1000),
        "u1F469": ([rect(150, 0, 850, 750)], 1000),
        "u1F467": ([rect(200, 0, 800, 600)], 1000),
        "u1F466": ([rect(250, 0, 750, 550)], 1000),
        "u200D": ([], 0),
        "uFE0F": ([], 0),
        "family": ([rect(50, 0, 950, 800)], 1000),
        "u1F1EF": ([rect(100, 0, 900, 700)], 1000),
        "u1F1F5": ([rect(100, 0, 900, 700)], 1000),
        "flag_jp": ([rect(50, 50, 950, 650)], 1000),
        "flag_dot": ([octagon(500, 350, 200)], 1000),
        "u1F44D": ([rect(200, 0, 800, 800)], 1000),
        "u1F3FB": ([rect(300, 0, 700, 400)], 1000),
        "thumbs_light": ([rect(200, 0, 800, 800)], 1000),
    }
    color_cmap = {int(name[1:], 16): name for name in color if name.startswith("u")}
    features = """
languagesystem DFLT dflt;
feature ccmp {
    sub u1F468 u200D u1F469 u200D u1F467 u200D u1F466 by family;
    sub u1F1EF u1F1F5 by flag_jp;
    sub u1F44D u1F3FB by thumbs_light;
} ccmp;
"""

    def setup_colr(fb: FontBuilder) -> None:
        palettes = [[(1.0, 0.8, 0.0, 1.0), (0.1, 0.1, 0.1, 1.0), (0.9, 0.0, 0.1, 1.0),
                     (1.0, 1.0, 1.0, 1.0), (0.0, 0.3, 1.0, 1.0), (0.9, 0.7, 0.5, 1.0)]]
        fb.setupCPAL(palettes)
        solid = ot.PaintFormat.PaintSolid
        layers = ot.PaintFormat.PaintColrLayers
        glyph = ot.PaintFormat.PaintGlyph
        gradient = {
            "Format": ot.PaintFormat.PaintLinearGradient,
            "ColorLine": {"ColorStop": [(0.0, 2), (1.0, 4)]},
            "x0": 300, "y0": 0, "x1": 750, "y1": 750, "x2": 0, "y2": 750,
        }
        # v0 layer records (glyph, palette index) and v1 paint graphs in one table.
        v0 = {
            "u1F600": [("u1F600", 0), ("eyes", 1)],
            "u2764": [("u2764", 2)],
            "flag_jp": [("flag_jp", 3), ("flag_dot", 2)],
        }
        v1 = {
            "u2721": [("u2721", 4)],
            "family": [("family", 4)],
            "thumbs_light": [("thumbs_light", 5)],
        }
        colr = dict(v0)
        colr.update(
            {
                name: (layers, [(glyph, (solid, index), layer) for layer, index in value])
                for name, value in v1.items()
            }
        )
        colr["u1F3B5"] = (glyph, gradient, "u1F3B5")
        fb.font["COLR"] = buildCOLR(colr, glyphMap=fb.font.getReverseGlyphMap())

    build_font(
        OUT / "bmz-test-color.ttf",
        "BMZ Test Color",
        1000,
        900,
        -200,
        color,
        color_cmap,
        features=features,
        setup=setup_colr,
    )

    # sbix PNG strike only (no outlines) for U+1F600.
    def setup_sbix(fb: FontBuilder) -> None:
        def pixel(x: int, y: int) -> tuple[int, int, int, int]:
            if 4 <= x < 28 and 4 <= y < 28:
                return (255, 0, 0, 255) if x < 16 else (0, 0, 255, 255)
            return (0, 0, 0, 0)

        table = _s_b_i_x.table__s_b_i_x()
        strike = sbixStrike.Strike(ppem=32, resolution=72)
        strike.glyphs["u1F600"] = sbixGlyph.Glyph(
            glyphName="u1F600",
            graphicType="png ",
            originOffsetX=0,
            originOffsetY=0,
            imageData=png_rgba(32, 32, pixel),
        )
        table.strikes[32] = strike
        fb.font["sbix"] = table

    build_font(
        OUT / "bmz-test-sbix.ttf",
        "BMZ Test Sbix",
        1000,
        900,
        -200,
        {"u1F600": ([], 1000)},
        {0x1F600: "u1F600"},
        setup=setup_sbix,
    )


if __name__ == "__main__":
    main()
