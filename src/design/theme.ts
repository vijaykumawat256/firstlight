import { create } from "zustand";

export type ThemeChoice = "system" | "light" | "dark" | "warm" | "black";
const CHOICES: readonly string[] = ["system", "light", "dark", "warm", "black"];

const STORAGE_KEY = "firstlight.theme";

function read(): ThemeChoice {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v && CHOICES.includes(v)) return v as ThemeChoice;
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
