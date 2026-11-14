import { create } from "zustand";

export type ThemeChoice = "system" | "light" | "dark";

const STORAGE_KEY = "daybrief.theme";

function read(): ThemeChoice {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v === "light" || v === "dark" || v === "system") return v;
  } catch {
    /* storage unavailable: fall through */
  }
  return "system";
}

export function applyTheme(choice: ThemeChoice) {
  const root = document.documentElement;
  if (choice === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", choice);
}

interface ThemeStore {
  choice: ThemeChoice;
  set: (c: ThemeChoice) => void;
}

export const useTheme = create<ThemeStore>((set) => ({
  choice: read(),
  set: (choice) => {
    try {
      localStorage.setItem(STORAGE_KEY, choice);
    } catch {
      /* ignore */
    }
    applyTheme(choice);
    set({ choice });
  },
}));
