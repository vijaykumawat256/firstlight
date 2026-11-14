import type { Briefing, BriefingItem, CalEvent } from "./types";

/**
 * A browser-only stand-in for the Rust briefing engine, used by tests and `npm run dev` in a plain browser.
 * The desktop app never uses it: it calls `get_briefing`. Keep it small; the real logic and its tests live
 * in `crates/core/src/briefing`.
 */

const DAY = 86_400_000;
const startOfDay = (d: Date, offset = 0) => {
  const x = new Date(d);
  x.setHours(0, 0, 0, 0);
  x.setDate(x.getDate() + offset);
  return x;
};
const key = (d: Date) => {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
};

export interface FixtureOptions {
  showDeclined: boolean;
  minGapMinutes: number;
  workStartHour: number;
  workEndHour: number;
  earlyHour: number;
}
const DEFAULTS: FixtureOptions = { showDeclined: false, minGapMinutes: 30, workStartHour: 9, workEndHour: 18, earlyHour: 8 };

function timed(events: CalEvent[], from: Date, to: Date): BriefingItem[] {
  const out: BriefingItem[] = [];
  for (const event of events) {
    if (event.when.kind !== "timed") continue;
    const s = new Date(event.when.start);
    const e = new Date(event.when.end);
    if (s < to && e > from) out.push({ event, start: event.when.start, end: event.when.end });
  }
  return out.sort((a, b) => +new Date(a.start) - +new Date(b.start) || a.event.title.localeCompare(b.event.title));
}

const allDay = (events: CalEvent[], date: Date) =>
  events.filter((e) => e.when.kind === "all_day" && e.when.start <= key(date) && key(date) < e.when.end);

export function fixtureBriefing(all: CalEvent[], now: Date, options: FixtureOptions = DEFAULTS): Briefing {
  const events = all.filter((e) => e.status !== "cancelled" && (options.showDeclined || e.my_response !== "declined"));
  const t0 = startOfDay(now);
  const t1 = startOfDay(now, 1);
  const t2 = startOfDay(now, 2);
  const items = timed(events, t0, t1);
  const past = items.filter((i) => new Date(i.end) <= now);
  const remaining = items.filter((i) => new Date(i.end) > now);
  const first = remaining[0];
  const next = first ? { ...first, in_progress: new Date(first.start) <= now } : null;

  const conflicts: Briefing["conflicts"] = [];
  remaining.forEach((a, i) => {
    for (const b of remaining.slice(i + 1)) {
      if (new Date(b.start) >= new Date(a.end)) break;
      const overlap = (Math.min(+new Date(a.end), +new Date(b.end)) - +new Date(b.start)) / 60_000;
      conflicts.push({ a: a.event.id, b: b.event.id, overlap_minutes: Math.round(overlap) });
    }
  });

  const work = (h: number) => {
    const d = new Date(t0);
    d.setHours(h);
    return d;
  };
  const free_gaps: Briefing["free_gaps"] = [];
  let cursor = new Date(Math.max(+now, +work(options.workStartHour)));
  for (const i of remaining) {
    const end = new Date(Math.min(+new Date(i.start), +work(options.workEndHour)));
    const minutes = Math.round((+end - +cursor) / 60_000);
    if (minutes >= options.minGapMinutes) free_gaps.push({ start: cursor.toISOString(), end: end.toISOString(), minutes });
    cursor = new Date(Math.max(+cursor, +new Date(i.end)));
  }

  const tomorrowItems = timed(events, t1, t2);
  const needs = [...remaining, ...tomorrowItems].map((i) => i.event).filter((e) => e.my_response === "needs_action");
  const today = allDay(events, t0);
  const state: Briefing["state"] =
    items.length === 0 ? (today.length ? "all_day_only" : "clear") : !next ? "done" : next.in_progress ? "in_progress" : "upcoming";

  return {
    generated_at: now.toISOString(),
    date: key(now),
    state,
    all_day_today: today,
    past,
    remaining,
    next,
    conflicts,
    back_to_back_runs: [],
    free_gaps,
    needs_response: needs,
    tomorrow: {
      timed: tomorrowItems,
      all_day: allDay(events, new Date(+t0 + DAY * 1.5)),
      early_start: tomorrowItems.some((i) => new Date(i.start).getHours() < options.earlyHour),
    },
  };
}
