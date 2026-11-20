#!/usr/bin/env python3
"""Checks that a frame Frontier draws is not a copy of another image.

    cargo test --lib -- --ignored frame_preview      # writes target/frame_*.png
    python3 tools/compare_frame.py target/frame_parchment_square.png OTHER.png

Both images are scaled to the same size; the script prints how many opaque pixels match
closely (each channel within 8) and the average colour distance. Needs Pillow.
"""
import sys
from PIL import Image

ours = Image.open(sys.argv[1]).convert("RGBA")
other = Image.open(sys.argv[2]).convert("RGBA").resize(ours.size, Image.NEAREST)
same = total = 0
dist = 0.0
def px(img):
    raw = img.tobytes()
    return (raw[i:i + 4] for i in range(0, len(raw), 4))


for a, b in zip(px(ours), px(other)):
    if a[3] < 128 and b[3] < 128:
        continue
    total += 1
    d = max(abs(a[i] - b[i]) for i in range(4))
    dist += sum(abs(a[i] - b[i]) for i in range(3)) / 3
    if d <= 8:
        same += 1
print(f"opaque pixels compared: {total}")
print(f"close matches: {same} ({100 * same / max(total, 1):.2f}%)")
print(f"mean colour distance: {dist / max(total, 1):.1f} / 255")
