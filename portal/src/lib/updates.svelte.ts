// Whether GitHub has a newer release, asked of the display now and then so the
// bar can say so. The answer is kept in the browser, so a reload doesn't ask again.
import { api } from './api';
import type { UpdateCheck } from './types';

const EVERY = 6 * 60 * 60 * 1000;
const KEY = 'coverussi.update';

export const updates = $state<{ check: UpdateCheck | null }>({ check: null });

function saved(): { at: number; check: UpdateCheck } | null {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? 'null');
  } catch {
    return null;
  }
}

async function ask() {
  try {
    const check = await api.checkUpdate();
    updates.check = check;
    localStorage.setItem(KEY, JSON.stringify({ at: Date.now(), check }));
  } catch {
    // No answer (offline, GitHub down): try again next time.
  }
}

/** Asks now if the last answer is old, then every few hours; returns a stop. */
export function watchUpdates(): () => void {
  const last = saved();
  if (last) updates.check = last.check;
  if (!last || Date.now() - last.at > EVERY) void ask();
  const timer = setInterval(() => void ask(), EVERY);
  return () => clearInterval(timer);
}
