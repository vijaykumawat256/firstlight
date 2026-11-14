/** Turns a key press into a global-shortcut string such as "Cmd+Shift+Space" (or null if it isn't usable). */

const NAMED: Record<string, string> = {
  " ": "Space", Enter: "Enter", Tab: "Tab", ArrowUp: "Up", ArrowDown: "Down", ArrowLeft: "Left", ArrowRight: "Right",
  Home: "Home", End: "End", PageUp: "PageUp", PageDown: "PageDown", Backspace: "Backspace", Delete: "Delete",
};

export interface KeyLike {
  key: string;
  code?: string;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
}

export function acceleratorFromEvent(e: KeyLike): string | null {
  // Letters and digits come from the physical key so Option/Shift don't change them ("Alt+K" is not "Alt+˚").
  let key: string | undefined;
  const code = e.code ?? "";
  if (/^Key[A-Z]$/.test(code)) key = code.slice(3);
  else if (/^Digit[0-9]$/.test(code)) key = code.slice(5);
  else if (/^F([1-9]|1[0-9]|2[0-4])$/.test(e.key)) key = e.key;
  else if (NAMED[e.key]) key = NAMED[e.key];
  else if (/^[a-z0-9]$/i.test(e.key)) key = e.key.toUpperCase();
  if (!key) return null;

  const mods = [e.metaKey && "Cmd", e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift"].filter(Boolean) as string[];
  // Shift alone would hijack ordinary typing; require a real modifier.
  if (!mods.some((m) => m !== "Shift")) return null;
  return [...mods, key].join("+");
}

const GLYPHS: Record<string, string> = { Cmd: "⌘", CmdOrCtrl: "⌘", CommandOrControl: "⌘", Ctrl: "⌃", Control: "⌃", Alt: "⌥", Option: "⌥", Shift: "⇧" };

/** "Cmd+Shift+Space" becomes "⌘⇧ Space". */
export function prettyAccelerator(s: string): string {
  const parts = s.split("+");
  const key = parts.pop() ?? "";
  return `${parts.map((p) => GLYPHS[p] ?? p).join("")} ${key}`.trim();
}
