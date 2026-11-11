<script lang="ts">
  // First-run setup: walks through what the display needs to start, one step at
  // a time, saving as it goes. Finishing or skipping sets setup_done.
  import { onDestroy } from 'svelte';
  import { api } from '../lib/api';
  import Icon, { type IconName } from '../lib/Icon.svelte';
  import { live } from '../lib/live.svelte';
  import Panel from '../lib/Panel.svelte';
  import { signal } from '../lib/format';
  import { maxLimitMa, PORT_MAX_A } from '../lib/power';
  import Segmented from '../lib/Segmented.svelte';
  import { session } from '../lib/session.svelte';
  import type { Settings } from '../lib/types';
  import Calibrate from './Calibrate.svelte';
  import Password, { PASSWORD_MIN } from './Password.svelte';
  import Sources from './Sources.svelte';

  let { ondone }: { ondone: () => void } = $props();

  const FLASHER = 'https://jasoncronje.github.io/coverussi/';
  const ROTATIONS = [0, 90, 180, 270].map((d) => ({ value: d, label: `${d}°` }));
  // Chargers people have, and the current limit each covers with the board's draw on top.
  const SUPPLIES = [2, 2.5, PORT_MAX_A].map((a) => ({ value: maxLimitMa(a), label: `${a} A` }));

  let draft = $state<Settings | null>(null);
  $effect(() => {
    if (!draft && live.settings) draft = structuredClone($state.snapshot(live.settings)) as Settings;
  });

  // The microphone step only matters when something is identified by listening.
  const listens = $derived(!!draft && (draft.sources.analog.length > 0 || draft.sources.listen_for_music));
  type Step = 'connect' | 'general' | 'panel' | 'sources' | 'mic';
  const steps = $derived<Step[]>(['connect', 'general', 'panel', 'sources', ...(listens ? ['mic' as const] : [])]);
  let step = $state<Step>('connect');
  const index = $derived(steps.indexOf(step));
  const last = $derived(index === steps.length - 1);

  let error = $state('');
  let busy = $state(false);

  async function save(extra: Partial<Settings> = {}) {
    if (!draft) return;
    const s = { ...$state.snapshot(draft), ...extra } as Settings;
    await api.saveSettings(s);
    await live.loadSettings();
  }

  async function run(fn: () => Promise<void>) {
    busy = true;
    error = '';
    try {
      await fn();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  function go(to: Step) {
    if (step === 'panel' && to !== 'panel') testPattern(false);
    step = to;
    error = '';
  }

  const next = () => run(async () => {
    // As in Settings: a new password replaces the old one; turning it off removes it.
    if (step === 'general' && (newPassword || passwordOn !== session.passwordSet)) {
      if (passwordOn && newPassword.length < PASSWORD_MIN)
        throw new Error(`Enter a password of at least ${PASSWORD_MIN} characters, or turn it off.`);
      await api.setPassword(passwordOn ? newPassword : '');
      session.passwordSet = passwordOn;
      newPassword = '';
    }
    await save(last ? { setup_done: true } : {});
    if (last) ondone();
    else go(steps[index + 1]);
  });

  const skip = () => run(async () => {
    testPattern(false);
    await save({ setup_done: true });
    ondone();
  });

  // Connect: what the board reaches. Wi-Fi works, or this page wouldn't load.
  const t = $derived(live.telemetry);
  // The time the board keeps, in its time zone, as a check that it's right.
  const boardTime = $derived.by(() => {
    try {
      return new Date(live.now).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit', timeZone: draft?.display.time_zone });
    } catch {
      return new Date(live.now).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
    }
  });
  const ha = $derived(!live.haUrl ? 'unset' : t.ha_connected ? 'connected' : 'failing');

  // Where: the browser's own time zone, when the board still has the default and knows it.
  let zones = $state<string[]>([]);
  api.timeZones().then((z) => (zones = z), () => {});
  let guessed = false;
  $effect(() => {
    if (guessed || !draft || !zones.length) return;
    guessed = true;
    const here = Intl.DateTimeFormat().resolvedOptions().timeZone;
    if (draft.display.time_zone === 'UTC' && zones.includes(here)) draft.display.time_zone = here;
  });
  const place = (z: string) => z.replaceAll('_', ' ').replaceAll('/', ' / ');

  // Panel: the test pattern shows which way up it is; rotation applies at once.
  // The board shows it for 8 s a time, so it's asked again while this step is open.
  function testPattern(on: boolean) {
    api.command('test_pattern', { on }).catch(() => {});
  }
  $effect(() => {
    if (step !== 'panel') return;
    testPattern(true);
    const again = setInterval(() => testPattern(true), 6000);
    return () => clearInterval(again);
  });
  onDestroy(() => {
    if (step === 'panel') testPattern(false);
  });
  function rotate(r: number) {
    if (!draft) return;
    draft.display.rotation = r;
    void run(() => save());
  }

  // Sources: at least one, or the panel never shows anything.
  const followsSomething = $derived(!!draft && (draft.sources.digital.length > 0 || listens));

  // Microphone
  let calibrated = $state(false);
  function useCalibration(music: number, gap: number) {
    if (!draft) return;
    draft.listening.music_threshold_db = music;
    draft.listening.gap_threshold_db = gap;
    calibrated = true;
  }

  // Password
  let passwordOn = $state(session.passwordSet);
  let newPassword = $state('');

  // On a phone the steps scroll sideways: keep the current one in view.
  function inView(el: HTMLElement, current: boolean) {
    const show = (on: boolean) => on && el.scrollIntoView({ block: 'nearest', inline: 'nearest' });
    show(current);
    return { update: show };
  }

  const icons: Record<Step, IconName> = { connect: 'wifi', general: 'cog', panel: 'grid', sources: 'music', mic: 'mic' };
  const titles: Record<Step, string> = {
    connect: 'Connections',
    general: 'General',
    panel: 'Panel',
    sources: 'Sources',
    mic: 'Microphones'
  };
</script>

<header>
  <div class="bar">
    <img class="mark" src="/logo.svg" alt="Coverussi" />
    <button class="link" disabled={busy || !draft} onclick={skip}>Skip setup</button>
  </div>
</header>

<main class:with-mirror={step === 'panel'}>
  {#if !draft}
    <p class="muted">Loading…</p>
  {:else}
    <nav class="steps" aria-label="Setup">
      <ol>
        {#each steps as s, i (s)}
          <li>
            <button class="step" use:inView={s === step} class:done={i < index} class:current={s === step} aria-current={s === step ? 'step' : undefined}
              disabled={busy || i >= index} onclick={() => go(s)}>
              <Icon name={icons[s]} />{titles[s]}{#if i < index}<span class="tick"><Icon name="check" size={14} /></span>{/if}
            </button>
          </li>
        {/each}
      </ol>
    </nav>

    <section class="content">
      <h1>{titles[step]}</h1>

      {#if step === 'connect'}
        <p class="muted">Set when the display was installed over USB. To change them, send them again from the <a href={FLASHER} target="_blank" rel="noreferrer">installer</a>.</p>
        <div class="rows">
          <div class="field">
            <span class="name">Wi-Fi<span class="sub">{[t.wifi_ssid, t.wifi_rssi !== undefined && `${signal(t.wifi_rssi)} signal (${t.wifi_rssi} dBm)`, t.ip].filter(Boolean).join(' · ')}</span></span>
            <span class="status"><span class="dot positive"></span>Connected</span>
          </div>
          <div class="field">
            <span class="name">Home Assistant<span class="sub">{live.haUrl ? live.haUrl.replace(/^https?:\/\//, '') : 'Needed to follow your players'}</span></span>
            {#if ha === 'connected'}<span class="status"><span class="dot positive"></span>Connected</span>
            {:else if ha === 'failing'}<span class="status"><span class="dot negative"></span>Not connecting</span>
            {:else}<span class="status"><span class="dot"></span>Not set</span>{/if}
          </div>
          <div class="field">
            <span class="name">Clock<span class="sub">Synced from the internet. Night brightness follows it.</span></span>
            {#if t.clock_set}<span class="status"><span class="dot positive"></span>{boardTime}</span>
            {:else}<span class="status"><span class="dot"></span>Waiting to sync</span>{/if}
          </div>
        </div>
        {#if ha === 'failing'}
          <p class="note warn">Check the address and token, then send them again from the installer.</p>
        {/if}

      {:else if step === 'general'}
        <div class="rows">
          <label class="field">
            <span class="name">Time zone</span>
            <select bind:value={draft.display.time_zone}>
              {#if !zones.includes(draft.display.time_zone)}<option value={draft.display.time_zone}>{place(draft.display.time_zone)}</option>{/if}
              {#each zones as z (z)}<option value={z}>{place(z)}</option>{/each}
            </select>
          </label>
          <Password bind:enabled={passwordOn} bind:value={newPassword} tip="Off by default. With one set, every browser and the Apple TV app have to log in." />
        </div>

      {:else if step === 'panel'}
        <div class="rows">
          <div class="field">
            <span class="name">Rotation</span>
            <Segmented label="Rotation" value={draft.display.rotation} options={ROTATIONS} disabled={busy} onchange={rotate} />
          </div>
          <div class="field">
            <span class="name">Charger (5 V)</span>
            <Segmented label="Charger" value={SUPPLIES.some((o) => o.value === draft!.display.current_limit_ma) ? draft.display.current_limit_ma : null}
              options={SUPPLIES} onchange={(v) => (draft!.display.current_limit_ma = v)} />
          </div>
        </div>

      {:else if step === 'sources'}
        <div class="group">
          <h2>Sources</h2>
          <p class="muted">{live.haUrl
            ? 'What the display follows. A player’s cover comes straight from it; anything that can’t say what’s playing is analog, and the display listens with its mics and identifies the song.'
            : 'Without Home Assistant, the display listens to the room with its mics and identifies the song.'}</p>
          <Sources ha={!!live.haUrl} sources={draft.sources} onchange={(s) => (draft!.sources = s)} />
        </div>

      {:else if step === 'mic'}
        <Calibrate onuse={useCalibration} />
        {#if calibrated}<p class="note"><span class="dot positive"></span>Music above {draft.listening.music_threshold_db} dBFS, gap below {draft.listening.gap_threshold_db} dBFS.</p>{/if}
      {/if}

      {#if error}<p class="error">{error}</p>{/if}

      <div class="actions">
        {#if index > 0}<button disabled={busy} onclick={() => go(steps[index - 1])}>Back</button>{/if}
        <span class="spacer"></span>
        {#if step === 'sources' && !followsSomething}<span class="faint">Add a source to go on</span>{/if}
        <button class="primary" disabled={busy || (step === 'sources' && !followsSomething)} onclick={next}>
          {#if last}<Icon name="check" />Finish{:else if step === 'mic' && !calibrated}Skip for now<Icon name="arrow-right" />{:else}Next<Icon name="arrow-right" />{/if}
        </button>
      </div>
    </section>

    {#if step === 'panel'}
      <aside class="mirror">
        <Panel size={320} />
        <p class="faint">Match the panel to this.</p>
      </aside>
    {/if}
  {/if}
</main>

<style>
  /* The app's own bar and 1200px column: the steps on the left, then the step; the panel step adds its mirror on the right. */
  header { position: sticky; top: 0; z-index: 2; background: var(--ground); border-bottom: 1px solid var(--border); }
  .bar, main { max-width: 1200px; margin: 0 auto; padding: 0 24px; }
  .bar { height: 48px; display: flex; align-items: center; justify-content: space-between; }
  .mark { height: 26px; display: block; }
  main { padding-top: 32px; padding-bottom: 64px; display: grid; grid-template-columns: 200px minmax(0, 720px); gap: 40px; align-items: start; }
  main.with-mirror { grid-template-columns: 200px minmax(0, 1fr) 320px; }

  .steps { position: sticky; top: 81px; }
  .steps ol { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
  .step { width: 100%; height: 36px; padding: 0 12px; gap: 10px; border-radius: var(--radius-inner); background: none; color: var(--faint); font-weight: 500; justify-content: flex-start; }
  .step:disabled { background: none; color: var(--faint); }
  .step.done { color: #bbbbbb; }
  .step.done:hover:not(:disabled) { background: none; color: #ffffff; }
  .step.current, .step.current:disabled { background: var(--raised); color: #ffffff; }
  .step .tick { margin-left: auto; display: inline-flex; color: var(--faint); }

  .content { display: flex; flex-direction: column; gap: 20px; min-width: 0; }
  .rows { display: flex; flex-direction: column; }
  .field { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 16px; align-items: center; min-height: 52px; padding: 8px 0; border-top: 1px solid var(--rule); }
  .field:last-child { border-bottom: 1px solid var(--rule); }
  .name { display: flex; flex-direction: column; gap: 2px; font-weight: 500; min-width: 0; }
  .sub { color: var(--muted); font-weight: 400; font-size: 13px; overflow-wrap: anywhere; }
  .status { display: inline-flex; align-items: center; gap: 8px; white-space: nowrap; }
  .content > .muted a { color: var(--text); }
  select { width: 300px; }
  .group { display: flex; flex-direction: column; gap: 4px; }
  .group > .muted { margin-bottom: 8px; }
  .note { display: flex; align-items: center; gap: 10px; color: var(--muted); }
  .note.warn { color: var(--warning); }
  .actions { display: flex; align-items: center; gap: 8px; padding-top: 4px; }
  .actions .spacer { flex: 1; }

  .mirror { position: sticky; top: 81px; display: flex; flex-direction: column; gap: 12px; }
  .mirror p { font-size: var(--fs-small); line-height: 16px; }

  @media (max-width: 1080px) {
    main, main.with-mirror { grid-template-columns: 200px minmax(0, 1fr); }
    .mirror { position: static; grid-column: 2; max-width: 320px; }
  }
  @media (max-width: 860px) {
    main, main.with-mirror { grid-template-columns: 1fr; gap: 24px; }
    .steps { position: static; margin: 0 -4px; overflow-x: auto; }
    .steps ol { flex-direction: row; }
    .step { width: auto; }
    .mirror { grid-column: auto; }
  }
  @media (max-width: 520px) {
    .bar, main { padding-left: 16px; padding-right: 16px; }
    main { padding-top: 20px; padding-bottom: 48px; }
    /* A label beside a short value stays on one line; a select or a choice goes under its label. */
    .field:has(select), .field:has(:global(.segmented)) { grid-template-columns: 1fr; gap: 10px; }
    select { width: 100%; }
    .mirror { max-width: none; }
    .mirror :global(canvas) { width: 100% !important; }
  }
</style>
