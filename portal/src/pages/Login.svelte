<script lang="ts">
  import { api, ApiError } from '../lib/api';

  let { onlogin }: { onlogin: () => void } = $props();
  let password = $state('');
  let error = $state('');
  let busy = $state(false);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = '';
    try {
      await api.login(password);
      onlogin();
    } catch (err) {
      error = err instanceof ApiError && err.status === 401 ? 'Wrong password.' : err instanceof ApiError ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

<form class="card" onsubmit={submit}>
  <h1><img src="/wordmark.svg" alt="Coverussi" /></h1>
  <label>
    <span>Password</span>
    <!-- svelte-ignore a11y_autofocus -->
    <input type="password" bind:value={password} autocomplete="current-password" autofocus required />
  </label>
  {#if error}<p class="error">{error}</p>{/if}
  <button class="primary" disabled={busy || !password}>{busy ? 'Logging in…' : 'Log in'}</button>
</form>

<style>
  form { max-width: 360px; margin: 18vh auto 0; gap: 16px; }
  label { display: flex; flex-direction: column; gap: 8px; font-weight: 500; }
  button { align-self: flex-start; }
  h1 img { display: block; width: 100%; }
</style>
