"""Draws the sleepy nightcap for the "Sleepy time" warning: 40x40 pixel art,
48 frames of it turning twice on its tilted, slowly tumbling axis, z's floating
off its tip and turning with it, its dark inside showing as it tips
towards you. Written as colour roles (not colours) so each theme paints it
in its own palette: assets/sprites/hat.bin = b"FRHAT1", frames, w, h, then
frames*w*h role bytes (0 = see-through).

    tools/.venv/bin/python tools/make_hat.py [preview.png]
"""
import math, sys
from pathlib import Path

N, F = 40, 48
TURN = 24  # frames per turn (the z's take two turns to come and go)
# Roles (the painter maps them to a theme's colours).
CLEAR, A, A_HI, A_LO, B, B_LO, FUR, FUR_LO, POM, POM_LO, INSIDE, OUT, ZED = range(13)


def curve(s):
    y = s * 21 - max(0, s - 0.55) ** 2 * 30  # up from the brim, flopping over
    d = max(0, s - 0.35) ** 1.5 * 16
    return d, y


def model():
    """Small balls (pos, normal, radius, part, s) in the hat's own space,
    y up: the cap as a hollow shell (rings around its bent centre line,
    open at the brim), the pom-pom and the fur brim as balls."""
    m = []
    S, A = 64, 30
    for i in range(S + 1):
        s = i / S
        d, y = curve(s)
        d2, y2 = curve(min(1, s + 0.01))
        d1, y1 = curve(max(0, s - 0.01))
        tx, ty = d2 - d1, y2 - y1
        tl = math.hypot(tx, ty) or 1
        tx, ty = tx / tl, ty / tl
        n1 = (-ty, tx, 0.0)  # across the centre line, in its plane
        r = 7.0 * (1 - s) ** 0.9 + 0.5
        for a in range(A):
            ph = a / A * 2 * math.pi
            nx, ny, nz = n1[0] * math.cos(ph), n1[1] * math.cos(ph), math.sin(ph)
            m.append(((d + r * nx, y - 9 + r * ny, r * nz), (nx, ny, nz), 1.35, "cap", s))
    d, y = curve(1.0)
    m.append(((d, y - 10.2, 0.0), None, 2.8, "pom", 1))
    for k in range(40):  # the fur brim
        a = k / 40 * 2 * math.pi
        m.append(((math.cos(a) * 7.6, -9.3, math.sin(a) * 7.6), None, 2.0, "fur", 0))
    return m


M = model()
TIP = (curve(1.0)[0], curve(1.0)[1] - 10.2, 0)
# z's float off the tip, up and away, each smaller (in the hat's space).
ZS = [((TIP[0] + 4, TIP[1] + 4, 0), 5), ((TIP[0] + 8, TIP[1] + 9, 0), 4), ((TIP[0] + 11, TIP[1] + 13, 0), 3)]
GLYPH = {5: ["#####", "...#.", "..#..", ".#...", "#####"], 4: ["####", "..#.", ".#..", "####"], 3: ["###", ".#.", "###"]}


def pose(t):
    spin = 2 * math.pi * t
    tilt = -0.15 + 0.6 * math.sin(2 * math.pi * t)  # rocking: now its top towards us, now its open bottom
    roll = -0.38 + 0.5 * math.sin(2 * math.pi * t)  # the axis swinging: a slow tumble
    return spin, tilt, roll


def rot(p, spin, tilt, roll):
    x, y, z = p
    x, z = x * math.cos(spin) - z * math.sin(spin), x * math.sin(spin) + z * math.cos(spin)
    y, z = y * math.cos(tilt) - z * math.sin(tilt), y * math.sin(tilt) + z * math.cos(tilt)
    x, y = x * math.cos(roll) - y * math.sin(roll), x * math.sin(roll) + y * math.cos(roll)
    return x, y, z


