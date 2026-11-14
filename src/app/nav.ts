import { create } from "zustand";

export type View = "briefing" | "accounts" | "calendar" | "settings" | "about";
export type CalendarMode = "day" | "week";

interface Nav {
  view: View;
  /** Which calendar view opens (set by the "default view" setting). */
  calendarMode: CalendarMode;
  go: (v: View) => void;
  openCalendar: (mode: CalendarMode) => void;
}

export const useNav = create<Nav>((set) => ({
  view: "briefing",
  calendarMode: "day",
  go: (view) => set({ view }),
  openCalendar: (calendarMode) => set({ view: "calendar", calendarMode }),
}));
