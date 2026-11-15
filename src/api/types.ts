/** Mirrors the JSON produced by `firstlight-core::model` (snake_case, RFC 3339 instants, ISO dates). */

export type When =
  | { kind: "timed"; start: string; end: string; tzid: string | null }
  | { kind: "all_day"; start: string; end: string };

export type EventStatus = "confirmed" | "tentative" | "cancelled";
export type Response = "accepted" | "declined" | "tentative" | "needs_action" | "none";

export interface CalEvent {
  id: string;
  calendar_id: string;
  remote_id: string;
  ical_uid: string;
  title: string;
  when: When;
  location: string | null;
  description: string | null;
  join_url: string | null;
  status: EventStatus;
  my_response: Response;
  organizer: string | null;
  recurrence_master_id: string | null;
  updated_at: string;
  /** Other calendars holding the same meeting (merged by the backend). */
  also_in: string[];
}

export interface CalendarInfo {
  id: string;
  account_id: string;
  remote_id: string;
  name: string;
  colour: string;
  visible: boolean;
  read_only: boolean;
}

export type ProviderKind = "fixture" | "google" | "microsoft" | "caldav" | "ics" | "system" | "local";

/** An event the user adds themselves. All-day dates are YYYY-MM-DD with `end` the last day (inclusive); timed events use ISO instants. */
export interface EventInput {
  title: string;
  all_day: boolean;
  start: string;
  end: string;
  location: string | null;
  join_url: string | null;
  notes: string | null;
  /** Repeats every `interval` days, weeks, months or years, optionally until a last day (YYYY-MM-DD). */
  repeat: { every: "daily" | "weekly" | "monthly" | "yearly"; interval: number; until: string | null } | null;
  /** Notify this many minutes before it starts (timed events only). */
  reminder_minutes: number | null;
  /** People it is shared with. Saving sends nothing: sharing opens your own mail app, or (if turned on) sends from your mailbox after you review it. */
  invitees: string[];
}

/** One of your own events with everything the edit form needs. */
export interface OwnEvent {
  id: string;
  input: EventInput;
  /** The Calendar app's calendar it was saved to; null if it lives only on this computer. */
  saved_to: string | null;
}

/** A calendar in the Calendar app that events can be added to. */
export interface WritableCalendar {
  id: string;
  name: string;
  source: string;
}

export interface ExportResult {
  path: string;
  count: number;
}

/** A mailbox an invitation can be sent from. */
export interface InviteSender {
  id: string;
  address: string;
  label: string;
}
/** The exact subject and text "Send now" would send. */
export interface InvitePreview {
  subject: string;
  body: string;
}
export interface InviteResult {
  path: string;
  mail_opened: boolean;
}
export type AccountStatus = "ok" | "needs_attention";

export interface AccountView {
  id: string;
  provider: ProviderKind;
  display_name: string;
  email: string | null;
  status: AccountStatus;
  calendars: CalendarInfo[];
  last_synced_at: string | null;
  /** A short code ("auth" | "offline" | "failed"), never content. */
  last_error: string | null;
}

export interface SyncSummary {
  ok: boolean;
  offline: boolean;
  needs_attention: boolean;
}

export interface OAuthAvailability {
  google: boolean;
  microsoft: boolean;
}

export type SystemAccess = "granted" | "not_determined" | "denied" | "restricted" | "unsupported";

export interface SystemStatus {
  supported: boolean;
  /** What it is called on this computer: "Calendar app" (Mac) or "Outlook" (Windows). */
  name: string;
  /** Whether events can be added there (not to Outlook yet). */
  writable: boolean;
  /** Whether the Mac's Mail app can be a mail source. */
  mail_app: boolean;
  access: SystemAccess;
}

/** Mirrors `firstlight_core::briefing::Briefing`. */
export interface BriefingItem {
  event: CalEvent;
  start: string;
  end: string;
}

export interface NextUp extends BriefingItem {
  in_progress: boolean;
}

export type DayState = "clear" | "all_day_only" | "done" | "in_progress" | "upcoming";

export interface Briefing {
  generated_at: string;
  /** The user's local date (YYYY-MM-DD). */
  date: string;
  state: DayState;
  all_day_today: CalEvent[];
  past: BriefingItem[];
  remaining: BriefingItem[];
  next: NextUp | null;
  conflicts: { a: string; b: string; overlap_minutes: number }[];
  back_to_back_runs: string[][];
  free_gaps: { start: string; end: string; minutes: number }[];
  needs_response: CalEvent[];
  tomorrow: { timed: BriefingItem[]; all_day: CalEvent[]; early_start: boolean };
}

/** A Yahoo or iCloud mailbox (IMAP). Its app password stays in the keychain, so it is not here. */
export type ImapPreset = "yahoo" | "icloud" | "custom";
export interface ImapMailbox {
  id: string;
  preset: ImapPreset;
  address: string;
  /** The server of a custom mailbox; null for Yahoo and iCloud. */
  host: string | null;
}