def frame(k):
    t = (k % TURN) / TURN
    spin, tilt, roll = pose(t)
    cx, cy = N / 2 - 3 + 1.2 * math.sin(2 * math.pi * t), N / 2 + 3 + 1.5 * math.sin(4 * math.pi * t)
    sc = 0.82
    L = (-0.55, -0.6, 0.58)  # light from the upper left, towards us
    px = [[CLEAR] * N for _ in range(N)]
    depth = [[-1e9] * N for _ in range(N)]
    for (p, n, r, part, s) in M:
        X, Y, Z = rot(p, spin, tilt, roll)
        sx, sy, sz, rr = cx + X * sc, cy - Y * sc, Z * sc, r * sc
        nn = rot(n, spin, tilt, roll) if n else None
        for py in range(int(sy - rr) - 1, int(sy + rr) + 2):
            for qx in range(int(sx - rr) - 1, int(sx + rr) + 2):
                if not (0 <= qx < N and 0 <= py < N):
                    continue
                dx, dy = qx + 0.5 - sx, py + 0.5 - sy
                q = dx * dx + dy * dy
                if q > rr * rr:
                    continue
                nz = math.sqrt(rr * rr - q)
                dd = sz + nz
                if dd <= depth[py][qx]:
                    continue
                depth[py][qx] = dd
                if nn:  # the shell: lit by its own surface; its inside is dark
                    ux, uy, uz = nn[0], -nn[1], nn[2]
                    if uz < -0.05:
                        px[py][qx] = INSIDE
                        continue
                else:
                    ux, uy, uz = dx / rr, dy / rr, nz / rr
                light = ux * L[0] + uy * L[1] + uz * L[2]
                hi, lo = light > 0.62, light < 0.12
                if part == "cap":
                    role = (A_HI if hi else A_LO if lo else A) if int(s * 6.5) % 2 == 0 else (B_LO if lo else B)
                elif part == "pom":
                    role = POM_LO if lo else POM
                else:
                    role = FUR_LO if lo else FUR
                px[py][qx] = role
    # A 1-px outline around the hat.
    src = [row[:] for row in px]
    for y in range(N):
        for x in range(N):
            if src[y][x] == CLEAR and any(0 <= x + a < N and 0 <= y + b < N and src[y + b][x + a] != CLEAR for a, b in ((1, 0), (-1, 0), (0, 1), (0, -1))):
                px[y][x] = OUT
    # z's: they turn with the hat (squeezed as they turn edge-on, mirrored
    # when seen from behind), hidden where the hat is in front of them.
    shown = (k // 12) % 4
    for i in range(shown):
        (p, size) = ZS[i]
        X, Y, Zd = rot(p, spin, tilt, roll)
        zx, zy = cx + X * sc, cy - Y * sc
        g = GLYPH[size]
        face = math.cos(spin)  # how much of the z faces us (negative: its back)
        w = max(math.ceil(size * 0.5), round(size * abs(face)))  # never thinner than half: still a z
        for j, row in enumerate(g):
            for c in range(w):
                src_c = int(c * size / w)
                if face < 0:
                    src_c = size - 1 - src_c
                if row[src_c] == "#":
                    x, y = int(zx - w / 2 + c), int(zy - size / 2 + j)
                    if 0 <= x < N and 0 <= y < N and (depth[y][x] < Zd * sc or px[y][x] in (CLEAR, OUT)):
                        px[y][x] = ZED
    return px


def main():
    frames = [frame(k) for k in range(F)]
    out = Path(__file__).resolve().parent.parent / "assets/sprites/hat.bin"
    out.parent.mkdir(parents=True, exist_ok=True)
    data = bytearray(b"FRHAT1") + bytes([F, N, N])
    for f in frames:
        for row in f:
            data += bytes(row)
    out.write_bytes(data)
    print(out, len(data), "bytes")
    if len(sys.argv) > 1:  # a preview strip, in the Windows 98 colours on grey
        from PIL import Image
        pal = {A: (0, 0, 128), A_HI: (0, 0, 255), A_LO: (0, 0, 64), B: (255, 255, 255), B_LO: (192, 192, 192), FUR: (255, 255, 255), FUR_LO: (160, 160, 160),
               POM: (255, 255, 0), POM_LO: (160, 160, 0), INSIDE: (0, 0, 0), OUT: (0, 0, 0), ZED: (0, 0, 128)}
        im = Image.new("RGB", (F * (N + 2), N), (192, 192, 192))
        for k, f in enumerate(frames):
            for y in range(N):
                for x in range(N):
                    if f[y][x]:
                        im.putpixel((k * (N + 2) + x, y), pal[f[y][x]])
        im.resize((im.width * 3, im.height * 3), Image.NEAREST).save(sys.argv[1])


if __name__ == "__main__":
    main()
