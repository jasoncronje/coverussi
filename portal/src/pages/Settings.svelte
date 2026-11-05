<script lang="ts">
  import { api } from '../lib/api';
  import Icon, { type IconName } from '../lib/Icon.svelte';
  import { live } from '../lib/live.svelte';
  import type { PanelColor, Settings } from '../lib/types';
  import Calibrate from './Calibrate.svelte';
  import DevicePage from './Device.svelte';
  import IdleScreen from './IdleScreen.svelte';
  import Password, { PASSWORD_MIN } from './Password.svelte';
  import { session } from '../lib/session.svelte';
  import PanelCalibration from './PanelCalibration.svelte';
  import Sources from './Sources.svelte';
  import TransitionPicker from '../lib/TransitionPicker.svelte';
  import Tip from '../lib/Tip.svelte';
  import PowerSupply from './PowerSupply.svelte';
  import Segmented from '../lib/Segmented.svelte';

  // area: the sidebar entry showing, from #/settings/<area>. problem: why Controller gets a mark.
  let { area = '', problem = '' }: { area?: string; problem?: string } = $props();

  // One sidebar entry per area. General's password and Controller draw their own; the rest are groups below.
  const areas = [
    { id: 'general', title: 'General', icon: 'cog' },
    { id: 'panel', title: 'Panel', icon: 'sun' },
    { id: 'display', title: 'Display', icon: 'display' },
    { id: 'sources', title: 'Sources', icon: 'music' },
    { id: 'device', title: 'Controller', icon: 'cpu' }
  ] as const satisfies readonly { id: string; title: string; icon: IconName }[];
  type Area = (typeof areas)[number]['id'];

  // Transition speeds; Show advanced adds the exact length in ms.
  const SPEEDS = [{ value: 250, label: 'Fast' }, { value: 400, label: 'Medium' }, { value: 800, label: 'Slow' }];

  // Clockwise turns for how the panel is mounted.
  const ROTATIONS = [0, 90, 180, 270].map((d) => ({ value: d, label: `${d}°` }));

  // What a picture that isn't square becomes.
  const FITS = [{ value: 'crop', label: 'Crop to square' }, { value: 'letterbox', label: 'Black bars' }, { value: 'pan', label: 'Pan across' }];

  // How fast the record turns while the song is identified.
  const RPMS = [33, 45, 78].map((r) => ({ value: r, label: `${r} rpm` }));

  type Kind = 'int' | 'float' | 'text' | 'time' | 'zone' | 'bool' | 'list' | 'numlist' | 'transition' | 'speed' | 'rotation' | 'fit' | 'rpm' | 'colour';
  // tip: what it does, in a tooltip. advanced: tuning most people never touch, hidden until Show advanced is on.
  // when: shown only while it holds for the settings being edited. relabel: its label for those settings.
  interface Field {
    key: string; label: string; kind: Kind; tip?: string; min?: number; max?: number; step?: number; unit?: string; advanced?: boolean;
    when?: (s: Settings) => boolean; relabel?: (s: Settings) => string;
  }
  // key: the settings section its fields live in; several groups can share one.
  // custom: drawn by its own component instead of fields.
  interface Group { id: string; area: Area; key: keyof Settings; title: string; tip?: string; fields: Field[]; custom?: boolean }

  const groups: Group[] = [
    {
      id: 'time', area: 'general', key: 'display', title: 'Time',
      fields: [
        { key: 'time_zone', label: 'Time zone', kind: 'zone', tip: 'Where the display is. The night hours follow it, daylight saving included.' }
      ]
    },
    {
      id: 'brightness', area: 'panel', key: 'display', title: 'Brightness',
      fields: [
        { key: 'brightness_schedule', label: 'Day and night brightness', kind: 'bool', tip: 'Dims the panel at night. Turn it off to keep one brightness, or to let Home Assistant set it over MQTT, like Adaptive Lighting does.' },
        { key: 'brightness_day', label: 'Day brightness', kind: 'int', min: 0, max: 255, unit: '/ 255', tip: 'Panel brightness outside night hours, or all day with Day and night brightness off. 0 is off, 255 is full. Home Assistant can change it until the display restarts.', relabel: (s) => s.display.brightness_schedule ? 'Day brightness' : 'Brightness' },
        { key: 'brightness_night', label: 'Night brightness', kind: 'int', min: 0, max: 255, unit: '/ 255', tip: 'Panel brightness from Night starts to Night ends.', when: (s) => s.display.brightness_schedule },
        { key: 'night_start', label: 'Night starts', kind: 'time', tip: 'When night brightness takes over, in the time zone under General.', when: (s) => s.display.brightness_schedule },
        { key: 'night_end', label: 'Night ends', kind: 'time', tip: 'When day brightness comes back, in the time zone under General.', when: (s) => s.display.brightness_schedule }
      ]
    },
    {
      id: 'orientation', area: 'panel', key: 'display', title: 'Orientation',
      fields: [
        { key: 'rotation', label: 'Rotation', kind: 'rotation', tip: 'Turns the picture clockwise, for a panel mounted on its side or upside down. The preview here stays upright.' }
      ]
    },
    {
      id: 'color', area: 'panel', key: 'display', title: 'Panel colour', custom: true, fields: [],
      tip: 'White balance, shadows and saturation for the panel. Calibrate shows an image on the panel that changes as you move the sliders.'
    },
    {
      id: 'artwork', area: 'display', key: 'display', title: 'Artwork',
      fields: [
        { key: 'fit', label: 'Wide pictures', kind: 'fit', tip: 'For art that isn’t square, like a 16:9 video thumbnail. Crop to square fills the panel with the middle; Black bars shows the whole picture; Pan across fills the panel and slides slowly to one end and back.' }
      ]
    },
    {
      id: 'album', area: 'display', key: 'recognition', title: 'Album',
      fields: [
        { key: 'prefer_album', label: 'Prefer the album version', kind: 'bool', tip: 'Shazam often names a single, a best-of or a live record. With this on, the display shows the studio album the song first came out on.' },
        { key: 'store_country', label: 'Store country', kind: 'text', advanced: true, tip: 'The Apple Music store covers and albums come from, as a two-letter code like US or GB. Empty: the country of the time zone.' }
      ]
    },
    {
      id: 'transitions', area: 'display', key: 'display', title: 'Transitions',
      fields: [
        { key: 'transition', label: 'Transition', kind: 'transition', tip: 'How one cover gives way to the next. The preview plays it with what’s on the panel now.' },
        { key: 'crossfade_ms', label: 'Speed', kind: 'speed', min: 0, max: 3000, step: 50, unit: 'ms', tip: 'How long a transition takes: Fast is 250 ms, Medium 400, Slow 800. Show advanced sets any length from 0 to 3000 ms.' }
      ]
    },
    {
      id: 'stopping', area: 'display', key: 'display', title: 'When music stops',
      fields: [
        { key: 'pause_grace_s', label: 'Keep art after pause', kind: 'int', min: 0, max: 3600, unit: 's', tip: 'A paused stream keeps its cover this long before the panel goes dark.' },
        { key: 'side_end_dim_s', label: 'Dim after side ends', kind: 'int', min: 0, max: 3600, unit: 's', advanced: true, tip: 'When a record side ends, the cover dims, then clears after this long.' }
      ]
    },
    {
      id: 'song', area: 'display', key: 'display', title: 'Title and artist',
      tip: 'Once a record is identified, its cover gives way to the song’s title and artist now and then.',
      fields: [
        { key: 'song_screen', label: 'Show between covers', kind: 'bool', tip: 'Off: the cover stays on the panel the whole song.' },
        { key: 'song_cover_s', label: 'Cover for', kind: 'int', min: 5, max: 600, unit: 's', tip: 'How long the cover shows before the title and artist.' },
        { key: 'song_text_s', label: 'Title for', kind: 'int', min: 2, max: 60, unit: 's', tip: 'How long the title and artist show before the cover comes back.' }
      ]
    },
    {
      id: 'record', area: 'display', key: 'display', title: 'Unknown record',
      tip: 'A record plays that isn’t identified yet, or Shazam doesn’t know it.',
      fields: [
        { key: 'record', label: 'Turning record', kind: 'bool', tip: 'Off: the panel shows what it shows when nothing plays.' },
        { key: 'record_rpm', label: 'Speed', kind: 'rpm', tip: 'How fast the record turns.' },
        { key: 'record_label', label: 'Label colour', kind: 'colour', tip: 'The colour of the label in the middle of the record.' },
        { key: 'record_sheen', label: 'Shines with the music', kind: 'bool', tip: 'The light on the vinyl gets brighter as the music gets louder.' }
      ]
    },
    {
      id: 'idle', area: 'display', key: 'idle', title: 'When nothing plays', custom: true, fields: [],
      tip: 'What the panel shows when there’s no cover: nothing, a clock, the weather, or a few Home Assistant entities.'
    },
    {
      id: 'power', area: 'panel', key: 'display', title: 'Power',
      fields: [
        { key: 'current_limit_ma', label: 'Current limit', kind: 'int', min: 500, max: 3000, step: 100, unit: 'mA', tip: 'The most the panel may draw. Bright covers dim just enough to stay under it, so the supply never browns out.' }
      ]
    },
    {
      id: 'digital', area: 'sources', key: 'sources', title: 'Digital sources', custom: true, fields: [],
      tip: 'Smart players that tell Home Assistant what’s playing, like a streamer or an Apple TV. Their cover shows while they play; limit one to some apps if you like. When several play, the highest on the list shows: drag to reorder.'
    },
    {
      id: 'analog', area: 'sources', key: 'sources', title: 'Analog sources', custom: true, fields: [],
      tip: 'Devices that can’t say what’s playing, like a turntable or tape deck. When a Home Assistant state below matches, the display listens with its mics and identifies the song. When several match, the highest on the list wins: drag to reorder. No Home Assistant entity? Turn on Listen for music.'
    },
    {
      id: 'listening', area: 'general', key: 'listening', title: 'Microphone',
      fields: [
        { key: 'music_threshold_db', label: 'Music above', kind: 'float', min: -90, max: 0, step: 0.5, unit: 'dBFS', tip: 'A mic level above this counts as music. Calibrate sets it for your room.' },
        { key: 'gap_threshold_db', label: 'Gap below', kind: 'float', min: -90, max: 0, step: 0.5, unit: 'dBFS', tip: 'A level below this counts as the silence between songs. Calibrate sets it for your room.' },
        { key: 'min_gap_ms', label: 'Shortest gap', kind: 'int', min: 100, max: 10000, step: 100, unit: 'ms', advanced: true, tip: 'Silence must last this long to count as a new track, so quiet passages inside a song don’t.' },
        { key: 'side_end_silence_s', label: 'Side over after', kind: 'int', min: 5, max: 600, unit: 's', advanced: true, tip: 'This much silence means the record side has ended.' },
        { key: 'clip_ms', label: 'Clip length', kind: 'int', min: 3000, max: 20000, step: 500, unit: 'ms', advanced: true, tip: 'How much audio is recorded and sent to Shazam. 6 s is usually enough; 10 s is safer in a noisy room.' },
        { key: 'retry_backoff_s', label: 'Retry after a miss', kind: 'numlist', unit: 's', advanced: true, tip: 'Seconds to wait before trying again after each miss in a row, comma separated. After the last one, it waits for the next gap between songs.' },
        { key: 'relisten_after_end_ms', label: 'Listen again after track end', kind: 'int', min: 0, max: 60000, step: 500, unit: 'ms', advanced: true, tip: 'Shazam says when a matched track ends. This long after that, the display listens again to catch the next song.' },
        { key: 'music_start_s', label: 'Music before identifying', kind: 'int', min: 1, max: 60, unit: 's', advanced: true, tip: 'Listening for music: sound must stay above Music above this long before the display identifies it, so a cough or a door doesn’t.' },
        { key: 'mic_gain_db', label: 'Mic gain', kind: 'float', min: 0, max: 37.5, step: 1.5, unit: 'dB', tip: 'Boost for the board’s microphones. Raise it if music reads too quiet; lower it if loud passages clip.' }
      ]
    },
    {
      id: 'recognition', area: 'general', key: 'recognition', title: 'Recognition',
      fields: [
        { key: 'min_interval_s', label: 'Seconds between Shazam calls', kind: 'int', min: 5, max: 600, unit: 's', advanced: true, tip: 'The shortest wait between identifications. Under about 20 s gets rate limited.' },
        { key: 'replace_after', label: 'Matches to switch album', kind: 'int', min: 1, max: 5, advanced: true, tip: 'A match on another album by the same artist only switches the cover after this many in a row, so compilations and best-ofs don’t flip it.' },
        { key: 'session_timeout_s', label: 'Listening session ends after', kind: 'int', min: 60, max: 7200, unit: 's', advanced: true, tip: 'With no match for this long, the album stops being held, and the next match starts fresh.' },
        { key: 'keep_clips', label: 'Keep clips for 24 h (debug)', kind: 'bool', advanced: true, tip: 'Saves each recorded clip on the hub for a day, to work out why a song wasn’t found.' }
      ]
    },
    {
      id: 'home_assistant', area: 'general', key: 'home_assistant', title: 'MQTT',
      fields: [
        { key: 'mqtt_discovery', label: 'MQTT discovery', kind: 'bool', tip: 'Publishes the display’s entities (now playing, panel power, brightness) to Home Assistant over MQTT.' },
        { key: 'name', label: 'Device name', kind: 'text', tip: 'The device name those entities appear under in Home Assistant.' }
      ]
    }
  ];

  const ADVANCED_KEY = 'settings.advanced';
  let advanced = $state(localStorage.getItem(ADVANCED_KEY) === '1');
  $effect(() => localStorage.setItem(ADVANCED_KEY, advanced ? '1' : '0'));
  const shown = (g: Group) => g.fields.filter((f) => (advanced || !f.advanced) && (!f.when || !draft || f.when(draft)));
  const labelOf = (f: Field) => (f.relabel && draft ? f.relabel(draft) : f.label);
  // An area with nothing to show until Show advanced is on leaves the sidebar.
  const visibleAreas = $derived(areas.filter((a) => a.id === 'general' || a.id === 'device'
    || groups.some((g) => g.area === a.id && (g.custom || shown(g).length))));
  const current = $derived(visibleAreas.find((a) => a.id === area) ?? visibleAreas[0]);

  let draft = $state<Settings | null>(null);
  let base = $state('');
  let error = $state('');
  let saved = $state('');
  let saving = $state(false);

  // Start from the hub's settings; take new ones when nothing here is edited.
  $effect(() => {
    const s = live.settings;
    if (!s) return;
    const incoming = JSON.stringify(s);
    // Only when they changed: replacing the draft reruns this effect, and an
    // unconditional replace looped until Svelte gave up, leaving the page half drawn.
    if (!draft || (incoming !== base && JSON.stringify(draft) === base)) {
      draft = structuredClone($state.snapshot(s)) as Settings;
      base = incoming;
    }
  });

  // The password saves with the rest: the switch, and a new one typed while it's on.
  let passwordOn = $state(session.passwordSet);
  let newPassword = $state('');
  const passwordDirty = $derived(passwordOn !== session.passwordSet || (passwordOn && newPassword !== ''));
  const settingsDirty = $derived(draft !== null && JSON.stringify(draft) !== base);
  const dirty = $derived(settingsDirty || passwordDirty);

  function value(g: Group, f: Field): any {
    return (draft as any)[g.key][f.key];
  }
  function set(g: Group, f: Field, v: any) {
    (draft as any)[g.key][f.key] = v;
    saved = '';
  }
  function parseNumber(f: Field, raw: string) {
    const n = f.kind === 'int' ? parseInt(raw, 10) : parseFloat(raw);
    return Number.isNaN(n) ? raw : n;
  }
  function isDefault(g: Group, f: Field) {
    return live.defaults && JSON.stringify((live.defaults as any)[g.key][f.key]) === JSON.stringify(value(g, f));
  }

  // Hidden advanced settings that aren't the default, so turning them off doesn't hide a change.
  const hiddenChanged = $derived(advanced || !draft || !live.defaults ? 0
    : groups.reduce((n, g) => n + g.fields.filter((f) => f.advanced && !isDefault(g, f)).length, 0));

  async function save() {
    if (!draft) return;
    saving = true;
    error = '';
    try {
      if (passwordDirty) {
        if (passwordOn && (newPassword || !session.passwordSet) && newPassword.length < PASSWORD_MIN)
          throw new Error(`Enter a password of at least ${PASSWORD_MIN} characters.`);
        await api.setPassword(passwordOn ? newPassword : '');
        session.passwordSet = passwordOn;
        newPassword = '';
      }
      if (settingsDirty) {
        await api.saveSettings(draft);
        base = JSON.stringify(draft);
        saved = 'Saved. The display picks it up right away.';
        await live.loadSettings();
      } else saved = 'Saved.';
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      saving = false;
    }
  }

  function revert() {
    draft = JSON.parse(base);
    passwordOn = session.passwordSet;
    newPassword = '';
    error = '';
  }

  // Resets the fields on screen; hidden advanced ones keep their values.
  function resetGroup(g: Group) {
    if (!draft || !live.defaults) return;
    for (const f of shown(g)) (draft as any)[g.key][f.key] = structuredClone($state.snapshot((live.defaults as any)[g.key][f.key]));
    saved = '';
  }

  // Calibrate the panel saves its own colour; take it without marking the page changed.
  let calibrating = $state(false);
  function useColor(c: PanelColor) {
    if (!draft) return;
    draft.display.color = c;
    const b = JSON.parse(base);
    b.display.color = c;
    base = JSON.stringify(b);
  }
  const colorSummary = (c: PanelColor) => {
    const parts = [];
    if (c.red_pct !== 100 || c.green_pct !== 100 || c.blue_pct !== 100) parts.push(`White balance ${c.red_pct} / ${c.green_pct} / ${c.blue_pct} %`);
    if (c.shadows) parts.push(`Shadows ${c.shadows > 0 ? '+' : ''}${c.shadows}`);
    if (c.saturation_pct !== 100) parts.push(`Saturation ${c.saturation_pct} %`);
    return parts.length ? parts.join(' · ') : 'Untouched';
  };

  // The board's time zones, by region, for the picker. Old aliases (US/Pacific, GB) only show when chosen.
  const REGIONS = ['Africa', 'America', 'Antarctica', 'Arctic', 'Asia', 'Atlantic', 'Australia', 'Europe', 'Indian', 'Pacific'];
  let zones = $state<string[]>([]);
  $effect(() => {
    api.timeZones().then((z) => (zones = z), () => {});
  });
  const zoneGroups = $derived(REGIONS.map((r) => ({ region: r, names: zones.filter((z) => z.startsWith(r + '/')) })).filter((g) => g.names.length));
  const regional = (z: string) => REGIONS.some((r) => z.startsWith(r + '/'));
  const place = (z: string) => z.slice(z.indexOf('/') + 1).replaceAll('_', ' ').replaceAll('/', ' / ');

  function useCalibration(music: number, gap: number) {
    if (!draft) return;
    draft.listening.music_threshold_db = music;
    draft.listening.gap_threshold_db = gap;
    saved = '';
  }
