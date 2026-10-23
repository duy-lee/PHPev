"""Generates assets/phpev.ico (stdlib only): a white "P" on an indigo rounded square.

Run: python assets/make_icon.py
Each size is rendered from the same geometry with 8x8 supersampling and stored
as a PNG entry (Windows Vista+ reads PNG icons at every size).
"""
import os
import struct
import zlib

SIZES = [16, 20, 24, 32, 40, 48, 64, 256]
BG = (0x4F, 0x46, 0xE5)  # --accent of the dashboard
FG = (0xFF, 0xFF, 0xFF)
SS = 8


def in_background(x, y, size):
    # Small icons use (almost) the whole square so they stay legible in the tray.
    margin = 0.0 if size <= 24 else 0.04
    radius = 0.18 if size <= 24 else 0.22
    lo, hi = margin, 1.0 - margin
    if not (lo <= x <= hi and lo <= y <= hi):
        return False
    cx = min(max(x, lo + radius), hi - radius)
    cy = min(max(y, lo + radius), hi - radius)
    return (x - cx) ** 2 + (y - cy) ** 2 <= radius ** 2


def in_glyph(x, y):
    """A geometric "P": stem, two bars and a half ring."""
    stem = 0.28 <= x <= 0.43 and 0.20 <= y <= 0.82
    bars = 0.28 <= x <= 0.50 and (0.20 <= y <= 0.34 or 0.50 <= y <= 0.64)
    d2 = (x - 0.50) ** 2 + (y - 0.42) ** 2
    ring = x >= 0.50 and 0.08 ** 2 <= d2 <= 0.22 ** 2
    return stem or bars or ring


def render(size):
    rows = []
    for py in range(size):
        row = bytearray([0])  # PNG filter: none
        for px in range(size):
            bg = fg = 0
            for sy in range(SS):
                for sx in range(SS):
                    x = (px + (sx + 0.5) / SS) / size
                    y = (py + (sy + 0.5) / SS) / size
                    if in_background(x, y, size):
                        bg += 1
                        if in_glyph(x, y):
                            fg += 1
            n = SS * SS
            alpha = bg / n
            if bg == 0:
                row += bytes(4)
                continue
            t = fg / bg
            rgb = [round(BG[i] * (1 - t) + FG[i] * t) for i in range(3)]
            row += bytes(rgb + [round(alpha * 255)])
        rows.append(bytes(row))
    return png(size, b"".join(rows))


def png(size, raw):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


def main():
    images = [render(s) for s in SIZES]
    out = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    for size, data in zip(SIZES, images):
        dim = 0 if size >= 256 else size
        out += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    out += b"".join(images)
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "phpev.ico")
    with open(path, "wb") as f:
        f.write(out)
    print(f"wrote {path} ({len(out)} bytes)")


if __name__ == "__main__":
    main()
