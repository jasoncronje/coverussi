# The Coverussi logo

The name in a 5 × 7 LED font, lit on a grid of dim LEDs, the way the panel would draw it. `scripts/logo.py` draws every file here; change it there, not by hand.

| File | Use |
| --- | --- |
| `wordmark.svg` | The logo. On a black tile, so it works on light and dark pages |
| `wordmark-bare.svg` | The same without the tile, for black backgrounds (the portal header) |
| `icon.svg` | A lit C on a 9 × 9 patch, for favicons, app icons and avatars |
| `social.svg`, `social.png` | GitHub's social preview, 1280 × 640: the name across a 64-LED-wide grid. Upload the PNG in the repo's Settings, General, Social preview |

| Colour | Hex | For |
| --- | --- | --- |
| Amber | `#f2b33d` | COVER, the C, and the portal's accent |
| Warm white | `#f4efe6` | USSI |
| Unlit LED | `#262626` | Every LED that isn't lit |
| Black | `#000000` | The panel |

Rules:

- USSI is always white. It's the part shared with Spotussi; the first half takes each product's colour.
- Keep the unlit LEDs: they're what makes it look like the panel.
- The wordmark is at least 18 px tall on screen. Below that, use the icon.
- Don't redraw it in another font, add glows or gradients, or change the spacing.
