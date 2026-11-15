import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Fragment, useEffect, type ReactNode } from "react";
import { api } from "@/api";
import { applyTheme } from "@/design";
import { CALENDAR_COLOURS } from "@/design/colours";
import { setTimeFormat } from "@/features/briefing/format";
import { setWeekStart } from "@/features/calendar/layout";
import { useNav } from "@/app/nav";

/** Remembers the theme so the next launch paints correctly before settings have loaded. */
const THEME_CACHE = "firstlight.theme";

/** Which part of the day it is, for the optional background tint. */
export function periodOf(d: Date): "morning" | "day" | "evening" | "night" {
  const h = d.getHours();
  if (h >= 5 && h < 11) return "morning";
  if (h >= 11 && h < 17) return "day";
  if (h >= 17 && h < 21) return "evening";
  return "night";
}

function applyAccent(accent: string) {
  const root = document.documentElement;
  // The stylesheet derives readable text and fill colours from the palette colour (see tokens.css).
  if (accent === "system" || !(CALENDAR_COLOURS as readonly string[]).includes(accent)) root.removeAttribute("data-accent");
  else root.setAttribute("data-accent", accent);
}

/**
 * Applies the settings that change how the whole app looks or reads: theme, accent colour, clock format
 * and week start. It also opens the default view once. Settings changes made anywhere (including from
 * the other window) arrive here and take effect with no restart.
 */
export function SettingsApplier({ children }: { children: ReactNode }) {
  const qc = useQueryClient();
  const { data: settings } = useQuery({ queryKey: ["settings"], queryFn: () => api.getSettings(), staleTime: Infinity });

  useEffect(() => {
    let off: (() => void) | undefined;
    let cancelled = false;
    void api.onSettingsChanged(() => void qc.invalidateQueries({ queryKey: ["settings"] })).then((u) => (cancelled ? u() : (off = u)));
    return () => {
      cancelled = true;
      off?.();
    };
  }, [qc]);

  const theme = settings?.appearance.theme;
  useEffect(() => {
    if (!theme) return;
    applyTheme(theme);
    try {
      localStorage.setItem(THEME_CACHE, theme);
    } catch {
      /* storage unavailable: the cache is only a convenience */
    }
  }, [theme]);

  // "Default" follows the system accent colour (read again whenever the window is focused, so changing it
  // in System Settings shows up without a restart); any other choice is used as it is.
  const accent = settings?.appearance.accent;
  const system = useQuery({ queryKey: ["system-accent"], queryFn: () => api.getSystemAccent(), refetchOnWindowFocus: true, staleTime: 30_000 });
  const effectiveAccent = accent === "system" ? (system.data ?? "system") : accent;
  useEffect(() => {
    if (effectiveAccent) applyAccent(effectiveAccent);
  }, [effectiveAccent]);

  // Optional time-of-day tint behind the app.
  const ambient = settings?.appearance.ambient;
  useEffect(() => {
    const root = document.documentElement;
    if (!ambient) {
      root.removeAttribute("data-ambient");
      return;
    }
    const set = () => root.setAttribute("data-ambient", periodOf(new Date()));
    set();
    const timer = setInterval(set, 5 * 60_000);
    return () => clearInterval(timer);
  }, [ambient]);

  // Clock format and week start are read while rendering, so remount the tree when they change.
  const timeFormat = settings?.general.time_format ?? "auto";
  const weekStart = settings?.general.week_start ?? "auto";
  setTimeFormat(timeFormat);
  setWeekStart(weekStart);

  // "Opens to": once when the app starts, and again each time an unlock brings the window back,
  // but never as a side effect of changing the setting itself.
  const defaultView = settings?.general.default_view;
  const loaded = defaultView !== undefined;
  useEffect(() => {
    if (!loaded || window.location.hash === "#/upnext" || sessionStorageFlag("firstlight.opened")) return;
    goToDefaultView(qc);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only the first time settings are available
  }, [loaded]);
  useEffect(() => {
    if (window.location.hash === "#/upnext") return;
    let off: (() => void) | undefined;
    let cancelled = false;
    void api.onUnlock(() => goToDefaultView(qc)).then((u) => (cancelled ? u() : (off = u)));
    return () => {
      cancelled = true;
      off?.();
    };
  }, [qc]);

  return <Fragment key={`${timeFormat}-${weekStart}`}>{children}</Fragment>;
}

function goToDefaultView(qc: ReturnType<typeof useQueryClient>) {
  const view = qc.getQueryData<{ general: { default_view: "briefing" | "day" | "week" | "month" } }>(["settings"])?.general.default_view ?? "briefing";
  if (view === "briefing") useNav.getState().go("briefing");
  else useNav.getState().openCalendar(view);
}

/** True if the flag was already set; sets it. Tolerates storage being unavailable. */
function sessionStorageFlag(key: string): boolean {
  try {
    if (sessionStorage.getItem(key)) return true;
    sessionStorage.setItem(key, "1");
  } catch {
    /* ignore */
  }
  return false;
}
