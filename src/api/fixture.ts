import { assertHttps, type Backend } from "./backend";
import { fixtureBriefing } from "./fixtureBriefing";
import type { AccountView, CalendarInfo, CalEvent, SyncSummary } from "./types";

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
    also_in: [],
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

const DEMO_ID = "fixture-account";

function demoAccount(): AccountView {
  return {
    id: DEMO_ID,
    provider: "fixture",
    display_name: "Demo account",
    email: null,
    status: "ok",
    calendars: CALENDARS.map((c) => ({ ...c })),
    last_synced_at: new Date().toISOString(),
    last_error: null,
  };
}

/**
 * Mutable in-memory state. Starts with the demo account unless the page is opened with
 * `?fresh` (to exercise onboarding) or `?attention` (an account needing reconnection).
 */
const params = typeof location === "undefined" ? new URLSearchParams() : new URLSearchParams(location.search);
let accounts: AccountView[] = params.has("fresh") ? [] : [demoAccount()];
if (params.has("attention")) {
  accounts.push({
    id: "google-old",
    provider: "google",
    display_name: "me@gmail.com",
    email: "me@gmail.com",
    status: "needs_attention",
    calendars: [{ id: "google-old:primary", account_id: "google-old", remote_id: "primary", name: "me@gmail.com", colour: "teal", visible: true, read_only: false }],
    last_synced_at: new Date(Date.now() - 5 * 3_600_000).toISOString(),
    last_error: "auth",
  });
  accounts.push({
    id: "ics-old",
    provider: "ics",
    display_name: "Team holidays",
    email: null,
    status: "needs_attention",
    calendars: [{ id: "ics-old:ics", account_id: "ics-old", remote_id: "ics", name: "Team holidays", colour: "orange", visible: true, read_only: true }],
    last_synced_at: new Date(Date.now() - 3 * 3_600_000).toISOString(),
    last_error: "auth",
  });
}
let oauthCancelled = false;
const listeners = new Set<(s: SyncSummary) => void>();
const notify = () => listeners.forEach((l) => l({ ok: true, offline: false, needs_attention: false }));

const visibleCalendarIds = () =>
  new Set(accounts.flatMap((a) => a.calendars.filter((c) => c.visible).map((c) => c.id)));

