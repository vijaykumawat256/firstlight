/** Pure layout helpers for the day and week views. */

export interface Block {
  id: string;
  start: Date;
  end: Date;
}

export interface Placed {
  id: string;
  /** Minutes from the top of the day grid. */
  top: number;
  /** Minutes tall (never less than `minMinutes`, so short events stay clickable). */
  height: number;
  /** Which of `cols` side-by-side columns this block sits in. */
  col: number;
  cols: number;
}

export const startOfLocalDay = (d: Date, offsetDays = 0): Date => new Date(d.getFullYear(), d.getMonth(), d.getDate() + offsetDays);

/**
 * Minutes since the local midnight of `day`, clamped to 0..1440. Uses the wall clock, not elapsed time,
 * so on a daylight-saving day an event at 10:00 sits at the "10:00" row.
 */
export function minutesOfDay(t: Date, day: Date): number {
  const next = startOfLocalDay(day, 1);
  if (t < startOfLocalDay(day)) return 0;
  if (t >= next) return 1440;
  return t.getHours() * 60 + t.getMinutes();
}

/**
 * Places a day's events into side-by-side columns. Events that overlap (directly or through a chain)
 * share a cluster and split its width; events that don't overlap anything use the full width.
 */
export function layoutColumn(blocks: Block[], day: Date, minMinutes = 20): Placed[] {
  const items = blocks
    .map((b) => ({ id: b.id, s: minutesOfDay(b.start, day), e: minutesOfDay(b.end, day), raw: b }))
    .filter((b) => b.e > b.s || (b.raw.start.getTime() === b.raw.end.getTime() && b.raw.start >= startOfLocalDay(day) && b.raw.start < startOfLocalDay(day, 1)))
    .sort((a, b) => a.s - b.s || b.e - a.e || a.id.localeCompare(b.id));

  const placed: Placed[] = [];
  let cluster: typeof items = [];
  let clusterEnd = -1;

  const flush = () => {
    const colEnds: number[] = [];
    const assigned = cluster.map((it) => {
      let col = colEnds.findIndex((end) => end <= it.s);
      if (col === -1) col = colEnds.push(0) - 1;
      colEnds[col] = it.e;
      return { it, col };
    });
    for (const { it, col } of assigned) {
      placed.push({ id: it.id, top: it.s, height: Math.max(it.e - it.s, minMinutes), col, cols: colEnds.length });
    }
    cluster = [];
    clusterEnd = -1;
  };

  for (const it of items) {
    if (cluster.length && it.s >= clusterEnd) flush();
    cluster.push(it);
    clusterEnd = Math.max(clusterEnd, it.e);
  }
  if (cluster.length) flush();
  return placed;
}

let weekStartPref: "auto" | "monday" | "sunday" | "saturday" = "auto";
export function setWeekStart(pref: typeof weekStartPref) {
  weekStartPref = pref;
}

/** The first day of the week as a JS weekday (0 = Sunday): the user's choice, else their region's. */
export function localeFirstDay(): number {
  if (weekStartPref === "monday") return 1;
  if (weekStartPref === "sunday") return 0;
  if (weekStartPref === "saturday") return 6;
  try {
    const loc = new Intl.Locale(navigator.language) as Intl.Locale & { getWeekInfo?: () => { firstDay: number }; weekInfo?: { firstDay: number } };
    const first = loc.getWeekInfo?.().firstDay ?? loc.weekInfo?.firstDay;
    if (typeof first === "number") return first % 7; // Intl: 1 = Monday ... 7 = Sunday
  } catch {
    /* fall through */
  }
  return 1;
}

/** Local midnight of the first day of the week containing `d`. */
export function startOfWeek(d: Date, firstDay: number): Date {
  const back = (d.getDay() - firstDay + 7) % 7;
  return startOfLocalDay(d, -back);
}

export const sameDay = (a: Date, b: Date): boolean => a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
