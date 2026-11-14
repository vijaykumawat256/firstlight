import { assertHttps, type Backend } from "./backend";
import type { CalendarInfo, CalEvent } from "./types";

/**
 * Browser-only stand-in with fake data. Used by tests, Playwright screenshots and
 * `npm run dev` in a plain browser. Mirrors the Rust fixture provider; the desktop app
 * never uses this.
 */

const CALENDARS: CalendarInfo[] = [
  { id: "fixture-work", account_id: "fixture-account", remote_id: "fixture-work", name: "Work", colour: "blue", visible: true, read_only: true },
  { id: "fixture-personal", account_id: "fixture-account", remote_id: "fixture-personal", name: "Personal", colour: "green", visible: true, read_only: true },
];

function localTime(base: Date, dayOffset: number, h: number, m: number): Date {
  const d = new Date(base);
  d.setDate(d.getDate() + dayOffset);
  d.setHours(h, m, 0, 0);
  return d;
}

function isoDate(d: Date): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

export function fixtureEvents(base: Date = new Date()): CalEvent[] {
  const ev = (
    key: string,
    cal: string,
    title: string,
    day: number,
    from: [number, number],
    to: [number, number],
    extra: Partial<CalEvent> = {},
  ): CalEvent => ({
    id: `${cal}:${key}`,
    calendar_id: cal,
    remote_id: key,
    ical_uid: `${key}@fixture`,
    title,
    when: {
      kind: "timed",
      start: localTime(base, day, ...from).toISOString(),
      end: localTime(base, day, ...to).toISOString(),
      tzid: Intl.DateTimeFormat().resolvedOptions().timeZone,
    },
    location: null,
    description: null,
    join_url: null,
    status: "confirmed",
    my_response: "accepted",
    organizer: null,
    recurrence_master_id: null,
    updated_at: "2023-11-14T22:13:20Z",
    ...extra,
  });

  const tomorrow = new Date(base);
  tomorrow.setDate(tomorrow.getDate() + 1);

  return [
    ev("standup", "fixture-work", "Team standup", 0, [9, 0], [9, 15], {
      recurrence_master_id: "standup-series",
      join_url: "https://meet.example.com/standup",
    }),
    ev("design-review", "fixture-work", "Design review", 0, [10, 30], [11, 30], {
      location: "Room 4B",
      join_url: "https://zoom.example.com/j/123456789",
      organizer: "Priya",
    }),
    ev("one-on-one", "fixture-work", "1:1 with Sam", 0, [11, 0], [11, 30], { my_response: "needs_action" }),
    ev("lunch", "fixture-personal", "Lunch with Alex", 0, [12, 30], [13, 30], { location: "Café Nero" }),
    ev("focus", "fixture-work", "Focus time", 0, [14, 0], [16, 0], { status: "tentative", my_response: "tentative" }),
    ev("planning", "fixture-work", "Roadmap planning", 0, [8, 0], [9, 0], {
      join_url: "https://teams.example.com/l/meetup-join/abc",
      organizer: "Jordan (PST)",
    }),
    ev("dentist", "fixture-personal", "Dentist", 0, [17, 30], [18, 15]),
    ev("cancelled", "fixture-work", "Vendor demo (cancelled)", 0, [15, 0], [15, 30], { status: "cancelled" }),
    ev("declined", "fixture-work", "All-hands rehearsal", 0, [16, 30], [17, 0], { my_response: "declined" }),
    {
      ...ev("birthday", "fixture-personal", "Maya's birthday", 0, [0, 0], [0, 0]),
      when: { kind: "all_day", start: isoDate(base), end: isoDate(tomorrow) },
    },
    ev("flight", "fixture-personal", "Flight to Berlin", 1, [6, 45], [9, 30], { location: "Terminal 2" }),
    ev("tomorrow-sync", "fixture-work", "Partner sync", 1, [11, 0], [11, 45]),
    ev("tomorrow-dinner", "fixture-personal", "Dinner at Nopi", 1, [19, 30], [21, 0]),
  ];
}

function overlaps(e: CalEvent, from: Date, to: Date): boolean {
  if (e.when.kind === "timed") return new Date(e.when.start) < to && new Date(e.when.end) > from;
  const s = new Date(`${e.when.start}T00:00:00`);
  const en = new Date(`${e.when.end}T00:00:00`);
  return s < to && en > from;
}

export const fixtureBackend: Backend = {
  async getEvents(startIso, endIso) {
    const from = new Date(startIso);
    const to = new Date(endIso);
    return fixtureEvents().filter((e) => overlaps(e, from, to));
  },
  async getCalendars() {
    return CALENDARS;
  },
  async getAutostart() {
    return false;
  },
  async setAutostart() {},
  async onUnlock() {
    return () => {};
  },
  async openUrl(url) {
    window.open(assertHttps(url).toString(), "_blank", "noopener,noreferrer");
  },
};
