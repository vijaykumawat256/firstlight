import { create } from "zustand";

export type View = "briefing" | "accounts";

interface Nav {
  view: View;
  go: (v: View) => void;
}

export const useNav = create<Nav>((set) => ({ view: "briefing", go: (view) => set({ view }) }));
