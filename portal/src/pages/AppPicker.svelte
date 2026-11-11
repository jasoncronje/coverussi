<script lang="ts">
  // What a player shows: one choice for everything it plays, then the apps or inputs
  // that differ. Each can show its cover, the progress screen or nothing, or be analog:
  // an input the player can't name the music of, which the display listens to.
  // The apps come from Home Assistant by way of the device, so a player added but not saved yet works.
  import { api } from '../lib/api';
  import { appShow, clearApp, isAnalog, otherShow, picks, setAnalog, setAppShow, setOtherShow, type RowShow } from '../lib/apps';
  import Icon from '../lib/Icon.svelte';
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
  <div class="app">
    <span class="label">{picked.length ? 'Everything else' : 'Everything it plays'}</span>
    <select aria-label="What it shows" value={otherShow(source)} onchange={(e) => onchange(setOtherShow(source, e.currentTarget.value as AppShow), analog)}>
      {#each SHOWS as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
    </select>
    <span class="clear"></span>
  </div>
  {#each picked as app (app)}
    <div class="app">
      <span class="label">{app}{#if same(app, current)}<span class="pill">Open now</span>{/if}</span>
      <select aria-label={`What ${app} shows`} value={rowShow(app)} onchange={(e) => setRow(app, e.currentTarget.value as RowShow)}>
        {#each ROW_SHOWS as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
      </select>
      <button class="clear" onclick={() => onchange(clearApp(source, app), setAnalog(analog, entity, app, false))}
        aria-label={`Remove ${app}`} title="Remove"><Icon name="x" size={14} /></button>
    </div>
  {/each}
  {#if addable.length}
    <select class="add" aria-label="Add an app or input" value="" onchange={(e) => { add(e.currentTarget.value); e.currentTarget.value = ''; }}>
      <option value="">+ Add an app or input</option>
      {#each addable as a (a)}<option value={a}>{a}{same(a, current) ? ' (open now)' : ''}</option>{/each}
    </select>
  {:else if asking}
    <span class="meta">Asking Home Assistant…</span>
  {/if}
  {#if error}<p class="error small">{error}</p>{/if}
</div>

<style>
  .picker { display: flex; flex-direction: column; gap: 6px; padding: 0 0 6px; max-width: 460px; }
  .app { display: flex; align-items: center; gap: 10px; min-height: 36px; }
  .label { flex: 1; min-width: 0; display: inline-flex; align-items: center; gap: 8px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .app select { flex: none; width: 124px; height: 32px; font-size: 13px; }
  .app .pill { height: 18px; padding: 0 7px; font-size: 11px; }
  .clear { flex: none; width: 28px; height: 28px; padding: 0; justify-content: center; background: none; color: var(--faint); }
  button.clear:hover:not(:disabled) { color: var(--text); }
  .add { align-self: flex-start; height: 32px; margin-top: 2px; font-size: 13px; color: var(--muted); background-color: transparent; border-color: transparent; padding-left: 0; }
  .add:hover { color: var(--text); }
  .meta { color: var(--muted); font-size: 13px; }
  .small { font-size: var(--fs-small); }
</style>
