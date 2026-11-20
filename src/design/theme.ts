import { create } from "zustand";

/** Every theme, in the order Settings shows them. `mode` is the base it builds on (tokens.css). */
export const THEMES = [
  { id: "system", label: "Auto", mode: null },
  { id: "light", label: "Light", mode: "light" },
  { id: "warm", label: "Warm", mode: "light" },
  { id: "mint", label: "Mint", mode: "light" },
  { id: "sky", label: "Sky", mode: "light" },
  { id: "lavender", label: "Lavender", mode: "light" },
  { id: "rose", label: "Rose", mode: "light" },
  { id: "dark", label: "Dark", mode: "dark" },
  { id: "black", label: "Black", mode: "dark" },
  { id: "midnight", label: "Midnight", mode: "dark" },
  { id: "forest", label: "Forest", mode: "dark" },
  { id: "grape", label: "Grape", mode: "dark" },
  { id: "mocha", label: "Mocha", mode: "dark" },
  { id: "slate", label: "Slate", mode: "dark" },
] as const;

export type ThemeChoice = (typeof THEMES)[number]["id"];
const CHOICES: readonly string[] = THEMES.map((t) => t.id);

/** Every font choice; tokens.css holds each one's font list (installed fonts only). */
export const FONTS = [
  { id: "system", label: "System" },
  { id: "rounded", label: "Rounded" },
  { id: "serif", label: "Serif" },
  { id: "mono", label: "Monospace" },
  { id: "avenir", label: "Avenir" },
  { id: "helvetica", label: "Helvetica" },
  { id: "futura", label: "Futura" },
  { id: "optima", label: "Optima" },
  { id: "georgia", label: "Georgia" },
  { id: "palatino", label: "Palatino" },
  { id: "baskerville", label: "Baskerville" },
  { id: "verdana", label: "Verdana" },
  { id: "trebuchet", label: "Trebuchet" },
  { id: "typewriter", label: "Typewriter" },
] as const;

/** Light or dark for a theme; null = follow the system. */
export const themeMode = (choice: ThemeChoice): "light" | "dark" | null => THEMES.find((t) => t.id === choice)?.mode ?? null;

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
  const mode = themeMode(choice);
  if (!mode) {
    root.removeAttribute("data-theme");
    root.removeAttribute("data-mode");
  } else {
    root.setAttribute("data-theme", choice);
    root.setAttribute("data-mode", mode);
  }
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
