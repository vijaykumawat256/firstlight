import { isTauri } from "@/api";

const ua = typeof navigator !== "undefined" ? navigator.userAgent : "";
const query = typeof window !== "undefined" ? new URLSearchParams(window.location.search) : new URLSearchParams();

/** The Android app (any maker: Google, Samsung and the rest share the system web view). `?android` in the browser. */
export const isAndroid = query.has("android") || (isTauri && /Android/.test(ua));

/**
 * The iPhone or iPad app. An iPad's web view can call itself a Mac, which is why touch points are checked too (a Mac
 * reports none). `?phone` in the browser.
 */
export const isIos = !isAndroid && (query.has("phone") || (isTauri && typeof navigator !== "undefined" && navigator.maxTouchPoints > 1 && /iPhone|iPad|Macintosh/.test(ua)));

/**
 * A phone or tablet app, either system. A phone has no Dock, menu bar, system-wide shortcuts, start at login, unlock
 * event or floating windows, so the controls for those are hidden there.
 */
export const isPhone = isIos || isAndroid;
