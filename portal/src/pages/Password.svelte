<script lang="ts" module>
  export const PASSWORD_MIN = 8;
</script>

<script lang="ts">
  // The web app's password: a switch, and the field when it's on. Settings' Save applies both.
  import { session } from '../lib/session.svelte';
  import Tip from '../lib/Tip.svelte';

  // tip: an explanation beside the switch, where no heading above gives one.
  let { enabled = $bindable(), value = $bindable(), tip = '' }: { enabled: boolean; value: string; tip?: string } = $props();
</script>

<label class="field">
  <span class="name">Require a password{#if tip}<Tip text={tip} />{/if}</span>
  <span class="control"><input type="checkbox" class="switch" bind:checked={enabled} /></span>
</label>
{#if enabled}
  <label class="field">
    <span class="name">{session.passwordSet ? 'New password' : 'Password'}</span>
    <span class="control">
      <input type="password" class="wide" bind:value autocomplete="new-password" maxlength={128}
        placeholder={session.passwordSet ? 'Unchanged' : `At least ${PASSWORD_MIN} characters`} />
    </span>
  </label>
{/if}

<style>
  .field { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 2px 16px; align-items: center; padding: 8px 0; border-top: 1px solid var(--rule); }
  .field:first-child { border-top: none; }
  .name { display: inline-flex; align-items: center; gap: 8px; font-weight: 500; }
  .control { display: flex; align-items: center; gap: 8px; justify-content: flex-end; }
  .wide { width: 300px; }
  @media (max-width: 520px) {
    .field { grid-template-columns: 1fr; }
    /* A switch stays beside its label. */
    .field:has(.switch) { grid-template-columns: minmax(0, 1fr) auto; }
    .control { justify-content: flex-start; }
    .wide { width: 100%; }
  }
</style>
