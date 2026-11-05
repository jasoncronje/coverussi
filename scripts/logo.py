#!/usr/bin/env python3
"""Draws the Coverussi logo files: the name in a 5 × 7 LED font and a lit C,
both on a grid of dim LEDs like the panel's.

    python3 scripts/logo.py

Writes docs/logo/*.svg and the portal's favicon and header wordmark. The
PNGs (docs/logo/social.png, portal/public/apple-touch-icon.png) are renders
of social.svg and icon.svg; render them again after a change.
"""

from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
AMBER = "#f2b33d"  # the portal's accent
WHITE = "#f4efe6"
DIM = "#262626"  # an unlit LED
BLACK = "#000000"

FONT = {
    "C": [".###.", "#...#", "#....", "#....", "#....", "#...#", ".###."],
    "O": [".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."],
    "V": ["#...#", "#...#", "#...#", "#...#", "#...#", ".#.#.", "..#.."],
    "E": ["#####", "#....", "#....", "####.", "#....", "#....", "#####"],
    "R": ["####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#"],
    "U": ["#...#", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."],
    "S": [".###.", "#...#", "#....", ".###.", "....#", "#...#", ".###."],
    "I": ["###", ".#.", ".#.", ".#.", ".#.", ".#.", "###"],
}


def place(lit: dict, letter: str, col: int, row: int, colour: str) -> None:
    for r, line in enumerate(FONT[letter]):
        for c, ch in enumerate(line):
            if ch == "#":
                lit[(col + c, row + r)] = colour


def leds(cols: int, rows: int, lit: dict, pitch: float, on: float, off: float, inset: float = 0) -> str:
    """Every LED in the grid: lit ones large and coloured, the rest small and dim."""
    out = []
    for r in range(rows):
        for c in range(cols):
            colour = lit.get((c, r))
            x, y = inset + pitch * (c + 0.5), inset + pitch * (r + 0.5)
            out.append(f'<circle cx="{x:g}" cy="{y:g}" r="{on if colour else off:g}" fill="{colour or DIM}"/>')
    return "".join(out)


def svg(width: float, height: float, body: str, title: str, tile: float | None) -> str:
    ground = f'<rect width="{width:g}" height="{height:g}" rx="{tile:g}" fill="{BLACK}"/>' if tile is not None else ""
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width:g} {height:g}" role="img" aria-label="{title}">'
            f"<title>{title}</title>{ground}{body}</svg>\n")


def wordmark(tile: bool) -> str:
    """COVER in amber, USSI in white: the ussi half stays white across the family."""
    lit: dict = {}
    col = 1
    for i, letter in enumerate("COVERUSSI"):
        place(lit, letter, col, 1, AMBER if i < 5 else WHITE)
        col += len(FONT[letter][0]) + 1
    return svg(col * 10, 90, leds(col, 9, lit, 10, 4.2, 2.2), "Coverussi", 8 if tile else None)


def icon() -> str:
    """A lit C on a 9 × 9 patch of the panel."""
    lit: dict = {}
    place(lit, "C", 2, 1, AMBER)
    return svg(256, 256, leds(9, 9, lit, 28, 11.5, 6, inset=2), "Coverussi", 36)


def social() -> str:
    """GitHub's 1280 × 640 preview: the name lit across a 64 × 32 grid, as wide as the panel."""
    lit: dict = {}
    width = sum(len(FONT[ch][0]) + 1 for ch in "COVERUSSI") - 1
    col = (64 - width) // 2
    for i, letter in enumerate("COVERUSSI"):
        place(lit, letter, col, 12, AMBER if i < 5 else WHITE)
        col += len(FONT[letter][0]) + 1
    return svg(1280, 640, leds(64, 32, lit, 20, 8.4, 4.4), "Coverussi", 0)


def main() -> None:
    files = {
        "docs/logo/wordmark.svg": wordmark(tile=True),
        "docs/logo/wordmark-bare.svg": wordmark(tile=False),
        "docs/logo/icon.svg": icon(),
        "docs/logo/social.svg": social(),
        "portal/public/favicon.svg": icon(),
        "portal/public/wordmark.svg": wordmark(tile=False),
    }
    for name, text in files.items():
        path = ROOT / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        print(f"wrote {name}")


if __name__ == "__main__":
    main()
