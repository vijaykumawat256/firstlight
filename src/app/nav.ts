import { create } from "zustand";

export type View = "briefing" | "accounts" | "calendar" | "settings" | "about" | "tasks";
export type CalendarMode = "day" | "week" | "month";

interface Nav {
  view: View;
  /** Which calendar view opens (set by the "default view" setting). */
  calendarMode: CalendarMode;
  /** A day to open the calendar on (from search); null means today. */
  calendarDate: Date | null;
  /** The settings page that is open, or null for the settings list. Kept here so it survives a remount. */
  settingsSection: string | null;
  setSettingsSection: (s: string | null) => void;
  go: (v: View) => void;
  openCalendar: (mode: CalendarMode, date?: Date) => void;
}

export const useNav = create<Nav>((set) => ({
  view: "briefing",
  calendarMode: "day",
  calendarDate: null,
  settingsSection: null,
  setSettingsSection: (settingsSection) => set({ settingsSection }),
  // Arriving at Settings always starts at the list.
  go: (view) => set({ view, settingsSection: null, calendarDate: null }),
  openCalendar: (calendarMode, date) => set({ view: "calendar", calendarMode, calendarDate: date ?? null, settingsSection: null }),
}));
