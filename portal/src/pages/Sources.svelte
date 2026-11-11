<script lang="ts">
  // The sources, in one list. A player says what it plays: its cover or the
  // progress screen shows, and an input it can't name the music of is analog,
  // which the display listens to. Anything else (a plug, a switch) is analog
  // while a Home Assistant state matches. Each is a row that opens to edit it,
  // with a switch to turn it off and a grip to drag it: the highest one playing
  // is shown. Last comes the room.
  import { untrack } from 'svelte';
  import { api } from '../lib/api';
  import { limited, onPlayer, priority, showSummary } from '../lib/apps';
  import EntityInput from '../lib/EntityInput.svelte';
  import Icon from '../lib/Icon.svelte';
  import { live } from '../lib/live.svelte';
  import SuggestInput from '../lib/SuggestInput.svelte';
  import Tip from '../lib/Tip.svelte';
  import type { AnalogSource, AppsResult, DigitalSource, HaTriggers, Settings } from '../lib/types';
  import AppPicker from './AppPicker.svelte';

  // ha: Home Assistant is set up; without it only the room can be listened to.
  let { sources, onchange, ha = true }: { sources: Settings['sources']; onchange: (s: Settings['sources']) => void; ha?: boolean } = $props();

  const ENTITY = /^[a-z0-9_]+\.[a-z0-9_]+$/;

  // The rows, the highest first. An analog source that is a listed player's input is
  // set on that player; any other on a player's entity sits right above it, heard first.
  type Row = { kind: 'other'; i: number; a: AnalogSource } | { kind: 'player'; i: number; d: DigitalSource };
  const rows = $derived<Row[]>(priority(sources).flatMap((entity) => [
    ...sources.analog.map((a, i) => ({ kind: 'other' as const, i, a })).filter((r) => r.a.entity === entity && !onPlayer(r.a, sources.digital)),
    ...sources.digital.map((d, i) => ({ kind: 'player' as const, i, d })).filter((r) => r.d.entity === entity),
  ]));
  const entityOf = (r: Row) => (r.kind === 'player' ? r.d.entity : r.a.entity);

  // Friendly names from Home Assistant, when the device can ask it.
  let names = $state<Record<string, string>>({});
  const asked = new Set<string>();  // asked once each; not state, so asking doesn't rerun the effect
  $effect(() => {
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
  const playerName = (entity: string) => names[entity] || pretty(entity);
  const otherName = (a: AnalogSource) => a.name || a.value || 'Unnamed source';

  // What each followed player is doing now, asked again when the display moves to another source or app.
  let info = $state<Record<string, AppsResult>>({});
  const followed = $derived(sources.digital.filter((d) => d.enabled).map((d) => d.entity).join(' '));
  const moved = $derived(`${live.telemetry.mode}|${live.telemetry.source}|${live.telemetry.app}`);
  let asking = 0;
  $effect(() => {
    void moved;
    const entities = followed ? followed.split(' ') : [];
    if (!ha || !entities.length) return;
    const mine = ++asking;
    untrack(async () => {
      for (const entity of entities) {  // one at a time: the device asks Home Assistant for each
        const r = await api.haApps(entity).catch(() => null);
        if (mine !== asking) return;
        if (r) info = { ...info, [entity]: r };
      }
    });
  });
  // The entity on the display: '' for the room, null for nothing.
  const shown = $derived(live.telemetry.mode && live.telemetry.mode !== 'off' ? (live.telemetry.source ?? '') : null);
  // A player showing its cover that sent no picture: the panel has nothing to show.
  const noCover = $derived(live.telemetry.mode === 'digital' && !live.telemetry.art);
  const playing = (entity: string) => ['playing', 'buffering'].includes(info[entity]?.state ?? '');
  const app = (entity: string) => info[entity]?.app_name || info[entity]?.source || '';
  const also = $derived(sources.digital.filter((d) => d.enabled && d.entity !== shown && playing(d.entity)));
  function status(d: DigitalSource): { text: string; pill: string } | null {
    if (!d.enabled) return { text: 'Not followed', pill: '' };
    const now = app(d.entity);
    if (shown === d.entity) return noCover ? { text: 'On display · no cover', pill: 'pill warning' } : { text: `On display${now ? ` · ${now}` : ''}`, pill: 'pill positive' };
    const r = info[d.entity];
    if (!r?.state) return null;
    if (playing(d.entity)) return { text: `Playing${now ? ` · ${now}` : ''}`, pill: 'pill' };
    return { text: r.state.charAt(0).toUpperCase() + r.state.slice(1), pill: '' };
  }

  // Rows open for editing, by key (entity for a player, index for the others).
  let open = $state<Record<string, boolean>>({});
  const toggle = (key: string) => (open[key] = !open[key]);

  // Reordering: drag a row by its grip, or focus the grip and use the arrow keys.
  let list = $state<HTMLUListElement>();
  let dragging = $state<number | null>(null);
  function move(from: number, to: number): boolean {
    const a = rows[from], b = rows[to];
    if (!a || !b || entityOf(a) === entityOf(b)) return false;
    const order = priority(sources);
    const at = order.indexOf(entityOf(b));
    order.splice(at, 0, ...order.splice(order.indexOf(entityOf(a)), 1));
    onchange({ ...sources, order });
    return true;
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
    const els = ([...list.children] as HTMLElement[]).slice(0, rows.length);
    let to = dragging;
    while (to > 0 && e.clientY < mid(els[to - 1])) to--;
    while (to < els.length - 1 && e.clientY > mid(els[to + 1])) to++;
    if (move(dragging, to)) dragging = to;
  }
  const mid = (r: HTMLElement) => { const b = r.getBoundingClientRect(); return b.top + b.height / 2; };
  function nudge(e: KeyboardEvent, i: number) {
    const by = e.key === 'ArrowUp' ? -1 : e.key === 'ArrowDown' ? 1 : 0;
    if (!by) return;
    e.preventDefault();
    if (!move(i, i + by)) return;
    // The grip moved with its row: keep it focused.
    const grip = e.currentTarget as HTMLElement;
    requestAnimationFrame(() => (list?.children[i + by]?.querySelector('.grip') as HTMLElement | null)?.focus() ?? grip.focus());
  }

  function digital(l: DigitalSource[]) {
    onchange({ ...sources, digital: l });
  }
  function analog(l: AnalogSource[]) {
    onchange({ ...sources, analog: l });
  }
  function setAnalog(i: number, a: Partial<AnalogSource>) {
    analog(sources.analog.map((x, j) => (j === i ? { ...x, ...a } : x)));
  }
  function removeAnalog(i: number) {
    analog(sources.analog.filter((_, j) => j !== i));
    open = Object.fromEntries(Object.entries(open).filter(([k]) => k.includes('.')));
  }

  // Adding: any Home Assistant entity. A media player is a player; anything else is analog while its state matches.
  let adding = $state<string | null>(null);  // the entity being typed, or null when not adding
  const isPlayer = (entity: string) => entity.startsWith('media_player.');
  const addError = $derived(
    !adding?.trim() ? '' : !ENTITY.test(adding.trim()) ? 'An entity id, like media_player.living_room or switch.cd_player.'
      : sources.digital.some((d) => d.entity === adding!.trim()) ? 'Already on the list.' : ''
  );
  function add(entity = adding ?? '') {
    entity = entity.trim();
    if (!entity || !ENTITY.test(entity) || sources.digital.some((d) => d.entity === entity)) return;
    if (isPlayer(entity)) {
      digital([...sources.digital, { entity, apps: [], enabled: true }]);
      open[entity] = true;
    } else {
      // Something switched on is the usual sign; anything else is picked on its row.
      const value = /^(switch|input_boolean|binary_sensor|light|fan)\./.test(entity) ? 'on' : '';
      analog([...sources.analog, { name: '', entity, attribute: 'state', value, enabled: true }]);
      open[`a${sources.analog.length}`] = true;
      want(entity);
    }
    adding = null;
  }

  const analogError = (a: AnalogSource) =>
    !ENTITY.test(a.entity) ? 'An entity id, like media_player.wiim_ultra or switch.cd_player.'
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

{#if shown !== null || also.length}
  <div class="now">
    {#if shown !== null}
      <p><span class="pill positive">On display</span>
        <span>{shown ? live.telemetry.source_name || playerName(shown) : 'The room'}{live.telemetry.app ? ` · ${live.telemetry.app}` : ''}{live.telemetry.mode === 'analog' ? ' · analog, identified by listening' : ''}{noCover ? ' · it sends no cover, so there’s nothing to show' : ''}</span></p>
    {:else}
      <p><span class="pill">On display</span><span class="muted">Nothing</span></p>
    {/if}
    {#if also.length}
      <p><span class="pill">Also playing</span>
        <span class="muted">{also.map((d) => `${playerName(d.entity)}${app(d.entity) ? ` · ${app(d.entity)}` : ''}`).join(', ')}</span></p>
    {/if}
  </div>
{/if}

<div class="head">
  <span class="count">{rows.length > 1 ? 'The highest one that’s playing is shown. Drag to reorder.' : rows.length ? '1 source' : 'No sources yet'}</span>
  <span class="spacer"></span>
  {#if ha && adding === null}<button onclick={() => (adding = '')}><Icon name="plus" size={15} />Add source</button>{/if}
</div>
{#if adding !== null}
  <div class="adder">
    <EntityInput label="Entity to add" placeholder="Search Home Assistant, or type media_player.living_room"
      exclude={sources.digital.map((d) => d.entity)} value={adding} autofocus onchange={(v) => (adding = v)} onpick={(v) => add(v)} />
    <button class="primary" disabled={!adding.trim() || !!addError} onclick={() => add()}>Add</button>
    <button class="link" onclick={() => (adding = null)}>Cancel</button>
    {#if addError}<p class="error small">{addError}</p>
    {:else}<p class="hint small">A media player shows what it plays. Anything else, like a switch, is analog while it’s on: the display listens.</p>{/if}
  </div>
{/if}
<ul class="list" bind:this={list}>
  {#each rows as r, n (r.kind === 'player' ? r.d.entity : `a${r.i}`)}
    {#if r.kind === 'player'}
      {@const d = r.d}
      {@const isOpen = !!open[d.entity]}
      {@const name = playerName(d.entity)}
      {@const s = status(d)}
      <li class="source" class:open={isOpen} class:off={!d.enabled} class:dragging={dragging === n}>
        <div class="summary">
          {#if rows.length > 1}
            <button class="grip" aria-label={`Move ${name}, with the arrow keys`} title="Drag to reorder"
              onpointerdown={(e) => grab(e, n)} onpointermove={drag} onpointerup={() => (dragging = null)} onpointercancel={() => (dragging = null)}
              onkeydown={(e) => nudge(e, n)}><Icon name="grip" size={16} /></button>
          {:else}<span class="grip"></span>{/if}
          <span class="n">{n + 1}</span>
          <button class="toggle" aria-expanded={isOpen} onclick={() => toggle(d.entity)}>
            <span class="chevron"><Icon name="chevron-down" size={16} /></span>
            <span class="what"><span class="name">{name}</span>
              <span class="sub" class:limited={!isOpen && d.enabled && limited(d, sources.analog)}>{d.enabled && !isOpen ? showSummary(d, sources.analog) : d.entity}</span></span>
            {#if s}<span class={s.pill || 'state'}>{s.text}</span>{/if}
          </button>
          <input type="checkbox" class="switch" checked={d.enabled} aria-label={`Follow ${name}`} title={d.enabled ? 'On' : 'Off'}
            onchange={(e) => digital(sources.digital.map((x, j) => (j === r.i ? { ...x, enabled: e.currentTarget.checked } : x)))} />
          <button class="remove" onclick={() => digital(sources.digital.filter((_, j) => j !== r.i))}
            aria-label={`Remove ${name}`} title="Remove"><Icon name="trash" size={15} /></button>
        </div>
        {#if isOpen}
          <div class="body">
            <AppPicker source={d} analog={sources.analog}
              onchange={(next, analog) => onchange({ ...sources, digital: sources.digital.map((x, j) => (j === r.i ? next : x)), analog })} />
          </div>
        {/if}
      </li>
    {:else}
      {@const a = r.a}
      {@const i = r.i}
      {@const key = `a${i}`}
      {@const isOpen = !!open[key]}
      {@const error = analogError(a)}
      <li class="source" class:open={isOpen} class:off={!a.enabled} class:dragging={dragging === n}>
        <div class="summary">
          {#if rows.length > 1}
            <button class="grip" aria-label={`Move ${otherName(a)}, with the arrow keys`} title="Drag to reorder"
              onpointerdown={(e) => grab(e, n)} onpointermove={drag} onpointerup={() => (dragging = null)} onpointercancel={() => (dragging = null)}
              onkeydown={(e) => nudge(e, n)}><Icon name="grip" size={16} /></button>
          {:else}<span class="grip"></span>{/if}
          <span class="n">{n + 1}</span>
          <button class="toggle" aria-expanded={isOpen} onclick={() => toggle(key)}>
            <span class="chevron"><Icon name="chevron-down" size={16} /></span>
            <span class="what">
              <span class="name">{otherName(a)}</span>
              <span class="sub">{a.entity && a.value ? `Analog while ${a.entity} ${trigger(a)}` : 'Not set up yet'}</span>
            </span>
            {#if error}<span class="pill negative">Incomplete</span>
            {:else if !a.enabled}<span class="state">Not followed</span>
            {:else if live.telemetry.mode === 'analog' && shown === a.entity}<span class="pill positive">On display</span>{/if}
          </button>
          <input type="checkbox" class="switch" checked={a.enabled} aria-label={`Follow ${otherName(a)}`} title={a.enabled ? 'On' : 'Off'}
            onchange={(e) => setAnalog(i, { enabled: e.currentTarget.checked })} />
          <button class="remove" onclick={() => removeAnalog(i)} aria-label={`Remove ${otherName(a)}`} title="Remove">
            <Icon name="trash" size={15} /></button>
        </div>
        {#if isOpen}
          <div class="body fields">
            <div class="line"><span>Entity</span>
              <EntityInput label="Entity" placeholder="Search, or type switch.cd_player" value={a.entity}
                onchange={(v) => setAnalog(i, { entity: v })} onpick={want} />
            </div>
            <div class="line"><span>Analog while</span>
              <span class="control">
                <select aria-label="Compare" value={a.attribute === 'state' ? 'state' : 'attribute'}
                  onchange={(e) => setAnalog(i, { attribute: e.currentTarget.value === 'state' ? 'state' : 'source' })}>
                  <option value="state">its state</option>
                  <option value="attribute">its attribute</option>
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
            <label class="line"><span>Name in History</span>
              <input class="wide" placeholder={a.value || 'Phono'} value={a.name} oninput={(e) => setAnalog(i, { name: e.currentTarget.value })} />
            </label>
            {#if error}<p class="error small">{error}</p>{/if}
          </div>
        {/if}
      </li>
    {/if}
  {/each}
  <li class="source room" class:off={!sources.listen_for_music}>
    <label class="summary">
      <span class="grip"></span>
      <span class="n"></span>
      <span class="toggle">
        <span class="chevron"></span>
        <span class="what">
          <span class="name">The room
            <Tip text="For music Home Assistant can’t see. With no source above playing, the mics listen to the room and the display identifies any music it hears. Any player playing, in any app, turns it off, so a TV added above stops it. If a few tries in a row find no match (a film, talking), it gives up until the room goes quiet." /></span>
          <span class="sub">Analog when nothing above plays. Always last.</span>
        </span>
        {#if sources.listen_for_music && live.telemetry.mode === 'analog' && shown === ''}<span class="pill positive">On display</span>{/if}
      </span>
      <input type="checkbox" class="switch" checked={sources.listen_for_music} aria-label="Listen to the room"
        onchange={(e) => onchange({ ...sources, listen_for_music: e.currentTarget.checked })} />
      <span class="remove-space"></span>
    </label>
  </li>
</ul>

<style>
  .now { display: flex; flex-direction: column; gap: 6px; padding: 4px 0 14px; margin-bottom: 4px; border-bottom: 1px solid var(--rule); }
  .now p { display: flex; align-items: center; gap: 10px; margin: 0; flex-wrap: wrap; }
  .muted { color: var(--muted); }
  .head { display: flex; align-items: center; gap: 10px; padding: 8px 0 12px; }
  .count { color: var(--muted); }
  .spacer { flex: 1; }
  .adder { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; padding: 0 0 14px; }
  .adder .error, .adder .hint { flex-basis: 100%; margin: 0; }
  .hint { color: var(--muted); }
  .list { list-style: none; margin: 0; padding: 0; }
  .source { border-top: 1px solid var(--rule); }
  .summary { display: flex; align-items: center; gap: 8px; }
  .toggle {
    flex: 1; min-width: 0; height: auto; padding: 12px 8px 12px 0; gap: 12px; border-radius: 0;
    background: none; text-align: left; justify-content: flex-start;
  }
  span.toggle { display: flex; align-items: center; }
  .toggle:hover:not(:disabled) { background: none; }
  button.toggle:hover .name { text-decoration: underline; text-underline-offset: 3px; text-decoration-color: var(--control); }
  .chevron { display: inline-flex; width: 16px; color: var(--faint); transform: rotate(-90deg); transition: transform 0.15s; }
  .open .chevron { transform: none; color: var(--text); }
  .what { display: flex; flex-direction: column; min-width: 0; flex: 1; }
  .name { display: inline-flex; align-items: center; gap: 8px; font-weight: 500; overflow: hidden; text-overflow: ellipsis; }
  .sub { color: var(--muted); font-size: 13px; font-weight: 400; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sub.limited { color: var(--text); }
  .state { flex: none; color: var(--muted); font-weight: 400; font-size: 13px; }
  .grip { flex: none; width: 24px; height: 32px; padding: 0; justify-content: center; background: none; border: none; color: var(--faint); cursor: grab; touch-action: none; }
  span.grip { cursor: default; }
  .n { flex: none; width: 14px; color: var(--faint); font-size: 13px; font-variant-numeric: tabular-nums; text-align: right; }
  button.grip:hover:not(:disabled) { background: none; color: var(--text); }
  .dragging { background: var(--raised); }
  .dragging .grip { cursor: grabbing; color: var(--text); }
  /* Turned off: the row stays, faded. */
  .off .what, .off .chevron { opacity: 0.5; }
  .switch { flex: none; }
  .remove { flex: none; width: 32px; height: 32px; padding: 0; justify-content: center; background: none; border-color: var(--control); color: var(--muted); }
  .remove:hover:not(:disabled) { background: var(--pill-negative-bg); border-color: var(--pill-negative-bg); color: var(--pill-negative-ink); }
  .remove-space { flex: none; width: 32px; }
  .room .summary { cursor: pointer; }
  .body { padding: 0 0 16px 82px; }
  .fields { display: flex; flex-direction: column; gap: 10px; }
  .line { display: grid; grid-template-columns: 124px minmax(0, 1fr); align-items: center; gap: 12px; color: var(--muted); }
  .control { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  input.wide { width: 300px; color: var(--text); }
  .small { font-size: var(--fs-small); }
  @media (max-width: 520px) {
    input.wide { width: 100%; }
    .line { grid-template-columns: 1fr; gap: 4px; }
    .body { padding-left: 0; }
  }
</style>