export const fixtureBackend: Backend = {
  async getEvents(startIso, endIso) {
    const from = new Date(startIso);
    const to = new Date(endIso);
    const ids = visibleCalendarIds();
    return fixtureEvents().filter((e) => ids.has(e.calendar_id) && overlaps(e, from, to));
  },
  async getCalendars() {
    return accounts.flatMap((a) => a.calendars);
  },
  async getBriefing() {
    const ids = visibleCalendarIds();
    return fixtureBriefing(fixtureEvents().filter((e) => ids.has(e.calendar_id)), new Date());
  },
  async dismissUpNext() {},
  async openBriefing() {},
  async onUpNextShown() {
    return () => {};
  },
  async listAccounts() {
    return structuredClone(accounts);
  },
  async addIcsAccount(name, url) {
    if (!/^(https|webcal):\/\/\S{4,}$/i.test(url.trim())) throw new Error("Calendar address must be an https:// or webcal:// link");
    if (url.includes("fail")) throw new Error("Couldn't read a calendar from that link. Check the address and try again.");
    const id = `ics-${accounts.length + 1}-${Math.floor(Math.random() * 1e6)}`;
    const view: AccountView = {
      id,
      provider: "ics",
      display_name: name.trim() || "Subscribed calendar",
      email: null,
      status: "ok",
      calendars: [{ id: `${id}:ics`, account_id: id, remote_id: "ics", name: name.trim() || "Subscribed calendar", colour: "orange", visible: true, read_only: true }],
      last_synced_at: new Date().toISOString(),
      last_error: null,
    };
    accounts = [...accounts, view];
    notify();
    return structuredClone(view);
  },
  async addCaldavAccount(name, server, username, password) {
    if (!/^https:\/\/\S{4,}$/i.test(server.trim())) throw new Error("server address must start with https://");
    if (!username.trim()) throw new Error("enter your username or email address");
    if (!password) throw new Error("enter the app password");
    if (password === "wrong") throw new Error("The server rejected that username or password. Use an app-specific password, not your normal one.");
    const id = `dav-${accounts.length + 1}-${Math.floor(Math.random() * 1e6)}`;
    const view: AccountView = {
      id,
      provider: "caldav",
      display_name: name.trim() || "Calendar",
      email: username.includes("@") ? username.trim() : null,
      status: "ok",
      calendars: [
        { id: `${id}:a`, account_id: id, remote_id: "a", name: "Home", colour: "pink", visible: true, read_only: true },
        { id: `${id}:b`, account_id: id, remote_id: "b", name: "Family", colour: "teal", visible: true, read_only: true },
      ],
      last_synced_at: new Date().toISOString(),
      last_error: null,
    };
    accounts = [...accounts, view];
    notify();
    return structuredClone(view);
  },
  async oauthAvailable() {
    return { google: true, microsoft: !params.has("nomicrosoft") };
  },
  async addOauthAccount(provider) {
    // Simulates the browser round trip so the waiting/cancel states can be exercised.
    oauthCancelled = false;
    await new Promise((r) => setTimeout(r, params.has("slowoauth") ? 4000 : 500));
    if (oauthCancelled) throw new Error("Sign-in was cancelled.");
    if (params.has("oauthfail")) throw new Error("Couldn't complete sign-in. Please try again.");
    const email = provider === "google" ? "me@gmail.com" : "me@outlook.com";
    const existing = accounts.find((a) => a.provider === provider && a.email === email);
    if (existing) {
      accounts = accounts.map((a) => (a.id === existing.id ? { ...a, status: "ok", last_error: null } : a));
      notify();
      return structuredClone(accounts.find((a) => a.id === existing.id)!);
    }
    const id = `${provider}-${Math.floor(Math.random() * 1e6)}`;
    const view: AccountView = {
      id,
      provider,
      display_name: email,
      email,
      status: "ok",
      calendars: [{ id: `${id}:primary`, account_id: id, remote_id: "primary", name: email, colour: provider === "google" ? "teal" : "blue", visible: true, read_only: false }],
      last_synced_at: new Date().toISOString(),
      last_error: null,
    };
    accounts = [...accounts, view];
    notify();
    return structuredClone(view);
  },
  async cancelOauth() {
    oauthCancelled = true;
  },
  async systemCalendarStatus() {
    return { supported: !params.has("nosystem"), access: params.has("denied") ? "denied" : "not_determined" } as const;
  },
  async addSystemAccount() {
    if (params.has("denied")) throw new Error("Calendar access is turned off for Daybrief. Open System Settings, switch Daybrief on under Calendars, then try again.");
    await new Promise((r) => setTimeout(r, 400));
    const view: AccountView = {
      id: "system-account",
      provider: "system",
      display_name: "Calendar app",
      email: null,
      status: "ok",
      calendars: [
        { id: "system-account:a", account_id: "system-account", remote_id: "a", name: "Home (iCloud)", colour: "green", visible: true, read_only: false },
        { id: "system-account:b", account_id: "system-account", remote_id: "b", name: "Work (Gmail)", colour: "blue", visible: true, read_only: false },
        { id: "system-account:c", account_id: "system-account", remote_id: "c", name: "Birthdays (Other)", colour: "pink", visible: false, read_only: true },
      ],
      last_synced_at: new Date().toISOString(),
      last_error: null,
    };
    accounts = [...accounts.filter((a) => a.id !== view.id), view];
    notify();
    return structuredClone(view);
  },
  async openPrivacySettings() {},
  async addDemoAccount() {
    accounts = [...accounts.filter((a) => a.id !== DEMO_ID), demoAccount()];
    notify();
    return demoAccount();
  },
  async removeAccount(id) {
    accounts = accounts.filter((a) => a.id !== id);
    notify();
  },
  async renameAccount(id, name) {
    if (!name.trim() || name.trim().length > 80) throw new Error("name must be 1-80 characters");
    accounts = accounts.map((a) => (a.id === id ? { ...a, display_name: name.trim() } : a));
  },
  async setCalendarVisible(calendarId, visible) {
    accounts = accounts.map((a) => ({ ...a, calendars: a.calendars.map((c) => (c.id === calendarId ? { ...c, visible } : c)) }));
  },
  async setCalendarColour(calendarId, colour) {
    accounts = accounts.map((a) => ({ ...a, calendars: a.calendars.map((c) => (c.id === calendarId ? { ...c, colour } : c)) }));
  },
  async refreshNow() {
    setTimeout(notify, 600);
  },
  async onSyncFinished(cb) {
    listeners.add(cb);
    return () => listeners.delete(cb);
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
