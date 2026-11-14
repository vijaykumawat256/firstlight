import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { assertHttps, type Backend } from "./backend";
import type { CalendarInfo, CalEvent } from "./types";

export const tauriBackend: Backend = {
  getEvents: (start, end) => invoke<CalEvent[]>("get_events", { start, end }),
  getCalendars: () => invoke<CalendarInfo[]>("get_calendars"),
  getAutostart: () => invoke<boolean>("get_autostart"),
  setAutostart: (enabled) => invoke<void>("set_autostart", { enabled }),
  onUnlock: async (callback) => listen("system-unlock", () => callback()),
  openUrl: (url) => openUrl(assertHttps(url).toString()),
};
