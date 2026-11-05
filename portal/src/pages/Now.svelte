<script lang="ts">
  import { api } from '../lib/api';
  import Icon from '../lib/Icon.svelte';
  import { clock, ago } from '../lib/format';
  import LevelGraph from '../lib/LevelGraph.svelte';
  import { live } from '../lib/live.svelte';
  import Panel from '../lib/Panel.svelte';
  import Segmented from '../lib/Segmented.svelte';

  const online = $derived(live.connected);
  const t = $derived(live.telemetry);
  const listener = $derived(t.listener);
  const playing = $derived(live.nowPlaying);
  const nextIdentify = $derived(listener ? Math.max(0, listener.next_identify_in_ms - live.telemetryAge) : 0);
  const showLevels = $derived(Boolean(t.mic) || t.mode === 'analog');

  let message = $state('');
  async function run(fn: () => Promise<unknown>, done?: string) {
    message = '';
    try {
      await fn();
      if (done) message = done;
    } catch (e) {
      message = e instanceof Error ? e.message : String(e);
    }
  }
  const command = (name: string, args: Record<string, unknown> = {}) => run(() => api.command(name, args));

  const stateLabel = { quiet: 'Waiting for music', listening: 'Listening', playing: 'Playing' } as const;
  const sourceName = (entity?: string) => entity?.replace(/^media_player\./, '').replaceAll('_', ' ') ?? '';
  const player = $derived(t.source_name || sourceName(t.source));
  const analogName = $derived(t.source_name || 'Analog source');

  const title = $derived(t.mode === 'analog' && playing ? playing.title : t.title || 'Nothing playing');
  const byline = $derived(
    t.mode === 'analog' && playing ? `${playing.artist} · ${playing.album}`
      : t.title ? `${t.artist}${t.album ? ` · ${t.album}` : ''}` : ''
  );
  // The one line under the title, only for what the cards don't already show.
  const finding = $derived.by(() => {
    if (!online) return 'Offline';
    if (t.test_pattern) return 'Test pattern';
    if (t.mode === 'analog' && playing) return `Matched ${ago(playing.matched_at, live.now)}${t.art_dimmed ? ' · side ended' : ''}`;
    return '';
  });
  // Songs identified on analog sources and played on digital ones, newest first.
  // As many as fit beside the art, and the mic card when it's there.
  const recent = $derived(
    live.recent.slice(0, showLevels ? 6 : 3).map((s, i) => ({
      key: `${s.at}-${i}`, at: s.at, title: s.title, artist: s.artist, album: s.album, art_url: s.art_url,
      from: s.heard ? s.source || 'Analog' : s.source === t.source && t.source_name ? t.source_name : sourceName(s.source)
    }))
  );
  // The mirror shows the panel's LEDs or the cover itself; this browser remembers which.
  const VIEW_KEY = 'home.view';
  let view = $state<'led' | 'album'>(localStorage.getItem(VIEW_KEY) === 'album' ? 'album' : 'led');
  $effect(() => localStorage.setItem(VIEW_KEY, view));
  // Home Assistant hands out paths (with their own token); covers from elsewhere are URLs.
  const cover = $derived(!t.art ? '' : t.art.startsWith('/') ? (live.haUrl ? live.haUrl.replace(/\/$/, '') + t.art : '') : /^https?:/.test(t.art) ? t.art : '');
  let coverFailed = $state(false);
  $effect(() => {
    void cover;
    coverFailed = false;
  });
  const shortTime = (epochS: number) => new Date(epochS * 1000).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
