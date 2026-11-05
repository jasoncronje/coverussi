<script lang="ts">
  // The 5 V supply a current limit needs, and what the panel draws now.
  import { live } from '../lib/live.svelte';
  import { BOARD_MA, HEADROOM, maxLimitMa, PORT_MAX_A, suggestedSupplyA } from '../lib/power';
  import Tip from '../lib/Tip.svelte';

  let { limit }: { limit: number } = $props();

  const supply = $derived(suggestedSupplyA(limit));
  const tooBig = $derived(supply > PORT_MAX_A);
  const drawing = $derived(live.connected ? live.telemetry.current_ma : undefined);
  const tip = `The limit, plus about ${BOARD_MA} mA for the board, plus ${Math.round((HEADROOM - 1) * 100)}% headroom, `
    + `rounded up to a common rating. A ${PORT_MAX_A} A supply covers limits up to ${maxLimitMa(PORT_MAX_A)} mA.`;
</script>

<div class="field">
  <span class="name">Suggested power supply <Tip text={tip} /></span>
  <span class="value"><strong>5 V {supply} A</strong></span>
  {#if tooBig}
    <span class="warn">More than the board’s POWER port takes ({PORT_MAX_A} A). Lower the limit to {maxLimitMa(PORT_MAX_A)} mA or less.</span>
  {/if}
  {#if drawing !== undefined}
    <span class="faint now">Drawing {drawing} mA now.</span>
  {/if}
</div>

<style>
  .field { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 2px 16px; align-items: center; padding: 8px 0; border-top: 1px solid var(--rule); }
  .name { display: inline-flex; align-items: center; gap: 8px; font-weight: 500; }
  .value { padding-right: 48px; }
  .warn { grid-column: 1 / -1; color: var(--warning); }
  .now { grid-column: 1 / -1; font-size: var(--fs-small); }
</style>
