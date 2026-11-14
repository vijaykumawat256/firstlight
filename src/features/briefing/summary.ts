import type { Briefing } from "@/api";
import { formatCountdown, formatTime } from "./format";

/** One or two plain sentences about the rest of today, from the engine's structured result. */
export function summarise(b: Briefing, now: Date): string {
  switch (b.state) {
    case "clear":
      return "Your day is clear.";
    case "all_day_only":
      return "Nothing scheduled, just all-day items.";
    case "done":
      return "You're done for today.";
    default:
  }
  const next = b.next!;
  const n = b.remaining.length;
  const left = `${n} ${n === 1 ? "event" : "events"} left today.`;
  const extra = b.conflicts.length ? ` ${b.conflicts.length} ${b.conflicts.length === 1 ? "overlap" : "overlaps"} to sort out.` : "";
  if (next.in_progress) return `${left} ${next.event.title} is on now, until ${formatTime(new Date(next.end))}.${extra}`;
  const ms = new Date(next.start).getTime() - now.getTime();
  const when = ms < 60 * 60_000 ? formatCountdown(ms) : `at ${formatTime(new Date(next.start))}`;
  return `${left} Next: ${next.event.title} ${when}.${extra}`;
}

/** "9:00 AM – 10:00 AM" style range. */
export const timeRange = (startIso: string, endIso: string): string => `${formatTime(new Date(startIso))} – ${formatTime(new Date(endIso))}`;
