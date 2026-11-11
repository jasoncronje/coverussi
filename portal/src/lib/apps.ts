// What each app on a digital source shows, read and changed the way the device
// reads it (Settings::app_show): progress, off, then the cover list, else `other`.
import type { AnalogSource, AppShow, DigitalSource } from './types';

const same = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();
const has = (list: string[] | undefined, app: string) => !!list?.some((a) => same(a, app));

/** Apps on no list; older settings: the cover when the list is empty or excludes, else nothing. */
export function otherShow(d: DigitalSource): AppShow {
  return d.other ?? (!d.apps.length || d.exclude ? 'cover' : 'off');
}

export function appShow(d: DigitalSource, app: string): AppShow {
  if (has(d.progress, app)) return 'progress';
  if (has(d.off, app)) return 'off';
  if (has(d.apps, app)) return d.exclude ? 'off' : 'cover';
  return otherShow(d);
}

/** The three lists and `other`, with an older exclude list moved to off. */
function lists(d: DigitalSource) {
  return {
    cover: d.exclude ? [] : d.apps,
    progress: d.progress ?? [],
    off: d.exclude ? [...(d.off ?? []), ...d.apps] : (d.off ?? []),
    other: otherShow(d),
  };
}

function save(d: DigitalSource, l: ReturnType<typeof lists>): DigitalSource {
  return { ...d, apps: l.cover, progress: l.progress, off: l.off, other: l.other, exclude: false };
}

/** Pick what one app shows; it stays picked even if every other app changes later. */
export function setAppShow(d: DigitalSource, app: string, show: AppShow): DigitalSource {
  const l = lists(d);
  const without = (list: string[]) => list.filter((a) => !same(a, app));
  const next = { cover: without(l.cover), progress: without(l.progress), off: without(l.off), other: l.other };
  next[show] = [...next[show], app];
  return save(d, next);
}

/** Forget a pick: the app follows every other app again. */
export function clearApp(d: DigitalSource, app: string): DigitalSource {
  const l = lists(d);
  const without = (list: string[]) => list.filter((a) => !same(a, app));
  return save(d, { cover: without(l.cover), progress: without(l.progress), off: without(l.off), other: l.other });
}

/** The apps picked on this player, and its inputs that are analog sources. */
export function picks(d: DigitalSource, analog: AnalogSource[] = []): string[] {
  const out: string[] = [];
  const all = [...analog.filter((a) => isInput(a, d.entity, a.value)).map((a) => a.value), ...d.apps, ...(d.progress ?? []), ...(d.off ?? [])];
  for (const a of all) if (a && !out.some((o) => same(o, a))) out.push(a);
  return out.sort((x, y) => x.localeCompare(y, undefined, { sensitivity: 'base' }));
}

/** An analog source set on a listed player's row, not a source of its own. */
export function onPlayer(a: AnalogSource, digital: DigitalSource[]): boolean {
  return digital.some((d) => isInput(a, d.entity, a.value));
}

export function setOtherShow(d: DigitalSource, show: AppShow): DigitalSource {
  return save(d, { ...lists(d), other: show });
}

/** A row's choice: what the app shows, or analog, where the display listens instead. */
export type RowShow = AppShow | 'analog';

// An analog source that is one of this player's inputs or apps. The device compares the value exactly.
function isInput(a: AnalogSource, entity: string, app: string): boolean {
  return a.entity === entity && (a.attribute === 'source' || a.attribute === 'app_name') && a.value === app;
}

/** This input is listened to: an analog source that's on wins over whatever the app shows. */
export function isAnalog(analog: AnalogSource[], entity: string, app: string): boolean {
  return analog.some((a) => a.enabled && isInput(a, entity, app));
}

/** The player's inputs that are listened to. */
function analogInputs(analog: AnalogSource[], entity: string): string[] {
  return analog.filter((a) => a.enabled && isInput(a, entity, a.value)).map((a) => a.value);
}

/** Make an input an analog source (one that's off is turned on), or stop it being one. */
export function setAnalog(analog: AnalogSource[], entity: string, app: string, on: boolean, attribute = 'source'): AnalogSource[] {
  if (!on) return analog.filter((a) => !isInput(a, entity, app));
  if (analog.some((a) => isInput(a, entity, app))) return analog.map((a) => (isInput(a, entity, app) ? { ...a, enabled: true } : a));
  return [...analog, { name: app, entity, attribute, value: app, enabled: true }];
}

/** A short line for the player's row: "All apps", "Analog: Phono · Progress: Netflix +2 · others off". */
export function showSummary(d: DigitalSource, analog: AnalogSource[] = []): string {
  const l = lists(d);
  const heard = analogInputs(analog, d.entity);
  const shown = (k: AppShow) => l[k].filter((a) => !heard.includes(a));
  const picked = (['cover', 'progress', 'off'] as const).filter((k) => shown(k).length && k !== l.other);
  if (!picked.length && !heard.length) return l.other === 'cover' ? 'All apps' : l.other === 'progress' ? 'Progress for all' : 'No apps';
  const names = (list: string[]) => (list.length <= 2 ? list.join(', ') : `${list.slice(0, 2).join(', ')} +${list.length - 2}`);
  const parts = picked.map((k) => `${k === 'cover' ? 'Cover' : k === 'progress' ? 'Progress' : 'Off'}: ${names(shown(k))}`);
  if (heard.length) parts.unshift(`Analog: ${names(heard)}`);
  return [...parts, `others ${l.other}`].join(' · ');
}

/** Showing anything but the cover for every app. */
export function limited(d: DigitalSource, analog: AnalogSource[] = []): boolean {
  const l = lists(d);
  return l.other !== 'cover' || l.progress.length > 0 || l.off.length > 0 || analogInputs(analog, d.entity).length > 0;
}
