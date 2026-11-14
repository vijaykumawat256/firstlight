/**
 * PROVISIONAL presentation logic for the Phase 1 briefing screen.
 * Phase 3 replaces this with the pure Rust briefing engine (PLAN.md, section 9); keep it small.
 */
import type { CalEvent } from "@/api";
import { formatCountdown, formatDuration, formatTime } from "./format";

export interface TimedItem {
  event: CalEvent;
  start: Date;
  end: Date;
}

export interface Conflict {
  a: TimedItem;
  b: TimedItem;
}

export interface BriefingModel {
  allDayToday: CalEvent[];
  past: TimedItem[];
  /** Not yet finished (includes the one in progress), in start order. */
  remaining: TimedItem[];
  next: TimedItem | null;
  nextInProgress: boolean;
  conflicts: Conflict[];
  needsResponse: CalEvent[];
  tomorrow: { timed: TimedItem[]; allDay: CalEvent[]; earlyStart: boolean };
  summary: string;
}

export const startOfDay = (d: Date, offsetDays = 0): Date => {
  const x = new Date(d);
  x.setHours(0, 0, 0, 0);
  x.setDate(x.getDate() + offsetDays);
  return x;
};

const localDateKey = (d: Date): string => {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
};

/** Cancelled and declined events are never shown by default. */
export const isShown = (e: CalEvent): boolean => e.status !== "cancelled" && e.my_response !== "declined";

function timed(events: CalEvent[]): TimedItem[] {
  const out: TimedItem[] = [];
  for (const event of events) {
    if (event.when.kind === "timed") out.push({ event, start: new Date(event.when.start), end: new Date(event.when.end) });
  }
  return out.sort((a, b) => a.start.getTime() - b.start.getTime() || a.event.title.localeCompare(b.event.title));
}

function dayParts(events: CalEvent[], dayStart: Date) {
  const dayEnd = startOfDay(dayStart, 1);
  const key = localDateKey(dayStart);
  const shown = events.filter(isShown);
  const allDay = shown.filter((e) => e.when.kind === "all_day" && e.when.start <= key && e.when.end > key);
  const items = timed(shown).filter((t) => t.start < dayEnd && t.end > dayStart);
  return { allDay, items };
}

export function buildBriefing(events: CalEvent[], now: Date): BriefingModel {
  const today = dayParts(events, startOfDay(now));
  const tomorrowStart = startOfDay(now, 1);
  const tomorrow = dayParts(events, tomorrowStart);

  const past = today.items.filter((t) => t.end <= now);
  const remaining = today.items.filter((t) => t.end > now);
  const next = remaining[0] ?? null;
  const nextInProgress = next !== null && next.start <= now;

  const conflicts: Conflict[] = [];
  for (let i = 0; i < remaining.length; i++) {
    for (let j = i + 1; j < remaining.length; j++) {
      const a = remaining[i]!;
      const b = remaining[j]!;
      if (b.start >= a.end) break; // sorted by start: nothing later can overlap `a`
      conflicts.push({ a, b });
    }
  }

  const needsResponse = [...remaining.map((t) => t.event), ...tomorrow.items.map((t) => t.event)].filter(
    (e) => e.my_response === "needs_action",
  );

  return {
    allDayToday: today.allDay,
    past,
    remaining,
    next,
    nextInProgress,
    conflicts,
    needsResponse,
    tomorrow: {
      timed: tomorrow.items,
      allDay: tomorrow.allDay,
      earlyStart: tomorrow.items.some((t) => t.start.getHours() < 8 && t.start >= tomorrowStart),
    },
    summary: summarise(today.items.length, today.allDay.length, remaining, next, nextInProgress, now),
  };
}

function summarise(
  total: number,
  allDay: number,
  remaining: TimedItem[],
  next: TimedItem | null,
  inProgress: boolean,
  now: Date,
): string {
  if (total === 0) return allDay > 0 ? "Nothing scheduled, just all-day items." : "Your day is clear.";
  if (!next) return "You're done for today.";
  const left = `${remaining.length} ${remaining.length === 1 ? "event" : "events"} left today.`;
  if (inProgress) return `${left} ${next.event.title} is on now, until ${formatTime(next.end)}.`;
  const ms = next.start.getTime() - now.getTime();
  const when = ms < 60 * 60_000 ? formatCountdown(ms) : `at ${formatTime(next.start)}`;
  return `${left} Next: ${next.event.title} ${when}.`;
}

export const durationLabel = (t: TimedItem): string => formatDuration((t.end.getTime() - t.start.getTime()) / 60_000);
