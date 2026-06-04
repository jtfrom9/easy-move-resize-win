#!/usr/bin/env python3
"""Generate the Easy Move+Resize app icon / logo from pure-Python vector math.

No third-party deps: shapes are drawn with signed-distance functions, rendered
with 4x supersampling for anti-aliasing, and written out as PNGs plus a
multi-size .ico. Run:  python3 tools/gen_icon.py

Design (deliberately distinct from the macOS original): a teal->blue rounded
square, a white "window" with an accent title bar, and a bold diagonal
double-headed arrow signifying move/resize.
"""

import math
import os
import struct
import zlib

# ---- colours (0..1) -------------------------------------------------------

def hexc(s):
    s = s.lstrip("#")
    return (int(s[0:2], 16) / 255, int(s[2:4], 16) / 255, int(s[4:6], 16) / 255)

TEAL = hexc("#15C7B8")   # top-left of gradient
BLUE = hexc("#2563EB")   # bottom-right of gradient
TITLE = hexc("#1E3A8A")  # window title bar
WHITE = hexc("#F7FAFC")  # window body
ARROW = hexc("#0B2F6B")  # diagonal arrow

# ---- maths ----------------------------------------------------------------

def clamp01(x):
    return 0.0 if x < 0 else (1.0 if x > 1 else x)

def lerp3(a, b, t):
    return (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t)

def sdf_round_rect(px, py, cx, cy, hw, hh, r):
    qx = abs(px - cx) - (hw - r)
    qy = abs(py - cy) - (hh - r)
    ax, ay = max(qx, 0.0), max(qy, 0.0)
    return math.hypot(ax, ay) + min(max(qx, qy), 0.0) - r

def sdf_seg(px, py, ax, ay, bx, by):
    abx, aby = bx - ax, by - ay
    apx, apy = px - ax, py - ay
    d = abx * abx + aby * aby
    t = clamp01((apx * abx + apy * aby) / d) if d > 0 else 0.0
    return math.hypot(apx - abx * t, apy - aby * t)

def over(dst, src):
    """Straight-alpha source-over. Colours are (r,g,b,a) in 0..1."""
    sa = src[3]
    da = dst[3] * (1 - sa)
    oa = sa + da
    if oa <= 0:
        return (0.0, 0.0, 0.0, 0.0)
    return (
        (src[0] * sa + dst[0] * da) / oa,
        (src[1] * sa + dst[1] * da) / oa,
        (src[2] * sa + dst[2] * da) / oa,
        oa,
    )

# ---- the icon, sampled in normalised [0,1] space --------------------------

# Window box.
WIN = dict(cx=0.5, cy=0.525, hw=0.255, hh=0.225, r=0.055)
TITLE_H = 0.085  # title-bar height from the window top

# Diagonal double arrow (tip A up-left, tip B down-right).
A = (0.395, 0.49)
B = (0.605, 0.665)
SHAFT_TH = 0.030
HEAD_LEN = 0.095
HEAD_ANG = math.radians(38)

def _arrowhead_segments(tip, other):
    """Two barb segments for an arrowhead at `tip`, pointing away from `other`."""
    dx, dy = tip[0] - other[0], tip[1] - other[1]
    n = math.hypot(dx, dy) or 1.0
    dx, dy = dx / n, dy / n          # outward pointing direction
    bx, by = -dx, -dy                # back along the shaft
    segs = []
    for ang in (HEAD_ANG, -HEAD_ANG):
        rx = bx * math.cos(ang) - by * math.sin(ang)
        ry = bx * math.sin(ang) + by * math.cos(ang)
        segs.append((tip[0], tip[1], tip[0] + rx * HEAD_LEN, tip[1] + ry * HEAD_LEN))
    return segs

ARROW_SEGS = [(A[0], A[1], B[0], B[1])]
ARROW_SEGS += _arrowhead_segments(A, B)
ARROW_SEGS += _arrowhead_segments(B, A)

WIN_TOP = WIN["cy"] - WIN["hh"]

