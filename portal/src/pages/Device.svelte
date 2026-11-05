<script lang="ts">
  import { tick } from 'svelte';
  import { api } from '../lib/api';
  import Icon from '../lib/Icon.svelte';
  import PanelCard from '../lib/PanelCard.svelte';
  import { bytes, duration, signal } from '../lib/format';
  import { live, type LogLine } from '../lib/live.svelte';
  import type { UpdateCheck } from '../lib/types';

  const t = $derived(live.telemetry);
  const online = $derived(live.connected);
  const panelOn = $derived(t.panel_on !== false);

  let uploading = $state(false);
  let restartArmed = $state(false);
  let message = $state('');
  let error = $state('');
  let follow = $state(true);
  let logBox = $state<HTMLDivElement>();
  let fileInput = $state<HTMLInputElement>();

  async function act(fn: () => Promise<unknown>, done: string) {
    error = message = '';
    try {
      await fn();
      message = done;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  const command = (name: string, args: Record<string, unknown> = {}) => void act(() => api.command(name, args), '');

  async function upload(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    uploading = true;
    await act(async () => {
      const r = await api.installFirmware(file);
      message = `Installed ${bytes(r.size)}, restarting`;
    }, '');
    uploading = false;
    input.value = '';
  }

  const firmwareName = $derived(t.firmware ?? '–');

  // GitHub's latest release, checked when the page opens and again after the board restarts into a new one.
  let check = $state<UpdateCheck | null>(null);
  let checkError = $state('');
  let requesting = $state(false);
  let updateError = $state('');
  const progress = $derived(live.update && live.update.phase !== 'failed' ? live.update : null);
  const busy = $derived(uploading || !!progress);
  const progressPercent = $derived(
    progress?.phase === 'restarting' ? 100 : progress?.phase === 'downloading' ? (progress.percent ?? 0) : 0
  );
  const progressText = $derived(
    progress?.phase === 'starting' ? 'Connecting to GitHub'
    : progress?.phase === 'downloading' ? `Downloading ${progressPercent}%`
    : 'Restarting'
  );

  async function checkForUpdate() {
    checkError = '';
    try {
      check = await api.checkUpdate();
    } catch (e) {
      check = null;
      checkError = e instanceof Error ? e.message : String(e);
    }
  }

  $effect(() => {
    if (online && firmwareName !== '–') void checkForUpdate();
  });

  $effect(() => {
    if (live.update?.phase === 'failed') updateError = live.update.error || 'unknown error';
  });

  async function startUpdate() {
    updateError = '';
    requesting = true;
    try {
      await api.startUpdate();
    } catch (e) {
      updateError = e instanceof Error ? e.message : String(e);
    }
    requesting = false;
  }

  // The firmware's lines read "I coverussi::work: text": the level, then the module, then the text.
  type Level = 'info' | 'warn' | 'error' | 'event' | 'command' | 'recognition';
  function row(l: LogLine): { time: string; source: string; level: Level; text: string } {
    const time = new Date(l.at).toLocaleTimeString([], { hour12: false });
    if (l.kind !== 'log') return { time, source: l.kind, level: l.kind, text: l.line };
    const m = /^([IWEDV]) ([\w:]+): ([\s\S]*)$/.exec(l.line);
    if (!m) return { time, source: '', level: 'info', text: l.line };
    const level = m[1] === 'W' ? 'warn' : m[1] === 'E' ? 'error' : 'info';
    const source = m[2] === 'coverussi' ? 'main' : (m[2].split('::').pop() ?? m[2]);
    return { time, source, level, text: m[3] };
  }
  const rows = $derived(live.logs.map(row));
  $effect(() => {
    void rows.length;
    if (follow) tick().then(() => logBox && (logBox.scrollTop = logBox.scrollHeight));
  });
</script>

<div class="title-row">
  <div>
    <h1>Controller</h1>
    <p class="status">
      <span class="dot" class:positive={online} class:negative={!online}></span>{online ? 'Online' : 'Offline'}
      {#if message}<span class="muted">· {message}</span>{/if}
    </p>
    {#if error}<p class="error">{error}</p>{/if}
  </div>
  <div class="actions">
    <button class="power" role="switch" aria-checked={panelOn} disabled={!online} onclick={() => command('panel', { on: !panelOn })}>
      <span class="switch" aria-hidden="true"><span></span></span>Panel
    </button>
    <button aria-pressed={Boolean(t.test_pattern)} disabled={!online || !panelOn} onclick={() => command('test_pattern', { on: !t.test_pattern })}>
      <Icon name={t.test_pattern ? 'x' : 'grid'} />{t.test_pattern ? 'Stop pattern' : 'Test pattern'}
    </button>
    <button disabled={!online || busy} onclick={() => fileInput?.click()}><Icon name="upload" />{uploading ? 'Installing…' : 'Install .bin'}</button>
    <input bind:this={fileInput} type="file" accept=".bin,application/octet-stream" onchange={upload} hidden />
    {#if restartArmed}
      <button onclick={() => (restartArmed = false)}>Cancel</button>
      <button class="danger" onclick={() => { restartArmed = false; void act(() => api.command('restart'), 'Restarting'); }}><Icon name="restart" />Restart</button>
    {:else}
      <button disabled={!online || busy} onclick={() => (restartArmed = true)}><Icon name="restart" />Restart…</button>
    {/if}
    {#if check?.newer || progress}
      <button class="primary" disabled={!online || requesting || !!progress} onclick={startUpdate}>
        <Icon name="download" />{progress ? 'Updating…' : `Update to ${check?.latest}`}
      </button>
    {/if}
  </div>
</div>

<div class="cards">
  <section class="card">
    <div class="card-head"><h2>Firmware</h2></div>
    <div class="version"><span class="label muted">Version</span><span class="value num">{firmwareName}</span></div>
    {#if progress}
      <div class="progress" role="progressbar" aria-label="Update" aria-valuemin="0" aria-valuemax="100" aria-valuenow={progressPercent}>
        <div class="fill" class:sweep={progress.phase === 'starting'} style:width={progress.phase === 'starting' ? null : `${progressPercent}%`}></div>
      </div>
      <p class="muted">{progressText}</p>
    {:else}
      {#if check?.newer}
        <p>{check.latest} available · <a href={check.page} target="_blank" rel="noreferrer">What's new</a></p>
      {:else if check}
        <p class="faint">Latest release</p>
      {:else if checkError}
        <p class="faint" title={checkError}>Couldn't reach GitHub</p>
      {/if}
      {#if updateError}<p class="error">Update failed: {updateError}</p>{/if}
    {/if}
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

<div class="block"><PanelCard /></div>

<div class="block"><section class="card">
  <div class="card-head">
    <h2>Live log</h2>
    <span class="spacer"></span>
    <label class="row muted"><input type="checkbox" bind:checked={follow} /> Follow</label>
  </div>
  <div class="log mono" bind:this={logBox}>
    {#each rows as r, i (i)}
      <div class="line {r.level}"><span class="time">{r.time}</span><span class="source">{r.source}</span><span class="text">{r.text}</span></div>
    {:else}
      <p class="empty-log faint">Nothing logged yet</p>
    {/each}
  </div>
</section></div>

<style>
  /* Every action in the title row, one height; Firmware, Status and Connections side by side below, then the panel and the log. */
  .title-row .actions button { height: var(--control-height); }
  .status .muted { margin-left: 2px; }
  .cards { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 20px 32px; }
  .block { border-top: 1px solid var(--rule); margin-top: 20px; padding-top: 20px; }
  .bad { color: var(--negative); }
  .version { display: flex; flex-direction: column; gap: 4px; }
  .version .label { font-size: 13px; line-height: 18px; }
  .version .value { font-size: var(--fs-kpi); line-height: 30px; font-weight: 500; }

  /* Power is a state, so it's a switch inside its button: white track when on. */
  .power { gap: 10px; padding-left: 10px; }
  .switch { position: relative; width: 30px; height: 18px; border-radius: 9px; background: var(--control); transition: background 160ms ease-out; }
  .switch span { position: absolute; top: 3px; left: 3px; width: 12px; height: 12px; border-radius: 50%; background: var(--muted); transition: transform 160ms ease-out, background 160ms ease-out; }
  .power[aria-checked='true'] .switch { background: var(--text); }
  .power[aria-checked='true'] .switch span { transform: translateX(12px); background: var(--ground); }
  button[aria-pressed='true'] { background: #ffffff; color: #000000; }
  button[aria-pressed='true']:hover:not(:disabled) { background: #e6e6e6; }

  .progress { height: 6px; border-radius: 999px; background: var(--raised); overflow: hidden; }
  .fill { height: 100%; border-radius: inherit; background: var(--accent); transition: width 300ms ease-out; }
  .fill.sweep { width: 30%; animation: sweep 1.4s ease-in-out infinite alternate; }
  @keyframes sweep { to { transform: translateX(233%); } }


  /* The log: time, module and text in columns, a long line wrapping inside its own column. */
  /* The rows share the box's columns, so the source column is as wide as its longest name. */
  .log { display: grid; grid-template-columns: auto auto minmax(0, 1fr); align-content: start; max-height: 420px; overflow-y: auto; background: var(--surface); border-radius: var(--radius-inner); padding: 10px 0; font-size: 13px; line-height: 20px; }
  .line { grid-column: 1 / -1; display: grid; grid-template-columns: subgrid; column-gap: 20px; padding: 2px 16px; }
  .line:hover { background: #181818; }
  .time { color: var(--faint); font-variant-numeric: tabular-nums; }
  .source { color: var(--muted); white-space: nowrap; }
  .text { color: var(--text); white-space: pre-wrap; overflow-wrap: break-word; min-width: 0; }
  .warn .source, .warn .text { color: var(--warning); }
  .error .source, .error .text { color: var(--negative); }
  .recognition .source { color: var(--accent); }
  .event .source, .command .source { color: var(--muted); }
  .empty-log { grid-column: 1 / -1; padding: 2px 16px; }

  @media (max-width: 1100px) {
    .cards { grid-template-columns: 1fr 1fr; }
    .cards > :first-child { grid-column: 1 / -1; }
  }
  @media (max-width: 640px) {
    .cards { grid-template-columns: 1fr; }
    .log { max-height: 360px; }
    .log { grid-template-columns: auto minmax(0, 1fr); }
    .line { column-gap: 12px; padding: 2px 12px; }
    .source { display: none; }
  }
  @media (prefers-reduced-motion: reduce) {
    .fill, .switch, .switch span { transition: none; }
    .fill.sweep { animation: none; width: 100%; opacity: 0.5; }
  }
</style>
