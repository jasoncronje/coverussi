import type { AppsResult, HaEntity, HaTriggers, Settings } from './types';

export class ApiError extends Error {
  constructor(public status: number, message: string) {
    super(message);
  }
}

let onUnauthorized = () => {};
export function whenUnauthorized(fn: () => void) {
  onUnauthorized = fn;
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const init: RequestInit = { method, credentials: 'same-origin' };
  if (body instanceof Blob) {
    init.body = body;
    init.headers = { 'Content-Type': 'application/octet-stream' };
  } else if (body !== undefined) {
    init.body = JSON.stringify(body);
    init.headers = { 'Content-Type': 'application/json' };
  }
  const r = await fetch(path, init);
  if (!r.ok) {
    if (r.status === 401 && path !== '/api/login') onUnauthorized();
    let message = r.statusText;
    try {
      message = String((await r.json()).detail);
    } catch {
      // not JSON
    }
    throw new ApiError(r.status, message);
  }
  return r.json() as Promise<T>;
}

export const api = {
  session: () => request<{ logged_in: boolean; password_set: boolean }>('GET', '/api/session'),
  login: (password: string) => request<{ ok: boolean }>('POST', '/api/login', { password }),
  logout: () => request<{ ok: boolean }>('POST', '/api/logout'),
  // Sets or changes the password; '' removes it.
  setPassword: (password: string) => request<{ ok: boolean }>('POST', '/api/password', { password }),
  timeZones: () => request<string[]>('GET', '/api/time_zones'),
  settings: () => request<{ version: number; settings: Settings; defaults: Settings }>('GET', '/api/settings'),
  saveSettings: (s: Settings) => request<{ version: number }>('PUT', '/api/settings', s),
  command: (name: string, args: Record<string, unknown> = {}) =>
    request<{ ok: boolean }>('POST', '/api/commands', { name, args }),
  pin: () => request<{ ok: boolean }>('POST', '/api/pin'),
  corrections: () => request<Record<string, { album: string; art_url: string | null }>>('GET', '/api/corrections'),
  saveCorrection: (c: { artist: string; shazam_album: string; album: string; art_url: string | null }) =>
    request<{ key: string }>('POST', '/api/corrections', c),
  deleteCorrection: (key: string) => request<{ ok: boolean }>('DELETE', `/api/corrections/${encodeURIComponent(key)}`),
  // A player's apps or inputs, read from Home Assistant (works before the player is saved).
  haApps: (entity: string) => request<AppsResult>('GET', `/api/ha/apps?entity=${encodeURIComponent(entity)}`),
  // An entity's state and text attributes with the values each can take, for an analog source's trigger.
  haTriggers: (entity: string) => request<HaTriggers>('GET', `/api/ha/triggers?entity=${encodeURIComponent(entity)}`),
  // Home Assistant entities for autocomplete; available is false without Home Assistant set up.
  haEntities: (q: string, domains: string[] = []) =>
    request<{ available: boolean; entities: HaEntity[] }>(
      'GET', `/api/ha/entities?q=${encodeURIComponent(q)}&domain=${encodeURIComponent(domains.join(','))}`),
  installFirmware: (file: Blob) => request<{ ok: boolean; size: number }>('POST', '/api/firmware', file)
};
