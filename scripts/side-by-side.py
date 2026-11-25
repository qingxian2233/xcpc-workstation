#!/usr/bin/env python3
"""Put a reference page and ours side by side, plus an overlay, as one PNG.

    scripts/side-by-side.py reference.pdf ours.pdf PAGE out.png [--dpi 60]

Columns: reference | ours | overlay (reference red, ours blue, both black).
The acceptance test for olymp-classic is that the pages look the same here.
Needs pdftoppm; standard library only.
"""

import argparse
import struct
import subprocess
import tempfile
import zlib
from pathlib import Path


def gray(pdf: Path, page: int, dpi: int, tmp: Path):
    stem = tmp / f"{pdf.stem}-{page}"
    subprocess.run(["pdftoppm", "-f", str(page), "-l", str(page), "-r", str(dpi), "-gray",
                    "-singlefile", str(pdf), str(stem)], check=True)
    data = stem.with_suffix(".pgm").read_bytes()
    _, rest = data.split(b"\n", 1)
    dims, rest = rest.split(b"\n", 1)
    _, pixels = rest.split(b"\n", 1)
    w, h = map(int, dims.split())
    return w, h, pixels


def png(path: Path, w: int, h: int, rows):
    raw = b"".join(b"\x00" + bytes(r) for r in rows)
    chunk = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d))
    path.write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
                     + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("reference", type=Path)
    ap.add_argument("ours", type=Path)
    ap.add_argument("page", type=int)
    ap.add_argument("out", type=Path)
    ap.add_argument("--dpi", type=int, default=60)
    a = ap.parse_args()
    with tempfile.TemporaryDirectory() as tmp:
        w, h, ref = gray(a.reference, a.page, a.dpi, Path(tmp))
        w2, h2, ours = gray(a.ours, a.page, a.dpi, Path(tmp))
    gap = 8
    rows = []
    for y in range(h):
        row = bytearray()
        for src, sw in ((ref, w), (ours, w2)):
            for x in range(w):
                v = src[y * sw + x] if x < sw and y < (h if src is ref else h2) else 255
                row += bytes((v, v, v))
            row += b"\x80\x80\x80" * gap
        for x in range(w):
            r = ref[y * w + x]
            o = ours[y * w2 + x] if x < w2 and y < h2 else 255
            # Reference ink shows red, ours blue; shared ink is dark.
            row += bytes((o, min(r, o), r))
        rows.append(row)
    png(a.out, 3 * w + 2 * gap, h, rows)


if __name__ == "__main__":
    main()
