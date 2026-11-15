import "./design/tokens.css";
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { isTauri } from "./api";
import { applyTheme, useTheme } from "./design";

if (isTauri) document.documentElement.setAttribute("data-shell", "tauri");
// Only macOS gets the see-through window (vibrancy); Windows gets an ordinary solid one.
if (isTauri && /Windows/i.test(navigator.userAgent)) document.documentElement.setAttribute("data-platform", "windows");
if (window.location.hash === "#/upnext") document.documentElement.setAttribute("data-view", "upnext");
applyTheme(useTheme.getState().choice);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
