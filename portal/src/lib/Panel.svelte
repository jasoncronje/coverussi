<script lang="ts">
  // The 64 × 64 panel as the device draws it, one dot an LED.
  import { live } from './live.svelte';

  let { size = 384 }: { size?: number } = $props();
  let canvas: HTMLCanvasElement;

  const N = 64;
  const expand5 = (v: number) => (v << 3) | (v >> 2);
  const expand6 = (v: number) => (v << 2) | (v >> 4);

  $effect(() => {
    void live.frameTick;
    const frame = live.frame;
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
        const i = (y * N + x) * 2;
        const v = frame ? frame[i] | (frame[i + 1] << 8) : 0;
        const r = expand5(v >> 11), g = expand6((v >> 5) & 63), b = expand5(v & 31);
        // An LED that's off still shows as a dark dot, like the real panel.
        ctx.fillStyle = v ? `rgb(${r},${g},${b})` : '#161616';
        ctx.fillRect(x * cell + gap / 2, y * cell + gap / 2, cell - gap, cell - gap);
      }
    }
  });
</script>

<canvas bind:this={canvas} style:width="{size}px" style:height="{size}px" aria-label="Panel mirror"></canvas>

<style>
  canvas { display: block; border-radius: 4px; max-width: 100%; height: auto !important; aspect-ratio: 1; background: #050505; }
</style>
