import { isTauri } from "@/api";

/**
 * True inside the iPhone or iPad app. A phone has no Dock, menu bar, system-wide shortcuts, start at login, unlock
 * event or floating windows, so the controls for those are hidden there. An iPad's web view can call itself a Mac,
 * which is why touch points are checked too (a Mac reports none). `?phone` turns it on in the browser for tests.
 */
export const isPhone =
  (typeof window !== "undefined" && new URLSearchParams(window.location.search).has("phone")) ||
  (isTauri && typeof navigator !== "undefined" && navigator.maxTouchPoints > 1 && /iPhone|iPad|Macintosh/.test(navigator.userAgent));
