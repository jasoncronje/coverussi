<script lang="ts">
  // Measures the room quiet, then music playing, and suggests the two thresholds.
  import { onDestroy } from 'svelte';
  import { api } from '../lib/api';
  import Icon from '../lib/Icon.svelte';
  import LevelGraph from '../lib/LevelGraph.svelte';
  import { live } from '../lib/live.svelte';
  import type { Calibration } from '../lib/types';

  let { onuse }: { onuse: (music: number, gap: number) => void } = $props();

  const SECONDS = 15;
  type Step = 'idle' | 'quiet' | 'quiet_done' | 'music' | 'done';
  let step = $state<Step>('idle');
  let quiet = $state<Calibration | null>(null);
  let music = $state<Calibration | null>(null);
  let error = $state('');
  let endsAt = $state(0);

  const off = live.onEvent((e) => {
    if (e.name !== 'calibrate_result') return;
    if (step === 'quiet') {
      quiet = e.data;
      step = 'quiet_done';
    } else if (step === 'music') {
      music = e.data;
      step = 'done';
    }
  });
  onDestroy(off);

  async function measure(next: 'quiet' | 'music') {
    error = '';
    try {
      await api.command('calibrate', { seconds: SECONDS });
      step = next;
      endsAt = Date.now() + SECONDS * 1000;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  const round = (n: number) => Math.round(n * 2) / 2;
  const suggestion = $derived.by(() => {
    if (!quiet || !music) return null;
    const gap = round(quiet.p90 + 3);
    const mid = (quiet.p90 + music.p10) / 2;
    const musicDb = round(Math.min(music.p10 - 2, Math.max(gap + 6, mid)));
    return { music: musicDb, gap, tooClose: music.p10 - quiet.p90 < 8 };
  });

  const left = $derived(Math.max(0, Math.ceil((endsAt - live.now) / 1000)));
  const online = $derived(live.connected);
</script>

<div class="calibrate">
  <div class="row"><strong>Calibrate</strong><span class="spacer"></span>
    {#if step !== 'idle'}<button class="link" onclick={() => { step = 'idle'; quiet = music = null; }}><Icon name="reset" />Start over</button>{/if}
  </div>

  {#if step === 'idle'}
    <p class="muted">First stop the music, so the display hears the room at its quietest.</p>
    <button disabled={!online} onclick={() => measure('quiet')}><Icon name="quiet" />Measure quiet</button>
  {:else if step === 'quiet'}
    <p>Measuring quiet… {left} s</p>
  {:else if step === 'quiet_done'}
    <p class="muted">Quiet: {quiet?.median.toFixed(1)} dBFS. Now play music on your analog source at your usual volume.</p>
    <button disabled={!online} onclick={() => measure('music')}><Icon name="music" />Measure music</button>
  {:else if step === 'music'}
    <p>Measuring music… {left} s</p>
  {:else if suggestion && quiet && music}
    <table>
      <thead><tr><th></th><th class="r">Quietest 10%</th><th class="r">Typical</th><th class="r">Loudest 10%</th></tr></thead>
      <tbody>
        <tr><td>Quiet</td><td class="r num">{quiet.p10.toFixed(1)}</td><td class="r num">{quiet.median.toFixed(1)}</td><td class="r num">{quiet.p90.toFixed(1)}</td></tr>
        <tr><td>Music</td><td class="r num">{music.p10.toFixed(1)}</td><td class="r num">{music.median.toFixed(1)}</td><td class="r num">{music.p90.toFixed(1)}</td></tr>
      </tbody>
    </table>
    <p>Suggested: music above <strong>{suggestion.music} dBFS</strong>, gap below <strong>{suggestion.gap} dBFS</strong>.</p>
    {#if suggestion.tooClose}<p class="warn">Music is barely louder than the room. Turn it up or raise mic gain, then retry.</p>{/if}
    <div class="row"><button class="primary" onclick={() => onuse(suggestion.music, suggestion.gap)}><Icon name="check" />Use these</button><span class="faint">Then Save.</span></div>
  {/if}
  {#if error}<p class="error">{error}</p>{/if}

  {#if (step === 'quiet' || step === 'music') && live.settings}
    <LevelGraph music={live.settings.listening.music_threshold_db} gap={live.settings.listening.gap_threshold_db} height={100} />
  {/if}
</div>

<style>
  .calibrate { border-top: 1px solid var(--rule); padding: 14px 0 12px; display: flex; flex-direction: column; gap: 10px; font-size: 14px; }
  .r { text-align: right; }
  .warn { color: var(--warning); }
  .calibrate button { align-self: flex-start; }
</style>
