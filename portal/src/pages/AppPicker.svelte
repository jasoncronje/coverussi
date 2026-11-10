<script lang="ts">
  import { untrack } from 'svelte';
  // Lists a player's apps or inputs (read from Home Assistant by the device, so a
  // player added but not saved yet works) so each can show its cover, the progress screen or nothing,
  // or be analog: an input the player can't name the music of, which the display listens to.
  import { api } from '../lib/api';
  import { appShow, isAnalog, otherShow, setAnalog, setAppShow, setOtherShow, type RowShow } from '../lib/apps';
  import Icon from '../lib/Icon.svelte';
  import Segmented from '../lib/Segmented.svelte';
  import type { AnalogSource, AppShow, AppsResult, DigitalSource } from '../lib/types';

  // One change carries both lists: picking analog, or leaving it, touches the analog sources too.
  let { source, analog, onchange }: { source: DigitalSource; analog: AnalogSource[]; onchange: (d: DigitalSource, analog: AnalogSource[]) => void } = $props();
  const entity = $derived(source.entity);

  let result = $state<AppsResult | null>(null);
  let asking = $state(false);
  let error = $state('');

  async function refresh() {
    const asked = entity;
    error = '';
    asking = true;
    try {
      const r = await api.haApps(asked);
      if (asked === entity) result = r;
    } catch (e) {
      if (asked === entity) error = e instanceof Error ? e.message : String(e);
    } finally {
      if (asked === entity) asking = false;
    }
  }

  // Ask when the page opens and whenever the player changes, so the list is ready.
  $effect(() => {
    if (entity) void refresh();
  });

  const SHOWS: { value: AppShow; label: string }[] = [
    { value: 'cover', label: 'Cover' }, { value: 'progress', label: 'Progress' }, { value: 'off', label: 'Off' },
  ];
  const ROW_SHOWS: { value: RowShow; label: string }[] = [...SHOWS.slice(0, 2), { value: 'analog', label: 'Analog' }, SHOWS[2]];
  const same = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();
  const rowShow = (app: string): RowShow => (isAnalog(analog, entity, app) ? 'analog' : appShow(source, app));
  function setRow(app: string, show: RowShow) {
    // Home Assistant names an input in `source`; a player with apps only (an Apple TV) in `app_name`.
    const attribute = result?.app_name && !result.source ? 'app_name' : 'source';
    if (show === 'analog') onchange(source, setAnalog(analog, entity, app, true, attribute));
    else onchange(setAppShow(source, app, show), setAnalog(analog, entity, app, false));
  }
  const current = $derived(result?.app_name || result?.source || '');
  // Every app HA lists, then the one open now, then picks this player doesn't list.
  const apps = $derived.by(() => {
    const out: string[] = [];
    const heard = analog.filter((a) => a.entity === entity && (a.attribute === 'source' || a.attribute === 'app_name')).map((a) => a.value);
    const picked = [...source.apps, ...(source.progress ?? []), ...(source.off ?? []), ...heard];
    for (const a of [...(result?.apps ?? []), current, ...picked]) if (a && !out.some((o) => same(o, a))) out.push(a);
    return out;
  });

  let filter = $state('');
  const matches = $derived(filter.trim() ? apps.filter((a) => a.toLowerCase().includes(filter.trim().toLowerCase())) : apps);

  // Apps picked to show something other than every other app go first. The split is
  // taken when the list arrives, so a row doesn't jump away while it's being changed.
  let picked = $state<string[]>([]);
  $effect(() => {
    if (result) picked = untrack(() => apps.filter((a) => rowShow(a) !== otherShow(source)));
  });
  const isPicked = (app: string) => picked.some((p) => same(p, app));
  const picks = $derived(matches.filter(isPicked));
  const rest = $derived(matches.filter((a) => !isPicked(a)));
  // The rest folds after a few, but the open app always shows.
  const FOLDED = 4;
  let unfolded = $state(false);
  const restShown = $derived(
    unfolded || filter.trim() ? rest : rest.filter((a, i) => i < FOLDED || same(a, current)),
  );

  const status = $derived.by(() => {
    if (asking) return 'Asking Home Assistant…';
    if (!result) return '';
    if (!result.apps.length) return 'Home Assistant lists no apps or inputs for this player.';
    const count = (show: RowShow) => apps.filter((a) => rowShow(a) === show).length;
    const heard = count('analog') ? ` · ${count('analog')} analog` : '';
    return `${count('cover')} cover · ${count('progress')} progress${heard} · ${count('off')} off`;
  });
</script>

