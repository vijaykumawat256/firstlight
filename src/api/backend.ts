import type { AccountView, AppSettings, Briefing, CalendarInfo, CalEvent, OAuthAvailability, SyncSummary, SystemStatus } from "./types";

/**
 * Everything the UI may ask of the outside world. The UI imports `api` and never
 * touches Tauri directly, so a web implementation can replace it later (PLAN.md, section 3).
 */
export interface Backend {
  /** Events touching `[startIso, endIso)`. */
  getEvents(startIso: string, endIso: string): Promise<CalEvent[]>;
  getCalendars(): Promise<CalendarInfo[]>;
  getSettings(): Promise<AppSettings>;
  /** Validates, applies and saves. Resolves to the settings as stored (out-of-range values corrected). */
  setSettings(settings: AppSettings): Promise<AppSettings>;
  /** The settings as portable JSON text: no accounts, no secrets. */
  exportSettings(): Promise<string>;
  /** Saves the export into the Downloads folder; resolves to the file's path. */
  saveSettingsExport(): Promise<string>;
  importSettings(text: string): Promise<AppSettings>;
  /** Deletes all accounts, events, history, settings and saved credentials. */
  eraseAllData(): Promise<void>;
  onSettingsChanged(callback: () => void): Promise<() => void>;
  /** Today and tomorrow, worked out by the briefing engine. */
  getBriefing(): Promise<Briefing>;
  /** Hides the small "up next" panel. */
  dismissUpNext(): Promise<void>;
  /** Brings the full briefing window to the front. */
  openBriefing(): Promise<void>;
  /** Fires each time the small panel is shown, so it can reload. */
  onUpNextShown(callback: () => void): Promise<() => void>;
  listAccounts(): Promise<AccountView[]>;
  /** Connects a calendar by link. Rejects with a plain-language message if it can't be read. */
  addIcsAccount(name: string, url: string): Promise<AccountView>;
  /** Connects a CalDAV server (iCloud, Yahoo, Fastmail, ...) with a username and app password. */
  addCaldavAccount(name: string, server: string, username: string, password: string): Promise<AccountView>;
  /** Which sign-in providers this build is configured for (client IDs present). */
  oauthAvailable(): Promise<OAuthAvailability>;
  /**
   * Opens the provider's sign-in page in the system browser and resolves when the user finishes.
   * Signing in again with an existing account reconnects it instead of adding a duplicate.
   */
  addOauthAccount(provider: "google" | "microsoft"): Promise<AccountView>;
  cancelOauth(): Promise<void>;
  /** Whether this computer has a Calendar app Daybrief can read, and whether the user allowed it. Never prompts. */
  systemCalendarStatus(): Promise<SystemStatus>;
  /** Connects the Calendar app; shows the system permission prompt the first time. */
  addSystemAccount(): Promise<AccountView>;
  /** Opens the System Settings pane where access can be switched on. */
  openPrivacySettings(pane: "calendars" | "automation"): Promise<void>;
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
