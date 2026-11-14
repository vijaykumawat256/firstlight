/** Mirrors the JSON produced by `daybrief-core::model` (snake_case, RFC 3339 instants, ISO dates). */

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

export type ProviderKind = "fixture" | "google" | "microsoft" | "caldav" | "ics" | "system";
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