<div class="picker">
  <div class="tools">
    {#if apps.length > 8}
      <label class="search"><Icon name="search" size={14} /><input placeholder="Filter apps" bind:value={filter} aria-label="Filter apps" /></label>
    {/if}
    <span class="meta">{status}</span>
    {#if current}<span class="now"><span class="dot accent"></span>Open now: {current}</span>{/if}
    <span class="spacer"></span>
    <button class="icon-button" disabled={asking} onclick={refresh} aria-label="Refresh the app list" title="Refresh the app list">
      <span class:spin={asking}><Icon name="refresh" size={15} /></span>
    </button>
  </div>
  {#snippet row(app: string)}
    {@const show = rowShow(app)}
    <li class="app" class:on={show !== 'off'} class:quiet={show === otherShow(source)}>
      <span class="label">{app}</span>
      {#if same(app, current)}<span class="pill">Open</span>
      {:else if result && !result.apps.some((a) => same(a, app))}<span class="faint small" title="This player doesn’t list it now">Not listed</span>{/if}
      <Segmented label={`What ${app} shows`} value={show} options={ROW_SHOWS} onchange={(v) => setRow(app, v)} />
    </li>
  {/snippet}
  {#if picks.length}
    <div class="divider">Your picks</div>
    <ul class="apps">{#each picks as app (app)}{@render row(app)}{/each}</ul>
  {/if}
  {#if rest.length}
    {#if picks.length}<div class="divider">Others</div>{/if}
    <ul class="apps">{#each restShown as app (app)}{@render row(app)}{/each}</ul>
    {#if restShown.length < rest.length}
      <button class="more" onclick={() => (unfolded = true)}>Show {rest.length - restShown.length} more</button>
    {/if}
  {/if}
  {#if !matches.length && filter}
    <p class="meta">No app matches “{filter}”.</p>
  {/if}
  <div class="app other on">
    <span class="label">Every other app</span>
    <Segmented label="What every other app shows" value={otherShow(source)} options={SHOWS} onchange={(v) => onchange(setOtherShow(source, v), analog)} />
  </div>
  {#if error}<p class="error small">{error}</p>{/if}
</div>

<style>
  .picker { display: flex; flex-direction: column; gap: 10px; padding: 4px 0 6px; }
  .now { display: inline-flex; align-items: center; gap: 8px; color: var(--muted); font-size: 13px; }
  .tools { display: flex; align-items: center; gap: 14px; flex-wrap: wrap; }
  .spacer { flex: 1; }
  .meta { color: var(--muted); font-size: 13px; margin: 0; }
  .search { position: relative; display: inline-flex; align-items: center; color: var(--faint); }
  .search :global(svg) { position: absolute; left: 11px; pointer-events: none; }
  .search input { height: 32px; width: 200px; padding-left: 32px; font-size: 13px; }
  .icon-button { width: 32px; height: 32px; padding: 0; justify-content: center; color: var(--muted); }
  .icon-button:hover:not(:disabled) { color: var(--text); }
  .spin { display: inline-flex; animation: spin 0.9s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) { .spin { animation: none; } }
  .apps { list-style: none; margin: 0; padding: 0; display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 2px 16px; }
  .app { display: flex; align-items: center; gap: 10px; min-height: 42px; padding: 0 4px 0 10px; border-radius: 8px; color: var(--muted); }
  .app:hover { background: var(--raised); }
  .app.on { color: var(--text); }
  /* Following every other app: quiet, so the picks stand out. */
  .app.quiet { color: var(--muted); }
  .app.quiet :global(.segmented) { background: none; border-color: transparent; opacity: 0.6; }
  .app.quiet :global(button.on) { background: none; color: var(--muted); }
  .app.quiet:hover :global(.segmented) { opacity: 1; border-color: var(--control); }
  .divider { display: flex; align-items: center; gap: 10px; margin-top: 4px; color: var(--muted); font-size: 11px; font-weight: 500; letter-spacing: 0.04em; text-transform: uppercase; }
  .divider::after { content: ''; flex: 1; border-top: 1px solid var(--rule); }
  .more { align-self: flex-start; height: auto; padding: 2px 10px; background: none; color: var(--muted); font-size: 13px; text-decoration: underline; }
  .more:hover { color: var(--text); }
  .other { border-top: 1px solid var(--rule); border-radius: 0; padding-top: 6px; margin-top: 2px; }
  .other:hover { background: none; }
  .other .label { font-weight: 500; }
  .label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .app .pill { height: 18px; padding: 0 7px; font-size: 11px; }
  .small { font-size: var(--fs-small); }
  @media (max-width: 760px) { .apps { grid-template-columns: minmax(0, 1fr); } .search input { width: 150px; } }
</style>
