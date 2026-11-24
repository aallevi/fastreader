"""Writes books/showcase.md: a short book with every reading feature
(pictures, a figure reference, lists, a nested list, a table, a catalogue),
for the README's GIF. Pictures are drawn here, so everything is ours."""
import io
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont
from convert import Images, CARD_BUDGET


def png(im):
    b = io.BytesIO()
    im.save(b, "PNG")
    return b.getvalue()


def chart():
    im = Image.new("RGB", (300, 170), "white")
    d = ImageDraw.Draw(im)
    f = ImageFont.truetype("/System/Library/Fonts/Supplemental/Arial.ttf", 15)
    for k, (name, wpm, col) in enumerate([("Speaking", 150, "#8a9bb0"), ("Reading", 250, "#5b7fa8"), ("Fast Reader", 475, "#e0a020")]):
        y = 22 + k * 48
        d.text((8, y + 6), name, fill="#222", font=f)
        d.rectangle((104, y, 104 + wpm * 0.3, y + 30), fill=col)
        d.text((110 + wpm * 0.3, y + 6), str(wpm), fill="#222", font=f)
    return im


def painting(sky, ground, sun):
    im = Image.new("RGB", (200, 130), sky)
    d = ImageDraw.Draw(im)
    d.ellipse((120, 18, 160, 58), fill=sun)
    d.polygon([(0, 130), (0, 80), (60, 55), (120, 85), (200, 60), (200, 130)], fill=ground)
    return im


imgs = Images(CARD_BUDGET)
marks = [imgs.add_bytes(png(p)) for p in (chart(), painting("#7fb2e5", "#3f7d4a", "#ffffff"), painting("#f2c27a", "#a0452e", "#d8321e"))]
assert all(marks), marks
text = """<!-- title: Everything at once -->
# Everything at once

Fast Reader keeps the **whole** book, not just its *words* (asides in brackets read quietly).

{image:0}
{caption} Figure 1. Words per minute. {/caption}

Pictures come in the stream. Later, as figure 1 showed, a reference brings the figure back below the word.

Packing for a trip:

• a good book
• a jacket
  • the warm one
  • the rain one
• tea

Then three steps:

1. Open the book.
2. Press play.
3. Keep reading.

A table reads cell by cell, at half speed:

| Way | Words a minute |
|---|---|
| Speaking | 150 |
| Reading | 250 |
| Fast Reader | 475 |

## A small catalogue

{image:1}

1. Blue morning, 1910.

{image:2}

2. Red evening, 1925.

The end.
"""
(Path(__file__).parent.parent / "books/showcase.md").write_bytes(text.encode() + imgs.section())
print("books/showcase.md", len(imgs.items), "pictures")
