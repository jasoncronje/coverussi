<script lang="ts">
  // The LED panel: power, what it draws against its ceiling, and the test pattern.
  import Icon from './Icon.svelte';
  import { live } from './live.svelte';
  import { testPattern } from './testPattern';

  let { command }: { command: (name: string, args?: Record<string, unknown>) => void } = $props();

  const t = $derived(live.telemetry);
  const online = $derived(live.connected);
  const panelState = $derived(!online ? 'Offline' : t.panel_on === false ? 'Off' : t.test_pattern ? 'Test pattern' : 'On');
  const limitMa = $derived(live.settings?.display.current_limit_ma ?? 3000);
  const held = $derived(t.requested_brightness !== undefined && t.brightness !== undefined && t.requested_brightness > t.brightness);
  // What the test pattern should look like, to hold up against the panel.
  function drawPattern(canvas: HTMLCanvasElement) {
    canvas.getContext('2d')?.putImageData(testPattern(), 0, 0);
  }
  const pct = (v: number | undefined, max: number) => (v === undefined || !max ? 0 : Math.max(0, Math.min(100, (v / max) * 100)));
</script>

<section class="card">
  <div class="card-head"><h2>Panel</h2><span class="spacer"></span>
    <span class="muted">{panelState}</span>
    <button class="switch" role="switch" aria-checked={t.panel_on !== false} aria-label="Panel power"
      disabled={!online} onclick={() => command('panel', { on: t.panel_on === false })}><span></span></button>
  </div>
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
  <div class="row">
    <button class="quiet" aria-pressed={Boolean(t.test_pattern)} disabled={!online || t.panel_on === false}
      onclick={() => command('test_pattern', { on: !t.test_pattern })}><Icon name={t.test_pattern ? 'x' : 'grid'} />{t.test_pattern ? 'Stop test pattern' : 'Test pattern'}</button>
  </div>
  {#if t.test_pattern}
    <div class="pattern">
      <canvas width="64" height="64" use:drawPattern aria-label="The test pattern as the panel should show it"></canvas>
      <p class="muted">The panel should look like this: red top left, green top right, blue bottom left, white bottom right.</p>
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

  /* Power is a state, so it's a switch: white track when on, the knob slides right. */
  .switch { position: relative; width: 40px; height: 24px; padding: 0; border: none; border-radius: 12px; background: var(--raised); transition: background 160ms ease-out; }
  .switch span { position: absolute; top: 3px; left: 3px; width: 18px; height: 18px; border-radius: 50%; background: var(--muted); transition: transform 160ms ease-out, background 160ms ease-out; }
  .switch[aria-checked='true'] { background: var(--text); }
  .switch[aria-checked='true'] span { transform: translateX(16px); background: var(--ground); }
  .switch:disabled { background: var(--raised); opacity: 0.5; }
  .switch:hover:not(:disabled) { border: none; }
  .switch[aria-checked='true']:hover:not(:disabled) { background: #e6e6e6; }
  .pattern { display: flex; align-items: center; gap: 16px; }
  .pattern canvas { width: 128px; height: 128px; flex: none; image-rendering: pixelated; border-radius: 4px; }
  .pattern p { font-size: 13px; line-height: 18px; }
  .quiet { height: 32px; padding: 0 14px; font-size: 13px; }
  @media (prefers-reduced-motion: reduce) { .switch, .switch span { transition: none; } }
</style>
