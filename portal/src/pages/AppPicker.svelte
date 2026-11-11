<script lang="ts">
  // What a player shows: one choice for everything it plays, then the apps or inputs
  // that differ. Each can show its cover, the progress screen or nothing, or be analog:
  // an input the player can't name the music of, which the display listens to.
  // The apps come from Home Assistant by way of the device, so a player added but not saved yet works.
  import { api } from '../lib/api';
  import { appShow, clearApp, isAnalog, otherShow, picks, setAnalog, setAppShow, setOtherShow, type RowShow } from '../lib/apps';
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

  // The exceptions: what's been picked. Everything else the player lists can be added.
  const picked = $derived(picks(source, analog));
  const addable = $derived.by(() => {
    const out: string[] = [];
    for (const a of [current, ...(result?.apps ?? [])]) if (a && ![...picked, ...out].some((o) => same(o, a))) out.push(a);
    return out;
  });
  // A new exception starts on what everything else shows, to be changed on its row.
  function add(app: string) {
    if (app) onchange(setAppShow(source, app, otherShow(source)), analog);
  }
</script>

<div class="picker">
  <div class="app all">
    <span class="label">When it plays<span class="sub">{picked.length ? 'Anything not listed below' : 'Every app and input'}</span></span>
    <Segmented label="What it shows" value={otherShow(source)} options={SHOWS} onchange={(v) => onchange(setOtherShow(source, v), analog)} />
  </div>
  {#if picked.length}
    <div class="divider">Except</div>
    <ul class="apps">
      {#each picked as app (app)}
        <li class="app">
          <span class="label">{app}</span>
          {#if same(app, current)}<span class="pill">Open now</span>
          {:else if result && !result.apps.some((a) => same(a, app))}<span class="faint small" title="This player doesn’t list it now">Not listed</span>{/if}
          <Segmented label={`What ${app} shows`} value={rowShow(app)} options={ROW_SHOWS} onchange={(v) => setRow(app, v)} />
          <button class="clear" onclick={() => onchange(clearApp(source, app), setAnalog(analog, entity, app, false))}
            aria-label={`Remove ${app}`} title="Remove: it follows everything else again"><Icon name="x" size={14} /></button>
        </li>
      {/each}
    </ul>
  {/if}
  <div class="tools">
    {#if addable.length}
      <select aria-label="Add an app or input" value="" onchange={(e) => { add(e.currentTarget.value); e.currentTarget.value = ''; }}>
        <option value="">Add an app or input…</option>
        {#each addable as a (a)}<option value={a}>{a}{same(a, current) ? ' (open now)' : ''}</option>{/each}
      </select>
    {:else if result && !result.apps.length && !asking}
      <span class="meta">Home Assistant lists no apps or inputs for this player.</span>
    {/if}
    {#if asking}<span class="meta">Asking Home Assistant…</span>{/if}
    <span class="spacer"></span>
    <button class="icon-button" disabled={asking} onclick={refresh} aria-label="Refresh the app list" title="Refresh the app list">
      <span class:spin={asking}><Icon name="refresh" size={15} /></span>
    </button>
  </div>
  {#if error}<p class="error small">{error}</p>{/if}
</div>

<style>
  .picker { display: flex; flex-direction: column; gap: 8px; padding: 4px 0 6px; }
  .tools { display: flex; align-items: center; gap: 14px; flex-wrap: wrap; }
  .tools select { height: 32px; font-size: 13px; max-width: 100%; }
  .spacer { flex: 1; }
  .meta { color: var(--muted); font-size: 13px; margin: 0; }
  .icon-button { width: 32px; height: 32px; padding: 0; justify-content: center; color: var(--muted); }
  .icon-button:hover:not(:disabled) { color: var(--text); }
  .spin { display: inline-flex; animation: spin 0.9s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) { .spin { animation: none; } }
  .apps { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
  .app { display: flex; align-items: center; gap: 10px; min-height: 42px; padding: 0 4px 0 10px; border-radius: 8px; }
  .apps .app:hover { background: var(--raised); }
  .all { padding-left: 0; }
  .all .label { display: flex; flex-direction: column; font-weight: 500; }
  .sub { color: var(--muted); font-size: 13px; font-weight: 400; }
  .divider { display: flex; align-items: center; gap: 10px; margin-top: 4px; color: var(--muted); font-size: 11px; font-weight: 500; letter-spacing: 0.04em; text-transform: uppercase; }
  .divider::after { content: ''; flex: 1; border-top: 1px solid var(--rule); }
  .label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .app .pill { height: 18px; padding: 0 7px; font-size: 11px; }
  .clear { flex: none; width: 28px; height: 28px; padding: 0; justify-content: center; background: none; color: var(--faint); }
  .clear:hover:not(:disabled) { color: var(--text); }
  .small { font-size: var(--fs-small); }
  @media (max-width: 620px) {
    .app { flex-wrap: wrap; padding: 6px 0; }
    .app .label { flex-basis: 100%; }
  }
</style>
