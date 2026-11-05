// The board's test pattern (art::test_pattern in crates/app), drawn the same way:
// primaries, a gray ramp, a hue sweep and corner markers (red top-left,
// green top-right, blue bottom-left, white bottom-right).
const SIZE = 64;

export function testPattern(): ImageData {
  const img = new ImageData(SIZE, SIZE);
  const set = (x: number, y: number, [r, g, b]: number[]) => img.data.set([r, g, b, 255], (y * SIZE + x) * 4);
  for (let y = 0; y < SIZE; y++) {
    for (let x = 0; x < SIZE; x++) {
      if (y < 16) {
        const band = Math.floor(x / 16);
        const on = (b: boolean) => (b ? 255 : 0);
        set(x, y, [on(band === 0 || band === 3), on(band === 1 || band === 3), on(band >= 2)]);
      } else if (y < 32) {
        const v = Math.floor((x * 255) / (SIZE - 1));
        set(x, y, [v, v, v]);
      } else if (y < 48) {
        const h = (x * 6) / SIZE;
        const f = h - Math.floor(h);
        const up = Math.floor(255 * f), down = Math.floor(255 * (1 - f));
        const rgb = [[255, up, 0], [down, 255, 0], [0, 255, up], [0, down, 255], [up, 0, 255], [255, 0, down]];
        set(x, y, rgb[Math.min(Math.floor(h), 5)]);
      } else {
        set(x, y, [0, 0, 0]);
      }
    }
  }
  for (const [cx, cy, c] of [[0, 0, [255, 0, 0]], [60, 0, [0, 255, 0]], [0, 60, [0, 0, 255]], [60, 60, [255, 255, 255]]] as const) {
    for (let dy = 0; dy < 4; dy++) for (let dx = 0; dx < 4; dx++) set(cx + dx, cy + dy, [...c]);
  }
  return img;
}
