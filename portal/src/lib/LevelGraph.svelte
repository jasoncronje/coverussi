<script lang="ts">
  // The mic level over the last 30 s against the music and gap thresholds.
  import { live } from './live.svelte';

  let { music, gap, height = 160 }: { music: number; gap: number; height?: number } = $props();
  let canvas: HTMLCanvasElement;
  let width = $state(600);

  const WINDOW_MS = 30_000;
  const LO = -90, HI = 0;
  const TICKS = [-90, -60, -30, 0];
  const LANE = 34; // the y labels' own lane, so no label sits on the line
  const ACCENT = '#e94f95', GRID = '#2b2b2b', REF = '#8a8a8a', LABEL = '#949494', SURFACE = '#111111';

  $effect(() => {
    void live.levelTick;
    const now = live.now;
    const ratio = window.devicePixelRatio || 1;
    const w = Math.round(width * ratio), h = Math.round(height * ratio);
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }
    const ctx = canvas.getContext('2d')!;
    const top = 8 * ratio, bottom = h - 2 * ratio, left = LANE * ratio;
    const y = (db: number) => bottom - ((Math.max(LO, Math.min(HI, db)) - LO) / (HI - LO)) * (bottom - top);
    const x = (at: number) => w - ((now - at) / WINDOW_MS) * (w - left);
    ctx.clearRect(0, 0, w, h);
    ctx.font = `${11 * ratio}px system-ui, sans-serif`;
    ctx.textBaseline = 'middle';

    // Four faint gridlines with their dB on the left; the floor is the heavier baseline.
    for (const db of TICKS) {
      ctx.strokeStyle = GRID;
      ctx.lineWidth = (db === LO ? 2 : 1) * ratio;
      ctx.beginPath();
      ctx.moveTo(left, Math.round(y(db)) + 0.5);
      ctx.lineTo(w, Math.round(y(db)) + 0.5);
      ctx.stroke();
      ctx.fillStyle = LABEL;
      ctx.textAlign = 'left';
      ctx.fillText(String(db), 0, y(db));
    }

    // The two thresholds: dashed references, labelled at the plot's oldest end, music above its
    // line and gap below, on the card's colour so the level never runs through the words.
    ctx.textAlign = 'left';
    for (const [db, label] of [[music, `music above ${music}`], [gap, `gap below ${gap}`]] as const) {
      ctx.strokeStyle = REF;
      ctx.lineWidth = ratio;
      ctx.setLineDash([4 * ratio, 4 * ratio]);
      ctx.beginPath();
      ctx.moveTo(left, y(db));
      ctx.lineTo(w, y(db));
      ctx.stroke();
      ctx.setLineDash([]);
      const ty = y(db) + (db === music ? -9 : 9) * ratio;
      const tw = ctx.measureText(label).width;
      ctx.fillStyle = SURFACE;
      ctx.fillRect(left + 4 * ratio, ty - 7 * ratio, tw + 8 * ratio, 14 * ratio);
      ctx.fillStyle = LABEL;
      ctx.fillText(label, left + 8 * ratio, ty);
    }

    const points = live.levels.filter((l) => l.at >= now - WINDOW_MS);
    if (points.length > 1) {
      ctx.beginPath();
      points.forEach((p, i) => (i ? ctx.lineTo(x(p.at), y(p.db)) : ctx.moveTo(x(p.at), y(p.db))));
      ctx.strokeStyle = ACCENT;
      ctx.lineWidth = 2 * ratio;
      ctx.lineJoin = 'round';
      ctx.stroke();
    }
  });

  const latest = $derived.by(() => {
    void live.levelTick;
    const list = live.levels;
    return list.length ? list[list.length - 1] : null;
  });
  const fresh = $derived(latest && live.now - latest.at < 2000 ? latest : null);
</script>

<div class="graph" bind:clientWidth={width}>
  <div class="now"><span class="value num">{fresh ? fresh.db.toFixed(1) : '–'}</span><span class="unit">dBFS now</span></div>
  <canvas bind:this={canvas} style:height="{height}px" aria-label="Microphone level over the last 30 seconds"></canvas>
  <div class="axis faint small"><span>30 s ago</span><span>now</span></div>
</div>

<style>
  .graph { width: 100%; display: flex; flex-direction: column; gap: 8px; }
  .now { line-height: 28px; }
  .value { font-size: var(--fs-kpi); }
  canvas { display: block; width: 100%; }
  .axis { display: flex; justify-content: space-between; padding-left: 34px; }
</style>
