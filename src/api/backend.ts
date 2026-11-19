import type { AccountView, AiProvider, AiStatus, AppInfo, AppSettings, Briefing, CalendarInfo, CalEvent, AskReply, ConflictHelp, EventInput, ExportResult, DayPlan, DayStatus, Task, TaskInput, TaskLists, WeekAhead, MeetingNote, MeetingPrep, FreeSlot, ImapPreset, InvitePreview, InviteSender, OwnEvent, MailResult, OAuthAvailability, OpenTarget, SyncSummary, SystemStatus, UpdateStatus, WritableCalendar } from "./types";

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
  /** With `calendarId` (one of `writableCalendars`) the event is saved into that Calendar-app calendar. */
  createEvent(input: EventInput, calendarId?: string): Promise<CalEvent>;
  /** The calendars events can be added to: empty unless the setting is on and access was allowed. */
  writableCalendars(): Promise<WritableCalendar[]>;
  getOwnEvent(id: string): Promise<OwnEvent | null>;
  /** Saves everything you added as one calendar file in Downloads. */
  exportOwnEvents(): Promise<ExportResult>;
  /** Removes one day of a repeating event of your own; the rest of the series stays. */
  skipOccurrence(id: string): Promise<void>;
  /** The macOS accent colour as a palette name, or null (default blue, graphite, other systems). */
  getSystemAccent(): Promise<string | null>;
  /** Mailboxes "Send now" can use. Empty unless the user turned sending on. */
  inviteSenders(): Promise<InviteSender[]>;
  /** The exact message that would be sent. */
  /** The exact message that would go out: an invitation (`request`) or a cancellation (`cancel`). */
  invitePreview(id: string, mailboxId: string, kind: "request" | "cancel"): Promise<InvitePreview>;
  /** Sends the invitation from the chosen mailbox. Only on the user's press of Send. */
  sendInviteMail(id: string, recipients: string, mailboxId: string, kind: "request" | "cancel"): Promise<void>;
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
  /** Which providers have an API key saved. */
  aiKeys(): Promise<AiProvider[]>;
  /** Signs in to OpenRouter in the browser and keeps the key it issues (no key to paste). */
  signInOpenrouter(): Promise<void>;
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
  /** The next seven days (from tomorrow): load per day, busiest and lightest, early starts, a block to focus. */
  getWeekAhead(): Promise<WeekAhead>;
  /** Which of two overlapping meetings is easier to move, why, and where it could go. Suggests only. `moveId` picks the one to move. */
  getConflictHelp(a: string, b: string, moveId: string | null): Promise<ConflictHelp | null>;
  /** Opens a new message in your own mail app; Firstlight sends nothing. */
  openMailDraft(to: string | null, subject: string, body: string): Promise<void>;
  /** What is worth knowing before a meeting, worked out locally (no AI, no network). */
  getMeetingPrep(id: string): Promise<MeetingPrep>;
  /** Your to-do list, worked out into Today (with carried-over tasks), Upcoming, Someday and Done. Kept on this computer. */
  listTasks(): Promise<TaskLists>;
  addTask(task: TaskInput): Promise<Task>;
  updateTask(id: string, task: TaskInput): Promise<Task>;
  setTaskDone(id: string, done: boolean): Promise<Task>;
  deleteTask(id: string): Promise<void>;
  /** Sets time aside for the task: an event of your own from `start` to `end`. */
  planTask(id: string, start: string, end: string): Promise<Task>;
  /** The lines of a meeting note that could become tasks. */
  noteTaskLines(text: string): Promise<string[]>;
  addTasksFromNote(titles: string[], meeting: string | null): Promise<Task[]>;
  /** Moves tasks to another day (YYYY-MM-DD), keeping each one's time of day. */
  moveTasks(ids: string[], date: string): Promise<void>;
  /** Whether the wrap-up moment has come, and whether meetings leave room for lunch today. */
  getDayStatus(): Promise<DayStatus>;
  /** Suggests when today's tasks could be done in today's free time. Changes nothing. With `useAi` (and both agreements) task titles, lengths and free times go to your AI service. */
  planMyDay(useAi: boolean): Promise<DayPlan>;
  /** Exactly what planning with AI would send right now. */
  dayPlanPreview(): Promise<string>;
  /** Records the agreement to send task titles for planning; `confirm` means the preview was read. */
  enableAiPlanning(confirm: boolean): Promise<AppSettings>;
  disableAiPlanning(): Promise<AppSettings>;
  /** Your private note for this meeting (shared by every time it happens), or null. */
  getMeetingNote(id: string): Promise<MeetingNote | null>;
  /** Keeps the note for this meeting; empty text removes it. Kept only on this computer. */
  saveMeetingNote(id: string, text: string): Promise<MeetingNote | null>;
  /** Answers a question about the calendar. Only the question (with today's date and time zone) is sent to the AI; the answer is built here from the stored events. */
  askCalendar(question: string): Promise<AskReply>;
  /** Free stretches in working hours, from now, long enough for `minutes`; weekends skipped. */
  findFreeTimes(minutes: number, days: number): Promise<FreeSlot[]>;
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
