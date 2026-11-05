<script lang="ts">
  // Lists a player's apps or inputs (read from Home Assistant by the device, so a
  // player added but not saved yet works) so the included or excluded ones can be picked instead of typed.
  import { api } from '../lib/api';
  import Icon from '../lib/Icon.svelte';
  import Segmented from '../lib/Segmented.svelte';
  import type { AppsResult } from '../lib/types';

  let { entity, selected, exclude, onchange }: {
    entity: string; selected: string[]; exclude: boolean; onchange: (apps: string[], exclude: boolean) => void;
  } = $props();

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

  const same = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();
  const isOn = (app: string) => selected.some((s) => same(s, app));
  const current = $derived(result?.app_name || result?.source || '');
  // Every app HA lists, then the one open now, then picks this player doesn't list.
  const apps = $derived.by(() => {
    const out: string[] = [];
    for (const a of [...(result?.apps ?? []), current, ...selected]) if (a && !out.some((o) => same(o, a))) out.push(a);
    return out;
  });

  function toggle(app: string) {
    onchange(isOn(app) ? selected.filter((s) => !same(s, app)) : [...selected, app], exclude);
  }

  // All apps (an empty list), only the ones picked, or all but them; Include or Exclude
  // can be chosen before any app is picked.
  type Mode = 'all' | 'include' | 'exclude';
  let wanted = $state<Mode>('all');
  const mode = $derived<Mode>(selected.length ? (exclude ? 'exclude' : 'include') : wanted);
  function setMode(m: Mode) {
    if (m === mode) return;
    wanted = m;
    // Switching sides starts over, so picks never flip their meaning.
    if (m === 'include') onchange(current ? [current] : [], false);  // start from what's open
    else onchange([], m === 'exclude');
  }

  let filter = $state('');
  const shown = $derived(filter.trim() ? apps.filter((a) => a.toLowerCase().includes(filter.trim().toLowerCase())) : apps);

  const status = $derived.by(() => {
    if (asking) return 'Asking Home Assistant…';
    if (!result) return '';
    if (!result.apps.length) return 'Home Assistant lists no apps or inputs for this player.';
    return `${selected.length} of ${result.apps.length} ${mode === 'exclude' ? 'excluded' : 'allowed'}`;
  });
</script>

<div class="picker">
  <div class="top">
    <Segmented label="Which apps show art" value={mode} onchange={setMode}
      options={[{ value: 'all', label: 'All apps' }, { value: 'include', label: 'Include' }, { value: 'exclude', label: 'Exclude' }]} />
    {#if current}<span class="now"><span class="dot accent"></span>Open now: {current}</span>{/if}
  </div>
  {#if mode !== 'all'}
    <div class="tools">
      {#if apps.length > 8}
        <label class="search"><Icon name="search" size={14} /><input placeholder="Filter apps" bind:value={filter} aria-label="Filter apps" /></label>
      {/if}
      <span class="meta">{status}</span>
      <span class="spacer"></span>
      <button class="icon-button" disabled={asking} onclick={refresh} aria-label="Refresh the app list" title="Refresh the app list">
        <span class:spin={asking}><Icon name="refresh" size={15} /></span>
      </button>
    </div>
    {#if !selected.length}<p class="meta">None picked yet: every app shows art until you pick one.</p>{/if}
    {#if shown.length}
      <ul class="apps">
        {#each shown as app (app)}
          {@const on = isOn(app)}
          <li>
            <label class="app" class:on>
              <input type="checkbox" checked={on} onchange={() => toggle(app)} />
              <span class="label">{app}</span>
              {#if same(app, current)}<span class="pill">Open</span>
              {:else if result && !result.apps.some((a) => same(a, app))}<span class="faint small" title="This player doesn’t list it now">Not listed</span>{/if}
            </label>
          </li>
        {/each}
      </ul>
    {:else if filter}
      <p class="meta">No app matches “{filter}”.</p>
    {/if}
  {/if}
  {#if error}<p class="error small">{error}</p>{/if}
</div>

<style>
  .picker { display: flex; flex-direction: column; gap: 12px; padding: 4px 0 6px; }
  .top { display: flex; align-items: center; gap: 14px; flex-wrap: wrap; }
  .now { display: inline-flex; align-items: center; gap: 8px; color: var(--muted); font-size: 13px; }
  .tools { display: flex; align-items: center; gap: 10px; }
  .meta { color: var(--muted); font-size: 13px; margin: 0; }
  .search { position: relative; display: inline-flex; align-items: center; color: var(--faint); }
  .search :global(svg) { position: absolute; left: 11px; pointer-events: none; }
  .search input { height: 32px; width: 200px; padding-left: 32px; font-size: 13px; }
  .icon-button { width: 32px; height: 32px; padding: 0; justify-content: center; color: var(--muted); }
  .icon-button:hover:not(:disabled) { color: var(--text); }
  .spin { display: inline-flex; animation: spin 0.9s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) { .spin { animation: none; } }
  .apps { list-style: none; margin: 0; padding: 0; display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 2px 12px; }
  .app { display: flex; align-items: center; gap: 10px; height: 34px; padding: 0 10px; border-radius: 8px; color: var(--muted); cursor: pointer; }
  .app:hover { background: var(--raised); color: var(--text); }
  .app.on { color: var(--text); }
  .label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .app .pill { height: 18px; padding: 0 7px; font-size: 11px; }
  .small { font-size: var(--fs-small); }
  @media (max-width: 640px) { .apps { grid-template-columns: repeat(2, minmax(0, 1fr)); } .search input { width: 150px; } }
</style>
