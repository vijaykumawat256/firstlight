import type { CalendarInfo, CalEvent } from "./types";

/**
 * Everything the UI may ask of the outside world. The UI imports `api` and never
 * touches Tauri directly, so a web implementation can replace it later (PLAN.md, section 3).
 */
export interface Backend {
  /** Events touching `[startIso, endIso)`. */
  getEvents(startIso: string, endIso: string): Promise<CalEvent[]>;
  getCalendars(): Promise<CalendarInfo[]>;
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