def sample(u, v):
    col = (0.0, 0.0, 0.0, 0.0)

    # Background rounded square (transparent outside).
    if sdf_round_rect(u, v, 0.5, 0.5, 0.44, 0.44, 0.205) < 0:
        c = lerp3(TEAL, BLUE, clamp01((u + v) / 2.0))
        col = over(col, (c[0], c[1], c[2], 1.0))

    # Window body.
    in_win = sdf_round_rect(u, v, WIN["cx"], WIN["cy"], WIN["hw"], WIN["hh"], WIN["r"]) < 0
    if in_win:
        col = over(col, (WHITE[0], WHITE[1], WHITE[2], 1.0))
        # Title bar (top strip, clipped to the window).
        if v < WIN_TOP + TITLE_H:
            col = over(col, (TITLE[0], TITLE[1], TITLE[2], 1.0))

    # Diagonal double arrow.
    dmin = min(sdf_seg(u, v, *s) for s in ARROW_SEGS)
    cov = 1.0 - clamp01((dmin - SHAFT_TH) / 0.004)  # soft edge; supersampling adds AA
    if cov > 0:
        col = over(col, (ARROW[0], ARROW[1], ARROW[2], cov))

    return col

# ---- raster ---------------------------------------------------------------

def render(size, ss=4):
    """Return raw RGBA bytes for an icon of `size` px, ss x ss supersampled."""
    out = bytearray(size * size * 4)
    inv = 1.0 / (size * ss)
    for y in range(size):
        for x in range(size):
            pr = pg = pb = pa = 0.0
            for sy in range(ss):
                vy = (y * ss + sy + 0.5) * inv
                for sx in range(ss):
                    vx = (x * ss + sx + 0.5) * inv
                    r, g, b, a = sample(vx, vy)
                    pr += r * a; pg += g * a; pb += b * a; pa += a
            n = ss * ss
            a = pa / n
            if a > 0:
                r = pr / pa; g = pg / pa; b = pb / pa
            else:
                r = g = b = 0.0
            i = (y * size + x) * 4
            out[i] = int(r * 255 + 0.5)
            out[i + 1] = int(g * 255 + 0.5)
            out[i + 2] = int(b * 255 + 0.5)
            out[i + 3] = int(a * 255 + 0.5)
    return bytes(out)

def png_bytes(size, rgba):
    def chunk(tag, data):
        return (struct.pack(">I", len(data)) + tag + data
                + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF))

    raw = bytearray()
    stride = size * 4
    for y in range(size):
        raw.append(0)  # filter: none
        raw += rgba[y * stride:(y + 1) * stride]
    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", ihdr)
            + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
            + chunk(b"IEND", b""))

def write_png(path, size, ss=4):
    data = png_bytes(size, render(size, ss))
    with open(path, "wb") as f:
        f.write(data)
    return data

def write_ico(path, frames):
    """frames: list of (size, png_bytes). Builds a PNG-compressed .ico."""
    n = len(frames)
    out = bytearray(struct.pack("<HHH", 0, 1, n))
    offset = 6 + 16 * n
    entries, blobs = bytearray(), bytearray()
    for size, data in frames:
        w = 0 if size >= 256 else size
        entries += struct.pack("<BBBBHHII", w, w, 0, 0, 1, 32, len(data), offset)
        blobs += data
        offset += len(data)
    with open(path, "wb") as f:
        f.write(out + entries + blobs)

def main():
    here = os.path.dirname(os.path.abspath(__file__))
    assets = os.path.join(here, "..", "assets")
    os.makedirs(assets, exist_ok=True)

    ico_sizes = [16, 32, 48, 64, 128, 256]
    frames = []
    for s in ico_sizes:
        data = png_bytes(s, render(s))
        frames.append((s, data))
        if s in (32, 256):
            with open(os.path.join(assets, f"icon-{s}.png"), "wb") as f:
                f.write(data)
    write_ico(os.path.join(assets, "easy-move-resize.ico"), frames)

    # Logo for the README (larger render).
    write_png(os.path.join(assets, "logo.png"), 512)
    print("wrote assets/easy-move-resize.ico, icon-32.png, icon-256.png, logo.png")

if __name__ == "__main__":
    main()