/** Mirrors `firstlight_core::settings::AppSettings`. */
export type DefaultView = "briefing" | "day" | "week" | "month";
export type WeekStart = "auto" | "monday" | "sunday" | "saturday";
export type TimeFormat = "auto" | "h12" | "h24";
export type ThemeChoice = "system" | "light" | "dark";

export interface AppSettings {
  version: number;
  general: {
    sync_interval_minutes: number;
    default_view: DefaultView;
    week_start: WeekStart;
    time_format: TimeFormat;
    global_shortcut: string | null;
    show_in_dock: boolean;
    write_to_calendar_app: boolean;
    writable_accounts: string[];
    join_shortcut: string | null;
    tray_next_meeting: boolean;
    check_for_updates: boolean;
    update_channel: "stable" | "beta";
  };
  briefing: { show_declined: boolean; min_gap_minutes: number; work_start_hour: number; work_end_hour: number; early_hour: number };
  sections: { tomorrow: boolean; free_time: boolean; attention: boolean };
  unlock: {
    first_unlock_after_hour: number;
    min_interval_minutes: number;
    /** `[from, to]` local hours; may wrap midnight. `null` means no quiet hours. */
    quiet_hours: [number, number] | null;
    /** Monday first. */
    days: boolean[];
    up_next_lead_minutes: number;
    skip_when_fullscreen: boolean;
  };
  notify: { before_event: boolean; before_minutes: number; morning: boolean; morning_hour: number; evening: boolean; evening_hour: number; time_24h: boolean; hide_titles: boolean };
  appearance: { theme: ThemeChoice; accent: string; ambient: boolean };
  email: { enabled: boolean; apple_mail: boolean; account_ids: string[]; imap: ImapMailbox[]; send_invitations: boolean; important_senders: string[]; days_back: number };
  ai: { enabled: boolean; provider: AiProvider; model: string; endpoint: string; custom_endpoint: string; include_email: boolean; consented: AiProvider | null; consented_host: string | null };
  privacy: { crash_reports: boolean };
}

export type AiProvider = "anthropic" | "local" | "openai" | "xai" | "gemini" | "mistral" | "groq" | "openrouter" | "custom";

export interface AiStatus {
  enabled: boolean;
  consented: boolean;
  provider: AiProvider;
  has_key: boolean;
}

/** Mirrors `firstlight_core::mail`. */
export type OpenTarget = { kind: "url"; url: string } | { kind: "apple_mail"; message_id: string } | { kind: "none" };

export interface MailMessage {
  id: string;
  sender_name: string;
  sender_address: string;
  subject: string;
  received: string;
  snippet: string;
  open: OpenTarget;
}

export interface InvitationItem {
  message: MailMessage;
  title: string | null;
  when: When | null;
  location: string | null;
  organizer: string | null;
}

export interface MailDigest {
  generated_at: string;
  unread_total: number;
  important: MailMessage[];
  invitations: InvitationItem[];
  other_unread: number;
}

export interface MailProblem {
  /** "apple_mail" or the account's name. */
  source: string;
  kind: "permission" | "not_running" | "offline" | "reconnect" | "failed";
}

export interface MailResult {
  enabled: boolean;
  digest: MailDigest | null;
  problems: MailProblem[];
}

export interface AppInfo {
  name: string;
  version: string;
  os: string;
  arch: string;
  /** Whether this package can update itself. */
  updates_configured: boolean;
}

export type UpdateStatus = { state: "not_configured" } | { state: "up_to_date" } | { state: "available"; version: string } | { state: "failed" };

/** Mirrors `firstlight_core::prep`. */
export interface MeetingPrep {
  earlier: Array<{ id: string; title: string; when: When; notes: string | null }>;
  next_time: { id: string; when: When } | null;
  before: { id: string; title: string; gap_minutes: number } | null;
  after: { id: string; title: string; gap_minutes: number } | null;
  overlaps: Array<{ id: string; title: string; minutes: number }>;
  /** Unread messages that look to be about the meeting (from the digest already fetched). */
  mail: MailMessage[];
}

/** The answer to a question about the calendar, worked out on this computer from the stored events. */
export interface CalendarAnswer {
  kind: "events" | "free";
  from: string;
  to: string;
  keywords: string[];
  minutes: number;
  /** How many events matched; `events` holds at most 50. */
  count: number;
  events: CalEvent[];
  slots: FreeSlot[];
}

export interface AskReply {
  /** AI is off, or not agreed to. */
  enabled: boolean;
  /** The question was about the calendar and could be run. */
  understood: boolean;
  problem: "limit" | "ai" | null;
  answer: CalendarAnswer | null;
}

/** A free stretch inside working hours (from "Find a time"). */
export interface FreeSlot {
  start: string;
  end: string;
  minutes: number;
}
