import type { AccountView, AiProvider, AiStatus, AppInfo, AppSettings, Briefing, CalendarInfo, CalEvent, EventInput, ExportResult, ImapPreset, InviteResult, OwnEvent, MailResult, OAuthAvailability, OpenTarget, SyncSummary, SystemStatus, UpdateStatus } from "./types";

/**
 * Everything the UI may ask of the outside world. The UI imports `api` and never
 * touches Tauri directly, so a web implementation can replace it later (PLAN.md, section 3).
 */
export interface Backend {
  /** Events touching `[startIso, endIso)`. */
  getEvents(startIso: string, endIso: string): Promise<CalEvent[]>;
  getCalendars(): Promise<CalendarInfo[]>;
  /** The email digest. Resolves to `{ enabled: false }` without doing anything when email is off. */
  getMailDigest(force: boolean): Promise<MailResult>;
  /** Asks macOS for permission to read the Mail app (one-time prompt) and turns it on. */
  enableAppleMail(): Promise<AppSettings>;
  /** Asks Google/Microsoft for permission to read this account's mail, then turns it on. */
  enableAccountMail(accountId: string): Promise<AppSettings>;
  /** Add, change or remove an event of your own (stored only on this computer). */
  createEvent(input: EventInput): Promise<CalEvent>;
  getOwnEvent(id: string): Promise<OwnEvent | null>;
  /** Saves everything you added as one calendar file in Downloads. */
  exportOwnEvents(): Promise<ExportResult>;
  /** Removes one day of a repeating event of your own; the rest of the series stays. */
  skipOccurrence(id: string): Promise<void>;
  /** The macOS accent colour as a palette name, or null (default blue, graphite, other systems). */
  getSystemAccent(): Promise<string | null>;
  /** Saves a calendar file to Downloads and, if asked, opens your mail app with a draft. Nothing is sent. */
  prepareInvite(id: string, recipients: string, openMail: boolean): Promise<InviteResult>;
  updateEvent(id: string, input: EventInput): Promise<CalEvent>;
  deleteEvent(id: string): Promise<void>;
  /** `host` is only used for a custom mailbox (an IMAP server on port 993). */
  addImapMailbox(preset: ImapPreset, address: string, password: string, host?: string): Promise<AppSettings>;
  removeImapMailbox(mailboxId: string): Promise<AppSettings>;
  openMailMessage(target: OpenTarget): Promise<void>;
  aiStatus(): Promise<AiStatus>;
  /** The exact text that would be sent to the AI service. */
  aiPreview(): Promise<string>;
  /** `confirm` must be true: the user has read the preview and agreed. Tests the key first. */
  enableAi(provider: AiProvider, key: string | null, confirm: boolean): Promise<AppSettings>;
  disableAi(): Promise<AppSettings>;
  /** Deletes the saved API key and withdraws consent. */
  forgetAiKey(): Promise<AppSettings>;
  /** A short AI summary, or null (AI off or anything failed). Never rejects. */
  getAiSummary(): Promise<string | null>;
  getAppInfo(): Promise<AppInfo>;
  checkForUpdate(): Promise<UpdateStatus>;
  /** Downloads, verifies the signature, installs and restarts. */
  installUpdate(): Promise<void>;
  /** Fires when the daily background check finds a new version. */
  onUpdateAvailable(callback: (version: string) => void): Promise<() => void>;
  /** The saved content-free crash report, if any. */
  getLastCrashReport(): Promise<string | null>;
  clearCrashReports(): Promise<void>;
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
  /** Whether this computer has a Calendar app Firstlight can read, and whether the user allowed it. Never prompts. */
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
