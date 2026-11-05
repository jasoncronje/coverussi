export type Mode = 'off' | 'digital' | 'analog';
export type ListenerState = 'quiet' | 'listening' | 'playing';

export interface Telemetry {
  mode: Mode;
  source: string;
  source_name?: string; // the player's Home Assistant name
  app?: string; // the Apple TV's current app
  title: string;
  artist: string;
  album?: string;
  art: string;
  art_dimmed: boolean;
  panel_on: boolean;
  brightness: number;
  requested_brightness: number;
  current_ma: number;
  fps: number;
  settings_version: number;
  mic: boolean;
  calibrating: boolean;
  test_pattern?: boolean;
  listener: {
    state: ListenerState;
    busy: boolean;
    next_identify_in_ms: number;
    failures: number;
    given_up?: boolean; // listening to the room: misses ran out, waiting for a quiet spell
    music: boolean;
    silent_ms: number;
    level_db: number;
  };
  // Added by the board
  firmware?: string;
  uptime_s?: number;
  ha_connected?: boolean;
  audio?: string;
  wifi_rssi?: number;
  wifi_ssid?: string;
  free_heap?: number;
  min_free_heap?: number;
  restart_reason?: string;
  free_psram?: number;
  ip?: string;
  clock_set?: boolean;
  rate_limit_wait_ms?: number;
}

export interface NowPlaying {
  title: string;
  artist: string;
  album: string;
  art_url: string | null;
  duration_ms: number | null;
  matched_at: number;
  offset_ms: number | null;
}

export interface Answer {
  match: boolean;
  reason: string;
  title?: string;
  artist?: string;
  album?: string;
  art_url?: string | null;
  offset_ms?: number;
  duration_ms?: number | null;
  ends_in_ms?: number | null;
  album_locked?: boolean;
  retry_in_ms?: number;
}

/** One of the last ten songs: a record identified by listening (heard) or a streamed one. */
export interface Song {
  at: number; // unix seconds
  source: string; // the analog source's name, or the player's entity
  title: string;
  artist: string;
  album?: string | null;
  art_url?: string | null;
  heard?: boolean;
  shazam_album?: string | null; // what a correction is keyed on
}

/** A smart player that tells Home Assistant what it plays; apps limits it to those apps or inputs. */
export interface DigitalSource {
  entity: string;
  apps: string[];
  /** The apps listed don't show art; every other one does. */
  exclude?: boolean;
  /** Off: kept on the list but not followed. */
  enabled: boolean;
}

/** A device that can't say what it plays; it plays while the entity's attribute (or "state") has this value. */
export interface AnalogSource {
  name: string;
  entity: string;
  attribute: string;
  value: string;
  /** Off: kept on the list but not followed. */
  enabled: boolean;
}

import type { Transition } from './transition';

// How the panel draws colour, set on the calibration screen.
export interface PanelColor {
  red_pct: number;
  green_pct: number;
  blue_pct: number;
  shadows: number;
  saturation_pct: number;
}

export interface Settings {
  display: {
    brightness_day: number;
    brightness_night: number;
    night_start: string;
    night_end: string;
    current_limit_ma: number;
    crossfade_ms: number;
    transition: Transition;
    fit: 'crop' | 'letterbox' | 'pan';
    rotation: number;
    pause_grace_s: number;
    side_end_dim_s: number;
    time_zone: string;
    color: PanelColor;
    song_screen: boolean;
    song_cover_s: number;
    song_text_s: number;
    record: boolean;
    record_rpm: 33 | 45 | 78;
    record_label: string; // #rrggbb
    record_sheen: boolean;
  };
  sources: { digital: DigitalSource[]; analog: AnalogSource[]; listen_for_music: boolean };
  listening: {
    music_threshold_db: number;
    gap_threshold_db: number;
    min_gap_ms: number;
    side_end_silence_s: number;
    clip_ms: number;
    retry_backoff_s: number[];
    relisten_after_end_ms: number;
    mic_gain_db: number;
    music_start_s: number;
  };
  recognition: { min_interval_s: number; replace_after: number; session_timeout_s: number; prefer_album: boolean; keep_clips: boolean; store_country: string };
  home_assistant: { mqtt_discovery: boolean; name: string };
  idle: Idle;
  setup_done: boolean; // the first-run setup was finished or skipped
}

/** What the panel shows with no art: dark, a clock, the weather, or up to four entities. */
export type IdleShow = 'off' | 'clock' | 'weather' | 'entities';

export interface Idle {
  show: IdleShow;
  weather: string;
  entities: { entity: string; name: string }[];
  clock_24h: boolean;
}

// The device's answer to the apps command: a player's apps or inputs as Home Assistant lists them.
export interface AppsResult {
  entity: string;
  followed: boolean;
  state: string;
  app_name: string;
  app_id: string;
  source: string;
  apps: string[];
}

export interface Calibration {
  p10: number;
  median: number;
  p90: number;
  samples: number;
}

/** A Home Assistant entity, for autocomplete. */
export interface HaTriggers {
  states: string[];
  attributes: Record<string, string[]>;
}

export interface HaEntity {
  entity: string;
  name: string;
  state: string;
}

/** The latest GitHub release against the board's firmware. */
export interface UpdateCheck {
  current: string;
  latest: string;
  newer: boolean;
  page: string;
  size: number;
}

/** An update from GitHub as it runs. */
export interface UpdateStatus {
  phase: 'starting' | 'downloading' | 'restarting' | 'failed';
  percent?: number;
  error?: string;
}