</script>

{#if calibrating && draft}
  <PanelCalibration saved={draft.display.color} onclose={() => (calibrating = false)} onsaved={useColor} />
{/if}

<div class="settings">
<nav class="areas" aria-label="Settings">
  {#each visibleAreas as a (a.id)}
    <a href="#/settings/{a.id}" class:active={current.id === a.id} aria-current={current.id === a.id ? 'page' : undefined}>
      <Icon name={a.icon} />{a.title}{#if a.id === 'device' && problem}<span class="dot warning" title={problem} aria-label={problem}></span>{/if}
    </a>
  {/each}
</nav>
<div class="content">
{#if current.id !== 'device'}<h1>{current.title}</h1>{/if}
{#if current.id === 'device'}
  <DevicePage />
{:else if !draft}
  <p class="muted">Loading…</p>
{:else}
  {#if current.id === 'general'}
    <section class="group">
      <div class="label"><h2>Password<Tip text="Off by default. With one set, every browser and the Apple TV app have to log in." /></h2></div>
      <div class="fields"><Password bind:enabled={passwordOn} bind:value={newPassword} /></div>
    </section>
  {/if}
  {#each groups.filter((g) => g.area === current.id && (g.custom || shown(g).length)) as g (g.id)}
    <section class="group">
      <div class="label">
        <h2>{g.title}{#if g.tip}<Tip text={g.tip} />{/if}</h2>
        {#if advanced && !g.custom}<button class="reset" onclick={() => resetGroup(g)}><Icon name="reset" size={14} />Reset to defaults</button>{/if}
      </div>
      <div class="fields">
        {#if g.id === 'color'}
          <div class="field">
            <span class="name">{colorSummary(draft.display.color)}</span>
            <span class="control"><button onclick={() => (calibrating = true)}><Icon name="grid" />Calibrate</button></span>
          </div>
        {/if}
        {#if g.id === 'idle'}
          <IdleScreen idle={draft.idle} onchange={(i) => { draft!.idle = i; saved = ''; }} />
        {/if}
        {#if g.id === 'digital' || g.id === 'analog'}
          <Sources kind={g.id} sources={draft.sources} onchange={(s) => { draft!.sources = s; saved = ''; }} />
        {/if}
        {#each shown(g) as f (f.key)}
          {#if f.kind === 'transition'}
            <div class="field transition">
              <span class="name">{f.label}{#if f.tip}<Tip text={f.tip} />{/if}{#if !isDefault(g, f)}<span class="changed" title="Not the default"></span>{/if}</span>
              <TransitionPicker value={value(g, f)} ms={draft.display.crossfade_ms} onchange={(t) => set(g, f, t)} />
            </div>
          {:else if f.kind === 'rotation'}
            <div class="field">
              <span class="name">{f.label}{#if f.tip}<Tip text={f.tip} />{/if}{#if !isDefault(g, f)}<span class="changed" title="Not the default"></span>{/if}</span>
              <span class="control">
                <Segmented label="Rotation" value={value(g, f)} options={ROTATIONS} onchange={(v) => set(g, f, v)} />
              </span>
            </div>
          {:else if f.kind === 'fit'}
            <div class="field">
              <span class="name">{f.label}{#if f.tip}<Tip text={f.tip} />{/if}{#if !isDefault(g, f)}<span class="changed" title="Not the default"></span>{/if}</span>
              <span class="control">
                <Segmented label="Wide pictures" value={value(g, f)} options={FITS} onchange={(v) => set(g, f, v)} />
              </span>
            </div>
          {:else if f.kind === 'rpm'}
            <div class="field">
              <span class="name">{f.label}{#if f.tip}<Tip text={f.tip} />{/if}{#if !isDefault(g, f)}<span class="changed" title="Not the default"></span>{/if}</span>
              <span class="control">
                <Segmented label="Record speed" value={value(g, f)} options={RPMS} onchange={(v) => set(g, f, v)} />
              </span>
            </div>
          {:else if f.kind === 'speed'}
            <div class="field">
              <span class="name">{f.label}{#if f.tip}<Tip text={f.tip} />{/if}{#if !isDefault(g, f)}<span class="changed" title="Not the default"></span>{/if}</span>
              <span class="control">
                <Segmented label="Transition speed" value={SPEEDS.some((s) => s.value === value(g, f)) ? value(g, f) : null}
                  options={SPEEDS} disabled={draft.display.transition === 'cut'} onchange={(v) => set(g, f, v)} />
                {#if advanced}
                  <span class="suffixed" data-unit={f.unit}><input type="number" class="num" min={f.min} max={f.max} step={f.step} aria-label="Transition length in ms"
                    value={value(g, f)} oninput={(e) => set(g, f, parseNumber(f, e.currentTarget.value))} /></span>
                {/if}
              </span>
            </div>
          {:else}
          <label class="field">
            <span class="name">{labelOf(f)}{#if f.tip}<Tip text={f.tip} />{/if}{#if !isDefault(g, f)}<span class="changed" title="Not the default"></span>{/if}</span>
            <span class="control">
              {#if f.kind === 'bool'}
                <input type="checkbox" class="switch" checked={value(g, f)} onchange={(e) => set(g, f, e.currentTarget.checked)} />
              {:else if f.kind === 'int' || f.kind === 'float'}
                <span class="suffixed" data-unit={f.unit}><input type="number" class="num" min={f.min} max={f.max} step={f.step ?? (f.kind === 'int' ? 1 : 0.5)}
                  value={value(g, f)} oninput={(e) => set(g, f, parseNumber(f, e.currentTarget.value))} /></span>
              {:else if f.kind === 'colour'}
                <input type="color" value={value(g, f)} oninput={(e) => set(g, f, e.currentTarget.value)} />
              {:else if f.kind === 'time'}
                <input type="time" value={value(g, f)} oninput={(e) => set(g, f, e.currentTarget.value)} />
              {:else if f.kind === 'zone'}
                <select class="wide" value={value(g, f)} onchange={(e) => set(g, f, e.currentTarget.value)}>
                  {#if value(g, f) === 'UTC' || !zones.includes(value(g, f)) || !regional(value(g, f))}<option value={value(g, f)}>{value(g, f)}</option>{/if}
                  {#if value(g, f) !== 'UTC'}<option value="UTC">UTC</option>{/if}
                  {#each zoneGroups as zg (zg.region)}
                    <optgroup label={zg.region}>
                      {#each zg.names as z (z)}<option value={z}>{place(z)}</option>{/each}
                    </optgroup>
                  {/each}
                </select>
              {:else if f.kind === 'list'}
                <input class="wide" value={value(g, f).join(', ')}
                  onchange={(e) => set(g, f, e.currentTarget.value.split(',').map((s) => s.trim()).filter(Boolean))} />
              {:else if f.kind === 'numlist'}
                <span class="suffixed" data-unit={f.unit}><input class="num" value={value(g, f).join(', ')}
                  onchange={(e) => set(g, f, e.currentTarget.value.split(',').map((s) => parseInt(s, 10)).filter((n) => !Number.isNaN(n)))} /></span>
              {:else}
                <input class="wide" value={value(g, f)} oninput={(e) => set(g, f, e.currentTarget.value)} />
              {/if}
            </span>
          </label>
          {/if}
        {/each}
        {#if g.id === 'listening'}
          <Calibrate onuse={useCalibration} />
        {:else if g.id === 'power'}
          <PowerSupply limit={draft.display.current_limit_ma} />
        {/if}
      </div>
    </section>
  {/each}
{/if}
{#if draft && (current.id !== 'device' || dirty)}
  <!-- Pinned to the bottom of the window. Controller only gets it with edits left from another area. -->
  <div class="savebar">
    <div class="inner">
      {#if error}<p class="error">{error}</p>
      {:else if dirty}<p class="muted">Unsaved changes</p>
      {:else if saved}<p class="muted">{saved}</p>{/if}
      <span class="spacer"></span>
      {#if current.id !== 'device'}
        <label class="toggle"><input type="checkbox" bind:checked={advanced} />Show advanced{#if hiddenChanged}<span class="faint">({hiddenChanged} changed)</span>{/if}</label>
      {/if}
      <button disabled={!dirty} onclick={revert}><Icon name="reset" />Revert</button>
      <button class="primary" disabled={!dirty || saving} onclick={save}><Icon name="check" />{saving ? 'Saving…' : 'Save'}</button>
    </div>
  </div>
{/if}
</div>
</div>

<style>
  /* The areas on the left (200), the one open beside them. */
  .settings { display: grid; grid-template-columns: 200px minmax(0, 1fr); gap: 40px; align-items: start; }
  .areas { position: sticky; top: 81px; display: flex; flex-direction: column; gap: 2px; }
  .areas a { display: flex; align-items: center; gap: 10px; height: 36px; padding: 0 12px; border-radius: var(--radius-inner); color: #bbbbbb; font-weight: 500; text-decoration: none; white-space: nowrap; }
  .areas a:hover, .areas a.active { color: #ffffff; }
  .areas a.active { background: var(--raised); }
  .areas .dot { margin-left: -2px; }
  .toggle { display: inline-flex; align-items: center; gap: 8px; margin-right: 8px; color: var(--muted); cursor: pointer; }
  .content { padding-bottom: 72px; }
  /* No panels behind groups or the Controller page's blocks: rules between them do the separating. */
  .content :global(.card) { background: none; border-radius: 0; padding: 0; }
  .content > h1 { margin-bottom: 20px; }
  .savebar { position: fixed; left: 0; right: 0; bottom: 0; z-index: 2; background: var(--ground); border-top: 1px solid var(--border); }
  .savebar .inner { max-width: 1200px; margin: 0 auto; padding: 12px 24px; display: flex; align-items: center; gap: 8px; }
  .savebar .spacer { flex: 1; }
  /* Each group: its title (and Reset) on a line, its fields under it. Space splits groups; rules split rows. */
  .group { display: flex; flex-direction: column; gap: 4px; }
  .group + .group { margin-top: 32px; }
  .label { display: flex; align-items: center; gap: 12px; min-height: 30px; }
  .reset { margin-left: auto; height: 30px; padding: 0 12px; font-size: 13px; color: var(--muted); }
  .reset:hover:not(:disabled) { color: var(--text); }
  .fields { display: flex; flex-direction: column; min-width: 0; }
  .field { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 2px 16px; align-items: center; padding: 8px 0; border-top: 1px solid var(--rule); }
  .field:first-child { border-top: none; }
  .name { display: inline-flex; align-items: center; gap: 8px; font-weight: 500; }
  .changed { width: 6px; height: 6px; border-radius: 50%; background: var(--muted); }
  .control { display: flex; align-items: center; gap: 8px; justify-content: flex-end; }
  .control input[type='number'], .control input.num { width: 128px; text-align: right; }
  /* The unit sits inside its field, so every control ends on the same right edge. */
  .suffixed { position: relative; display: flex; }
  .suffixed[data-unit] input { padding-right: 52px; }
  .suffixed[data-unit]::after { content: attr(data-unit); position: absolute; right: 12px; top: 0; bottom: 0; display: flex; align-items: center; color: var(--faint); font-size: var(--fs-small); pointer-events: none; }
  .control input.wide, .control select.wide { width: 300px; }
  h2 { display: inline-flex; align-items: center; gap: 8px; }
  /* The picker and its preview need the full width: the label goes above. */
  .transition { grid-template-columns: 1fr; gap: 12px; }
  @media (max-width: 860px) {
    .settings { grid-template-columns: 1fr; gap: 20px; }
    .areas { position: static; flex-direction: row; overflow-x: auto; margin: 0 -4px; }
    .control input.wide, .control select.wide { width: 200px; }
  }
  @media (max-width: 520px) {
    .field { grid-template-columns: 1fr; }
    /* A switch or a number stays beside its label. */
    .field:has(.switch), .field:has(.suffixed) { grid-template-columns: minmax(0, 1fr) auto; }
    .control { justify-content: flex-start; }
    .control input.wide, .control select.wide { width: 100%; flex: 1; }
    .savebar .inner { padding: 10px 16px; flex-wrap: wrap; }
  }
</style>
