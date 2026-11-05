<script lang="ts">
  // A small panel that plays a transition over and over: what's on the panel
  // now (or a sample) to a sample cover and back, holding each a moment.
  import { live } from './live.svelte';
  import { blend, fromFrame, N, SAMPLES, type Rgb, type Transition } from './transition';

  let { how, ms, size = 192 }: { how: Transition; ms: number; size?: number } = $props();
  let canvas: HTMLCanvasElement;

  const HOLD_MS = 1200;
  let started = performance.now();

  // Restart from the top whenever the transition or its length changes.
  $effect(() => {
    void how, ms;
    started = performance.now();
  });

  function covers(): [Rgb, Rgb] {
    const frame = live.frame;
    // The panel's own picture when it's showing one, so the preview uses real art.
    if (frame && frame.some((b) => b !== 0)) return [fromFrame(frame), SAMPLES[1]];
    return [SAMPLES[0], SAMPLES[1]];
  }

  $effect(() => {
    let raf = 0;
    const out = new Uint8ClampedArray(N * N * 3);
    const draw = (now: number) => {
      const length = how === 'cut' ? 0 : Math.max(0, ms);
      const leg = HOLD_MS + length;
      const at = (now - started) % (leg * 2);
      const [a, b] = covers();
      // Each leg changes picture, then holds the new one.
      const [from, to] = at < leg ? [b, a] : [a, b];
      const into = at % leg;
      blend(from, to, how, into < length ? into / length : 1, out);
      paint(out);
      raf = requestAnimationFrame(draw);
    };
    raf = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(raf);
  });

  function paint(img: Rgb) {
    const ratio = window.devicePixelRatio || 1;
    const px = Math.round(size * ratio);
    if (canvas.width !== px) canvas.width = canvas.height = px;
    const ctx = canvas.getContext('2d')!;
    const cell = px / N;
    const gap = Math.max(1, cell * 0.16);
    ctx.fillStyle = '#050505';
    ctx.fillRect(0, 0, px, px);
    for (let y = 0; y < N; y++) {
      for (let x = 0; x < N; x++) {
        const i = (y * N + x) * 3;
        const r = img[i], g = img[i + 1], b = img[i + 2];
        ctx.fillStyle = r | g | b ? `rgb(${r},${g},${b})` : '#161616';
        ctx.fillRect(x * cell + gap / 2, y * cell + gap / 2, cell - gap, cell - gap);
      }
    }
  }
</script>

<canvas bind:this={canvas} style:width="{size}px" style:height="{size}px" aria-label="Transition preview"></canvas>

<style>
  canvas { display: block; border-radius: 4px; max-width: 100%; height: auto !important; aspect-ratio: 1; background: #050505; }
</style>