</script>

  <div class="title-row">
    <div>
      <h1 class:faint={!t.title && !playing}>{title}</h1>
      {#if byline}<p class="muted">{byline}</p>{/if}
      {#if finding}<p class="faint">{finding}</p>{/if}
    </div>
    {#if t.mode === 'analog'}
      <div class="actions">
        {#if playing}
          <button onclick={() => run(() => api.pin(), `Keeping ${playing.album} until ${analogName} stops.`)}><Icon name="pin" />Keep this album</button>
        {/if}
        <button class="primary" disabled={!online || listener?.busy} onclick={() => command('identify')}>
          <Icon name="identify" />
          {listener?.busy ? 'Identifying…' : 'Identify now'}
        </button>
      </div>
    {/if}
  </div>
  {#if message}<p class="muted message">{message}</p>{/if}

  <div class="top">
    <div class="col">
      <section class="card mirror" class:off={t.panel_on === false || !online}>
        <div class="card-head"><h2>Display</h2><span class="spacer"></span>
          <Segmented label="Show" value={view} onchange={(v) => (view = v)}
            options={[{ value: 'led', label: 'LED panel' }, { value: 'album', label: 'Album' }]} />
        </div>
        {#if view === 'led'}<Panel size={448} />
        {:else if cover && !coverFailed}<img class="cover" class:dimmed={t.art_dimmed} src={cover} alt={t.album || title} onerror={() => (coverFailed = true)} />
        {:else}<div class="cover empty faint">{cover ? 'Cover didn’t load' : 'No cover'}</div>{/if}
        {#if !online}<div class="overlay">Offline</div>
        {:else if t.panel_on === false}<div class="overlay">Panel off</div>{/if}
      </section>
      {#if showLevels}
        <section class="card mic-card">
          <div class="card-head"><h2>Microphone</h2><span class="spacer"></span>
            {#if t.calibrating}<span class="pill warning">Calibrating</span>{/if}
          </div>
          {#if live.settings}
            <LevelGraph music={live.settings.listening.music_threshold_db} gap={live.settings.listening.gap_threshold_db} />
          {/if}
        </section>
      {/if}
    </div>

    <div class="side">
      {#if t.mode === 'analog' && listener}
        <section class="card">
          <div class="card-head"><h2>{analogName}</h2><span class="spacer"></span>
            <span class="status muted"><span class="dot" class:accent={listener.state === 'playing'} class:warning={listener.state === 'listening'}></span>{stateLabel[listener.state]}</span>
          </div>
          <dl class="facts">
            <dt>Next identify</dt>
            <dd>{listener.busy ? 'sending clip…' : listener.given_up ? 'after a quiet spell (not music)' : listener.state === 'quiet' ? 'when music starts' : nextIdentify > 0 ? `in ${clock(nextIdentify)}` : 'now'}</dd>
            <dt>Music heard</dt><dd>{listener.music ? 'yes' : 'no'}{!listener.music && listener.silent_ms > 0 ? ` · quiet for ${clock(listener.silent_ms + live.telemetryAge)}` : ''}</dd>
            <dt>Misses in a row</dt><dd>{listener.failures}</dd>
            <dt>Track length</dt><dd>{playing?.duration_ms ? clock(playing.duration_ms) : '–'}</dd>
            <dt>Shazam cooldown</dt><dd>{live.rateLimitLeftMs > 0 ? clock(live.rateLimitLeftMs) : '–'}</dd>
          </dl>
        </section>
      {:else}
        <section class="card">
          <div class="card-head"><h2>Source</h2></div>
          <dl class="facts">
            <dt>Mode</dt><dd>{t.mode === 'digital' ? 'Digital' : t.mode === 'analog' ? 'Analog' : t.mode === 'film' ? 'Film' : 'Nothing playing'}</dd>
            <dt>Player</dt><dd>{player || '–'}</dd>
            {#if t.app}<dt>App</dt><dd>{t.app}</dd>{/if}
            <dt>Album</dt><dd>{t.album || '–'}</dd>
            <dt>Art</dt><dd>{!t.art ? '–' : t.art_dimmed ? 'dimmed' : 'showing'}</dd>
          </dl>
        </section>
      {/if}

      <section class="card grow">
        <div class="card-head"><h2>Recent</h2><span class="spacer"></span><a class="muted view-all" href="#/recent">More<Icon name="arrow-right" /></a></div>
        {#if recent.length === 0}
          <p class="empty">Nothing played yet.</p>
        {:else}
          <ul class="recent">
            {#each recent as r (r.key)}
              <li>
                {#if r.art_url}<img class="art-thumb" src={r.art_url} alt="" />{:else}<div class="art-thumb"></div>{/if}
                <div class="who">
                  <span class="name">{r.title}</span>
                  <span class="sub">{[r.artist, r.album].filter(Boolean).join(' · ')}</span>
                </div>
                <div class="when">
                  <span class="faint num">{shortTime(r.at)}</span>
                  <span class="faint small">{r.from}</span>
                </div>
              </li>
            {/each}
          </ul>
        {/if}
      </section>
    </div>
  </div>

<style>
  /* 1152 = 492 (the mirror: 448 + 2 × 22) + 14 + 646. */
  .top { display: grid; grid-template-columns: 492px minmax(0, 1fr); gap: var(--gap); }
  /* Uneven bento: the art over the mic on the left; Source over Recent, which runs the full height, on the right. */
  .col, .side { display: flex; flex-direction: column; gap: var(--gap); }
  .grow { flex: 1; }

  .view-all { display: inline-flex; align-items: center; gap: 5px; white-space: nowrap; text-decoration: none; }
  .view-all:hover { color: var(--text); }
  .message { margin: -12px 0 16px; }

  .mirror { position: relative; justify-content: center; }
  /* The switch is taller than a card title: it overhangs the head so the title lines up with Source's. */
  .mirror .card-head :global(.segmented) { margin: -7px 0; }
  .mirror.off :global(canvas), .mirror.off .cover { opacity: 0.25; }
  .cover { display: block; width: 448px; max-width: 100%; aspect-ratio: 1; object-fit: cover; border-radius: 4px; background: #050505; }
  .cover.dimmed { opacity: 0.5; }
  .cover.empty { display: grid; place-items: center; }
  .overlay { position: absolute; inset: 0; display: grid; place-items: center; color: var(--muted); font-weight: 500; pointer-events: none; }

  .recent { list-style: none; margin: 0; padding: 0; }
  .recent li { display: flex; align-items: center; gap: 12px; padding: 12px 0; border-top: 1px solid var(--rule); }
  .recent li:first-child { border-top: none; padding-top: 0; }
  .who { flex: 1; min-width: 0; }
  .name, .sub { display: block; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .name { font-weight: 500; }
  .when { flex: none; display: flex; flex-direction: column; align-items: flex-end; gap: 2px; }

  @media (max-width: 1000px) {
    .top { grid-template-columns: minmax(0, 1fr); }
    .col, .side { display: contents; }
    .mic-card { order: 1; }
    .mirror { justify-self: center; width: 100%; max-width: 492px; }
  }
</style>
