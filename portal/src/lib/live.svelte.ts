// The display's live feed (/api/live): telemetry, panel frames, mic levels,
// recognitions, the recent songs and logs, kept here for every page to read.
import { api } from './api';
import type { Answer, NowPlaying, Settings, Song, Telemetry, UpdateStatus } from './types';

export interface Level {
  at: number; // ms, Date.now()
  db: number;
}

export interface LogLine {
  at: number;
  kind: 'log' | 'event' | 'command' | 'recognition';
  line: string;
}

export interface DeviceEvent {
  name: string;
  data: any;
}

const LEVEL_KEEP_MS = 60_000;
const LOG_KEEP = 500;

class Live {
  connected = $state(false);
  telemetry = $state<Partial<Telemetry>>({});
  telemetryAt = $state(Date.now()); // when it arrived, for countdowns
  nowPlaying = $state<NowPlaying | null>(null);
  haUrl = $state(''); // where Home Assistant's art paths resolve
  recent = $state<Song[]>([]);
  rateLimitWaitMs = $state(0);
  rateLimitAt = $state(0);
  logs = $state<LogLine[]>([]);
  settings = $state<Settings | null>(null);
  defaults = $state<Settings | null>(null);
  settingsVersion = $state(0);
  update = $state<UpdateStatus | null>(null);
  now = $state(Date.now());

  // Frames and levels change ten times a second: plain values plus a counter to redraw on.
  frame: Uint8Array | null = null;
  frameTick = $state(0);
  levels: Level[] = [];
  levelTick = $state(0);

  private ws: WebSocket | null = null;
  private running = false;
  private retryMs = 1000;
  private clock: ReturnType<typeof setInterval> | null = null;
  private listeners = new Set<(e: DeviceEvent) => void>();

  /** ms since the telemetry arrived */
  get telemetryAge(): number {
    return this.now - this.telemetryAt;
  }

  get rateLimitLeftMs(): number {
    return Math.max(0, this.rateLimitWaitMs - (this.now - this.rateLimitAt));
  }

  onEvent(fn: (e: DeviceEvent) => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  start() {
    if (this.running) return;
    this.running = true;
    this.clock = setInterval(() => (this.now = Date.now()), 250);
    void this.loadSettings();
    this.connect();
  }

  stop() {
    this.running = false;
    if (this.clock) clearInterval(this.clock);
    this.ws?.close();
    this.ws = null;
    this.connected = false;
  }

  async loadSettings() {
    const s = await api.settings();
    this.settings = s.settings;
    this.defaults = s.defaults;
    this.settingsVersion = s.version;
  }

  private connect() {
    if (!this.running) return;
    const ws = new WebSocket(`${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/api/live`);
    ws.binaryType = 'arraybuffer';
    this.ws = ws;
    ws.onopen = () => {
      this.connected = true;
      this.retryMs = 1000;
    };
    ws.onmessage = (e) => {
      if (e.data instanceof ArrayBuffer) {
        this.frame = new Uint8Array(e.data);
        this.frameTick++;
        return;
      }
      try {
        this.handle(JSON.parse(e.data));
      } catch (err) {
        console.warn('live message', err);
      }
    };
    ws.onclose = () => {
      this.connected = false;
      if (this.ws !== ws) return;
      this.ws = null;
      // A closed feed may mean logged out: the session check says, and shows the login page if so.
      void api.session().then((s) => {
        if (!s.logged_in) this.running = false;
      }, () => {});
      setTimeout(() => this.connect(), this.retryMs);
      this.retryMs = Math.min(this.retryMs * 2, 15_000);
    };
  }

  private log(kind: LogLine['kind'], line: string) {
    const next = [...this.logs, { at: Date.now(), kind, line }];
    this.logs = next.length > LOG_KEEP ? next.slice(-LOG_KEEP) : next;
  }

  private rateLimit(ms: number) {
    this.rateLimitWaitMs = ms;
    this.rateLimitAt = Date.now();
  }

  private handle(m: any) {
    switch (m.type) {
      case 'snapshot':
        this.telemetry = m.telemetry ?? {};
        this.telemetryAt = Date.now();
        this.nowPlaying = m.now_playing ?? null;
        this.haUrl = m.ha_url ?? '';
        this.recent = m.recent ?? [];
        this.rateLimit(m.rate_limit_wait_ms ?? 0);
        this.update = m.update ?? null;
        if (m.settings_version !== this.settingsVersion) void this.loadSettings();
        break;
      case 'telemetry':
        this.telemetry = m.data;
        this.telemetryAt = Date.now();
        break;
      case 'level': {
        const now = Date.now();
        this.levels.push({ at: now, db: m.db });
        while (this.levels.length && this.levels[0].at < now - LEVEL_KEEP_MS) this.levels.shift();
        this.levelTick++;
        break;
      }
      case 'now_playing':
        this.nowPlaying = m.data ?? null;
        break;
      case 'recent':
        this.recent = m.songs ?? [];
        break;
      case 'recognition': {
        const a = m.answer as Answer;
        if (a.reason === 'rate_limited') this.rateLimit(a.retry_in_ms ?? 0);
        this.log('recognition', a.match ? `match: ${a.artist} – ${a.title} (${a.album}; ${a.reason})` : `no match: ${a.reason}`);
        break;
      }
      case 'event':
        if (m.name === 'analog_stopped') this.nowPlaying = null;
        this.log('event', `${m.name}${m.data ? ' ' + JSON.stringify(m.data) : ''}`);
        for (const fn of this.listeners) fn({ name: m.name, data: m.data });
        break;
      case 'log':
        this.log('log', m.line);
        break;
      case 'update':
        this.update = m.data ?? null;
        break;
      case 'settings':
        if (m.version !== this.settingsVersion) void this.loadSettings();
        break;
    }
  }
}

export const live = new Live();
