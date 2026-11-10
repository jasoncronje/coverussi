<script lang="ts">
  // The film screen's ring colour, and each app's own: matched like the app lists,
  // by Home Assistant's app name, app id or source, any case. The first that names the app wins.
  import Icon from '../lib/Icon.svelte';
  import type { Settings } from '../lib/types';

  type Display = Settings['display'];
  let { display, onchange }: { display: Display; onchange: (d: Display) => void } = $props();

  const set = (d: Partial<Display>) => onchange({ ...display, ...d });
  const setApp = (n: number, c: Partial<Display['film_colors'][number]>) =>
    set({ film_colors: display.film_colors.map((x, j) => (j === n ? { ...x, ...c } : x)) });
</script>

<label class="field">
  <span class="name">Ring colour</span>
  <input type="color" value={display.film_ring} oninput={(e) => set({ film_ring: e.currentTarget.value })} />
</label>

<ul class="list">
  {#each display.film_colors as c, n}
    <li class="row">
      <input class="app" aria-label="App" placeholder="App, like YouTube" value={c.app}
        oninput={(e) => setApp(n, { app: e.currentTarget.value })} />
      <input type="color" aria-label="{c.app || 'App'} colour" value={c.color}
        oninput={(e) => setApp(n, { color: e.currentTarget.value })} />
      <button class="remove" onclick={() => set({ film_colors: display.film_colors.filter((_, j) => j !== n) })}
        aria-label="Remove {c.app || 'app'}" title="Remove"><Icon name="trash" size={15} /></button>
      {#if !c.app.trim()}<p class="error small">Name the app as Home Assistant does, like YouTube.</p>{/if}
    </li>
  {/each}
</ul>
<div class="add">
  <span class="muted small">{display.film_colors.length} apps</span>
  <button onclick={() => set({ film_colors: [...display.film_colors, { app: '', color: display.film_ring }] })}>
    <Icon name="plus" size={15} />Add app</button>
</div>

<style>
  .field { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 2px 16px; align-items: center; padding: 8px 0; border-top: 1px solid var(--rule); }
  .field:first-child { border-top: none; }
  .name { font-weight: 500; }
  .list { list-style: none; margin: 0; padding: 0; }
  .row { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; padding: 10px 0; border-top: 1px solid var(--rule); }
  .row .error { flex-basis: 100%; margin: 0; }
  input.app { flex: 1; min-width: 0; max-width: 300px; }
  .remove { flex: none; width: 32px; height: 32px; padding: 0; justify-content: center; background: none; border-color: var(--control); color: var(--muted); margin-left: auto; }
  .remove:hover:not(:disabled) { background: var(--pill-negative-bg); border-color: var(--pill-negative-bg); color: var(--pill-negative-ink); }
  .add { display: flex; align-items: center; justify-content: space-between; padding: 8px 0; border-top: 1px solid var(--rule); }
</style>
