<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../lib/api';
  import Icon from '../lib/Icon.svelte';
  import { time } from '../lib/format';
  import { live } from '../lib/live.svelte';
  import type { Song } from '../lib/types';

  let error = $state('');
  let corrections = $state<Record<string, { album: string; art_url: string | null }>>({});
  let fixing = $state<number | null>(null);
  let fix = $state({ album: '', art_url: '' });

  async function loadCorrections() {
    corrections = await api.corrections();
  }
  onMount(() => void loadCorrections());

  const correctionCount = $derived(Object.keys(corrections).length);
  const sourceName = (entity: string) => entity.replace(/^media_player\./, '').replaceAll('_', ' ');

  function startFix(i: number, s: Song) {
    fixing = i;
    fix = { album: s.album ?? '', art_url: s.art_url ?? '' };
  }

  async function saveFix(s: Song) {
    error = '';
    try {
      await api.saveCorrection({ artist: s.artist, shazam_album: s.shazam_album ?? '', album: fix.album.trim(), art_url: fix.art_url.trim() || null });
      fixing = null;
      await loadCorrections();
    } catch (e) {
      error = String(e);
    }
  }

  async function removeCorrection(key: string) {
    await api.deleteCorrection(key);
    await loadCorrections();
  }
</script>

<div class="title-row">
  <div>
    <h1>Recent</h1>
    <p class="muted">The last {live.recent.length || 10} songs · {correctionCount} album {correctionCount === 1 ? 'correction' : 'corrections'}</p>
  </div>
</div>
{#if error}<p class="error">{error}</p>{/if}

<section>
  {#if live.recent.length === 0}
    <p class="empty">Nothing yet.</p>
  {:else}
    <div class="scroll">
      <table>
        <thead><tr><th class="art"></th><th>Track</th><th>Album</th><th>From</th><th class="r">When</th><th class="r"></th></tr></thead>
        <tbody>
          {#each live.recent as s, i (`${s.at}-${i}`)}
            <tr>
              <td class="art">{#if s.art_url}<img class="art-thumb" src={s.art_url} alt="" loading="lazy" />{:else}<div class="art-thumb"></div>{/if}</td>
              <td class="track"><span class="name">{s.title}</span><span class="sub">{s.artist}</span></td>
              <td>
                {s.album ?? '–'}
                {#if s.heard && s.shazam_album && s.shazam_album !== s.album}<span class="sub">Shazam: {s.shazam_album}</span>{/if}
              </td>
              <td>{s.heard ? s.source || 'Analog' : sourceName(s.source)}</td>
              <td class="r num muted">{time(s.at)}</td>
              <td class="r">{#if s.heard && s.shazam_album}<button class="link" onclick={() => startFix(i, s)}><Icon name="pencil" />Fix album</button>{/if}</td>
            </tr>
            {#if fixing === i}
              <tr class="fix">
                <td></td>
                <td colspan="5">
                  <form class="row" onsubmit={(e) => { e.preventDefault(); void saveFix(s); }}>
                    <span class="muted">When Shazam says <strong>{s.shazam_album}</strong> for {s.artist}, show</span>
                    <input aria-label="Album" placeholder="Album" bind:value={fix.album} required />
                    <input class="grow" aria-label="Art URL" placeholder="Art URL (optional)" bind:value={fix.art_url} />
                    <button class="primary"><Icon name="check" />Save</button>
                    <button type="button" onclick={() => (fixing = null)}>Cancel</button>
                  </form>
                </td>
              </tr>
            {/if}
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</section>

<section class="corrections">
  <div class="label">
    <h2>Album corrections</h2>
  </div>
  <div class="card">
    {#if correctionCount === 0}
      <p class="empty">No corrections.</p>
    {:else}
      <table>
        <thead><tr><th class="art"></th><th>Shazam says</th><th>Shows</th><th class="r"></th></tr></thead>
        <tbody>
          {#each Object.entries(corrections) as [key, c] (key)}
            <tr>
              <td class="art">{#if c.art_url}<img class="art-thumb" src={c.art_url} alt="" />{:else}<div class="art-thumb"></div>{/if}</td>
              <td class="muted">{key}</td>
              <td>{c.album}</td>
              <td class="r"><button class="link" onclick={() => removeCorrection(key)}><Icon name="x" />Remove</button></td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </div>
</section>

<style>
  .scroll { overflow-x: auto; }
  th.art, td.art { width: 60px; }
  .track { min-width: 200px; }
  .name { font-weight: 500; }
  .fix td { background: var(--surface); }
  .fix form { padding: 4px 0; }
  .grow { flex: 1; min-width: 200px; }
  .corrections { display: grid; grid-template-columns: 360px minmax(0, 1fr); gap: var(--gap) 32px; margin-top: 48px; padding-top: 32px; border-top: 1px solid var(--border); }
  .label { display: flex; flex-direction: column; gap: 6px; }
  @media (max-width: 860px) { .corrections { grid-template-columns: 1fr; } }
</style>
