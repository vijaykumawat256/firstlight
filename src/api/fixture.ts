import { assertHttps, type Backend } from "./backend";
import { fixtureBriefing } from "./fixtureBriefing";
import type { AccountView, AiProvider, AiStatus, AppInfo, AppSettings, CalendarInfo, CalEvent, EventInput, MailResult, SyncSummary, UpdateStatus } from "./types";

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
const ownRecords: { id: string; input: EventInput; skipped?: string[] }[] = [];

/** The dated events those records produce (repeats run 120 days ahead, like the real app's window). */
function ownEvents(): CalEvent[] {
  const out: CalEvent[] = [];
  for (const rec of ownRecords) out.push(...expandOwn(rec.id, rec.input).filter((e) => !rec.skipped?.includes(e.remote_id.split(":")[1] ?? "")));
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
  general: { sync_interval_minutes: 10, default_view: "briefing", week_start: "auto", time_format: "auto", global_shortcut: null, check_for_updates: true, update_channel: "stable" },
  briefing: { show_declined: false, min_gap_minutes: 30, work_start_hour: 9, work_end_hour: 18, early_hour: 8 },
  sections: { tomorrow: true, free_time: true, attention: true },
  unlock: { first_unlock_after_hour: 6, min_interval_minutes: 30, quiet_hours: [22, 6], days: [true, true, true, true, true, true, true], up_next_lead_minutes: 15, skip_when_fullscreen: true },
  notify: { before_event: true, before_minutes: 10, morning: true, morning_hour: 8, evening: true, evening_hour: 18, time_24h: false, hide_titles: false },
  appearance: { theme: "system", accent: "system", ambient: params.has("ambient") },
  email: { enabled: false, apple_mail: false, account_ids: [], imap: [], important_senders: [], days_back: 3 },
  ai: { enabled: false, provider: "anthropic", model: "claude-haiku-5-5", endpoint: "http://127.0.0.1:11434", include_email: false, consented: null },
  privacy: { crash_reports: false },
};
let settings: AppSettings = structuredClone(DEFAULT_SETTINGS);
if (["week", "day", "month"].includes(params.get("defaultview") ?? "")) settings.general.default_view = params.get("defaultview") as "day" | "week" | "month";
const settingsListeners = new Set<() => void>();
const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, n));

const listeners = new Set<(s: SyncSummary) => void>();
const notify = () => listeners.forEach((l) => l({ ok: true, offline: false, needs_attention: false }));

const LOCAL_CAL = "local-events";
const pad2 = (n: number) => String(n).padStart(2, "0");
const ymd = (d: Date) => `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;

/** Mirrors the Rust checks (`daybrief_core::local`) closely enough for the browser tests. */
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

function expandOwn(id: string, input: EventInput): CalEvent[] {
  const key = id.slice(LOCAL_CAL.length + 1);
  const base = (when: CalEvent["when"], suffix: string | null): CalEvent => ({
    id: suffix ? `${id}:${suffix}` : id,
    calendar_id: LOCAL_CAL,
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

function saveFixtureEvent(id: string | null, input: EventInput): CalEvent {
  const clean = checkOwn(input);
  const master = id ? `${LOCAL_CAL}:${id.slice(LOCAL_CAL.length + 1).split(":")[0]}` : null;
  if (master && !ownRecords.some((r) => r.id === master)) throw new Error("That event can't be changed here.");
  const recordId = master ?? `${LOCAL_CAL}:own${ownRecords.length + 1}-${Date.now()}`;
  const at = ownRecords.findIndex((r) => r.id === recordId);
  if (at >= 0) ownRecords[at] = { id: recordId, input: clean, skipped: ownRecords[at]!.skipped };
  else ownRecords.push({ id: recordId, input: clean });
  if (!accounts.some((a) => a.id === "local-account")) {
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
  return expandOwn(recordId, clean)[0]!;
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
let aiState = { enabled: false, consented: false, hasKey: false, provider: "anthropic" as AiProvider };
if (params.has("aion")) {
  aiState = { enabled: true, consented: true, hasKey: true, provider: "anthropic" };
  settings.ai = { ...settings.ai, enabled: true, consented: "anthropic" };
}
if (params.has("mailon")) settings.email = { ...settings.email, enabled: true, apple_mail: true };
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
  async createEvent(input) {
    return saveFixtureEvent(null, input);
  },
  async updateEvent(id, input) {
    return saveFixtureEvent(id, input);
  },
  async deleteEvent(id) {
    const master = `${LOCAL_CAL}:${id.slice(LOCAL_CAL.length + 1).split(":")[0]}`;
    const i = ownRecords.findIndex((r) => r.id === master);
    if (i < 0) throw new Error("That event can't be changed here.");
    ownRecords.splice(i, 1);
  },
  async skipOccurrence(id) {
    const [master, key] = [`${LOCAL_CAL}:${id.slice(LOCAL_CAL.length + 1).split(":")[0]}`, id.split(":")[2]];
    const rec = ownRecords.find((r) => r.id === master);
    if (!rec || !key || !rec.input.repeat) throw new Error("Only one day of a repeating event can be removed.");
    rec.skipped = [...new Set([...(rec.skipped ?? []), key])];
  },
  async getSystemAccent() {
    return params.get("sysaccent");
  },
  async getOwnEvent(id) {
    const master = `${LOCAL_CAL}:${id.slice(LOCAL_CAL.length + 1).split(":")[0]}`;
    const rec = ownRecords.find((r) => r.id === master);
    return rec ? structuredClone(rec) : null;
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
    return { enabled: aiState.enabled, consented: aiState.consented, provider: aiState.provider, has_key: aiState.hasKey } satisfies AiStatus;
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
    if (provider === "anthropic" && !aiState.hasKey && !(key && key.length >= 10)) throw new Error("Enter your API key.");
    if (key === "sk-ant-rejected-key") throw new Error("The service rejected the key. Check it and try again.");
    aiState = { enabled: true, consented: true, hasKey: provider === "anthropic" ? true : aiState.hasKey, provider };
    settings = { ...settings, ai: { ...settings.ai, enabled: true, provider, consented: provider } };
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
    aiState = { enabled: false, consented: false, hasKey: false, provider: aiState.provider };
    settings = { ...settings, ai: { ...settings.ai, enabled: false, consented: null } };
    settingsListeners.forEach((l) => l());
    return structuredClone(settings);
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
    const fixed = structuredClone(next);
    fixed.email.imap = settings.email.imap; // mailboxes change only through add/remove
    fixed.ai.consented = settings.ai.consented; // consent is only ever given by the enable action
    if (fixed.ai.consented !== fixed.ai.provider) fixed.ai.enabled = false;
    fixed.general.sync_interval_minutes = clamp(fixed.general.sync_interval_minutes, 1, 1440);
    settings = fixed;
    settingsListeners.forEach((l) => l());
    return structuredClone(settings);
  },
  async exportSettings() {
    return JSON.stringify({ format: "daybrief-settings", version: 1, settings }, null, 2);
  },
  async saveSettingsExport() {
    return "/Users/you/Downloads/daybrief-settings.json";
  },
  async importSettings(text) {
    let parsed: { format?: string; settings?: Partial<AppSettings> };
    try {
      parsed = JSON.parse(text);
    } catch {
      throw new Error("That isn't a valid settings file.");
    }
    if (parsed.format !== "daybrief-settings") throw new Error("That isn't a Firstlight settings file.");
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
    return { supported: !params.has("nosystem"), access: params.has("denied") ? "denied" : "not_determined" } as const;
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
