import type { TimeFormat } from "@/api";

/** The user's clock preference. Set from Settings; "auto" follows the system. */
let timePref: TimeFormat = "auto";
const cache = new Map<string, Intl.DateTimeFormat>();

export function setTimeFormat(pref: TimeFormat) {
  timePref = pref;
  cache.clear();
}

function fmt(kind: "time" | "hour" | "date"): Intl.DateTimeFormat {
  const key = `${kind}:${timePref}`;
  let f = cache.get(key);
  if (!f) {
    const clock = timePref === "h12" ? { hour12: true } : timePref === "h24" ? { hourCycle: "h23" as const } : {};
    f =
      kind === "date"
        ? new Intl.DateTimeFormat(undefined, { weekday: "long", day: "numeric", month: "long" })
        : new Intl.DateTimeFormat(undefined, { hour: "numeric", ...(kind === "time" ? { minute: "2-digit" as const } : {}), ...clock });
    cache.set(key, f);
  }
  return f;
}

export const formatTime = (d: Date) => fmt("time").format(d);
/** Hour-only label: "9 AM", or "09" on a 24-hour clock. */
export const formatHour = (h: number) => fmt("hour").format(new Date(2000, 0, 1, h));
export const formatDate = (d: Date) => fmt("date").format(d);

export function formatDuration(minutes: number): string {
  const m = Math.max(0, Math.round(minutes));
  if (m < 60) return `${m} min`;
  const h = Math.floor(m / 60);
  const rest = m % 60;
  return rest === 0 ? `${h} h` : `${h} h ${rest} min`;
}

/** "in 25 min", "in 1 h 5 min", or "now". */
export function formatCountdown(ms: number): string {
  if (ms <= 0) return "now";
  const mins = Math.ceil(ms / 60_000);
  return `in ${formatDuration(mins)}`;
}

export function greeting(now: Date): string {
  const h = now.getHours();
  if (h < 5) return "Good evening";
  if (h < 12) return "Good morning";
  if (h < 18) return "Good afternoon";
  return "Good evening";
}
