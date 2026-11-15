import { assertHttps, type Backend } from "./backend";
import { fixtureBriefing } from "./fixtureBriefing";
import type { AccountView, AiProvider, AiStatus, AppInfo, AppSettings, CalendarInfo, CalEvent, AskReply, ConflictHelp, WeekAhead, WeekDay, EventInput, FreeSlot, MeetingPrep, MailResult, SyncSummary, UpdateStatus } from "./types";

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

/** Events added in the browser fixture: the record the user typed, like the real app keeps. */
const ownRecords: { id: string; input: EventInput; skipped?: string[]; destination?: string }[] = [];

/** The id an event gets: local ones keep the record's id; one saved into a calendar looks like a synced one. */
const eventPrefix = (r: { id: string; destination?: string }) => (r.destination ? `${r.destination}:ek-${r.id.slice("local-events:".length)}` : r.id);

/** The record behind any event id (a local one, or the synced copy of one saved into a calendar). */
function findRecord(eventId: string) {
  return ownRecords.find((r) => eventId === r.id || eventId === eventPrefix(r) || eventId.startsWith(`${eventPrefix(r)}:`));
}

/** The dated events those records produce (repeats run 120 days ahead, like the real app's window). */
function ownEvents(): CalEvent[] {
  const out: CalEvent[] = [];
  for (const rec of ownRecords) out.push(...expandOwn(rec.id, rec.input, rec.destination).filter((e) => !rec.skipped?.includes(e.remote_id.split(":")[1] ?? "")));
  return out;
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

  const all: CalEvent[] = [
    ev("standup", "fixture-work", "Team standup", 0, [9, 0], [9, 15], {
      recurrence_master_id: "standup-series",
      join_url: "https://meet.example.com/standup",
    }),
    ev("design-review", "fixture-work", "Design review", 0, [10, 30], [11, 30], {
      location: "Room 4B",
      join_url: "https://zoom.example.com/j/123456789",
      organizer: "Priya",
    }),
    ev("design-review-last-week", "fixture-work", "Design review", -7, [10, 30], [11, 30], {
      organizer: "Priya",
      description: "Agree the navigation pattern. See https://docs.example.com/nav?pw=secret for the mock-ups. Priya to share the revised flow.",
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
    ev("retro", "fixture-work", "Sprint retro", -1, [15, 0], [16, 0]),
    ev("offsite", "fixture-work", "Planning offsite", 2, [9, 30], [12, 0], { location: "Granary Square" }),
    ev("offsite-lunch", "fixture-personal", "Lunch with Dev", 2, [11, 0], [12, 30]),
    ev("gym", "fixture-personal", "Gym", 3, [7, 0], [8, 0]),
    ev("board", "fixture-work", "Board prep", 4, [14, 0], [16, 30], { join_url: "https://meet.google.com/abc-defg-hij" }),
  ];
  all.push(...ownEvents());
  // `?quietdays`: nothing today or tomorrow, to test the empty-day screen.
  if (typeof location !== "undefined" && new URLSearchParams(location.search).has("quietdays")) {
    const start = new Date(base.getFullYear(), base.getMonth(), base.getDate());
    return all.filter((e) => {
      const d = e.when.kind === "all_day" ? new Date(`${e.when.start}T00:00:00`) : new Date(e.when.start);
      return Math.round((new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime() - start.getTime()) / 86_400_000) >= 2;
    });
  }
  return all;
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
export const DEFAULT_SETTINGS: AppSettings = {
  version: 1,
  general: { sync_interval_minutes: 10, default_view: "briefing", week_start: "auto", time_format: "auto", global_shortcut: null, show_in_dock: true, write_to_calendar_app: params.has("writeon"), writable_accounts: [], join_shortcut: null, tray_next_meeting: false, check_for_updates: true, update_channel: "stable" },
  briefing: { show_declined: false, min_gap_minutes: 30, work_start_hour: 9, work_end_hour: 18, early_hour: 8 },
  sections: { tomorrow: true, free_time: true, attention: true },
  unlock: { first_unlock_after_hour: 6, min_interval_minutes: 30, quiet_hours: [22, 6], days: [true, true, true, true, true, true, true], up_next_lead_minutes: 15, skip_when_fullscreen: true },
  notify: { before_event: true, before_minutes: 10, morning: true, morning_hour: 8, evening: true, evening_hour: 18, week_preview: true, time_24h: false, hide_titles: false },
  appearance: { theme: "system", accent: "system", ambient: params.has("ambient") },
  email: { enabled: false, apple_mail: false, account_ids: [], imap: [], send_invitations: false, important_senders: [], days_back: 3 },
  ai: { enabled: false, provider: "anthropic", model: "claude-haiku-5-5", endpoint: "http://127.0.0.1:11434", custom_endpoint: "", include_email: false, consented: null, consented_host: null },
  privacy: { crash_reports: false },
};
let settings: AppSettings = structuredClone(DEFAULT_SETTINGS);
if (["week", "day", "month"].includes(params.get("defaultview") ?? "")) settings.general.default_view = params.get("defaultview") as "day" | "week" | "month";
const settingsListeners = new Set<() => void>();
const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, n));

const listeners = new Set<(s: SyncSummary) => void>();
const notify = () => listeners.forEach((l) => l({ ok: true, offline: false, needs_attention: false }));

/** The host of an https address, or null (used to tie the agreement for a custom AI service to its host). */
function customHost(address: string): string | null {
  try {
    const u = new URL(address);
    return u.protocol === "https:" ? u.hostname : null;
  } catch {
    return null;
  }
}

/** The Calendar-app calendars the fixture lets events be added to. */
const WRITABLE = [
  { id: "fixture-work", name: "Work", source: "Demo account" },
  { id: "fixture-personal", name: "Personal", source: "Demo account" },
];

/** What the "Save to" menu offers now: the Calendar app's calendars if allowed, and the changeable calendars of each CalDAV account the user allowed. */
function writableNow(): { id: string; name: string; source: string }[] {
  const out = settings.general.write_to_calendar_app ? [...WRITABLE] : [];
  for (const a of accounts) {
    if (a.provider !== "caldav" || !settings.general.writable_accounts.includes(a.id)) continue;
    for (const c of a.calendars) if (!c.read_only) out.push({ id: c.id, name: c.name, source: a.display_name });
  }
  return out;
}

const LOCAL_CAL = "local-events";
const pad2 = (n: number) => String(n).padStart(2, "0");
const ymd = (d: Date) => `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;

/** Mirrors the Rust checks (`firstlight_core::local`) closely enough for the browser tests. */
function checkOwn(input: EventInput): EventInput {
  const title = input.title.trim();
  if (!title) throw new Error("Give the event a title.");
  if (title.length > 200) throw new Error("That title is too long.");
  const link = (input.join_url ?? "").trim();
  if (link && !/^https:\/\/[^\s]+$/.test(link)) throw new Error("The meeting link must start with https:// and be a single link.");
  if (input.all_day) {
    if (input.end < input.start) throw new Error("The event can't end before it starts.");
  } else if (!(new Date(input.end) > new Date(input.start))) throw new Error("The event must end after it starts.");
  if (input.reminder_minutes !== null && input.all_day) throw new Error("Reminders need a start time.");
  if (input.reminder_minutes !== null && (input.reminder_minutes < 1 || input.reminder_minutes > 10080)) throw new Error("Choose a reminder of up to a week.");
  const invitees: string[] = [];
  for (const raw of input.invitees) {
    const a = raw.trim().toLowerCase();
    if (!a) continue;
    if (!/^[a-z0-9._%+'-]+@[a-z0-9-]+(\.[a-z0-9-]+)+$/.test(a)) throw new Error("One of those email addresses doesn't look right.");
    if (!invitees.includes(a)) invitees.push(a);
  }
  if (invitees.length > 20) throw new Error("That's too many people for one invitation.");
  if (input.repeat) {
    if (!["daily", "weekly", "monthly", "yearly"].includes(input.repeat.every)) throw new Error("Choose how often it repeats.");
    if (input.repeat.interval < 1 || input.repeat.interval > 30) throw new Error("It can repeat every 1 to 30 days, weeks, months or years.");
    const first = input.all_day ? input.start : ymd(new Date(input.start));
    if (input.repeat.until && input.repeat.until < first) throw new Error("It can't stop repeating before it starts.");
  }
  return { ...input, title, invitees };
}

function expandOwn(id: string, input: EventInput, calendar?: string): CalEvent[] {
  const key = id.slice(LOCAL_CAL.length + 1);
  const prefix = eventPrefix({ id, destination: calendar });
  const base = (when: CalEvent["when"], suffix: string | null): CalEvent => ({
    id: suffix ? `${prefix}:${suffix}` : prefix,
    calendar_id: calendar ?? LOCAL_CAL,
    remote_id: suffix ? `${key}:${suffix}` : key,
    ical_uid: `${key}@firstlight.local`,
    title: input.title,
    when,
    location: input.location?.trim() || null,
    description: input.notes?.trim() || null,
    join_url: input.join_url?.trim() || null,
    status: "confirmed",
    my_response: "none",
    organizer: null,
    recurrence_master_id: suffix ? key : null,
    updated_at: new Date().toISOString(),
    also_in: [],
  });
  const lastDay = (() => {
    const end = new Date(`${input.end}T00:00:00`);
    end.setDate(end.getDate() + 1);
    return ymd(end);
  })();
  const first = input.all_day ? new Date(`${input.start}T00:00:00`) : new Date(input.start);
  const length = input.all_day ? 0 : new Date(input.end).getTime() - new Date(input.start).getTime();
  const spanDays = input.all_day ? Math.round((new Date(`${input.end}T00:00:00`).getTime() - first.getTime()) / 86_400_000) : 0;
  const make = (at: Date, suffix: string | null): CalEvent => {
    if (!input.all_day) return base({ kind: "timed", start: at.toISOString(), end: new Date(at.getTime() + length).toISOString(), tzid: null }, suffix);
    const e = new Date(at);
    e.setDate(e.getDate() + spanDays + 1);
    return base({ kind: "all_day", start: ymd(at), end: ymd(e) }, suffix);
  };
  void lastDay;
  if (!input.repeat) return [make(first, null)];
  const out: CalEvent[] = [];
  const horizon = new Date(Date.now() + 120 * 86_400_000);
  const until = input.repeat.until ? new Date(`${input.repeat.until}T23:59:59`) : null;
  for (let n = 0; n < 800; n++) {
    const at = new Date(first);
    const k = n * input.repeat.interval;
    if (input.repeat.every === "daily") at.setDate(at.getDate() + k);
    else if (input.repeat.every === "weekly") at.setDate(at.getDate() + 7 * k);
    else if (input.repeat.every === "monthly") at.setMonth(at.getMonth() + k);
    else at.setFullYear(at.getFullYear() + k);
    if (at > horizon || (until && at > until)) break;
    out.push(make(at, input.all_day ? ymd(at).replaceAll("-", "") : String(Math.floor(at.getTime() / 1000))));
  }
  return out;
}

function saveFixtureEvent(id: string | null, input: EventInput, destination?: string): CalEvent {
  const clean = checkOwn(input);
  const existing = id ? findRecord(id) : undefined;
  if (id && !existing) throw new Error("That event can't be changed here.");
  const recordId = existing?.id ?? `${LOCAL_CAL}:own${ownRecords.length + 1}-${Date.now()}`;
  const where = existing ? existing.destination : destination; // an event stays where it was saved
  const at = ownRecords.findIndex((r) => r.id === recordId);
  if (at >= 0) ownRecords[at] = { id: recordId, input: clean, skipped: ownRecords[at]!.skipped, destination: where };
  else ownRecords.push({ id: recordId, input: clean, destination: where });
  if (!where && !accounts.some((a) => a.id === "local-account")) {
    accounts.push({
      id: "local-account",
      provider: "local",
      display_name: "On this computer",
      email: null,
      status: "ok",
      calendars: [{ id: LOCAL_CAL, account_id: "local-account", remote_id: "events", name: "My events", colour: "purple", visible: true, read_only: false }],
      last_synced_at: null,
      last_error: null,
    });
  }
  return expandOwn(recordId, clean, where)[0]!;
}

const visibleCalendarIds = () =>
  new Set(accounts.flatMap((a) => a.calendars.filter((c) => c.visible).map((c) => c.id)));

const FIXTURE_MAIL = () => {
  const ago = (mins: number) => new Date(Date.now() - mins * 60_000).toISOString();
  const msg = (id: string, name: string, subject: string, snippet: string, mins: number) => ({
    id,
    sender_name: name,
    sender_address: `${name.split(" ")[0]!.toLowerCase()}@example.com`,
    subject,
    received: ago(mins),
    snippet,
    open: { kind: "apple_mail" as const, message_id: `${id}@example.com` },
  });
  return {
    generated_at: new Date().toISOString(),
    unread_total: 37,
    important: [
      msg("m1", "Priya Shah", "Agenda for Tuesday", "Hi all, attached is the agenda for tomorrow's planning session. Please add your items before…", 25),
      msg("m2", "Maya Singh", "Dinner on Friday?", "Are we still on for Friday? I can book the place on Nopi Street if you like", 95),
    ],
    invitations: [
      {
        message: msg("m3", "Jordan Lee", "Invitation: Roadmap review", "You have been invited", 40),
        title: "Roadmap review",
        when: { kind: "timed" as const, start: new Date(Date.now() + 26 * 3_600_000).toISOString(), end: new Date(Date.now() + 27 * 3_600_000).toISOString(), tzid: null },
        location: "Room 12",
        organizer: "Jordan Lee",
      },
    ],
    other_unread: 34,
  };
};
let aiState = { enabled: false, consented: false, provider: "anthropic" as AiProvider, keys: new Set<AiProvider>() };
if (params.has("aion")) {
  aiState = { enabled: true, consented: true, provider: "anthropic", keys: new Set<AiProvider>(["anthropic"]) };
  settings.ai = { ...settings.ai, enabled: true, consented: "anthropic" };
}
if (params.has("sendon")) {
  settings.email = { ...settings.email, send_invitations: true, imap: [{ id: "imap1", preset: "yahoo", address: "me@yahoo.com", host: null }] };
}
if (params.has("mailon")) settings.email = { ...settings.email, enabled: true, apple_mail: true };
/** A CalDAV account with one calendar the server lets the user change and one it does not. */
if (params.has("withcaldav")) {
  accounts.push({
    id: "caldav-ok",
    provider: "caldav",
    display_name: "Fastmail",
    email: "me@fastmail.example",
    status: "ok",
    calendars: [
      { id: "caldav-ok:work", account_id: "caldav-ok", remote_id: "https://dav.example/cal/work/", name: "Fastmail Work", colour: "orange", visible: true, read_only: false },
      { id: "caldav-ok:shared", account_id: "caldav-ok", remote_id: "https://dav.example/cal/shared/", name: "Team (read only)", colour: "purple", visible: true, read_only: true },
    ],
    last_synced_at: new Date().toISOString(),
    last_error: null,
  });
}
if (params.has("withgoogle")) {
  accounts.push({
    id: "google-ok",
    provider: "google",
    display_name: "me@gmail.com",
    email: "me@gmail.com",
    status: "ok",
    calendars: [{ id: "google-ok:primary", account_id: "google-ok", remote_id: "primary", name: "me@gmail.com", colour: "teal", visible: true, read_only: false }],
    last_synced_at: new Date().toISOString(),
    last_error: null,
  });
}
/** Counts calls so tests can prove that nothing mail- or AI-related runs while those features are off. */
const bump = (name: string) => {
  const g = globalThis as unknown as Record<string, number>;
  g[name] = (g[name] ?? 0) + 1;
};
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

let crashReport: string | null = params.has("crashreport")
  ? "Firstlight crash report\nTime: 2026-11-14 09:30:00 UTC\nVersion: 0.1.0\nSystem: macos aarch64\nThread: sync\nStopped at: crates/core/src/ics/mod.rs:123\n\nThis report contains no calendar, email or account information.\n"
  : null;

export const fixtureBackend: Backend = {
  async getAppInfo() {
    return { name: "Firstlight", version: "0.1.0", os: "macos", arch: "aarch64", updates_configured: params.has("updates") } satisfies AppInfo;
  },
  async checkForUpdate() {
    await sleep(300);
    if (!params.has("updates")) return { state: "not_configured" } satisfies UpdateStatus;
    if (params.has("updatefail")) return { state: "failed" } satisfies UpdateStatus;
    return params.has("newversion") ? ({ state: "available", version: "0.2.0" } satisfies UpdateStatus) : ({ state: "up_to_date" } satisfies UpdateStatus);
  },
  async installUpdate() {
    await sleep(500);
  },
  async onUpdateAvailable() {
    return () => {};
  },
  async getLastCrashReport() {
    return crashReport;
  },
  async clearCrashReports() {
    crashReport = null;
  },
  async getMailDigest() {
    bump("__mailCalls");
    if (!settings.email.enabled) return { enabled: false, digest: null, problems: [] } satisfies MailResult;
    const problem = params.get("mailproblem");
    if (problem === "permission") return { enabled: true, digest: null, problems: [{ source: "apple_mail", kind: "permission" }] };
    if (problem === "not_running") return { enabled: true, digest: null, problems: [{ source: "apple_mail", kind: "not_running" }] };
    if (params.has("emptymail")) return { enabled: true, digest: { generated_at: new Date().toISOString(), unread_total: 4, important: [], invitations: [], other_unread: 4 }, problems: [] };
    return { enabled: true, digest: FIXTURE_MAIL(), problems: [] };
  },
  async enableAppleMail() {
    await sleep(300);
    if (params.has("macdenied")) throw new Error("Firstlight isn't allowed to control Mail. Open System Settings, go to Automation, switch Mail on under Firstlight, then try again.");
    settings = { ...settings, email: { ...settings.email, apple_mail: true, enabled: true } };
    settingsListeners.forEach((l) => l());
    return structuredClone(settings);
  },
  async enableAccountMail(accountId) {
    await sleep(400);
    settings = { ...settings, email: { ...settings.email, enabled: true, account_ids: [...new Set([...settings.email.account_ids, accountId])] } };
    settingsListeners.forEach((l) => l());
    return structuredClone(settings);
  },
  async addImapMailbox(preset, address, password, host) {
    await sleep(400);
    if (!address.includes("@")) throw new Error("Enter your full email address.");
    if (!password.trim()) throw new Error("Enter the app-specific password (not your normal password).");
    const label = preset === "yahoo" ? "Yahoo Mail" : preset === "icloud" ? "iCloud Mail" : "Your mail server";
    const server = host?.trim().toLowerCase() ?? "";
    if (preset === "custom" && !/^(?=.{4,253}$)([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z][a-z0-9-]*$/.test(server)) throw new Error("Enter the mail server's name, like imap.example.com (not an address or a port).");
    if (preset === "custom" && (server === "localhost" || /\.(local|lan|home|internal|localhost|localdomain)$/.test(server))) throw new Error("Enter the mail server's name, like imap.example.com (not an address or a port).");
    if (params.has("imapbad")) throw new Error(`${label} didn't accept that. Use an app-specific password, and make sure IMAP access is allowed for the account.`);
    (window as unknown as Record<string, unknown>).__imapPasswordsSent = ((window as unknown as Record<string, number>).__imapPasswordsSent ?? 0) + 1;
    const wanted = preset === "custom" ? server : null;
    const existing = settings.email.imap.find((m) => m.preset === preset && m.host === wanted && m.address.toLowerCase() === address.trim().toLowerCase());
    const imap = existing ? settings.email.imap : [...settings.email.imap, { id: `imap-${settings.email.imap.length + 1}`, preset, address: address.trim(), host: wanted }];
    settings = { ...settings, email: { ...settings.email, enabled: true, imap } };
    settingsListeners.forEach((l) => l());
    return structuredClone(settings);
  },
  async removeImapMailbox(mailboxId) {
    settings = { ...settings, email: { ...settings.email, imap: settings.email.imap.filter((m) => m.id !== mailboxId) } };
    settingsListeners.forEach((l) => l());
    return structuredClone(settings);
  },
  async createEvent(input, calendarId) {
    if (calendarId) {
      if (!writableNow().some((c) => c.id === calendarId)) throw new Error("That calendar can't be added to.");
      if (params.has("calfail")) throw new Error("The server couldn't save that. Nothing was changed.");
    }
    return saveFixtureEvent(null, input, calendarId);
  },
  async writableCalendars() {
    return structuredClone(writableNow());
  },
  async updateEvent(id, input) {
    return saveFixtureEvent(id, input);
  },
  async deleteEvent(id) {
    const rec = findRecord(id);
    if (!rec) throw new Error("That event can't be changed here.");
    if (rec.destination && !writableNow().some((c) => c.id === rec.destination)) throw new Error("Turn on adding events to that calendar to delete this one.");
    ownRecords.splice(ownRecords.indexOf(rec), 1);
  },
  async skipOccurrence(id) {
    const rec = findRecord(id);
    const key = id.split(":")[2];
    if (rec?.destination) throw new Error("Single days can't be removed from an event in the Calendar app yet.");
    if (!rec || !key || !rec.input.repeat) throw new Error("Only one day of a repeating event can be removed.");
    rec.skipped = [...new Set([...(rec.skipped ?? []), key])];
  },
  async exportOwnEvents() {
    const local = ownRecords.filter((r) => !r.destination);
    if (local.length === 0) throw new Error("You haven't added any events of your own yet.");
    return { path: "/Users/you/Downloads/firstlight-events.ics", count: local.length };
  },
  async getSystemAccent() {
    return params.get("sysaccent");
  },
  async getOwnEvent(id) {
    const rec = findRecord(id);
    return rec ? { id: rec.id, input: structuredClone(rec.input), saved_to: rec.destination ?? null } : null;
  },
  async inviteSenders() {
    if (!settings.email.send_invitations) return [];
    return settings.email.imap.filter((m) => m.preset !== "custom").map((m) => ({ id: m.id, address: m.address, label: m.preset === "yahoo" ? "Yahoo Mail" : "iCloud Mail" }));
  },
  async invitePreview(id) {
    const ev = fixtureEvents().concat(ownEvents()).find((e) => e.id === id || findRecord(id)?.id === e.id) ?? ownEvents()[0];
    return { subject: `Invitation: ${ev?.title ?? "Event"}`, body: `${ev?.title ?? "Event"}\n\nWhen: soon\n\nThe calendar file is attached, so you can add it to your calendar.` };
  },
  async sendInviteMail(id, recipients, mailboxId) {
    await sleep(150);
    if (!settings.email.send_invitations) throw new Error("Turn on “Send invitations from Firstlight” in Settings first.");
    if (params.has("sendfail")) throw new Error("The mail server didn't accept the saved password. Add the mailbox again with a fresh app password.");
    const list = recipients.split(/[\s,;]+/).filter(Boolean);
    if (list.length === 0) throw new Error("Add at least one email address.");
    if (list.some((a) => !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(a))) throw new Error("One of those email addresses doesn't look right.");
    if (!settings.email.imap.some((m) => m.id === mailboxId)) throw new Error("That mailbox isn't connected.");
    const w = window as unknown as Record<string, unknown>;
    w.__sent = [...((w.__sent as unknown[]) ?? []), { id, list, mailboxId }];
  },
  async prepareInvite(id, recipients, openMail) {
    await sleep(150);
    const list = recipients.split(/[\s,;]+/).filter(Boolean);
    if (list.some((a) => !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(a))) throw new Error("One of those email addresses doesn't look right.");
    if (openMail && list.length === 0) throw new Error("Add at least one email address.");
    const w = window as unknown as Record<string, unknown>;
    w.__invites = [...((w.__invites as unknown[]) ?? []), { id, list, openMail }];
    return { path: "/Users/you/Downloads/invitation.ics", mail_opened: openMail };
  },
  async openMailMessage(target) {
    if (target.kind === "url" && !target.url.startsWith("https://")) throw new Error("Refusing to open that link.");
  },
  async aiStatus() {
    return { enabled: aiState.enabled, consented: aiState.consented, provider: aiState.provider, has_key: aiState.keys.has(aiState.provider) } satisfies AiStatus;
  },
  async aiPreview() {
    return [
      "INSTRUCTIONS FOR THE MODEL",
      "You write a short, friendly summary of a person's day from the facts below…",
      "",
      "YOUR INFORMATION BEING SENT",
      "Today is Saturday, 14 November. It is 9:30 AM.",
      "Rest of today:",
      "- 10:30 AM-11:30 AM Design review (Room 4B)",
      "- 12:30 PM-1:30 PM Lunch with Alex",
      "Tomorrow: 3 event(s), first at 6:45 AM (an early start)",
      ...(settings.ai.include_email ? ["Email: 37 unread", '- Important email from Priya Shah: "Agenda for Tuesday"'] : []),
    ].join("\n");
  },
  async enableAi(provider, key, confirm) {
    bump("__aiCalls");
    await sleep(300);
    if (!confirm) throw new Error("Please confirm what will be shared first.");
    const needsKey = provider !== "local";
    if (needsKey && !aiState.keys.has(provider) && !(key && key.length >= 10)) throw new Error("Enter your API key.");
    if (key === "sk-ant-rejected-key" || key === "rejected-key-0000") throw new Error("The service rejected the key. Check it and try again.");
    if (provider === "custom" && !/^https:\/\/[a-z0-9.-]+\.[a-z]+(\/\S*)?$/.test(settings.ai.custom_endpoint)) throw new Error("Enter the service's address, starting with https://");
    if (needsKey && key && key.length >= 10) aiState.keys.add(provider);
    const host = provider === "custom" ? customHost(settings.ai.custom_endpoint) : null;
    aiState = { ...aiState, enabled: true, consented: true, provider };
    settings = { ...settings, ai: { ...settings.ai, enabled: true, provider, consented: provider, consented_host: host } };
    (window as unknown as Record<string, unknown>).__aiEnabledFor = { provider, model: settings.ai.model };
    settingsListeners.forEach((l) => l());
    return structuredClone(settings);
  },
  async disableAi() {
    aiState = { ...aiState, enabled: false };
    settings = { ...settings, ai: { ...settings.ai, enabled: false } };
    settingsListeners.forEach((l) => l());
    return structuredClone(settings);
  },
  async forgetAiKey() {
    aiState.keys.delete(aiState.provider);
    aiState = { ...aiState, enabled: false, consented: false };
    settings = { ...settings, ai: { ...settings.ai, enabled: false, consented: null, consented_host: null } };
    settingsListeners.forEach((l) => l());
    return structuredClone(settings);
  },
  async aiKeys() {
    return [...aiState.keys];
  },
  async signInOpenrouter() {
    await sleep(params.has("slowoauth") ? 1500 : 300);
    if (params.has("orfail")) throw new Error("OpenRouter didn't accept that sign-in. Try again.");
    aiState.keys.add("openrouter");
  },
  async getAiSummary() {
    bump("__aiCalls");
    if (!aiState.enabled || params.has("aifail")) return null;
    await sleep(params.has("aislow") ? 3000 : 250);
    return "A busy morning with a design review at 10:30, then a quieter afternoon. Tomorrow starts early with your flight.";
  },
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
    const b = settings.briefing;
    return fixtureBriefing(fixtureEvents().filter((e) => ids.has(e.calendar_id)), new Date(), {
      showDeclined: b.show_declined,
      minGapMinutes: b.min_gap_minutes,
      workStartHour: b.work_start_hour,
      workEndHour: b.work_end_hour,
      earlyHour: b.early_hour,
    });
  },
  async askCalendar(question) {
    // A stand-in for the Rust path: records what would be sent (only the question), then answers from the
    // fixture events with a very small reading of the question. The real planner is crates/logic/src/ask.rs.
    const w = window as unknown as { __asked?: string[] };
    if (!aiState.enabled || !aiState.consented) return { enabled: false, understood: false, problem: null, answer: null } satisfies AskReply;
    (w.__asked ??= []).push(question.trim().slice(0, 300));
    await sleep(150);
    if (params.has("askfail")) return { enabled: true, understood: false, problem: "ai", answer: null } satisfies AskReply;
    const q = question.toLowerCase();
    if (/weather|joke|recipe/.test(q)) return { enabled: true, understood: false, problem: null, answer: null } satisfies AskReply;
    const day = new Date();
    day.setHours(0, 0, 0, 0);
    const to = new Date(day.getTime() + 14 * 86_400_000);
    const iso = (d: Date) => d.toISOString().slice(0, 10);
    if (/free|available/.test(q)) {
      const slots = await this.findFreeTimes(60, 7);
      return { enabled: true, understood: true, problem: null, answer: { kind: "free", from: iso(day), to: iso(to), keywords: [], minutes: 60, count: 0, events: [], slots } } satisfies AskReply;
    }
    const stop = new Set(["what", "when", "is", "my", "the", "a", "do", "i", "have", "next", "with", "on", "in", "me", "show", "any", "are", "there"]);
    const keywords = q.replace(/[^a-z0-9 ]/g, " ").split(/\s+/).filter((x) => x.length > 2 && !stop.has(x));
    const events = fixtureEvents().filter((e) => overlaps(e, day, to) && keywords.every((k) => `${e.title} ${e.location ?? ""}`.toLowerCase().includes(k)));
    return { enabled: true, understood: true, problem: null, answer: { kind: "events", from: iso(day), to: iso(to), keywords, minutes: 30, count: events.length, events: events.slice(0, 50), slots: [] } } satisfies AskReply;
  },
  async getWeekAhead() {
    // A stand-in for crates/logic/src/weekahead.rs (simpler: no back-to-back or focus search).
    const ids = visibleCalendarIds();
    const evs = fixtureEvents().filter((e) => ids.has(e.calendar_id) && e.status !== "cancelled" && e.my_response !== "declined");
    const first = new Date();
    first.setHours(0, 0, 0, 0);
    first.setDate(first.getDate() + 1);
    const iso = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
    const early = settings.briefing.early_hour;
    const days: WeekDay[] = [];
    for (let i = 0; i < 7; i++) {
      const d = new Date(first);
      d.setDate(d.getDate() + i);
      const [lo, hi] = [d.getTime(), d.getTime() + 86_400_000];
      const timed = evs
        .filter((e): e is typeof e & { when: { kind: "timed"; start: string; end: string; tzid: string | null } } => e.when.kind === "timed")
        .map((e) => [Date.parse(e.when.start), Date.parse(e.when.end)] as const)
        .filter(([s]) => s >= lo && s < hi)
        .sort((x, y) => x[0] - y[0]);
      let minutes = 0;
      let cursor = 0;
      for (const [s, e] of timed) {
        const from = Math.max(s, cursor);
        if (e > from) minutes += (Math.min(e, hi) - from) / 60_000;
        cursor = Math.max(cursor, e);
      }
      const allDay = evs.filter((e) => e.when.kind === "all_day" && e.when.start <= iso(d) && iso(d) < e.when.end).map((e) => e.title).slice(0, 3);
      days.push({
        date: iso(d),
        weekend: d.getDay() === 0 || d.getDay() === 6,
        meetings: timed.length,
        minutes: Math.round(minutes),
        first_start: timed[0] ? new Date(timed[0][0]).toISOString() : null,
        last_end: timed.length ? new Date(Math.max(...timed.map((t) => t[1]))).toISOString() : null,
        early_start: !!timed[0] && new Date(timed[0][0]).getHours() < early,
        back_to_back_runs: 0,
        conflicts: 0,
        heavy: minutes >= 300,
        all_day: allDay,
      });
    }
    const busiestDay = days.filter((d) => d.minutes > 0).sort((x, y) => y.minutes - x.minutes)[0];
    const working = days.filter((d) => !d.weekend);
    const lightestDay = [...working].sort((x, y) => x.minutes - y.minutes)[0];
    const spread = new Set(working.map((d) => d.minutes)).size > 1;
    return {
      days,
      meetings: days.reduce((n, d) => n + d.meetings, 0),
      minutes: days.reduce((n, d) => n + d.minutes, 0),
      busiest: busiestDay?.date ?? null,
      lightest: spread && lightestDay && lightestDay.date !== busiestDay?.date ? lightestDay.date : null,
      focus: null,
      needs_response: evs.filter((e) => e.my_response === "needs_action" && e.when.kind === "timed" && Date.parse(e.when.start) >= first.getTime() && Date.parse(e.when.start) < first.getTime() + 7 * 86_400_000).length,
      conflicts: 0,
      early_starts: days.filter((d) => d.early_start).map((d) => d.date),
    } satisfies WeekAhead;
  },
  async getConflictHelp(a, b, moveId) {
    // A small stand-in for crates/logic/src/conflict.rs.
    const all = fixtureEvents();
    const ea = all.find((e) => e.id === a);
    const eb = all.find((e) => e.id === b);
    if (!ea || !eb || ea.when.kind !== "timed" || eb.when.kind !== "timed") return null;
    const mins = (e: CalEvent) => (e.when.kind === "timed" ? (Date.parse(e.when.end) - Date.parse(e.when.start)) / 60_000 : 0);
    const mine = (e: CalEvent) => e.id.startsWith("local-events:");
    const score = (e: CalEvent, o: CalEvent) =>
      (mine(e) ? 3 : 0) + (e.my_response === "needs_action" && o.my_response !== "needs_action" ? 2 : 0) + (e.status === "tentative" && o.status !== "tentative" ? 2 : 0) + (mins(e) < mins(o) ? 1 : 0);
    const moveA = moveId ? moveId === ea.id : score(ea, eb) > score(eb, ea) || (score(ea, eb) === score(eb, ea) && Date.parse(ea.when.start) >= Date.parse(eb.when.start));
    const [moved, kept] = moveA ? [ea, eb] : [eb, ea];
    const reasons: ConflictHelp["reasons"] = moveId ? ["your_choice"] : [];
    if (!moveId) {
      if (mine(moved)) reasons.push("yours");
      if (moved.my_response === "needs_action" && kept.my_response !== "needs_action") reasons.push("not_answered");
      if (moved.status === "tentative" && kept.status !== "tentative") reasons.push("tentative");
      if (mins(moved) < mins(kept)) reasons.push("shorter");
      if (reasons.length === 0) reasons.push("later");
    }
    // The pile-up this pair belongs to (3 to 10 meetings in a chain of overlaps), a simplified copy of the Rust rules.
    const span = (e: CalEvent) => (e.when.kind === "timed" ? ([Date.parse(e.when.start), Date.parse(e.when.end)] as const) : null);
    const hit = (x: CalEvent, y: CalEvent) => {
      const [p, q] = [span(x), span(y)];
      return !!p && !!q && p[0] < q[1] && q[0] < p[1];
    };
    const pileUp = async (): Promise<ConflictHelp["pile_up"]> => {
      const pool = all.filter((e) => e.when.kind === "timed" && e.status !== "cancelled" && e.my_response !== "declined");
      const group = pool.filter((e) => e.id === ea.id || e.id === eb.id);
      for (let grew = true; grew; ) {
        const more = pool.filter((e) => !group.includes(e) && group.some((g) => hit(g, e)));
        grew = more.length > 0;
        group.push(...more);
        if (group.length > 10) return null;
      }
      if (group.length < 3) return null;
      group.sort((x, y) => span(x)![0] - span(y)![0]);
      const n = group.length;
      const value = group.map((e) => 10 - ((mine(e) ? 3 : 0) + (e.my_response === "needs_action" ? 2 : 0) + (e.my_response === "tentative" || e.status === "tentative" ? 2 : 0)));
      let best = -1;
      let bestMask = 0;
      for (let mask = 1; mask < 1 << n; mask++) {
        const kept = [...Array(n).keys()].filter((i) => mask & (1 << i));
        if (kept.some((i, x) => kept.slice(x + 1).some((j) => hit(group[i]!, group[j]!)))) continue;
        const score = kept.reduce((t, i) => t + value[i]! * 1000 + (n - i), 0);
        if (score > best) [best, bestMask] = [score, mask];
      }
      const keep = group.filter((_, i) => bestMask & (1 << i)).map((e) => e.id);
      const taken = new Set<string>();
      const moves = [];
      for (const e of group.filter((g) => !keep.includes(g.id))) {
        const need = Math.max(5, Math.round(((span(e)![1] - span(e)![0]) / 60_000)));
        const slots = (await this.findFreeTimes(need, 14)).filter((s) => !taken.has(s.start));
        const seen = new Set<string>();
        const options = slots
          .filter((s) => (seen.has(new Date(s.start).toDateString()) ? false : (seen.add(new Date(s.start).toDateString()), true)))
          .slice(0, 3)
          .map((s) => ({ start: s.start, end: new Date(Date.parse(s.start) + need * 60_000).toISOString() }));
        if (options[0]) taken.add(options[0].start);
        moves.push({ event: e, yours: mine(e), options });
      }
      return { events: group, keep, moves };
    };
    const [s1, e1, s2, e2] = [Date.parse(ea.when.start), Date.parse(ea.when.end), Date.parse(eb.when.start), Date.parse(eb.when.end)];
    const need = Math.max(5, Math.round(mins(moved)));
    const slots = await this.findFreeTimes(need, 14);
    const seen = new Set<string>();
    const options = slots
      .filter((s) => {
        const day = new Date(s.start).toDateString();
        if (seen.has(day)) return false;
        seen.add(day);
        return true;
      })
      .slice(0, 3)
      .map((s) => ({ start: s.start, end: new Date(Date.parse(s.start) + need * 60_000).toISOString() }));
    return { moved, kept, reasons, overlap_minutes: Math.round((Math.min(e1, e2) - Math.max(s1, s2)) / 60_000), options, yours: mine(moved), pile_up: await pileUp() } satisfies ConflictHelp;
  },
  async openMailDraft(to, subject, body) {
    const w = window as unknown as { __drafts?: Array<{ to: string | null; subject: string; body: string }> };
    (w.__drafts ??= []).push({ to, subject, body });
  },
  async getMeetingPrep(id) {
    // A small stand-in for crates/logic/src/prep.rs.
    const ids = visibleCalendarIds();
    const all = fixtureEvents().filter((e) => ids.has(e.calendar_id) && e.status !== "cancelled" && e.my_response !== "declined");
    const target = all.find((e) => e.id === id);
    const empty: MeetingPrep = { earlier: [], next_time: null, before: null, after: null, overlaps: [], mail: [] };
    if (!target || target.when.kind !== "timed") return empty;
    const [ts, te] = [Date.parse(target.when.start), Date.parse(target.when.end)];
    const norm = (s: string) => s.trim().replace(/\s+/g, " ").toLowerCase();
    const same = (e: CalEvent) => e.id !== target.id && norm(e.title).length >= 4 && norm(e.title) === norm(target.title);
    const timed = all.filter((e): e is CalEvent & { when: { kind: "timed"; start: string; end: string; tzid: string | null } } => e.id !== target.id && e.when.kind === "timed");
    const tidy = (s: string | null) => {
      const t = (s ?? "").split(/\s+/).filter((w) => w && !/^http/i.test(w)).join(" ");
      return t ? (t.length > 300 ? `${t.slice(0, 299)}…` : t) : null;
    };
    const earlier = all
      .filter((e) => same(e) && e.when.kind === "timed" && Date.parse(e.when.start) < ts)
      .sort((a, b) => Date.parse(b.when.start) - Date.parse(a.when.start))
      .slice(0, 3)
      .map((e) => ({ id: e.id, title: e.title, when: e.when, notes: tidy(e.description) }));
    const nextSame = all.filter((e) => same(e) && e.when.kind === "timed" && Date.parse(e.when.start) >= te).sort((a, b) => Date.parse(a.when.start) - Date.parse(b.when.start))[0];
    const hr4 = 4 * 3_600_000;
    const before = timed.filter((e) => Date.parse(e.when.end) <= ts && ts - Date.parse(e.when.end) <= hr4).sort((a, b) => Date.parse(b.when.end) - Date.parse(a.when.end))[0];
    const after = timed.filter((e) => Date.parse(e.when.start) >= te && Date.parse(e.when.start) - te <= hr4).sort((a, b) => Date.parse(a.when.start) - Date.parse(b.when.start))[0];
    const overlaps = timed
      .filter((e) => Date.parse(e.when.start) < te && Date.parse(e.when.end) > ts)
      .map((e) => ({ id: e.id, title: e.title, minutes: Math.round((Math.min(Date.parse(e.when.end), te) - Math.max(Date.parse(e.when.start), ts)) / 60_000) }));
    const mail =
      params.has("prepmail") && target.organizer === "Priya"
        ? [{ id: "m1", sender_name: "Priya", sender_address: "priya@example.com", subject: "Design review: slides attached", received: new Date().toISOString(), snippet: "", open: { kind: "url", url: "https://mail.example.com/m/1" } as const }]
        : [];
    return {
      earlier,
      next_time: nextSame ? { id: nextSame.id, when: nextSame.when } : null,
      before: before ? { id: before.id, title: before.title, gap_minutes: Math.round((ts - Date.parse(before.when.end)) / 60_000) } : null,
      after: after ? { id: after.id, title: after.title, gap_minutes: Math.round((Date.parse(after.when.start) - te) / 60_000) } : null,
      overlaps,
      mail,
    } satisfies MeetingPrep;
  },
  async findFreeTimes(minutes, days) {
    // A small browser-only stand-in for the Rust finder (crates/logic/src/briefing/slots.rs).
    const ids = visibleCalendarIds();
    const b = settings.briefing;
    const need = Math.min(Math.max(minutes, 5), 720) * 60_000;
    const now = Date.now();
    const out: FreeSlot[] = [];
    const first = new Date();
    first.setHours(0, 0, 0, 0);
    const evs = fixtureEvents().filter(
      (e) => ids.has(e.calendar_id) && e.when.kind === "timed" && e.status !== "cancelled" && e.my_response !== "declined",
    );
    for (let i = 0; i < Math.min(Math.max(days, 1), 21); i++) {
      const day = new Date(first);
      day.setDate(day.getDate() + i);
      if (day.getDay() === 0 || day.getDay() === 6) continue;
      const at = (h: number) => new Date(day.getFullYear(), day.getMonth(), day.getDate(), h).getTime();
      const close = at(b.work_end_hour);
      let cursor = Math.max(at(b.work_start_hour), now);
      const busy: Array<{ s: number; e: number }> = [];
      for (const ev of evs) {
        if (ev.when.kind !== "timed") continue;
        const span = { s: Date.parse(ev.when.start), e: Date.parse(ev.when.end) };
        if (span.s < close && span.e > cursor) busy.push(span);
      }
      busy.sort((x, y) => x.s - y.s);
      const push = (s: number, e: number) => out.push({ start: new Date(s).toISOString(), end: new Date(e).toISOString(), minutes: Math.round((e - s) / 60_000) });
      for (const { s, e } of busy) {
        if (s - cursor >= need) push(cursor, s);
        cursor = Math.max(cursor, e);
      }
      if (close - cursor >= need) push(cursor, close);
    }
    return out;
  },
  async dismissUpNext() {},
  async openBriefing() {},
  async onUpNextShown() {
    return () => {};
  },
  async getSettings() {
    return structuredClone(settings);
  },
  async setSettings(next) {
    if (next.briefing.work_end_hour <= next.briefing.work_start_hour) throw new Error("Working hours must end after they start.");
    const sc = next.general.global_shortcut;
    if (sc && !/^((cmd|ctrl|alt|shift|cmdorctrl)\+)+[a-z0-9]$/i.test(sc)) throw new Error("That shortcut isn't valid. Use at least one modifier key (such as Cmd or Ctrl) plus a letter, number or function key.");
    if (sc === "Cmd+Q") throw new Error("That shortcut couldn't be used. Another app may already own it.");
    const js = next.general.join_shortcut;
    if (js && !/^((cmd|ctrl|alt|shift|cmdorctrl)\+)+[a-z0-9]$/i.test(js)) throw new Error("That shortcut isn't valid. Use at least one modifier key (such as Cmd or Ctrl) plus a letter, number or function key.");
    if (js && js === sc) throw new Error("Choose a different key for each shortcut.");
    const fixed = structuredClone(next);
    fixed.email.imap = settings.email.imap; // mailboxes change only through add/remove
    fixed.ai.consented = settings.ai.consented; // consent is only ever given by the enable action
    fixed.ai.consented_host = settings.ai.consented_host;
    if (fixed.ai.consented === "custom" && customHost(fixed.ai.custom_endpoint) !== fixed.ai.consented_host) fixed.ai.consented = null;
    if (fixed.ai.consented !== fixed.ai.provider) fixed.ai.enabled = false;
    fixed.general.sync_interval_minutes = clamp(fixed.general.sync_interval_minutes, 1, 1440);
    settings = fixed;
    settingsListeners.forEach((l) => l());
    return structuredClone(settings);
  },
  async exportSettings() {
    return JSON.stringify({ format: "firstlight-settings", version: 1, settings }, null, 2);
  },
  async saveSettingsExport() {
    return "/Users/you/Downloads/firstlight-settings.json";
  },
  async importSettings(text) {
    let parsed: { format?: string; settings?: Partial<AppSettings> };
    try {
      parsed = JSON.parse(text);
    } catch {
      throw new Error("That isn't a valid settings file.");
    }
    if (parsed.format !== "firstlight-settings") throw new Error("That isn't a Firstlight settings file.");
    settings = { ...structuredClone(DEFAULT_SETTINGS), ...parsed.settings } as AppSettings;
    settingsListeners.forEach((l) => l());
    return structuredClone(settings);
  },
  async eraseAllData() {
    accounts = [];
    settings = structuredClone(DEFAULT_SETTINGS);
    settingsListeners.forEach((l) => l());
    notify();
  },
  async onSettingsChanged(cb) {
    settingsListeners.add(cb);
    return () => settingsListeners.delete(cb);
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
    return { supported: !params.has("nosystem"), name: params.has("outlook") ? "Outlook" : "Calendar app", writable: !params.has("outlook"), mail_app: !params.has("outlook"), access: params.has("denied") ? "denied" : "not_determined" } as const;
  },
  async addSystemAccount() {
    if (params.has("denied")) throw new Error("Calendar access is turned off for Firstlight. Open System Settings, switch Firstlight on under Calendars, then try again.");
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
