#!/usr/bin/env python3
"""Draws Frontier's icon.png (128x128): a round map with hills, a river, a trail and a pin."""
import math, struct, zlib, sys

N = 128
S = 4  # supersampling per axis

def noise(x, y):
    v = 0.0
    for f, a in ((0.031, 1.0), (0.067, 0.5), (0.143, 0.25)):
        v += a * math.sin(x * f * 1.7 + math.cos(y * f * 1.3) * 2.1) * math.cos(y * f * 1.9 - math.sin(x * f) * 1.4)
    return v

def lerp(a, b, t):
    return tuple(a[i] + (b[i] - a[i]) * t for i in range(len(a)))

def over(dst, src, a):
    return lerp(dst, src, max(0.0, min(1.0, a)))

def sample(x, y):
    cx = cy = N / 2
    dx, dy = x - cx, y - cy
    r = math.hypot(dx, dy)
    col, alpha = (0, 0, 0), 0.0
    if r < 61:
        h = noise(x, y) + (cy - y) * 0.012
        low, high = (88, 140, 74), (206, 196, 150)
        col = lerp(low, high, max(0.0, min(1.0, (h + 0.6) / 1.6)))
        # shading from the upper left
        e = noise(x + 1, y) - noise(x - 1, y)
        s = noise(x, y + 1) - noise(x, y - 1)
        lit = 1.0 - 1.6 * (e * 0.6 + s * 0.6)
        col = tuple(min(255, c * max(0.7, min(1.25, lit))) for c in col)
        # snow on the highest hills
        if h > 0.62:
            col = over(col, (240, 242, 236), (h - 0.62) * 6)
        # river
        rv = 64 + 18 * math.sin(y * 0.07) + 6 * math.sin(y * 0.19)
        d = abs(x - rv - (y - 64) * 0.25)
        if d < 4.5:
            col = over(col, (52, 112, 176), (4.5 - d) * 1.2)
        # dotted trail
        t = (x - 22) / 80
        if 0 <= t <= 1:
            ty = 98 - 70 * t + 10 * math.sin(t * 6.3)
            if abs(y - ty) < 1.6 and int(x / 5) % 2 == 0:
                col = over(col, (92, 52, 30), 0.9)
        alpha = 1.0
    # rim
    if 58 <= r < 64:
        ring = (42, 38, 34) if r < 59.5 or r >= 62.5 else (214, 168, 74)
        col, alpha = ring, 1.0
    # compass ticks
    if 52 < r < 58:
        ang = math.degrees(math.atan2(dy, dx)) % 90
        if min(ang, 90 - ang) < 2.2:
            col, alpha = (42, 38, 34), 1.0
    # the pin: a red drop at the trail's end
    px, py = 96, 34
    pd = math.hypot(x - px, y - py + 6)
    tip = (y > py - 6) and abs(x - px) < (py + 10 - y) * 0.55
    if pd < 11 or tip:
        col, alpha = ((20, 14, 12) if pd > 9 and not tip else (214, 52, 44)), 1.0
        if tip and abs(x - px) > (py + 10 - y) * 0.55 - 1.6:
            col = (20, 14, 12)
        if pd < 4:
            col = (250, 236, 220)
    return col, alpha

rows = []
for j in range(N):
    row = bytearray([0])
    for i in range(N):
        acc, aa = [0.0, 0.0, 0.0], 0.0
        for sj in range(S):
            for si in range(S):
                c, a = sample(i + (si + 0.5) / S, j + (sj + 0.5) / S)
                for k in range(3):
                    acc[k] += c[k] * a
                aa += a
        n = S * S
        if aa > 0:
            row += bytes(int(round(acc[k] / aa)) for k in range(3)) + bytes([int(round(255 * aa / n))])
        else:
            row += bytes(4)
    rows.append(bytes(row))

def chunk(t, d):
    return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", N, N, 8, 6, 0, 0, 0)) \
    + chunk(b"IDAT", zlib.compress(b"".join(rows), 9)) + chunk(b"IEND", b"")
out = sys.argv[1] if len(sys.argv) > 1 else "icon.png"
open(out, "wb").write(png)
print(out, len(png), "bytes")
