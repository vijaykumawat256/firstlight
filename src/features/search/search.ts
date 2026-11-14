import type { CalEvent } from "@/api";
import { startOfLocalDay } from "@/features/calendar/layout";
import { isShown } from "@/features/calendar/TimeGrid";

/** Lower-case and strip accents, so "cafe" finds "Café". */
export function normalise(s: string): string {
  return s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();
}

const startOf = (e: CalEvent): Date => (e.when.kind === "all_day" ? new Date(`${e.when.start}T00:00:00`) : new Date(e.when.start));
const endOf = (e: CalEvent): Date => (e.when.kind === "all_day" ? new Date(`${e.when.end}T00:00:00`) : new Date(e.when.end));

/**
 * Events whose title or place contains every word typed. Cancelled and declined events are left out, as in
 * the calendar views. Upcoming and running events come first, soonest first; then past ones, most recent
 * first. With nothing typed it lists what is coming up next.
 */
export function searchEvents(events: CalEvent[], query: string, now: Date, limit = 30): CalEvent[] {
  const terms = normalise(query).split(/\s+/).filter(Boolean);
  const seen = new Set<string>();
  const matches = events.filter((e) => {
    if (!isShown(e) || seen.has(e.id)) return false;
    seen.add(e.id);
    if (terms.length === 0) return true;
    const hay = normalise(`${e.title} ${e.location ?? ""}`);
    return terms.every((t) => hay.includes(t));
  });
  const upcoming = matches.filter((e) => endOf(e) >= now).sort((a, b) => startOf(a).getTime() - startOf(b).getTime());
  if (terms.length === 0) return upcoming.slice(0, Math.min(limit, 8));
  const past = matches.filter((e) => endOf(e) < now).sort((a, b) => startOf(b).getTime() - startOf(a).getTime());
  return [...upcoming, ...past].slice(0, limit);
}

const MONTHS = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
const WEEKDAYS = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

const month = (word: string): number => MONTHS.findIndex((m) => word.length >= 3 && m === word.slice(0, 3) && "january february march april may june july august september october november december".split(" ")[MONTHS.indexOf(m)]!.startsWith(word));

/** A real calendar date or null (31 February is not a date). */
function real(y: number, m: number, d: number): Date | null {
  const date = new Date(y, m, d);
  return date.getFullYear() === y && date.getMonth() === m && date.getDate() === d ? date : null;
}

/**
 * Understands what people type to jump to a day: "today", "tomorrow", "yesterday", a weekday ("friday",
 * always the next one), "2026-12-24", "dec 24", "24 dec 2027". A date with no year is its next occurrence.
 * Returns the start of that local day, or null if the text is not a date.
 */
export function parseJump(query: string, now: Date): Date | null {
  const q = query.trim().toLowerCase().replace(/[,.]/g, " ").replace(/\s+/g, " ");
  if (!q) return null;
  const today = startOfLocalDay(now);
  if (q === "today") return today;
  if (q === "tomorrow") return startOfLocalDay(today, 1);
  if (q === "yesterday") return startOfLocalDay(today, -1);

  const wd = WEEKDAYS.findIndex((w) => q.length >= 3 && ["sunday", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday"][WEEKDAYS.indexOf(w)]!.startsWith(q));
  if (wd >= 0) return startOfLocalDay(today, ((wd - today.getDay() + 6) % 7) + 1);

  const iso = /^(\d{4})-(\d{1,2})-(\d{1,2})$/.exec(q);
  if (iso) return real(Number(iso[1]), Number(iso[2]) - 1, Number(iso[3]));

  const named = /^(?:([a-z]{3,9}) (\d{1,2})|(\d{1,2}) ([a-z]{3,9}))(?: (\d{4}))?$/.exec(q);
  if (named) {
    const m = month(named[1] ?? named[4] ?? "");
    const d = Number(named[2] ?? named[3]);
    if (m < 0) return null;
    if (named[5]) return real(Number(named[5]), m, d);
    const thisYear = real(today.getFullYear(), m, d);
    if (thisYear && thisYear >= today) return thisYear;
    return real(today.getFullYear() + 1, m, d);
  }
  return null;
}
