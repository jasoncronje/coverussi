<script lang="ts">
  // Digital sources: smart players whose art shows, each with an optional app
  // list. Analog sources: devices that can't say what they play, each with the
  // Home Assistant state that means it's playing. Each source is a row that
  // opens to edit it, with a switch to turn it off and a grip to drag it: the
  // first one playing wins.
  import { api } from '../lib/api';
  import EntityInput from '../lib/EntityInput.svelte';
  import Icon from '../lib/Icon.svelte';
  import SuggestInput from '../lib/SuggestInput.svelte';
  import Tip from '../lib/Tip.svelte';
  import type { AnalogSource, DigitalSource, HaTriggers, Settings } from '../lib/types';
  import AppPicker from './AppPicker.svelte';

  // kind: which list this card edits; Settings shows each in its own card.
  let { kind, sources, onchange }: { kind: 'digital' | 'analog'; sources: Settings['sources']; onchange: (s: Settings['sources']) => void } = $props();

  const ENTITY = /^[a-z0-9_]+\.[a-z0-9_]+$/;

  // Friendly names from Home Assistant, when the hub can ask it.
  let names = $state<Record<string, string>>({});
  const asked = new Set<string>();  // asked once each; not state, so asking doesn't rerun the effect
  $effect(() => {
    if (kind !== 'digital') return;
    for (const { entity } of sources.digital) {
      if (asked.has(entity)) continue;
      asked.add(entity);
      api.haEntities(entity).then((r) => {
        const hit = r.entities.find((e) => e.entity === entity);
        if (hit?.name) names = { ...names, [entity]: hit.name };
      }).catch(() => {});
    }
  });
  const pretty = (entity: string) => {
    const s = entity.replace(/^[a-z_]+\./, '').replaceAll('_', ' ');
    return s.charAt(0).toUpperCase() + s.slice(1);
  };

  // Rows open for editing, by key (entity for digital, index for analog).
  let open = $state<Record<string, boolean>>({});
  const toggle = (key: string) => (open[key] = !open[key]);

  // Reordering: drag a row by its grip, or focus the grip and use the arrow keys.
  let list = $state<HTMLUListElement>();
  let dragging = $state<number | null>(null);
  function move(from: number, to: number) {
    const count = kind === 'digital' ? sources.digital.length : sources.analog.length;
    if (to < 0 || to >= count || to === from) return;
    const reorder = <T,>(l: T[]) => { const c = [...l]; c.splice(to, 0, ...c.splice(from, 1)); return c; };
    if (kind === 'digital') digital(reorder(sources.digital));
    else {
      // Analog rows are open by index: the open ones move with their source.
      const wasOpen = reorder(sources.analog.map((_, i) => !!open[`a${i}`]));
      open = Object.fromEntries(wasOpen.map((o, i) => [`a${i}`, o]));
      analog(reorder(sources.analog));
    }
  }
  function grab(e: PointerEvent, i: number) {
    if (e.button !== 0) return;
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dragging = i;
  }
  // Swaps once the pointer passes a neighbour's middle, so rows of different heights don't flip back and forth.
  function drag(e: PointerEvent) {
    if (dragging === null || !list) return;
    const rows = [...list.children] as HTMLElement[];
    let to = dragging;
    while (to > 0 && e.clientY < mid(rows[to - 1])) to--;
    while (to < rows.length - 1 && e.clientY > mid(rows[to + 1])) to++;
    if (to !== dragging) {
      move(dragging, to);
      dragging = to;
    }
  }
  const mid = (r: HTMLElement) => { const b = r.getBoundingClientRect(); return b.top + b.height / 2; };
  function nudge(e: KeyboardEvent, i: number) {
    const by = e.key === 'ArrowUp' ? -1 : e.key === 'ArrowDown' ? 1 : 0;
    if (!by) return;
    e.preventDefault();
    move(i, i + by);
    // The grip moved with its row: keep it focused.
    const grip = e.currentTarget as HTMLElement;
    requestAnimationFrame(() => (list?.children[i + by]?.querySelector('.grip') as HTMLElement | null)?.focus() ?? grip.focus());
  }

  // Digital
  let adding = $state<string | null>(null);  // the entity being typed, or null when not adding
  const addError = $derived(
    !adding?.trim() ? '' : !ENTITY.test(adding.trim()) ? 'An entity id, like media_player.living_room.'
      : sources.digital.some((d) => d.entity === adding!.trim()) ? 'Already on the list.' : ''
  );
  function digital(list: DigitalSource[]) {
    onchange({ ...sources, digital: list });
  }
  function add(entity = adding ?? '') {
    entity = entity.trim();
    if (!entity || !ENTITY.test(entity) || sources.digital.some((d) => d.entity === entity)) return;
    digital([...sources.digital, { entity, apps: [], enabled: true }]);
    adding = null;
  }
  function setApps(i: number, apps: string[], exclude: boolean) {
    digital(sources.digital.map((d, j) => (j === i ? { ...d, apps, exclude } : d)));
  }
  const appsSummary = (d: DigitalSource) => {
    if (!d.apps.length) return 'All apps';
    const names = d.apps.length <= 2 ? d.apps.join(', ') : `${d.apps.slice(0, 2).join(', ')} +${d.apps.length - 2}`;
    return d.exclude ? `All but ${names}` : names;
  };

  // Analog
  function analog(list: AnalogSource[]) {
    onchange({ ...sources, analog: list });
  }
  function setAnalog(i: number, a: Partial<AnalogSource>) {
    analog(sources.analog.map((x, j) => (j === i ? { ...x, ...a } : x)));
  }
  function addAnalog() {
    const name = sources.analog.length ? '' : 'Turntable';
    analog([...sources.analog, { name, entity: '', attribute: 'source', value: '', enabled: true }]);
    open[`a${sources.analog.length}`] = true;
  }
  function removeAnalog(i: number) {
    analog(sources.analog.filter((_, j) => j !== i));
    open = {};
  }
  const analogError = (a: AnalogSource) =>
    !ENTITY.test(a.entity) ? 'An entity id, like media_player.wiim_ultra or switch.turntable.'
      : !a.attribute.trim() ? 'Name the attribute, or pick its state.'
      : !a.value.trim() ? 'The value that means it’s playing.' : '';
  // What each entity can match on, asked of Home Assistant when a trigger field is focused.
  let triggers = $state<Record<string, HaTriggers>>({});
  function want(entity: string) {
    if (!ENTITY.test(entity) || triggers[entity]) return;
    api.haTriggers(entity).then((t) => (triggers = { ...triggers, [entity]: t })).catch(() => {});
  }
  const attributes = (a: AnalogSource) => Object.keys(triggers[a.entity]?.attributes ?? {});
  const values = (a: AnalogSource) =>
    (a.attribute === 'state' ? triggers[a.entity]?.states : triggers[a.entity]?.attributes[a.attribute]) ?? [];
  const trigger = (a: AnalogSource) => `${a.attribute === 'state' ? 'state' : a.attribute} is ${a.value}`;
