<script lang="ts">
  import { tick } from 'svelte';
  import { api } from '../lib/api';
  import Icon from '../lib/Icon.svelte';
  import PanelCard from '../lib/PanelCard.svelte';
  import { bytes, duration, signal, time } from '../lib/format';
  import { live } from '../lib/live.svelte';

  const t = $derived(live.telemetry);
  const online = $derived(live.connected);

  let uploading = $state(false);
  let restartArmed = $state(false);
  let message = $state('');
  let error = $state('');
  let follow = $state(true);
  // The log stays folded until asked for.
  let logOpen = $state(false);
  let logBox = $state<HTMLDivElement>();

  async function act(fn: () => Promise<unknown>, done: string) {
    error = message = '';
    try {
      await fn();
      message = done;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  async function upload(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    uploading = true;
    await act(async () => {
      const r = await api.installFirmware(file);
      message = `Installed ${bytes(r.size)}. Restarting; this page reconnects in about 15 s.`;
    }, '');
    uploading = false;
    input.value = '';
  }

  const logs = $derived(live.logs);
  $effect(() => {
    void logs.length;
    if (follow && logOpen) tick().then(() => logBox && (logBox.scrollTop = logBox.scrollHeight));
  });

  const kindClass = { log: '', event: 'event', command: 'command', recognition: 'recognition' } as const;
  const firmwareName = $derived(t.firmware ?? '–');
</script>

  <div class="title-row">
    <div>
      <p class="status"><span class="dot" class:positive={online} class:negative={!online}></span>{online ? 'Online' : 'Offline'}</p>
      {#if message}<p class="muted">{message}</p>{/if}
      {#if error}<p class="error">{error}</p>{/if}
    </div>
    <div class="actions">
      {#if restartArmed}
        <button onclick={() => (restartArmed = false)}>Cancel</button>
        <button class="danger" onclick={() => { restartArmed = false; void act(() => api.command('restart'), 'Restarting. This page reconnects in about 10 s.'); }}><Icon name="restart" />Restart</button>
      {:else}
        <button disabled={!online} onclick={() => (restartArmed = true)}><Icon name="restart" />Restart…</button>
      {/if}
    </div>
  </div>

  <div class="cards">
    <section class="card">
      <div class="card-head"><h2>Firmware</h2></div>
      <div class="version"><span class="label muted">Version</span><span class="value num">{firmwareName}</span></div>
      <label class="button" class:disabled={uploading}>
        <Icon name="upload" />{uploading ? 'Installing…' : 'Install .bin'}
        <input type="file" accept=".bin,application/octet-stream" onchange={upload} disabled={uploading} hidden />
      </label>
      <p class="faint">The app image from a release (<span class="mono">coverussi.bin</span>). It's checked before the board restarts into it.</p>
    </section>

    <section class="card">
      <div class="card-head"><h2>Status</h2></div>
      <dl class="facts">
        <dt>Running for</dt><dd>{t.uptime_s !== undefined ? duration(t.uptime_s) : '–'}</dd>
        <dt>Free memory</dt><dd>{t.free_heap !== undefined ? bytes(t.free_heap) : '–'}{#if t.min_free_heap !== undefined}<span class="faint">, lowest {bytes(t.min_free_heap)}</span>{/if}</dd>
        <dt>Last start</dt><dd class:bad={t.restart_reason === 'crash' || t.restart_reason === 'hang'}>{t.restart_reason ? t.restart_reason[0].toUpperCase() + t.restart_reason.slice(1) : '–'}</dd>
        <dt>Clock</dt><dd>{t.clock_set === undefined ? '–' : t.clock_set ? 'Set' : 'Not set yet'}</dd>
        <dt>Settings revision</dt><dd>{t.settings_version ?? live.settingsVersion}</dd>
      </dl>
    </section>

    <section class="card">
      <div class="card-head"><h2>Connections</h2></div>
      <dl class="facts">
        <dt>Wi-Fi signal</dt><dd>{#if t.wifi_rssi !== undefined}{signal(t.wifi_rssi)} <span class="faint">· {t.wifi_rssi} dBm</span>{:else}–{/if}</dd>
        <dt>Address</dt><dd>{t.ip || '–'}</dd>
        <dt>Home Assistant</dt><dd>{#if t.ha_connected === undefined}–{:else}<span class="pill" class:positive={t.ha_connected} class:negative={!t.ha_connected}>{t.ha_connected ? 'Connected' : 'Not connected'}</span>{/if}</dd>
        <dt>Audio in</dt><dd>{t.audio ?? '–'}</dd>
      </dl>
    </section>
  </div>

  <div class="block"><PanelCard command={(name, args = {}) => act(() => api.command(name, args), '')} /></div>

  <section class="card block">
    <div class="card-head">
      <button class="link head" aria-expanded={logOpen} onclick={() => (logOpen = !logOpen)}>
        <span class="chevron" class:open={logOpen}><Icon name="chevron-down" /></span><h2>Live log</h2>
      </button>
      <span class="spacer"></span>
      {#if logOpen}<label class="row muted"><input type="checkbox" bind:checked={follow} /> Follow</label>{/if}
    </div>
    {#if logOpen}
      <div class="log mono" bind:this={logBox}>
        {#each logs as l, i (i)}
          <div class="line"><span class="faint">{time(l.at / 1000)}</span> <span class="kind {kindClass[l.kind]}">{l.kind}</span> {l.line}</div>
        {:else}
          <p class="faint">Waiting for the display…</p>
        {/each}
      </div>
    {/if}
  </section>

<style>
  /* Firmware, Status and Connections side by side, then the panel and the log across the width, ruled off. */
  .cards { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 20px 32px; }
  .block { border-top: 1px solid var(--rule); margin-top: 20px; padding-top: 20px; }
  .bad { color: var(--negative); }
  .version { display: flex; flex-direction: column; gap: 4px; }
  .version .label { font-size: 13px; line-height: 18px; }
  .version .value { font-size: var(--fs-kpi); line-height: 30px; font-weight: 500; }
  .cards label.button { align-self: flex-start; }
  .head { color: var(--text); gap: 8px; }
  .head:hover:not(:disabled) { color: var(--text); }
  .chevron { display: flex; transform: rotate(-90deg); transition: transform 160ms ease-out; }
  .chevron.open { transform: none; }
  .log { height: 420px; overflow-y: auto; background: var(--ground); border-radius: var(--radius-inner); padding: 12px 14px; font-size: 12px; line-height: 19px; }
  .line { white-space: pre-wrap; overflow-wrap: anywhere; }
  .kind { display: inline-block; min-width: 88px; color: var(--faint); }
  .kind.event { color: var(--muted); }
  .kind.command { color: var(--text); }
  .kind.recognition { color: var(--accent); }
  label.button.disabled { opacity: 0.5; }
  @media (max-width: 1100px) {
    .cards { grid-template-columns: 1fr 1fr; }
    .cards > :first-child { grid-column: 1 / -1; }
  }
  @media (max-width: 640px) {
    .cards { grid-template-columns: 1fr; }
    .log { height: 360px; }
  }
  @media (prefers-reduced-motion: reduce) { .chevron { transition: none; } }
</style>
