import "./design/tokens.css";
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { isTauri } from "./api";
import { FONTS, applyTheme, useTheme } from "./design";

if (isTauri) document.documentElement.setAttribute("data-shell", "tauri");
// Only macOS gets the see-through window (vibrancy); Windows gets an ordinary solid one.
if (isTauri && /Windows/i.test(navigator.userAgent)) document.documentElement.setAttribute("data-platform", "windows");
if (window.location.hash === "#/upnext") document.documentElement.setAttribute("data-view", "upnext");
applyTheme(useTheme.getState().choice);
// Font and text size from the last run, so the first paint does not change size when settings arrive.
try {
  const font = localStorage.getItem("firstlight.font");
  const size = localStorage.getItem("firstlight.textSize");
  if (font && font !== "system" && FONTS.some((f) => f.id === font)) document.documentElement.setAttribute("data-font", font);
  if (size && ["small", "large", "larger"].includes(size)) document.documentElement.setAttribute("data-text-size", size);
} catch {
  /* storage unavailable: settings apply them a moment later */
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
