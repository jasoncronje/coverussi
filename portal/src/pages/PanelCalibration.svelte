<script lang="ts">
  // Full screen: the panel shows the calibration image (or the cover) and takes each slider as it moves.
  // Nothing is kept until Save; Cancel, or a closed tab after two minutes, gives the saved tuning back.
  import { onDestroy, onMount, untrack } from 'svelte';
  import { api } from '../lib/api';
  import Icon from '../lib/Icon.svelte';
  import Panel from '../lib/Panel.svelte';
  import Segmented from '../lib/Segmented.svelte';
  import type { PanelColor } from '../lib/types';

  let { saved, onclose, onsaved }: { saved: PanelColor; onclose: () => void; onsaved: (c: PanelColor) => void } = $props();

  const NEUTRAL: PanelColor = { red_pct: 100, green_pct: 100, blue_pct: 100, shadows: 0, saturation_pct: 100 };
  type Key = keyof PanelColor;
  interface Slider { key: Key; label: string; min: number; max: number; unit: string; tip: string }
  const groups: { title: string; tip: string; sliders: Slider[] }[] = [
    {
      title: 'White balance', tip: 'Look at the white block and the gray beside it. Lower whichever colour tints them until both look neutral.',
      sliders: [
        { key: 'red_pct', label: 'Red', min: 50, max: 100, unit: '%', tip: 'Lower it if white looks pink or warm.' },
        { key: 'green_pct', label: 'Green', min: 50, max: 100, unit: '%', tip: 'Lower it if white looks green or yellow. The green LEDs are the brightest, so it’s often this one.' },
        { key: 'blue_pct', label: 'Blue', min: 50, max: 100, unit: '%', tip: 'Lower it if white looks cold or blue.' }
      ]
    },
    {
      title: 'Tone', tip: 'Shadows for the row of dark steps, saturation for the colours and the skin tone.',
      sliders: [
        { key: 'shadows', label: 'Shadows', min: -50, max: 50, unit: '', tip: 'Raise it until each dark step is just visible, so dark covers keep their detail. Lower it for deeper blacks.' },
        { key: 'saturation_pct', label: 'Saturation', min: 50, max: 150, unit: '%', tip: 'LEDs are vivid, so covers can look loud. Lower it until the skin tone looks natural.' }
      ]
    }
  ];

  let color = $state<PanelColor>(untrack(() => ({ ...saved })));
  let image = $state<'pattern' | 'cover'>('pattern');
  let error = $state('');
  let saving = $state(false);
  let closed = false;

  // One request at a time, the latest values winning: a dragged slider sends dozens of changes.
  let busy = false;
  let again = false;
  async function push() {
    if (closed) return;
    if (busy) { again = true; return; }
    busy = true;
    again = false;
    try {
      await api.command('panel_calibration', { color: $state.snapshot(color), image });
      error = '';
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
      if (again) void push();
    }
  }

  function set(key: Key, v: number) {
    color[key] = v;
    void push();
  }

  function reset() {
    color = { ...NEUTRAL };
    void push();
  }

  async function save() {
    saving = true;
    error = '';
    try {
      // The newest settings, so a change made elsewhere meanwhile isn't undone.
      const { settings } = await api.settings();
      settings.display.color = $state.snapshot(color);
      await api.saveSettings(settings);
      onsaved(settings.display.color);
      close();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      saving = false;
    }
  }

  function close() {
    if (closed) return;
    closed = true;
    void api.command('panel_calibration', { on: false }).catch(() => {});
    onclose();
  }

  const changed = $derived(JSON.stringify(color) !== JSON.stringify(saved));
  const neutral = $derived(JSON.stringify(color) === JSON.stringify(NEUTRAL));

  // Typed values: tried as soon as they're in range, pulled into range (or back) when the box is left.
  function typed(s: Slider, raw: string, final: boolean) {
    const n = parseInt(raw, 10);
    if (Number.isNaN(n)) return;
    if (n >= s.min && n <= s.max) set(s.key, n);
    else if (final) set(s.key, Math.min(s.max, Math.max(s.min, n)));
  }

  let keepAlive: ReturnType<typeof setInterval>;
  onMount(() => {
    void push();
    keepAlive = setInterval(() => void push(), 30_000);
    const overflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') close(); };
    window.addEventListener('keydown', onKey);
    return () => {
      document.body.style.overflow = overflow;
      window.removeEventListener('keydown', onKey);
    };
  });
  onDestroy(() => {
    clearInterval(keepAlive);
    close();
  });
</script>

