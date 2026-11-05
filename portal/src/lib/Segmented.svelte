<script lang="ts" generics="T extends string | number">
  // A few mutually exclusive choices side by side, as a radio group.
  let { options, value, onchange, label, disabled = false }: {
    options: { value: T; label: string }[];
    value: T | null;
    onchange: (v: T) => void;
    label: string;
    disabled?: boolean;
  } = $props();
</script>

<div class="segmented" role="radiogroup" aria-label={label}>
  {#each options as o (o.value)}
    <button type="button" role="radio" aria-checked={value === o.value} class:on={value === o.value} {disabled}
      onclick={() => onchange(o.value)}>{o.label}</button>
  {/each}
</div>

<style>
  .segmented { display: inline-flex; padding: 3px; gap: 2px; border-radius: 999px; background: var(--ground); border: 1px solid var(--control); }
  button { height: 28px; padding: 0 14px; background: none; color: var(--muted); font-weight: 500; font-size: 13px; }
  button:hover:not(:disabled) { background: var(--raised); color: var(--text); }
  button.on, button.on:hover:not(:disabled) { background: #ffffff; color: #000000; }
  button:disabled { background: none; }
</style>
