import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { assertHttps, type Backend } from "./backend";
import type { AccountView, CalendarInfo, CalEvent, OAuthAvailability, SyncSummary, SystemStatus } from "./types";

export const tauriBackend: Backend = {
  getEvents: (start, end) => invoke<CalEvent[]>("get_events", { start, end }),
  getCalendars: () => invoke<CalendarInfo[]>("get_calendars"),
  listAccounts: () => invoke<AccountView[]>("list_accounts"),
  addIcsAccount: (name, url) => invoke<AccountView>("add_ics_account", { name, url }),
  addCaldavAccount: (name, server, username, password) => invoke<AccountView>("add_caldav_account", { name, server, username, password }),
  oauthAvailable: () => invoke<OAuthAvailability>("oauth_available"),
  addOauthAccount: (provider) => invoke<AccountView>("add_oauth_account", { provider }),
  cancelOauth: () => invoke<void>("cancel_oauth"),
  systemCalendarStatus: () => invoke<SystemStatus>("system_calendar_status"),
  addSystemAccount: () => invoke<AccountView>("add_system_account"),
  openPrivacySettings: (pane) => invoke<void>("open_privacy_settings", { pane }),
  addDemoAccount: () => invoke<AccountView>("add_demo_account"),
  removeAccount: (id) => invoke<void>("remove_account", { id }),
  renameAccount: (id, name) => invoke<void>("rename_account", { id, name }),
  setCalendarVisible: (calendarId, visible) => invoke<void>("set_calendar_visible", { calendarId, visible }),
  setCalendarColour: (calendarId, colour) => invoke<void>("set_calendar_colour", { calendarId, colour }),
  refreshNow: () => invoke<void>("refresh_now"),
  onSyncFinished: async (callback) => listen<SyncSummary>("sync-finished", (e) => callback(e.payload)),
  getAutostart: () => invoke<boolean>("get_autostart"),
  setAutostart: (enabled) => invoke<void>("set_autostart", { enabled }),
  onUnlock: async (callback) => listen("system-unlock", () => callback()),
  openUrl: (url) => openUrl(assertHttps(url).toString()),
};
