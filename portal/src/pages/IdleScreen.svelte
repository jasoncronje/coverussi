<script lang="ts">
  // What the panel shows with no cover: nothing, a clock, a weather entity, or up
  // to four Home Assistant entities, each with an optional label.
  import EntityInput from '../lib/EntityInput.svelte';
  import Icon from '../lib/Icon.svelte';
  import Segmented from '../lib/Segmented.svelte';
  import type { Idle, IdleShow } from '../lib/types';

  let { idle, onchange }: { idle: Idle; onchange: (i: Idle) => void } = $props();

  const MAX = 4;
  const ENTITY = /^[a-z0-9_]+\.[a-z0-9_]+$/;
  const SHOWS: { value: IdleShow; label: string }[] = [
    { value: 'off', label: 'Nothing' },
    { value: 'clock', label: 'Clock' },
    { value: 'weather', label: 'Weather' },
    { value: 'entities', label: 'Entities' }
  ];

  const set = (i: Partial<Idle>) => onchange({ ...idle, ...i });
  const setEntity = (n: number, e: Partial<Idle['entities'][number]>) =>
    set({ entities: idle.entities.map((x, j) => (j === n ? { ...x, ...e } : x)) });
</script>

<div class="field">
  <span class="name">Show</span>
  <Segmented label="Show when nothing plays" value={idle.show} options={SHOWS} onchange={(show) => set({ show })} />
</div>

{#if idle.show === 'weather'}
  <div class="field">
    <span class="name">Weather entity</span>
    <span class="control">
      <EntityInput label="Weather entity" placeholder="Search, or type weather.home" domains={['weather']}
        value={idle.weather} onchange={(weather) => set({ weather })} />
    </span>
    {#if idle.weather && !ENTITY.test(idle.weather)}<p class="error small">An entity id, like weather.home.</p>{/if}
  </div>
{/if}

{#if idle.show === 'entities'}
  <ul class="list">
    {#each idle.entities as e, n}
      <li class="row">
        <EntityInput label="Entity" placeholder="Search, or type sensor.outside" value={e.entity}
          exclude={idle.entities.map((x) => x.entity)} onchange={(entity) => setEntity(n, { entity })} />
        <input class="label" aria-label="Label" placeholder="Label (optional)" value={e.name}
          oninput={(ev) => setEntity(n, { name: ev.currentTarget.value })} />
        <button class="remove" onclick={() => set({ entities: idle.entities.filter((_, j) => j !== n) })}
          aria-label="Remove entity" title="Remove"><Icon name="trash" size={15} /></button>
        {#if e.entity && !ENTITY.test(e.entity)}<p class="error small">An entity id, like sensor.outside_temperature.</p>{/if}
      </li>
    {/each}
  </ul>
  <div class="add">
    <span class="muted small">{idle.entities.length} of {MAX}</span>
    <button disabled={idle.entities.length >= MAX} onclick={() => set({ entities: [...idle.entities, { entity: '', name: '' }] })}>
      <Icon name="plus" size={15} />Add entity</button>
  </div>
{/if}

{#if idle.show === 'clock' || idle.show === 'weather'}
  <label class="field">
    <span class="name">24-hour clock</span>
    <input type="checkbox" class="switch" checked={idle.clock_24h} onchange={(e) => set({ clock_24h: e.currentTarget.checked })} />
  </label>
{/if}

<style>
  .field { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 2px 16px; align-items: center; padding: 8px 0; border-top: 1px solid var(--rule); }
  .field:first-child { border-top: none; }
  .name { font-weight: 500; }
  .control { display: flex; justify-content: flex-end; }
  .list { list-style: none; margin: 0; padding: 0; }
  .row { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; padding: 10px 0; border-top: 1px solid var(--rule); }
  .row .error { flex-basis: 100%; margin: 0; }
  input.label { width: 180px; }
  .remove { flex: none; width: 32px; height: 32px; padding: 0; justify-content: center; background: none; border-color: var(--control); color: var(--muted); }
  .remove:hover:not(:disabled) { background: var(--pill-negative-bg); border-color: var(--pill-negative-bg); color: var(--pill-negative-ink); }
  .add { display: flex; align-items: center; justify-content: space-between; padding: 8px 0; border-top: 1px solid var(--rule); }
  .small { font-size: var(--fs-small); }
  .error { grid-column: 1 / -1; margin: 0; }
  @media (max-width: 520px) {
    .field { grid-template-columns: 1fr; }
    .field:has(.switch) { grid-template-columns: minmax(0, 1fr) auto; }
    .control { justify-content: flex-start; }
    input.label { width: 100%; }
  }
</style>
