<script lang="ts">
  import { onMount } from 'svelte';
  import { api, whenUnauthorized } from './lib/api';
  import { live } from './lib/live.svelte';
  import { session as auth } from './lib/session.svelte';
  import { updates, watchUpdates } from './lib/updates.svelte';
  import Icon from './lib/Icon.svelte';
  import Login from './pages/Login.svelte';
  import Now from './pages/Now.svelte';
  import Recent from './pages/Recent.svelte';
  import SettingsPage from './pages/Settings.svelte';
  import Setup from './pages/Setup.svelte';

  const pages = { home: 'Home', recent: 'Recent', settings: 'Settings' } as const;
  type Page = keyof typeof pages;
  // Links saved before a rename or move still land; the old Display page is the Controller area now.
  const moved: Record<string, string> = { now: 'home', history: 'recent', device: 'settings/device', display: 'settings/device', 'settings/listening': 'settings/general', 'settings/home_assistant': 'settings/general' };

  let session = $state<'checking' | 'in' | 'out'>('checking');
  // #/settings/panel: the page, then where in it.
  let route = $state(readHash());
  const page = $derived(route[0] as Page);
  // First run, or #/setup to go through it again.
  let setupAsked = $state(location.hash === '#/setup');
  const setup = $derived(setupAsked || live.settings?.setup_done === false);

  function readHash(): [Page, string] {
    let p = location.hash.replace(/^#\/?/, '');
    p = moved[p] ?? p;
    const [first, rest = ''] = p.split(/\/(.*)/);
    return first in pages ? [first as Page, rest] : ['home', ''];
  }

  let stopUpdates: (() => void) | null = null;

  function enter() {
    session = 'in';
    live.start();
    stopUpdates ??= watchUpdates();
  }

  async function logout() {
    await api.logout();
    live.stop();
    stopUpdates?.();
    stopUpdates = null;
    session = 'out';
  }

  onMount(() => {
    whenUnauthorized(() => {
      live.stop();
      session = 'out';
    });
    const onHash = () => {
      route = readHash();
      setupAsked = location.hash === '#/setup';
    };
    window.addEventListener('hashchange', onHash);
    api.session().then((s) => {
      auth.passwordSet = s.password_set;
      if (s.logged_in) enter();
      else session = 'out';
    }, () => (session = 'out'));
    return () => window.removeEventListener('hashchange', onHash);
  });

  // A newer release than the one running, until it's installing; Controller has the button.
  const update = $derived.by(() => {
    const c = updates.check;
    const running = live.telemetry.firmware;
    return c?.newer && c.latest !== running && !live.update ? c.latest : '';
  });

  // Only a problem earns a mark in the bar, on Settings, which has the details under Controller.
  const problem = $derived(!live.connected ? 'Offline' : live.telemetry.ha_connected === false ? 'Home Assistant not connected' : '');
</script>

{#if session === 'out'}
  <Login onlogin={enter} />
{:else if session === 'in' && setup}
  <Setup ondone={() => { setupAsked = false; location.hash = '#/home'; }} />
{:else if session === 'in'}
  <header>
    <div class="bar">
      <a class="brand" href="#/home">
        <img class="mark" src="/wordmark.svg" alt="Coverussi" />
      </a>
      <nav>
        {#each Object.entries(pages) as [key, label]}
          <a href="#/{key}" class:active={page === key} aria-current={page === key ? 'page' : undefined}><Icon name={key as Page} />{label}{#if key === 'settings' && problem}<span class="dot warning" title={problem} aria-label={problem}></span>{/if}</a>
        {/each}
      </nav>
      <div class="end">
        {#if update}<a class="update" href="#/settings/device" title="Install it under Settings > Controller"><Icon name="download" />Update to {update}</a>{/if}
        {#if auth.passwordSet}<button class="link" onclick={logout}><Icon name="log-out" />Log out</button>{/if}
      </div>
    </div>
  </header>

  <main>
    {#if page === 'home'}<Now />
    {:else if page === 'recent'}<Recent />
    {:else}<SettingsPage area={route[1]} problem={problem} />{/if}
  </main>
{/if}

<style>
  /* One centred 1152px column: logo and Log out on the content's edges, the pages centred between. */
  header { position: sticky; top: 0; z-index: 2; background: var(--ground); border-bottom: 1px solid var(--border); }
  .bar, main { max-width: 1200px; margin: 0 auto; padding: 0 24px; }
  .bar { height: 48px; display: grid; grid-template-columns: 1fr auto 1fr; align-items: center; gap: 28px; }
  .end { justify-self: end; display: flex; align-items: center; gap: 16px; }
  .update { display: inline-flex; align-items: center; gap: 6px; height: 26px; padding: 0 11px; border-radius: 999px; background: var(--accent); color: #000000; font-size: 13px; font-weight: 500; text-decoration: none; white-space: nowrap; }
  .update:hover { background: #ffc85c; }
  .brand { justify-self: start; display: flex; align-items: center; gap: 10px; font-weight: 600; text-decoration: none; }
  .mark { height: 20px; display: block; }
  nav { display: flex; gap: 22px; }
  nav a { text-decoration: none; color: #bbbbbb; font-weight: 500; display: inline-flex; align-items: center; gap: 7px; }
  nav a:hover, nav a.active { color: #ffffff; }
  nav .dot { margin-left: -2px; }
  main { padding-top: 32px; padding-bottom: 64px; }
  @media (max-width: 640px) {
    .bar { height: auto; display: flex; flex-wrap: wrap; justify-content: space-between; row-gap: 0; column-gap: 16px; padding: 0 16px; }
    .brand { height: 48px; }
    nav { order: 3; width: 100%; height: 40px; align-items: center; gap: 20px; overflow-x: auto; }
    main { padding: 20px 16px 48px; }
  }
</style>