</script>

{#if kind === 'digital'}
  <div class="head">
    <span class="count">{sources.digital.length ? `${sources.digital.length} player${sources.digital.length === 1 ? '' : 's'}` : 'No players yet'}</span>
    <span class="spacer"></span>
    {#if adding === null}<button onclick={() => (adding = '')}><Icon name="plus" size={15} />Add player</button>{/if}
  </div>
  {#if adding !== null}
    <div class="adder">
      <EntityInput label="Media player to add" placeholder="Search, or type media_player.living_room" domains={['media_player']}
        exclude={sources.digital.map((d) => d.entity)} value={adding} autofocus onchange={(v) => (adding = v)} onpick={(v) => add(v)} />
      <button class="primary" disabled={!adding.trim() || !!addError} onclick={() => add()}>Add</button>
      <button class="link" onclick={() => (adding = null)}>Cancel</button>
      {#if addError}<p class="error small">{addError}</p>{/if}
    </div>
  {/if}
  <ul class="list" bind:this={list}>
    {#each sources.digital as d, i (d.entity)}
      {@const isOpen = !!open[d.entity]}
      {@const name = names[d.entity] || pretty(d.entity)}
      <li class="source" class:open={isOpen} class:off={!d.enabled} class:dragging={dragging === i}>
        <div class="summary">
          {#if sources.digital.length > 1}
            <button class="grip" aria-label={`Move ${name}, with the arrow keys`} title="Drag to reorder"
              onpointerdown={(e) => grab(e, i)} onpointermove={drag} onpointerup={() => (dragging = null)} onpointercancel={() => (dragging = null)}
              onkeydown={(e) => nudge(e, i)}><Icon name="grip" size={16} /></button>
          {/if}
          <button class="toggle" aria-expanded={isOpen} onclick={() => toggle(d.entity)}>
            <span class="chevron"><Icon name="chevron-down" size={16} /></span>
            <span class="what"><span class="name">{name}</span><span class="sub">{d.entity}</span></span>
            <span class="state" class:limited={d.apps.length}>{d.enabled ? appsSummary(d) : 'Off'}</span>
          </button>
          <input type="checkbox" class="switch" checked={d.enabled} aria-label={`Follow ${name}`} title={d.enabled ? 'On' : 'Off'}
            onchange={(e) => digital(sources.digital.map((x, j) => (j === i ? { ...x, enabled: e.currentTarget.checked } : x)))} />
          <button class="remove" onclick={() => digital(sources.digital.filter((_, j) => j !== i))}
            aria-label={`Remove ${name}`} title="Remove"><Icon name="trash" size={15} /></button>
        </div>
        {#if isOpen}
          <div class="body"><AppPicker entity={d.entity} selected={d.apps} exclude={!!d.exclude} onchange={(apps, ex) => setApps(i, apps, ex)} /></div>
        {/if}
      </li>
    {/each}
  </ul>
{:else}
  <div class="head">
    <span class="count">{sources.analog.length ? `${sources.analog.length} source${sources.analog.length === 1 ? '' : 's'}` : 'No sources yet'}</span>
    <span class="spacer"></span>
    <button onclick={addAnalog}><Icon name="plus" size={15} />Add source</button>
  </div>
  <ul class="list" bind:this={list}>
    {#each sources.analog as a, i}
      {@const key = `a${i}`}
      {@const isOpen = !!open[key]}
      {@const error = analogError(a)}
      <li class="source" class:open={isOpen} class:off={!a.enabled} class:dragging={dragging === i}>
        <div class="summary">
          {#if sources.analog.length > 1}
            <button class="grip" aria-label={`Move ${a.name || 'analog source'}, with the arrow keys`} title="Drag to reorder"
              onpointerdown={(e) => grab(e, i)} onpointermove={drag} onpointerup={() => (dragging = null)} onpointercancel={() => (dragging = null)}
              onkeydown={(e) => nudge(e, i)}><Icon name="grip" size={16} /></button>
          {/if}
          <button class="toggle" aria-expanded={isOpen} onclick={() => toggle(key)}>
            <span class="chevron"><Icon name="chevron-down" size={16} /></span>
            <span class="what">
              <span class="name">{a.name || 'Unnamed source'}</span>
              <span class="sub">{a.entity && a.value ? `${a.entity} · ${trigger(a)}` : 'Not set up yet'}</span>
            </span>
            {#if error}<span class="pill negative">Incomplete</span>{:else if !a.enabled}<span class="state">Off</span>{/if}
          </button>
          <input type="checkbox" class="switch" checked={a.enabled} aria-label={`Follow ${a.name || 'analog source'}`} title={a.enabled ? 'On' : 'Off'}
            onchange={(e) => setAnalog(i, { enabled: e.currentTarget.checked })} />
          <button class="remove" onclick={() => removeAnalog(i)} aria-label={`Remove ${a.name || 'analog source'}`} title="Remove">
            <Icon name="trash" size={15} /></button>
        </div>
        {#if isOpen}
          <div class="body fields">
            <label class="line"><span>Name</span>
              <input class="wide" placeholder="Turntable" value={a.name} oninput={(e) => setAnalog(i, { name: e.currentTarget.value })} />
            </label>
            <div class="line"><span>Entity</span>
              <EntityInput label="Entity" placeholder="Search, or type switch.turntable" value={a.entity}
                onchange={(v) => setAnalog(i, { entity: v })} onpick={want} />
            </div>
            <div class="line"><span>Playing when</span>
              <span class="control">
                <select aria-label="Compare" value={a.attribute === 'state' ? 'state' : 'attribute'}
                  onchange={(e) => setAnalog(i, { attribute: e.currentTarget.value === 'state' ? 'state' : 'source' })}>
                  <option value="attribute">its attribute</option>
                  <option value="state">its state</option>
                </select>
                {#if a.attribute !== 'state'}
                  <SuggestInput label="Attribute" placeholder="source" value={a.attribute} trim
                    options={attributes(a)} onfocus={() => want(a.entity)}
                    onchange={(v) => setAnalog(i, { attribute: v })} />
                {/if}
                <span class="faint">is</span>
                <SuggestInput label="Value" placeholder={a.attribute === 'state' ? 'on' : 'Phono In'} value={a.value}
                  options={values(a)} onfocus={() => want(a.entity)}
                  onchange={(v) => setAnalog(i, { value: v })} />
              </span>
            </div>
            {#if error}<p class="error small">{error}</p>{/if}
          </div>
        {/if}
      </li>
    {/each}
  </ul>
  <label class="listen">
    <span class="listen-name">Listen for music when nothing else plays
      <Tip text="For a turntable or tape deck Home Assistant can’t see. With no source above playing, the mics listen to the room and the display identifies any music it hears. Any digital source playing, in any app, turns it off, so a TV added there stops it. If a few tries in a row find no match (a film, talking), it gives up until the room goes quiet." /></span>
    <span class="spacer"></span>
    <input type="checkbox" checked={sources.listen_for_music} onchange={(e) => onchange({ ...sources, listen_for_music: e.currentTarget.checked })} />
  </label>
{/if}

<style>
  .head { display: flex; align-items: center; gap: 10px; padding: 8px 0 12px; }
  .count { color: var(--muted); }
  .adder { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; padding: 0 0 14px; }
  .adder .error { flex-basis: 100%; margin: 0; }
  .list { list-style: none; margin: 0; padding: 0; }
  .source { border-top: 1px solid var(--rule); }
  .summary { display: flex; align-items: center; gap: 8px; }
  .toggle {
    flex: 1; min-width: 0; height: auto; padding: 12px 8px 12px 0; gap: 12px; border-radius: 0;
    background: none; text-align: left; justify-content: flex-start;
  }
  .toggle:hover:not(:disabled) { background: none; }
  .toggle:hover .name { text-decoration: underline; text-underline-offset: 3px; text-decoration-color: var(--control); }
  .chevron { display: inline-flex; color: var(--faint); transform: rotate(-90deg); transition: transform 0.15s; }
  .open .chevron { transform: none; color: var(--text); }
  .what { display: flex; flex-direction: column; min-width: 0; flex: 1; }
  .name { font-weight: 500; overflow: hidden; text-overflow: ellipsis; }
  .sub { color: var(--muted); font-size: 13px; font-weight: 400; overflow: hidden; text-overflow: ellipsis; }
  .state { flex: none; color: var(--muted); font-weight: 400; font-size: 13px; }
  .state.limited { color: var(--text); }
  .grip { flex: none; width: 24px; height: 32px; padding: 0; justify-content: center; background: none; border: none; color: var(--faint); cursor: grab; touch-action: none; }
  .grip:hover:not(:disabled) { background: none; color: var(--text); }
  .dragging { background: var(--raised); }
  .dragging .grip { cursor: grabbing; color: var(--text); }
  /* Turned off: the row stays, faded. */
  .off .what, .off .chevron { opacity: 0.5; }
  .switch { flex: none; }
  .remove { flex: none; width: 32px; height: 32px; padding: 0; justify-content: center; background: none; border-color: var(--control); color: var(--muted); }
  .remove:hover:not(:disabled) { background: var(--pill-negative-bg); border-color: var(--pill-negative-bg); color: var(--pill-negative-ink); }
  .body { padding: 0 0 16px 28px; }
  .fields { display: flex; flex-direction: column; gap: 10px; }
  .line { display: grid; grid-template-columns: 104px minmax(0, 1fr); align-items: center; gap: 12px; color: var(--muted); }
  .control { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  input.wide { width: 300px; color: var(--text); }
  .listen { display: flex; align-items: center; gap: 8px; margin-top: 4px; padding: 16px 0 6px; border-top: 1px solid var(--rule); cursor: pointer; }
  .listen-name { display: inline-flex; align-items: center; gap: 8px; font-weight: 500; }
  .small { font-size: var(--fs-small); }
  @media (max-width: 520px) {
    input.wide { width: 100%; }
    .line { grid-template-columns: 1fr; gap: 4px; }
    .body { padding-left: 0; }
    .sub { white-space: nowrap; }
  }
</style>
