#!/usr/bin/env python3
"""Computes the golden art files of the ascii styles and of the quadrant mode
for test_fixtures/4x4.png from its pixel table, independently of jaci's
renderer. Run from the repository root: python3 test_fixtures/make_goldens.py
"""
import math
import pathlib

ESC = "\x1b"
RESET = ESC + "[0m"
# test_fixtures/4x4.png, row by row: (R, G, B, A).
PIXELS = [
    [(255, 0, 0, 255), (0, 255, 0, 255), (0, 0, 0, 0), (10, 20, 30, 0)],
    [(0, 0, 255, 255), (0, 0, 0, 0), (255, 255, 0, 255), (10, 20, 30, 0)],
    [(255, 255, 255, 255), (128, 128, 128, 128), (0, 255, 255, 255), (64, 32, 16, 255)],
    [(0, 0, 0, 255), (200, 100, 50, 1), (0, 0, 0, 0), (17, 34, 51, 255)],
]
SHORT = " .:-=+*#%@"
DENSE = " .'`^\",:;Il!i><~+_-?][}{1)(|\\/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$"
QUADRANTS = " ▘▝▀▖▌▞▛▗▚▐▜▄▙▟█"


def half_away(x):
    """Rounds a number that is not negative half away from zero."""
    return int(math.floor(x + 0.5))


def fg(colour):
    return f"{ESC}[38;2;{colour[0]};{colour[1]};{colour[2]}m"


def bg(colour):
    return f"{ESC}[48;2;{colour[0]};{colour[1]};{colour[2]}m"


def luma(pixel):
    return 299 * pixel[0] + 587 * pixel[1] + 114 * pixel[2]


def bright(charset, tinted):
    ys = sorted(luma(p) for row in PIXELS for p in row if p[3] != 0)
    lo, hi = ys[2 * len(ys) // 100], ys[98 * len(ys) // 100]
    rows = []
    for row in PIXELS:
        out = ""
        for p in row:
            if p[3] == 0:
                out += " "
                continue
            y = luma(p)
            t = y / 255000 if hi == lo else min(max((y - lo) / (hi - lo), 0.0), 1.0)
            level = math.sqrt(math.sqrt(t * t * t))
            glyph = charset[min(int(level * len(charset)), len(charset) - 1)]
            value = 0.6 + 0.4 * level
            top = max(p[:3])
            if top == 0:
                colour = [half_away(value * 255)] * 3
            else:
                colour = [half_away(c / top * value * 255) for c in p[:3]]
            tint = bg([half_away(c * 0.4) for c in p[:3]]) if tinted else ""
            out += fg(colour) + tint + glyph + RESET
        rows.append(out)
    return rows


def mean(pixels):
    """The mean colour, each channel rounded half up."""
    n = len(pixels)
    return [(2 * sum(p[i] for p in pixels) + n) // (2 * n) for i in range(3)]


def quadrant():
    rows = []
    for y in range(0, 4, 2):
        out = ""
        for x in range(0, 4, 2):
            cell = [PIXELS[y][x], PIXELS[y][x + 1], PIXELS[y + 1][x], PIXELS[y + 1][x + 1]]
            opaque = [i for i, p in enumerate(cell) if p[3] != 0]
            if not opaque:
                out += " "
            elif len(opaque) < 4:
                mask = sum(1 << i for i in opaque)
                out += fg(mean([cell[i] for i in opaque])) + QUADRANTS[mask] + RESET
            else:
                best = None
                for mask in range(1, 8):
                    groups = (
                        [cell[i] for i in range(4) if mask >> i & 1],
                        [cell[i] for i in range(4) if not mask >> i & 1],
                    )
                    # 6 × the squared error, an integer: for a group of n
                    # pixels, 6·Σ|p|² − (6 ÷ n)·|Σp|².
                    score = sum(
                        6 * sum(c * c for p in group for c in p[:3])
                        - 6 // len(group) * sum(sum(p[i] for p in group) ** 2 for i in range(3))
                        for group in groups
                    )
                    if best is None or score < best[0]:
                        best = (score, mask, groups)
                _, mask, (front, back) = best
                out += fg(mean(front)) + bg(mean(back)) + QUADRANTS[mask] + RESET
        rows.append(out)
    return rows


def write(name, rows):
    """Writes an art file: each row, a reset and a newline."""
    text = "".join(row + RESET + "\n" for row in rows)
    pathlib.Path("test_fixtures", name).write_text(text, encoding="utf-8")


write("4x4-ascii-bright.ansi", bright(SHORT, tinted=False))
write("4x4-ascii-dense.ansi", bright(DENSE, tinted=False))
write("4x4-ascii-tinted.ansi", bright(SHORT, tinted=True))
write("4x4-quadrant.ansi", quadrant())
