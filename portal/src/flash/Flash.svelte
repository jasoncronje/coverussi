<script lang="ts">
  import { onMount, tick } from 'svelte';
  import Icon from '../lib/Icon.svelte';
  import { Console, install, knownPort, loadManifest, pickPort, supported, type Info, type Manifest, type Values } from './device';

  // A link can fill in the Home Assistant address: #ha=… (the fragment never reaches GitHub).
  // Tokens are never passed this way.
  const given = new URLSearchParams(location.hash.slice(1));

  /** One step open at a time: install, connect, open the web app. */
  let step = $state<1 | 2 | 3>(1);

  let manifest = $state<Manifest | null | undefined>(undefined);
  let erase = $state(false);
  let installing = $state(false);
  let progress = $state(0);
  let installed = $state(false);
  let installError = $state('');

  let form = $state({ ssid: '', password: '', ha_url: given.get('ha') ?? '', ha_token: '', mqtt_url: '' });
  let busy = $state<'' | 'info' | 'send' | 'erase'>('');
  let info = $state<Info | null>(null);
  let sent = $state(false);
  let connectError = $state('');
  let joining = $state(false);
  let joinFailed = $state(false);
  let toolsResult = $state('');
  let toolsError = $state('');
  let eraseArmed = $state(false);

  let log = $state<string[]>([]);
  let logBox = $state<HTMLPreElement>();
  function addLog(line: string) {
    log = [...log.slice(-400), line];
    void tick().then(() => logBox && (logBox.scrollTop = logBox.scrollHeight));
  }

  onMount(() => {
    if (supported) void loadManifest().then((m) => (manifest = m));
  });

  const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
  const webApp = $derived(info?.ip ? `http://${info.ip}/` : '');

  function problem(e: unknown): string {
    if (e instanceof DOMException && e.name === 'NotFoundError') return ''; // closed the port picker
    const text = e instanceof Error ? e.message : String(e);
    if (/open|busy|access/i.test(text)) return 'The port is in use. Close any other app or tab talking to the board (a serial monitor, another flasher) and try again.';
    return text;
  }

  async function runInstall() {
    if (!manifest) return;
    installError = '';
    installed = false;
    let port: SerialPort;
    try {
      port = await pickPort();
    } catch (e) {
      installError = problem(e);
      return;
    }
    installing = true;
    progress = 0;
    try {
      await install(port, manifest, { erase, progress: (f) => (progress = f), log: addLog });
      installed = true;
      if (erase) info = null;
      step = 2;
    } catch (e) {
      installError = problem(e) || 'Install stopped.';
      installError += ' If it keeps failing, hold the board’s BOOT button while plugging it in, then try again.';
    } finally {
      installing = false;
    }
  }

  /** Opens the display's console, waits for the firmware, and runs `job` with it. */
  async function withDisplay(kind: 'info' | 'send' | 'erase', job: (c: Console) => Promise<void>, fail: (e: string) => void) {
    busy = kind;
    let con: Console | null = null;
    try {
      // Right after Install the display is restarting; give it a moment to come back.
      const port = (await knownPort(installed ? 8000 : 0)) ?? (await pickPort());
      con = await Console.open(port, addLog);
      info = await con.info();
      if (!info) throw new Error('The display didn’t answer. Unplug it, plug it back in, and try again.');
      await job(con);
    } catch (e) {
      fail(problem(e));
    } finally {
      await con?.close();
      busy = '';
    }
  }

  const reasons: Record<string, string> = {
    wifi: 'The display refused the Wi-Fi details: the name can be up to 32 characters, the password 8 to 63 (or empty for an open network).',
    url: 'The display refused an address: it has to start with http:// or https://.',
    mqtt: 'The display refused the MQTT broker: it has to look like mqtt://user:password@host.',
    token: 'The display refused a token: tokens are at least 16 characters, without spaces or quotes.',
    storage: 'The display couldn’t save the settings. Erase its settings (below), then send them again.',
    timeout: 'The display didn’t confirm. Send again.'
  };

  function values(): Values | string {
    const v: Values = {};
    const f = Object.fromEntries(Object.entries(form).map(([k, x]) => [k, k === 'password' ? x : x.trim()])) as typeof form;
    if (f.ssid) {
      if (f.ssid.length > 32) return 'The Wi-Fi name can be up to 32 characters.';
      if (f.password && (f.password.length < 8 || f.password.length > 63)) return 'A Wi-Fi password is 8 to 63 characters (leave it empty for an open network).';
      v.ssid = f.ssid;
      v.password = f.password;
    }
    if (f.ha_url) {
      if (!/^https?:\/\/\S+$/.test(f.ha_url)) return 'The Home Assistant address has to start with http:// or https://.';
      v.ha_url = f.ha_url.replace(/\/+$/, '');
    }
    if (f.ha_token) {
      if (f.ha_token.length < 16 || /[\s"]/.test(f.ha_token)) return 'The Home Assistant token looks wrong: at least 16 characters, without spaces or quotes.';
      v.ha_token = f.ha_token;
    }
    if (f.mqtt_url) {
      if (!/^mqtts?:\/\/\S+$/.test(f.mqtt_url)) return 'The MQTT broker has to look like mqtt://user:password@host.';
      v.mqtt_url = f.mqtt_url;
    }
    return Object.keys(v).length ? v : 'Fill in the Wi-Fi at least.';
  }

  function send() {
    connectError = '';
    const v = values();
    if (typeof v === 'string') {
      connectError = v;
      return;
    }
    void withDisplay(
      'send',
      async (c) => {
        const answer = await c.provision(v);
        if (answer !== 'ok') throw new Error(reasons[answer] ?? `The display refused the settings (${answer}).`);
        form.password = form.ha_token = form.mqtt_url = '';
        sent = true;
        step = 3;
      },
      (e) => (connectError = e)
    ).then(() => {
      if (sent && step === 3) return waitForWifi();
    });
  }

  /** After Send the display restarts and joins the Wi-Fi: ask it until it has an address. */
  async function waitForWifi() {
    joining = true;
    joinFailed = false;
    if (info) info = { ...info, ip: '' };
    const end = Date.now() + 60000;
    let con: Console | null = null;
    try {
      await sleep(2000); // the restart drops the port
      while (Date.now() < end && !info?.ip) {
        if (!con) {
          const port = await knownPort(5000);
          if (port) con = await Console.open(port, addLog).catch(() => null);
        }
        const answer = con ? await con.info(3000) : null;
        if (answer) info = answer;
        if (!info?.ip) await sleep(2000);
      }
    } finally {
      await con?.close();
      joining = false;
      joinFailed = !info?.ip;
    }
  }

  function check() {
    toolsResult = toolsError = '';
    void withDisplay('info', async () => {}, (e) => (toolsError = e));
  }

  function eraseSettings() {
    eraseArmed = false;
    toolsResult = toolsError = '';
    void withDisplay(
      'erase',
      async (c) => {
        if (!(await c.erase())) throw new Error('The display didn’t confirm the erase.');
        info = null;
        sent = false;
        step = 2;
        toolsResult = 'Erased. The display restarts with nothing set up (a pulsing amber square).';
      },
      (e) => (toolsError = e)
    );
  }

  const set = (on: boolean | undefined) => (on ? 'Saved on the display' : '');
</script>

<main>
  <header>
    <img class="mark" src="./logo.svg" alt="Coverussi" />
    <h1>Set up a display</h1>
    <p class="muted">Plug the board’s <strong>USB</strong> port (not POWER) into this computer.</p>
  </header>

  {#if !supported}
    <section class="card">
      <h2>This browser can’t reach USB devices</h2>
      <p class="muted">Open this page in Chrome or Edge on a computer. Safari, Firefox and phones don’t support Web Serial.</p>
    </section>
  {:else}
    <ol class="steps">
      <li class="card" class:open={step === 1}>
        <button class="head" onclick={() => (step = 1)} aria-expanded={step === 1}>
          <span class="num" class:done={installed}>{#if installed}<Icon name="check" />{:else}1{/if}</span>
          <h2>Install the firmware</h2>
          <span class="spacer"></span>
          {#if step !== 1 && installed && manifest}<span class="faint">{manifest.version}</span>{/if}
        </button>
        {#if step === 1}
          <div class="body">
            {#if manifest === undefined}
              <p class="muted">Looking for the latest build…</p>
            {:else if manifest === null}
              <p class="empty">No firmware build is published here yet.</p>
            {:else if installing}
              <div class="bar" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(progress * 100)}>
                <span style:width="{progress * 100}%"></span>
              </div>
              <p class="muted num">{progress < 0.01 ? 'Connecting…' : `Writing… ${Math.round(progress * 100)}%`}</p>
            {:else}
              <p class="muted">{manifest.name} {manifest.version} · {new Date(manifest.built).toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' })}. When Chrome asks for a port, pick <strong>USB JTAG/serial debug unit</strong>.</p>
              <label class="check"><input type="checkbox" bind:checked={erase} />Erase the board first <span class="faint">(starts over; leave off to update)</span></label>
              {#if installError}<p class="error">{installError}</p>{/if}
              <div class="row">
                <button class="primary" onclick={runInstall} disabled={!!busy}><Icon name="usb" />{installed ? 'Install again' : 'Install'}</button>
                <span class="spacer"></span>
                {#if !installed}<button class="link" onclick={() => (step = 2)}>Already installed? Connect it<Icon name="arrow-right" /></button>{/if}
              </div>
            {/if}
          </div>
        {/if}
      </li>

      <li class="card" class:open={step === 2}>
        <button class="head" onclick={() => (step = 2)} aria-expanded={step === 2}>
          <span class="num" class:done={sent}>{#if sent}<Icon name="check" />{:else}2{/if}</span>
          <h2>Connect it</h2>
          <span class="spacer"></span>
          {#if step !== 2 && sent}<span class="faint">Sent</span>{/if}
        </button>
        {#if step === 2}
          <div class="body">
            <fieldset>
              <legend>Wi-Fi</legend>
              <div class="pair">
                <label><span>Name</span><input bind:value={form.ssid} autocomplete="off" spellcheck="false" placeholder={set(info?.wifi)} /></label>
                <label><span>Password</span><input type="password" bind:value={form.password} autocomplete="off" /></label>
              </div>
            </fieldset>
            <fieldset>
              <legend>Home Assistant</legend>
              <div class="pair">
                <label><span>Address</span><input bind:value={form.ha_url} placeholder={info?.ha_url || 'http://homeassistant.local:8123'} spellcheck="false" /></label>
                <label><span>Token</span><input type="password" bind:value={form.ha_token} autocomplete="off" placeholder={set(info?.ha_token)} /></label>
              </div>
              <p class="hint faint">The token is a long-lived access token from your Home Assistant profile, Security tab.</p>
            </fieldset>
            <details class="more" open={!!form.mqtt_url}>
              <summary><Icon name="chevron-down" />MQTT broker <span class="faint">(optional)</span></summary>
              <label><input type="password" bind:value={form.mqtt_url} placeholder={set(info?.mqtt) || 'mqtt://user:password@homeassistant.local'} autocomplete="off" spellcheck="false" /></label>
              <p class="hint faint">Adds the display to Home Assistant: what it hears, and the panel as a light.</p>
            </details>
            {#if connectError}<p class="error">{connectError}</p>{/if}
            <div class="row">
              <button class="primary" onclick={send} disabled={!!busy || installing}><Icon name="arrow-right" />{busy === 'send' ? 'Sending…' : 'Send to display'}</button>
              {#if info}<span class="faint small">Empty fields keep what the display has.</span>{/if}
            </div>
          </div>
        {/if}
      </li>

      <li class="card" class:open={step === 3}>
        <button class="head" onclick={() => (step = 3)} disabled={!sent && !webApp} aria-expanded={step === 3}>
          <span class="num" class:done={!!webApp}>{#if webApp}<Icon name="check" />{:else}3{/if}</span>
          <h2>Open the web app</h2>
        </button>
        {#if step === 3}
          <div class="body">
            {#if joining}
              <p class="status"><span class="dot accent pulse"></span>Joining Wi-Fi… The panel shows what it’s waiting for.</p>
            {:else if webApp}
              <p class="muted">It’s on your network. The web app walks you through the rest: the panel, what to follow and the mics.</p>
              <div class="row"><a class="button primary" href={webApp} target="_blank" rel="noopener"><Icon name="external-link" />Open {webApp}</a></div>
            {:else if joinFailed}
              <p class="error">The display didn’t join the Wi-Fi within a minute. Check the name and password, then send them again.</p>
              <div class="row"><button onclick={() => (step = 2)}>Back to Connect</button></div>
            {/if}
            {#if info && !joining}
              <dl class="facts">
                <dt>Wi-Fi</dt><dd>{info.wifi ? 'Set' : 'Not set'}</dd>
                <dt>Home Assistant</dt><dd>{info.ha_url ? `${info.ha_url}${info.ha_token ? '' : ' (no token)'}` : 'Not set'}</dd>
                <dt>MQTT</dt><dd>{info.mqtt ? 'Set' : 'Not set'}</dd>
              </dl>
            {/if}
          </div>
        {/if}
      </li>
    </ol>

    <details class="tools">
      <summary><Icon name="chevron-down" />Troubleshooting</summary>
      <div class="row">
        <button onclick={check} disabled={!!busy || installing}><Icon name="search" />{busy === 'info' ? 'Asking…' : 'Check what it has'}</button>
        <span class="spacer"></span>
        {#if eraseArmed}
          <button onclick={() => (eraseArmed = false)}>Cancel</button>
          <button class="danger" onclick={eraseSettings}><Icon name="trash" />Erase settings</button>
        {:else}
          <button class="link" onclick={() => (eraseArmed = true)} disabled={!!busy || installing}><Icon name="trash" />{busy === 'erase' ? 'Erasing…' : 'Erase settings…'}</button>
        {/if}
      </div>
      {#if toolsResult}<p class="status"><span class="dot positive"></span>{toolsResult}</p>{/if}
      {#if toolsError}<p class="error">{toolsError}</p>{/if}
      {#if info && busy !== 'info'}
        <dl class="facts">
          <dt>Firmware</dt><dd>{info.version}</dd>
          <dt>MAC</dt><dd class="mono">{info.mac}</dd>
          <dt>Address</dt><dd>{info.ip || 'Not on Wi-Fi'}</dd>
          <dt>Wi-Fi</dt><dd>{info.wifi ? 'Set' : 'Not set'}</dd>
          <dt>Home Assistant</dt><dd>{info.ha_url || 'Not set'}{info.ha_url && !info.ha_token ? ' (no token)' : ''}</dd>
          <dt>MQTT</dt><dd>{info.mqtt ? 'Set' : 'Not set'}</dd>
        </dl>
      {/if}
      {#if log.length}<pre class="log mono" bind:this={logBox}>{log.join('\n')}</pre>{/if}
      <p class="faint small">From a terminal: <span class="mono">scripts/esp32-rs.sh flash</span> and <span class="mono">scripts/provision.py</span> (docs/setup.md).</p>
    </details>
  {/if}
</main>

<style>
  main { max-width: 600px; margin: 0 auto; padding: 40px 24px 64px; display: flex; flex-direction: column; gap: 24px; }
  header { display: flex; flex-direction: column; gap: 6px; }
  .mark { height: 30px; display: block; align-self: flex-start; margin-bottom: 18px; }

  .steps { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 10px; }
  .steps .card { padding: 0; gap: 0; }
  .head { width: 100%; height: auto; padding: 18px var(--pad); border-radius: var(--radius-card); background: none; justify-content: flex-start; gap: 12px; text-align: left; }
  .head:hover:not(:disabled) { background: none; }
  .head:disabled { background: none; color: var(--faint); }
  .head .spacer { flex: 1; }
  .open .head { padding-bottom: 6px; }
  .num { flex: none; width: 26px; height: 26px; border-radius: 50%; background: var(--raised); display: inline-grid; place-items: center; font-size: var(--fs-small); font-weight: 600; }
  .num.done { background: var(--pill-positive-bg); color: var(--positive); }
  .head:disabled .num { color: var(--faint); }
  .body { padding: 12px var(--pad) var(--pad) calc(var(--pad) + 38px); display: flex; flex-direction: column; gap: 16px; }

  .check { display: flex; align-items: center; gap: 10px; }
  .bar { height: 6px; border-radius: 999px; background: var(--raised); overflow: hidden; }
  .bar span { display: block; height: 100%; background: var(--accent); transition: width 0.2s; }

  fieldset { border: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 8px; min-width: 0; }
  legend { padding: 0; margin-bottom: 8px; font-weight: 500; }
  .pair { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  label { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
  label > span { color: var(--muted); font-size: var(--fs-small); line-height: 16px; }
  .check { flex-direction: row; }
  .hint { font-size: var(--fs-small); line-height: 16px; }

  details summary { list-style: none; cursor: pointer; display: inline-flex; align-items: center; gap: 6px; color: var(--muted); }
  details summary::-webkit-details-marker { display: none; }
  details summary :global(svg) { transition: transform 160ms ease-out; transform: rotate(-90deg); }
  details[open] summary :global(svg) { transform: none; }
  details summary:hover { color: var(--text); }
  .more { display: flex; flex-direction: column; gap: 8px; }
  .more[open] summary { margin-bottom: 8px; }
  .tools { padding: 0 4px; }
  .tools[open] { display: flex; flex-direction: column; gap: 14px; }
  .tools[open] summary { margin-bottom: 4px; }

  .status { white-space: normal; }
  .pulse { animation: pulse 1.4s ease-in-out infinite; }
  @keyframes pulse { 50% { opacity: 0.3; } }
  .log { margin: 0; max-height: 240px; overflow: auto; white-space: pre-wrap; overflow-wrap: anywhere; color: var(--muted); background: var(--surface); border-radius: var(--radius-inner); padding: 12px; }

  @media (max-width: 560px) {
    main { padding: 24px 16px 48px; }
    .pair { grid-template-columns: 1fr; }
    .body { padding-left: var(--pad); }
  }
  @media (prefers-reduced-motion: reduce) { .pulse { animation: none; } details summary :global(svg) { transition: none; } }
</style>
