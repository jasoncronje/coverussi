export function clock(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000));
  const m = Math.floor(s / 60);
  return m >= 60 ? `${Math.floor(m / 60)}:${String(m % 60).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`
    : `${m}:${String(s % 60).padStart(2, '0')}`;
}

export function ago(epochS: number, nowMs = Date.now()): string {
  const s = Math.max(0, Math.round(nowMs / 1000 - epochS));
  if (s < 5) return 'just now';
  if (s < 60) return `${s} s ago`;
  if (s < 3600) return `${Math.floor(s / 60)} min ago`;
  if (s < 86400) return `${Math.floor(s / 3600)} h ago`;
  return new Date(epochS * 1000).toLocaleDateString();
}

export function time(epochS: number): string {
  const d = new Date(epochS * 1000);
  const today = new Date().toDateString() === d.toDateString();
  return today ? d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })
    : d.toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
}

export function bytes(n: number): string {
  return n > 1e6 ? `${(n / 1e6).toFixed(2)} MB` : `${Math.round(n / 1e3)} KB`;
}

export const outcomeLabel: Record<string, string> = {
  match: 'Match',
  no_match: 'No match',
  rate_limited: 'Rate limited',
  error: 'Error'
};

// Uptime in words: 40 s, 12 min, 14 h 15 min, 3 d 4 h.
export function duration(s: number): string {
  s = Math.max(0, Math.round(s));
  const d = Math.floor(s / 86400), h = Math.floor(s / 3600) % 24, m = Math.floor(s / 60) % 60;
  if (d) return `${d} d ${h} h`;
  if (h) return `${h} h ${m} min`;
  return m ? `${m} min` : `${s} s`;
}

// RSSI in words, so -49 reads as a strong signal.
export const signal = (dbm: number) => (dbm >= -55 ? 'Strong' : dbm >= -67 ? 'Good' : dbm >= -75 ? 'Fair' : 'Weak');
