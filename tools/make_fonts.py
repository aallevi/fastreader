"""Build the subset fonts embedded in the firmware.

Downloads live in assets/fonts-src (not committed). Output: assets/fonts/*.ttf,
static instances (variable fonts pinned to Regular/Bold) subset to Latin-1 plus
typographic punctuation, so each file is ~20–60 KB instead of 100–900 KB.

    python3 tools/make_fonts.py
"""
from pathlib import Path
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer
from fontTools import subset

SRC = Path("assets/fonts-src")
OUT = Path("assets/fonts")
OUT.mkdir(parents=True, exist_ok=True)

UNICODES = list(range(0x20, 0x7F)) + list(range(0xA0, 0x180)) + [
    0x2010, 0x2011, 0x2012, 0x2013, 0x2014, 0x2015, 0x2018, 0x2019, 0x201A, 0x201C, 0x201D, 0x201E,
    0x2020, 0x2021, 0x2022, 0x2026, 0x2030, 0x2032, 0x2033, 0x2039, 0x203A, 0x20AC, 0x2122, 0x2212,
]

# (output name, source file, variable-axis pins or None)
FONTS = [
    ("atkinson-r", "AtkinsonHyperlegible-Regular.ttf", None),
    ("atkinson-b", "AtkinsonHyperlegible-Bold.ttf", None),
    ("atkinson-i", "AtkinsonHyperlegible-Italic.ttf", None),
    ("atkinson-bi", "AtkinsonHyperlegible-BoldItalic.ttf", None),
    ("literata-r", "LiterataVar.ttf", {"wght": 400, "opsz": 24}),
    ("literata-b", "LiterataVar.ttf", {"wght": 700, "opsz": 24}),
    ("literata-i", "LiterataItalicVar.ttf", {"wght": 400, "opsz": 24}),
    ("literata-bi", "LiterataItalicVar.ttf", {"wght": 700, "opsz": 24}),
    ("plexsans-r", "PlexSansVar.ttf", {"wght": 400, "wdth": 100}),
    ("plexsans-b", "PlexSansVar.ttf", {"wght": 600, "wdth": 100}),
    ("plexsans-i", "PlexSansItalicVar.ttf", {"wght": 400, "wdth": 100}),
    ("plexsans-bi", "PlexSansItalicVar.ttf", {"wght": 600, "wdth": 100}),
    ("plexserif-r", "IBMPlexSerif-Regular.ttf", None),
    ("plexserif-b", "IBMPlexSerif-Bold.ttf", None),
    ("plexserif-i", "IBMPlexSerif-Italic.ttf", None),
    ("plexserif-bi", "IBMPlexSerif-BoldItalic.ttf", None),
    ("crimson-r", "CrimsonText-Regular.ttf", None),
    ("crimson-b", "CrimsonText-Bold.ttf", None),
    ("crimson-i", "CrimsonText-Italic.ttf", None),
    ("crimson-bi", "CrimsonText-BoldItalic.ttf", None),
    ("courier-r", "CourierPrime-Regular.ttf", None),
    ("courier-b", "CourierPrime-Bold.ttf", None),
    ("courier-i", "CourierPrime-Italic.ttf", None),
    ("courier-bi", "CourierPrime-BoldItalic.ttf", None),
    ("gelasio-r", "GelasioVar.ttf", {"wght": 400}),
    ("gelasio-b", "GelasioVar.ttf", {"wght": 700}),
    ("gelasio-i", "GelasioItalicVar.ttf", {"wght": 400}),
    ("gelasio-bi", "GelasioItalicVar.ttf", {"wght": 700}),
    ("plexmono-r", "IBMPlexMono-Regular.ttf", None),
    ("plexmono-b", "IBMPlexMono-Bold.ttf", None),
    ("selawik-r", "selawik/selawk.ttf", None),
    ("selawik-b", "selawik/selawkb.ttf", None),
]

for name, src, pins in FONTS:
    path = next(SRC.rglob(Path(src).name)) if not (SRC / src).exists() else SRC / src
    font = TTFont(path)
    if pins:
        font = instancer.instantiateVariableFont(font, pins)
    opts = subset.Options()
    opts.layout_features = ["kern"]
    opts.hinting = False
    opts.name_IDs = [0, 1, 2]
    s = subset.Subsetter(opts)
    s.populate(unicodes=UNICODES)
    s.subset(font)
    out = OUT / f"{name}.ttf"
    font.save(out)
    print(f"{out}  {out.stat().st_size // 1024} KB")
