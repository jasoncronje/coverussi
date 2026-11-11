#!/usr/bin/env python3
"""Derives every Coverussi logo file from docs/logo/coverussi-light.svg.

    python3 scripts/logo.py

Writes the dark logo, the mark, the social preview, and the portal's header
logo, favicon and touch icon. The PNGs (docs/logo/social.png,
portal/public/apple-touch-icon.png) are rendered with headless Chrome, and
skipped if it isn't installed.
"""

import re
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "docs/logo/coverussi-light.svg"
INK = "#16171B"  # the record and the wordmark, on light
INK_ON_DARK = "#ECEEF1"
MARK_BOX = (0, -875, 1480.5, 1050)  # the sleeve and the record: x, y, width, height
CHROME = Path("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome")


def dark(light: str) -> str:
    assert light.count(INK) == 2
    return light.replace(INK, INK_ON_DARK)


def mark(light: str) -> str:
    """The logo without the wordmark."""
    svg, n = re.subn(r'<path transform=[^>]*/>', "", light)
    assert n == 1
    return re.sub(r'viewBox="[^"]*"', 'viewBox="%g %g %g %g"' % MARK_BOX, svg, count=1)


def inner(svg: str) -> str:
    """What's inside the root element, without the title."""
    body = svg[svg.index(">") + 1 : svg.rindex("</svg>")]
    return re.sub(r"<title>.*?</title>", "", body)


def square(pad: float) -> tuple[float, float, float]:
    """The mark centred in a square with `pad` of its width on each side: x, y, side."""
    x, y, w, h = MARK_BOX
    side = w * (1 + 2 * pad)
    return x - (side - w) / 2, y - (side - h) / 2, side


def favicon(light: str) -> str:
    """The mark in a square; the record turns light when the browser is dark."""
    x, y, side = square(0.03)
    body = inner(mark(light)).replace(f'fill="{INK}"', 'class="ink"', 1)
    style = f"<style>.ink{{fill:{INK}}}@media (prefers-color-scheme:dark){{.ink{{fill:{INK_ON_DARK}}}}}</style>"
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{x:g} {y:g} {side:g} {side:g}">{style}{body}</svg>'


def touch_icon(light: str, px: int) -> str:
    """The mark on a light tile: phones fill a transparent icon with black."""
    x, y, side = square(0.2)
    tile = f'<rect x="{x:g}" y="{y:g}" width="{side:g}" height="{side:g}" fill="{INK_ON_DARK}"/>'
    return f'<svg xmlns="http://www.w3.org/2000/svg" width="{px}" height="{px}" viewBox="{x:g} {y:g} {side:g} {side:g}">{tile}{inner(mark(light))}</svg>'


def social(light: str) -> str:
    """GitHub's social preview, 1280 × 640: the light logo on a light ground."""
    box = re.search(r'viewBox="([^"]*)"', light).group(1)
    w = 880
    h = w * 1176 / 5812.5
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" width="1280" height="640" viewBox="0 0 1280 640">'
        f'<rect width="1280" height="640" fill="{INK_ON_DARK}"/>'
        f'<svg x="{(1280 - w) / 2:g}" y="{(640 - h) / 2:g}" width="{w}" height="{h:g}" viewBox="{box}">{inner(light)}</svg></svg>'
    )


def render(svg: str, out: Path, width: int, height: int) -> None:
    with tempfile.TemporaryDirectory() as tmp:
        page = Path(tmp) / "page.svg"
        page.write_text(svg)
        subprocess.run(
            [str(CHROME), "--headless=new", f"--screenshot={out}", f"--window-size={width},{height}",
             "--force-device-scale-factor=1", "--hide-scrollbars", page.as_uri()],
            check=True, capture_output=True,
        )


def main() -> None:
    light = SOURCE.read_text()
    files = {
        "docs/logo/coverussi-dark.svg": dark(light),
        "docs/logo/coverussi-mark.svg": mark(light),
        "docs/logo/social.svg": social(light),
        "portal/public/logo.svg": dark(light),  # the portal is black
        "portal/public/favicon.svg": favicon(light),
    }
    for path, svg in files.items():
        (ROOT / path).write_text(svg)
        print(f"{path}  {len(svg) // 1024} KB")
    if not CHROME.exists():
        print("No Chrome: social.png and apple-touch-icon.png not rendered")
        return
    for path, svg, w, h in [
        ("docs/logo/social.png", social(light), 1280, 640),
        ("portal/public/apple-touch-icon.png", touch_icon(light, 180), 180, 180),
    ]:
        render(svg, ROOT / path, w, h)
        print(path)


if __name__ == "__main__":
    main()
