<script lang="ts">
  // The MQTT broker Home Assistant uses, for its entities. Settings' Save applies it.
  import Tip from '../lib/Tip.svelte';

  // passwordSet: one is stored, so an empty field keeps it. status: connected or not, under the fields.
  let { address = $bindable(), username = $bindable(), password = $bindable(), passwordSet, status }:
    { address: string; username: string; password: string; passwordSet: boolean; status: string } = $props();
</script>

<label class="field">
  <span class="name">Broker<Tip text="The broker Home Assistant’s MQTT integration uses, like mqtt://homeassistant.local:1883. With one set, the display shows up in Home Assistant as a device: the panel as a light, and what analog sources play. Empty: no MQTT." /></span>
  <span class="control"><input class="wide" bind:value={address} placeholder="mqtt://homeassistant.local:1883" autocomplete="off" /></span>
</label>
{#if address.trim()}
  <label class="field">
    <span class="name">Username</span>
    <span class="control"><input class="wide" bind:value={username} autocomplete="off" /></span>
  </label>
  <label class="field">
    <span class="name">Password</span>
    <span class="control"><input type="password" class="wide" bind:value={password} autocomplete="new-password" placeholder={passwordSet ? 'Unchanged' : ''} /></span>
  </label>
{/if}
{#if status}<p class="status muted">{status}</p>{/if}

<style>
  .field { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 2px 16px; align-items: center; padding: 8px 0; border-top: 1px solid var(--rule); }
  .field:first-child { border-top: none; }
  .name { display: inline-flex; align-items: center; gap: 8px; font-weight: 500; }
  .control { display: flex; align-items: center; gap: 8px; justify-content: flex-end; }
  .wide { width: 300px; }
  .status { padding: 0 0 8px; font-size: var(--fs-small); }
  @media (max-width: 520px) {
    .field { grid-template-columns: 1fr; }
    .control { justify-content: flex-start; }
    .wide { width: 100%; }
  }
</style>
