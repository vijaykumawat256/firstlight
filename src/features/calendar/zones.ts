/** Other time zones: labels, the hour in another zone, and "3pm in London" from search. */

/** Zones offered in Settings: business cities around the world. */
export const COMMON_ZONES = [
  "America/Los_Angeles", "America/Denver", "America/Chicago", "America/New_York", "America/Toronto", "America/Mexico_City", "America/Sao_Paulo",
  "Europe/London", "Europe/Dublin", "Europe/Lisbon", "Europe/Paris", "Europe/Berlin", "Europe/Madrid", "Europe/Amsterdam", "Europe/Stockholm", "Europe/Warsaw", "Europe/Istanbul", "Europe/Moscow",
  "Africa/Lagos", "Africa/Cairo", "Africa/Johannesburg",
  "Asia/Dubai", "Asia/Karachi", "Asia/Kolkata", "Asia/Dhaka", "Asia/Bangkok", "Asia/Singapore", "Asia/Hong_Kong", "Asia/Shanghai", "Asia/Tokyo", "Asia/Seoul",
  "Australia/Perth", "Australia/Sydney", "Pacific/Auckland", "UTC",
];

/** "New York" from "America/New_York". */
export function zoneLabel(zone: string): string {
  return (zone.split("/").pop() ?? zone).replace(/_/g, " ");
}

/** The wall-clock parts of `date` in `zone`. */
function partsIn(date: Date, zone: string) {
  const f = new Intl.DateTimeFormat("en-US", { timeZone: zone, hourCycle: "h23", year: "numeric", month: "numeric", day: "numeric", hour: "numeric", minute: "numeric" });
  const get = (t: string) => Number(f.formatToParts(date).find((p) => p.type === t)?.value);
  return { y: get("year"), m: get("month"), d: get("day"), h: get("hour"), min: get("minute") };
}

/** How far `zone` is ahead of UTC at `date`, in minutes. */
function offsetMinutes(date: Date, zone: string): number {
  const p = partsIn(date, zone);
  return (Date.UTC(p.y, p.m - 1, p.d, p.h, p.min) - Math.floor(date.getTime() / 60_000) * 60_000) / 60_000;
}

/** A short time in another zone: "3 PM", or "3:30 PM" when it isn't on the hour. */
export function timeIn(date: Date, zone: string): string {
  const onHour = partsIn(date, zone).min === 0;
  return new Intl.DateTimeFormat(undefined, { timeZone: zone, hour: "numeric", ...(onHour ? {} : { minute: "2-digit" }) }).format(date);
}

/** Every zone the browser knows, by its city name in lower case (falls back to the common list). */
function zonesByCity(): Map<string, string> {
  const all: string[] = (Intl as unknown as { supportedValuesOf?: (k: string) => string[] }).supportedValuesOf?.("timeZone") ?? COMMON_ZONES;
  const map = new Map<string, string>();
  for (const z of [...all, ...COMMON_ZONES]) map.set(zoneLabel(z).toLowerCase(), z);
  for (const [alias, z] of [["nyc", "America/New_York"], ["new york city", "America/New_York"], ["sf", "America/Los_Angeles"], ["san francisco", "America/Los_Angeles"], ["la", "America/Los_Angeles"], ["bangalore", "Asia/Kolkata"], ["bengaluru", "Asia/Kolkata"], ["mumbai", "Asia/Kolkata"], ["delhi", "Asia/Kolkata"], ["new delhi", "Asia/Kolkata"], ["beijing", "Asia/Shanghai"], ["munich", "Europe/Berlin"], ["boston", "America/New_York"], ["seattle", "America/Los_Angeles"], ["washington", "America/New_York"], ["utc", "UTC"], ["gmt", "Europe/London"]] as const)
    map.set(alias, z);
  return map;
}

export interface ZoneAnswer {
  zone: string;
  /** The moment meant: that time in that zone, on today's date there (tomorrow there if it has passed). */
  at: Date;
}

/** "3pm in London", "15:30 in Tokyo", "9am london time": the moment meant, to show in your own time. */
export function parseZoneQuestion(input: string, now: Date): ZoneAnswer | null {
  const m = /^\s*(\d{1,2})(?:[:.](\d{2}))?\s*(am|pm)?\s+(?:in\s+)?([a-z][a-z .'-]*?)(?:\s+time)?\s*\??\s*$/i.exec(input);
  if (!m) return null;
  let h = Number(m[1]);
  const min = Number(m[2] ?? 0);
  const ap = m[3]?.toLowerCase();
  if (min > 59 || (ap && (h < 1 || h > 12)) || h > 23) return null;
  if (ap === "pm" && h !== 12) h += 12;
  if (ap === "am" && h === 12) h = 0;
  if (!ap && !m[2] && h < 13 && !/\bin\b|\btime\b/i.test(input)) return null;
  const zone = zonesByCity().get(m[4]!.trim().toLowerCase());
  if (!zone) return null;
  const p = partsIn(now, zone);
  let at = new Date(Date.UTC(p.y, p.m - 1, p.d, h, min) - offsetMinutes(now, zone) * 60_000);
  // Correct for a different offset on that day (daylight saving), then prefer the coming one.
  at = new Date(Date.UTC(p.y, p.m - 1, p.d, h, min) - offsetMinutes(at, zone) * 60_000);
  if (at.getTime() < now.getTime() - 3_600_000) at = new Date(at.getTime() + 86_400_000);
  return { zone, at };
}
