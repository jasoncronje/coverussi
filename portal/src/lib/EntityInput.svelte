<script lang="ts">
  // A Home Assistant entity id field that suggests entities as you type (the
  // board asks Home Assistant). Without Home Assistant on the board, a plain field.
  import { api } from './api';
  import type { HaEntity } from './types';

  let { value, onchange, onpick, domains = [], exclude = [], placeholder = '', label, autofocus = false }: {
    value: string;
    onchange: (v: string) => void;
    onpick?: (v: string) => void;  // a suggestion chosen, or Enter
    domains?: string[];
    exclude?: string[];  // already chosen: not suggested
    placeholder?: string;
    label: string;
    autofocus?: boolean;
  } = $props();

  const id = `entity-${Math.random().toString(36).slice(2, 9)}`;
  let input: HTMLInputElement;
  let hits = $state<HaEntity[]>([]);
  let open = $state(false);
  let active = $state(-1);
  let loading = $state(false);
  let failed = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let asked = 0;

  $effect(() => {
    if (autofocus) input.focus();
  });

  function lookup(q: string) {
    clearTimeout(timer);
    timer = setTimeout(async () => {
      const mine = ++asked;
      loading = true;
      // Show "Searching…" right away rather than nothing until the answer.
      open = document.activeElement === input;
      try {
        const r = await api.haEntities(q, domains);
        if (mine !== asked) return;  // a later keystroke answered first
        hits = r.entities.filter((e) => !exclude.includes(e.entity));
        failed = false;
        active = -1;
        open = r.available && document.activeElement === input;
      } catch {
        if (mine === asked) {
          hits = [];
          failed = true;
          open = document.activeElement === input;
        }
      } finally {
        if (mine === asked) loading = false;
      }
    }, 250);
  }

  function pick(e: HaEntity) {
    onchange(e.entity);
    onpick?.(e.entity);
    open = false;
  }

  function key(e: KeyboardEvent) {
    if (e.key === 'ArrowDown' && hits.length) {
      e.preventDefault();
      open = true;
      active = (active + 1) % hits.length;
    } else if (e.key === 'ArrowUp' && hits.length) {
      e.preventDefault();
      active = active <= 0 ? hits.length - 1 : active - 1;
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (open && active >= 0) pick(hits[active]);
      else onpick?.(value);
    } else if (e.key === 'Escape') {
      open = false;
    }
  }
</script>

<div class="entity">
  <input bind:this={input} {placeholder} {value} aria-label={label} autocomplete="off" spellcheck="false"
    role="combobox" aria-expanded={open} aria-controls={id} aria-autocomplete="list"
    aria-activedescendant={open && active >= 0 ? `${id}-${active}` : undefined}
    oninput={(e) => { onchange(e.currentTarget.value.trim()); lookup(e.currentTarget.value); }}
    onfocus={(e) => lookup(e.currentTarget.value)}
    onblur={() => setTimeout(() => (open = false), 120)}
    onkeydown={key} />
  {#if open}
    <ul class="list" role="listbox" {id}>
      {#if loading && !hits.length}
        <li class="none">Searching Home Assistant…</li>
      {:else if failed}
        <li class="none">Couldn't reach Home Assistant. Keep typing to try again.</li>
      {/if}
      {#each hits as h, i (h.entity)}
        <li id="{id}-{i}" role="option" aria-selected={i === active} class:active={i === active}
          onmousedown={(e) => { e.preventDefault(); pick(h); }}>
          <span class="name">{h.name || h.entity}</span>
          <span class="meta"><span class="id">{h.entity}</span><span class="state">{h.state}</span></span>
        </li>
      {:else}
        {#if !loading && !failed}<li class="none">No matching entities in Home Assistant.</li>{/if}
      {/each}
    </ul>
  {/if}
</div>

<style>
  .entity { position: relative; width: 300px; max-width: 100%; }
  input { width: 100%; }
  .list {
    position: absolute; z-index: 6; top: calc(100% + 4px); left: 0; right: 0; max-height: 280px; overflow: auto;
    margin: 0; padding: 4px; list-style: none; background: var(--raised); border-radius: var(--radius-inner);
    box-shadow: 0 10px 28px rgb(0 0 0 / 0.55);
  }
  li { display: flex; flex-direction: column; gap: 1px; padding: 7px 10px; border-radius: 7px; cursor: pointer; }
  li.active, li:hover { background: #363636; }
  .name { font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .meta { display: flex; gap: 8px; font-size: var(--fs-small); color: var(--muted); }
  .id { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .state { margin-left: auto; color: var(--faint); flex: none; }
  .none { cursor: default; color: var(--muted); font-size: var(--fs-small); }
  .none:hover { background: none; }
</style>