<div class="screen" role="dialog" aria-modal="true" aria-labelledby="calibrate-title">
  <header>
    <div class="bar">
      <div>
        <h1 id="calibrate-title">Calibrate the panel</h1>
        {#if error}<p class="error small">{error}</p>
        {:else}<p class="muted small">Look at the panel itself. It changes as you move each slider; nothing is kept until you save.</p>{/if}
      </div>
      <div class="actions">
        <button onclick={close}><Icon name="x" />Cancel</button>
        <button class="primary" disabled={!changed || saving} onclick={save}><Icon name="check" />{saving ? 'Saving…' : 'Save'}</button>
      </div>
    </div>
  </header>

  <div class="body">
    <section class="look">
      <Segmented label="On the panel" value={image} onchange={(v) => { image = v; void push(); }}
        options={[{ value: 'pattern', label: 'Calibration image' }, { value: 'cover', label: 'Current cover' }]} />
      <Panel size={320} />
      {#if image === 'pattern'}
        <ol class="legend">
          <li><span class="swatch" style:background="linear-gradient(90deg, #fff 50%, #808080 50%)"></span>White and gray: <strong>White balance</strong></li>
          <li><span class="swatch" style:background="linear-gradient(90deg, #000, #505050)"></span>Dark steps: <strong>Shadows</strong></li>
          <li><span class="swatch" style:background="linear-gradient(90deg, #f00, #ff8000, #ff0, #0f0, #0ff, #00f, #f0f, #e6aa82)"></span>Colours and a skin tone: <strong>Saturation</strong></li>
          <li><span class="swatch" style:background="linear-gradient(90deg, #000, #fff)"></span>A smooth ramp, black to white</li>
        </ol>
      {:else}
        <p class="muted small">What’s playing, with your changes on the panel. Nothing playing leaves it dark.</p>
      {/if}
      <p class="faint small">This preview shows the image untouched; the tuning only changes the panel.</p>
    </section>

    <section class="card">
      {#each groups as g (g.title)}
        <div class="group">
          <h2>{g.title}</h2>
          <p class="muted small">{g.tip}</p>
          {#each g.sliders as s (s.key)}
            <div class="slider">
              <label class="name" for="cal-{s.key}">{s.label}</label>
              <input id="cal-{s.key}" type="range" min={s.min} max={s.max} step="1" value={color[s.key]}
                oninput={(e) => set(s.key, parseInt(e.currentTarget.value, 10))} />
              <span class="value">
                <input type="number" class="num" min={s.min} max={s.max} step="1" value={color[s.key]} aria-label="{s.label}{s.unit ? ' in %' : ''}"
                  oninput={(e) => typed(s, e.currentTarget.value, false)}
                  onchange={(e) => { typed(s, e.currentTarget.value, true); e.currentTarget.value = String(color[s.key]); }} />
                <span class="unit faint">{s.unit}</span>
              </span>
              <span class="tip faint small">{s.tip}</span>
            </div>
          {/each}
        </div>
      {/each}
      <div class="row">
        <button class="link" disabled={neutral} onclick={reset}><Icon name="reset" />Reset to untouched</button>
      </div>
    </section>
  </div>
</div>

<style>
  .screen { position: fixed; inset: 0; z-index: 10; background: var(--ground); overflow-y: auto; }
  header { position: sticky; top: 0; background: var(--ground); border-bottom: 1px solid var(--border); }
  .bar, .body { max-width: 1200px; margin: 0 auto; padding: 0 24px; }
  .bar { min-height: 64px; display: flex; align-items: center; gap: var(--gap); padding-top: 10px; padding-bottom: 10px; }
  .bar > div:first-child { flex: 1; display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .actions { display: flex; gap: 8px; }
  .error { color: var(--negative); }
  /* The panel and its legend on the left, the sliders on the right. */
  .body { display: grid; grid-template-columns: 320px minmax(0, 1fr); gap: 32px; padding-top: 32px; padding-bottom: 64px; align-items: start; }
  .look { display: flex; flex-direction: column; gap: var(--gap); }
  .legend { margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; gap: 8px; color: var(--muted); }
  .legend li { display: flex; align-items: center; gap: 10px; }
  .legend strong { color: var(--text); font-weight: 500; }
  .swatch { flex: none; width: 36px; height: 12px; border-radius: 3px; }
  .card { gap: 0; }
  .group { display: flex; flex-direction: column; gap: 4px; padding-bottom: 20px; }
  .group + .group { border-top: 1px solid var(--rule); padding-top: 20px; }
  .group > p { margin-bottom: 8px; }
  /* Name, slider, value; the hint underneath. */
  .slider { display: grid; grid-template-columns: 96px minmax(0, 1fr) auto; align-items: center; gap: 2px 16px; padding: 8px 0; }
  .name { font-weight: 500; }
  .value { display: flex; align-items: center; gap: 6px; }
  .value input { width: 72px; text-align: right; }
  .unit { width: 12px; }
  .tip { grid-column: 2 / 4; }
  .slider input[type='range'] { height: auto; padding: 0; border: none; background: none; accent-color: #ffffff; width: 100%; }
  @media (max-width: 760px) {
    .body { grid-template-columns: 1fr; }
    .bar { flex-wrap: wrap; }
  }
  @media (max-width: 520px) {
    .slider { grid-template-columns: 1fr auto; }
    .slider input[type='range'] { grid-column: 1 / 3; grid-row: 2; }
    .tip { grid-column: 1 / 3; }
  }
</style>
