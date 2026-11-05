<script lang="ts">
  // A text field that suggests from a list it's given, filtered by what's typed.
  // Any text is allowed; with no suggestions, a plain field.
  let { value, onchange, options, onfocus, placeholder = '', label, trim = false }: {
    value: string;
    onchange: (v: string) => void;
    options: string[];
    onfocus?: () => void;  // e.g. to fetch the options
    placeholder?: string;
    label: string;
    trim?: boolean;
  } = $props();

  const id = `suggest-${Math.random().toString(36).slice(2, 9)}`;
  let focused = $state(false);
  let typed = $state(false);  // all options on focus; filtered once typing starts
  let active = $state(-1);
  const hits = $derived(
    typed && value ? options.filter((o) => o.toLowerCase().includes(value.toLowerCase()) && o !== value) : options
  );
  const open = $derived(focused && hits.length > 0);

  function pick(o: string) {
    onchange(o);
    typed = false;
    focused = false;
  }

  function key(e: KeyboardEvent) {
    if (e.key === 'ArrowDown' && hits.length) {
      e.preventDefault();
      focused = true;
      active = (active + 1) % hits.length;
    } else if (e.key === 'ArrowUp' && hits.length) {
      e.preventDefault();
      active = active <= 0 ? hits.length - 1 : active - 1;
    } else if (e.key === 'Enter' && open && active >= 0) {
      e.preventDefault();
      pick(hits[active]);
    } else if (e.key === 'Escape') {
      focused = false;
    }
  }
</script>

<div class="suggest">
  <input {placeholder} {value} aria-label={label} autocomplete="off" spellcheck="false"
    role="combobox" aria-expanded={open} aria-controls={id} aria-autocomplete="list"
    aria-activedescendant={open && active >= 0 ? `${id}-${active}` : undefined}
    oninput={(e) => { onchange(trim ? e.currentTarget.value.trim() : e.currentTarget.value); typed = true; focused = true; active = -1; }}
    onfocus={() => { focused = true; typed = false; active = -1; onfocus?.(); }}
    onblur={() => setTimeout(() => (focused = false), 120)}
    onkeydown={key} />
  {#if open}
    <ul class="list" role="listbox" {id}>
      {#each hits as h, i (h)}
        <li id="{id}-{i}" role="option" aria-selected={i === active} class:active={i === active}
          onmousedown={(e) => { e.preventDefault(); pick(h); }}>{h}</li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .suggest { position: relative; width: 140px; }
  input { width: 100%; }
  .list {
    position: absolute; z-index: 6; top: calc(100% + 4px); left: 0; min-width: 100%; width: max-content; max-width: 300px;
    max-height: 280px; overflow: auto; margin: 0; padding: 4px; list-style: none; background: var(--raised);
    border-radius: var(--radius-inner); box-shadow: 0 10px 28px rgb(0 0 0 / 0.55);
  }
  li { padding: 7px 10px; border-radius: 7px; cursor: pointer; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  li.active, li:hover { background: #363636; }
</style>
