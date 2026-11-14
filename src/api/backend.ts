import type { AccountView, CalendarInfo, CalEvent, SyncSummary } from "./types";

/**
 * Everything the UI may ask of the outside world. The UI imports `api` and never
 * touches Tauri directly, so a web implementation can replace it later (PLAN.md, section 3).
 */
export interface Backend {
  /** Events touching `[startIso, endIso)`. */
  getEvents(startIso: string, endIso: string): Promise<CalEvent[]>;
  getCalendars(): Promise<CalendarInfo[]>;
  listAccounts(): Promise<AccountView[]>;
  /** Connects a calendar by link. Rejects with a plain-language message if it can't be read. */
  addIcsAccount(name: string, url: string): Promise<AccountView>;
  /** Connects a CalDAV server (iCloud, Yahoo, Fastmail, ...) with a username and app password. */
  addCaldavAccount(name: string, server: string, username: string, password: string): Promise<AccountView>;
  addDemoAccount(): Promise<AccountView>;
  /** Deletes the account, its cached events and its saved credentials. */
  removeAccount(id: string): Promise<void>;
  renameAccount(id: string, name: string): Promise<void>;
  setCalendarVisible(calendarId: string, visible: boolean): Promise<void>;
  setCalendarColour(calendarId: string, colour: string): Promise<void>;
  /** Starts a sync of every account; completion arrives via `onSyncFinished`. */
  refreshNow(): Promise<void>;
  onSyncFinished(callback: (summary: SyncSummary) => void): Promise<() => void>;
  getAutostart(): Promise<boolean>;
  setAutostart(enabled: boolean): Promise<void>;
  /** Called whenever the computer is unlocked. Returns an unsubscribe function. */
  onUnlock(callback: () => void): Promise<() => void>;
  /** Opens an https link in the default browser/app. Anything else is rejected. */
  openUrl(url: string): Promise<void>;
}

export function assertHttps(url: string): URL {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    throw new Error("Invalid link");
  }
  if (parsed.protocol !== "https:") throw new Error("Only https links can be opened");
  return parsed;
}

/** Backend errors arrive as plain strings (Tauri) or Errors (browser fixture). */
export function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return "Something went wrong.";
}
