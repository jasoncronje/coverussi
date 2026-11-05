// What each app on a digital source shows, read and changed the way the device
// reads it (Settings::app_show): film, off, then the art list, else `other`.
import type { AppShow, DigitalSource } from './types';

const same = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();
const has = (list: string[] | undefined, app: string) => !!list?.some((a) => same(a, app));

/** Apps on no list; older settings: art when the list is empty or excludes, else nothing. */
export function otherShow(d: DigitalSource): AppShow {
  return d.other ?? (!d.apps.length || d.exclude ? 'art' : 'off');
}

export function appShow(d: DigitalSource, app: string): AppShow {
  if (has(d.film, app)) return 'film';
  if (has(d.off, app)) return 'off';
  if (has(d.apps, app)) return d.exclude ? 'off' : 'art';
  return otherShow(d);
}

/** The three lists and `other`, with an older exclude list moved to off. */
function lists(d: DigitalSource) {
  return {
    art: d.exclude ? [] : d.apps,
    film: d.film ?? [],
    off: d.exclude ? [...(d.off ?? []), ...d.apps] : (d.off ?? []),
    other: otherShow(d),
  };
}

function save(d: DigitalSource, l: ReturnType<typeof lists>): DigitalSource {
  return { ...d, apps: l.art, film: l.film, off: l.off, other: l.other, exclude: false };
}

/** Pick what one app shows; it stays picked even if every other app changes later. */
export function setAppShow(d: DigitalSource, app: string, show: AppShow): DigitalSource {
  const l = lists(d);
  const without = (list: string[]) => list.filter((a) => !same(a, app));
  const next = { art: without(l.art), film: without(l.film), off: without(l.off), other: l.other };
  next[show] = [...next[show], app];
  return save(d, next);
}

export function setOtherShow(d: DigitalSource, show: AppShow): DigitalSource {
  return save(d, { ...lists(d), other: show });
}

/** A short line for the player's row: "All apps", "Art for 2 · film for 5 · others off". */
export function showSummary(d: DigitalSource): string {
  const l = lists(d);
  const picked = (['art', 'film', 'off'] as const).filter((k) => l[k].length && k !== l.other);
  if (!picked.length) return l.other === 'art' ? 'All apps' : l.other === 'film' ? 'Film for all' : 'No apps';
  const names = (list: string[]) => (list.length <= 2 ? list.join(', ') : `${list.slice(0, 2).join(', ')} +${list.length - 2}`);
  const parts = picked.map((k) => `${k === 'art' ? 'Art' : k === 'film' ? 'Film' : 'Off'}: ${names(l[k])}`);
  return [...parts, `others ${l.other}`].join(' · ');
}

/** Showing anything but art for every app. */
export function limited(d: DigitalSource): boolean {
  const l = lists(d);
  return l.other !== 'art' || l.film.length > 0 || l.off.length > 0;
}
