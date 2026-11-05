// The panel's transitions, drawn the way the device draws them (crates/app/src/art.rs's
// Compositor::render), for the Settings preview. Keep the two in step.

export type Transition = 'crossfade' | 'fade_black' | 'slide' | 'wipe' | 'dissolve' | 'cut';

export const TRANSITIONS: { id: Transition; label: string }[] = [
  { id: 'crossfade', label: 'Crossfade' },
  { id: 'fade_black', label: 'Fade through black' },
  { id: 'slide', label: 'Slide' },
  { id: 'wipe', label: 'Wipe' },
  { id: 'dissolve', label: 'Dissolve' },
  { id: 'cut', label: 'Cut' }
];

export const N = 64;
export type Rgb = Uint8ClampedArray; // N × N × 3

const smooth = (t: number) => t * t * (3 - 2 * t);
// The same pseudo-random order as the device's dissolve.
function rank(i: number) {
  let h = i >>> 0;
  h ^= h >>> 16; h = Math.imul(h, 0x7feb352d); h ^= h >>> 15; h = Math.imul(h, 0x846ca68b); h ^= h >>> 16;
  return (h & 0xffff) / 65536;
}

/** The frame t (0-1) of the way from one picture to the next; null is black. */
export function blend(from: Rgb | null, to: Rgb | null, how: Transition, t: number, out: Rgb): Rgb {
  const e = smooth(t);
  const shift = Math.round(e * N);
  const at = (img: Rgb | null, x: number, y: number, k: number) => (img ? img[(y * N + x) * 3 + k] : 0);
  for (let y = 0; y < N; y++) {
    for (let x = 0; x < N; x++) {
      const i = y * N + x;
      for (let k = 0; k < 3; k++) {
        const a = at(from, x, y, k), b = at(to, x, y, k);
        let c: number;
        switch (t >= 1 ? 'cut' : how) {
          case 'cut': c = b; break;
          case 'crossfade': c = a + (b - a) * t; break;
          case 'fade_black': c = t < 0.5 ? a * (1 - 2 * t) : b * (2 * t - 1); break;
          case 'slide': c = x + shift < N ? at(from, x + shift, y, k) : at(to, x + shift - N, y, k); break;
          case 'wipe': c = a + (b - a) * Math.min(1, Math.max(0, (e * (N + 4) - x) / 4)); break;
          case 'dissolve': c = rank(i) < t ? b : a; break;
        }
        out[i * 3 + k] = c;
      }
    }
  }
  return out;
}

function paint(fn: (x: number, y: number) => [number, number, number]): Rgb {
  const img = new Uint8ClampedArray(N * N * 3);
  for (let y = 0; y < N; y++) for (let x = 0; x < N; x++) img.set(fn(x, y), (y * N + x) * 3);
  return img;
}

/** Two made-up covers to show a transition with: a sunset and a record on teal. */
export const SAMPLES: Rgb[] = [
  paint((x, y) => {
    const sun = Math.hypot(x - 32, y - 38) < 14;
    if (y > 44) return [20, 10 + (y - 44) * 2, 40 + (y - 44) * 3]; // water
    if (sun) return [255, 214, 90];
    return [255 - y * 2, 90 + y, 60 + y * 2];
  }),
  paint((x, y) => {
    const r = Math.hypot(x - 31.5, y - 31.5);
    if (r < 4) return [10, 10, 10];
    if (r < 10) return [230, 70, 60]; // label
    if (r < 27) return r % 3 < 1 ? [40, 40, 44] : [14, 14, 16]; // grooves
    return [20 + x, 150, 150 + y];
  })
];

/** An RGB565 panel frame (little-endian, as the device mirrors it) as RGB. */
export function fromFrame(frame: Uint8Array): Rgb {
  const img = new Uint8ClampedArray(N * N * 3);
  for (let i = 0; i < N * N; i++) {
    const v = frame[i * 2] | (frame[i * 2 + 1] << 8);
    const r5 = v >> 11, g6 = (v >> 5) & 63, b5 = v & 31;
    img[i * 3] = (r5 << 3) | (r5 >> 2);
    img[i * 3 + 1] = (g6 << 2) | (g6 >> 4);
    img[i * 3 + 2] = (b5 << 3) | (b5 >> 2);
  }
  return img;
}
