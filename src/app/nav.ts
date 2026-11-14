import { create } from "zustand";

export type View = "briefing" | "accounts" | "calendar" | "settings" | "about";
export type CalendarMode = "day" | "week";

interface Nav {
  view: View;
  /** Which calendar view opens (set by the "default view" setting). */
  calendarMode: CalendarMode;
  /** The settings page that is open, or null for the settings list. Kept here so it survives a remount. */
  settingsSection: string | null;
  setSettingsSection: (s: string | null) => void;
  go: (v: View) => void;
  openCalendar: (mode: CalendarMode) => void;
}

export const useNav = create<Nav>((set) => ({
  view: "briefing",
  calendarMode: "day",
  settingsSection: null,
  setSettingsSection: (settingsSection) => set({ settingsSection }),
  // Arriving at Settings always starts at the list.
  go: (view) => set({ view, settingsSection: null }),
  openCalendar: (calendarMode) => set({ view: "calendar", calendarMode, settingsSection: null }),
}));
