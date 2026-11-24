"""Draws the app icon (1024 px) and builds packaging/AppIcon.icns.
A blue rounded square; on it the reader's white screen with one word between
the guide lines, the recognition letter on the ticks, and an amber marker."""
import os, subprocess, sys
from PIL import Image, ImageDraw, ImageFilter, ImageFont

here = os.path.dirname(os.path.abspath(__file__))
S = 1024
img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
# Background: vertical blue gradient in a rounded square (macOS icon grid).
grad = Image.new("RGBA", (S, S))
gd = ImageDraw.Draw(grad)
for y in range(S):
    t = y / S
    gd.line([(0, y), (S, y)], fill=(int(46 + (20 - 46) * t), int(118 + (60 - 118) * t), int(196 + (120 - 196) * t), 255))
mask = Image.new("L", (S, S), 0)
ImageDraw.Draw(mask).rounded_rectangle([80, 80, S - 80, S - 80], radius=190, fill=255)
img.paste(grad, (0, 0), mask)
# The screen, with a soft shadow.
shadow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
ImageDraw.Draw(shadow).rounded_rectangle([190, 300, S - 190, 760], radius=40, fill=(0, 0, 0, 110))
img.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(18)))
d = ImageDraw.Draw(img)
d.rounded_rectangle([180, 280, S - 180, 740], radius=40, fill=(255, 255, 255, 255))
# Guide lines with their ticks at the recognition letter.
grey = (150, 150, 150, 255)
cx = 470
for y, down in [(380, True), (640, False)]:
    d.line([(230, y), (S - 230, y)], fill=grey, width=6)
    d.line([(cx, y), (cx, y + (34 if down else -34))], fill=grey, width=6)
font = ImageFont.truetype(os.path.join(here, "..", "assets", "fonts", "atkinson-b.ttf"), 200)
word = "read"
# Put the second letter ("e") on the ticks.
eb = d.textbbox((d.textlength("r", font=font), 0), "e", font=font)  # the "e" as placed after "r"
x = cx - (eb[0] + eb[2]) / 2
d.text((x, 395), word, font=font, fill=(25, 25, 30, 255))
# The amber "current" marker, as for list items and table cells.
d.rounded_rectangle([230, 476, 268, 514], radius=8, fill=(245, 197, 66, 255))
out = os.path.join(here, "icon-1024.png")
img.save(out)
# .icns via iconutil.
iconset = os.path.join(here, "AppIcon.iconset")
os.makedirs(iconset, exist_ok=True)
for size in [16, 32, 128, 256, 512]:
    for scale in [1, 2]:
        px = size * scale
        name = f"icon_{size}x{size}{'@2x' if scale == 2 else ''}.png"
        img.resize((px, px), Image.LANCZOS).save(os.path.join(iconset, name))
subprocess.run(["iconutil", "-c", "icns", iconset, "-o", os.path.join(here, "AppIcon.icns")], check=True)
print("packaging/AppIcon.icns")
