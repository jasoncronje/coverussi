<script lang="ts">
  // The transition choices beside a preview that plays the chosen one.
  import TransitionPreview from './TransitionPreview.svelte';
  import { TRANSITIONS, type Transition } from './transition';

  let { value, ms, onchange }: { value: Transition; ms: number; onchange: (t: Transition) => void } = $props();
</script>

<div class="picker">
  <div class="choices" role="radiogroup" aria-label="Transition">
    {#each TRANSITIONS as t (t.id)}
      <button role="radio" aria-checked={value === t.id} class:on={value === t.id} onclick={() => onchange(t.id)}>{t.label}</button>
    {/each}
  </div>
  <TransitionPreview how={value} {ms} />
</div>

<style>
  .picker { display: flex; gap: 16px; align-items: flex-start; justify-content: space-between; }
  .choices { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 6px; }
  .choices button { text-align: left; white-space: nowrap; }
  .choices button.on { border-color: var(--accent); color: var(--accent); }
  @media (max-width: 520px) {
    .picker { flex-direction: column-reverse; align-items: stretch; }
  }
</style>
