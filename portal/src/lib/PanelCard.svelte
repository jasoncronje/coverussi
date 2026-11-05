<script lang="ts">
  // The LED panel's readings: what it draws against its ceiling, and the test pattern to hold up against it.
  // Its power and test pattern controls are in the Controller page's title row.
  import { live } from './live.svelte';
  import { testPattern } from './testPattern';

  const t = $derived(live.telemetry);
  const limitMa = $derived(live.settings?.display.current_limit_ma ?? 3000);
  const held = $derived(t.requested_brightness !== undefined && t.brightness !== undefined && t.requested_brightness > t.brightness);
  // What the test pattern should look like, to hold up against the panel.
  function drawPattern(canvas: HTMLCanvasElement) {
    canvas.getContext('2d')?.putImageData(testPattern(), 0, 0);
  }
  const pct = (v: number | undefined, max: number) => (v === undefined || !max ? 0 : Math.max(0, Math.min(100, (v / max) * 100)));
</script>

<section class="card">
  <div class="card-head"><h2>Panel</h2></div>
  <div class="gauges">
    <div class="gauge">
      <span class="label">Brightness</span>
      <span class="value num">{t.brightness ?? '–'}<span class="unit">/ 255</span></span>
      <span class="bar" aria-hidden="true"><span style:width="{pct(t.brightness, 255)}%"></span></span>
    </div>
    <div class="gauge">
      <span class="label">Current (est.)</span>
      <span class="value num">{t.current_ma !== undefined ? (t.current_ma / 1000).toFixed(2) : '–'}<span class="unit">/ {(limitMa / 1000).toFixed(1)} A</span></span>
      <span class="bar" class:held aria-hidden="true"><span style:width="{pct(t.current_ma, limitMa)}%"></span></span>
    </div>
    <div class="gauge">
      <span class="label">Frame rate</span>
      <span class="value num">{#if t.fps}{t.fps.toFixed(1)}<span class="unit">fps</span>{:else}<span class="still">Still</span>{/if}</span>
    </div>
  </div>
  {#if held}<p class="note warning-ink">Held at {t.brightness} by the current limit; asked {t.requested_brightness}.</p>{/if}
  {#if t.test_pattern}
    <div class="pattern">
      <canvas width="64" height="64" use:drawPattern aria-label="The test pattern as the panel should show it"></canvas>
      <p class="muted">Should match the panel</p>
    </div>
  {/if}
</section>

<style>
  /* Three readings side by side, each over a bar against its ceiling. */
  .gauges { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); border-top: 1px solid var(--rule); }
  .gauge { display: flex; flex-direction: column; gap: 6px; padding: 16px 16px 18px; min-width: 0; }
  .gauge:first-child { padding-left: 0; }
  .gauge + .gauge { border-left: 1px solid var(--rule); }
  .gauge .label { color: var(--muted); font-size: 13px; line-height: 18px; }
  .gauge .value { font-size: var(--fs-kpi); line-height: 30px; font-weight: 500; white-space: nowrap; }
  .gauge .unit { font-size: var(--fs-body); font-weight: 400; }
  .still { color: var(--muted); font-weight: 400; }
  .bar { height: 4px; border-radius: 2px; background: var(--raised); overflow: hidden; margin-top: 4px; }
  .bar span { display: block; height: 100%; background: var(--text); transition: width 300ms ease-out; }
  .bar.held span { background: var(--warning); }
  .note { font-size: 13px; line-height: 18px; margin-top: -4px; }
  .warning-ink { color: var(--warning); }

  .pattern { display: flex; align-items: center; gap: 16px; }
  .pattern canvas { width: 128px; height: 128px; flex: none; image-rendering: pixelated; border-radius: 4px; }
  .pattern p { font-size: 13px; line-height: 18px; }
  @media (prefers-reduced-motion: reduce) { .bar span { transition: none; } }
</style>
