import clsx from "clsx";
import { FONTS, THEMES, type ThemeChoice } from "@/design";
import type { FontStyle } from "@/api";

/**
 * Theme cards. Each card is drawn with its own theme's colours (the `.theme-<id>` classes in tokens.css redefine the
 * colour tokens inside the card), so what you see is what you get. Auto shows light and dark side by side.
 */
export function ThemePicker({ value, onChange }: { value: ThemeChoice; onChange: (v: ThemeChoice) => void }) {
  return (
    <div role="radiogroup" aria-label="Theme" className="grid grid-cols-3 gap-2.5 sm:grid-cols-4">
      {THEMES.map((t) => {
        const on = value === t.id;
        return (
          <button
            key={t.id}
            type="button"
            role="radio"
            aria-checked={on}
            aria-label={t.label}
            onClick={() => onChange(t.id)}
            className={clsx(
              "group flex flex-col items-stretch gap-1.5 rounded-card p-1 text-left outline-offset-2 transition-transform duration-150 ease-standard hover:scale-[1.03]",
              on ? "ring-2 ring-accent" : "ring-1 ring-separator",
            )}
          >
            {t.mode ? (
              <Swatch theme={t.id} mode={t.mode} />
            ) : (
              <span className="grid grid-cols-2 overflow-hidden rounded-[8px]">
                <Swatch theme="light" mode="light" half />
                <Swatch theme="dark" mode="dark" half />
              </span>
            )}
            <span className="px-1 pb-0.5 text-footnote font-medium text-fg">{t.label}</span>
          </button>
        );
      })}
    </div>
  );
}

/** A small picture of a theme: its background, a card with two lines of text, and its accent. */
function Swatch({ theme, mode, half }: { theme: string; mode: "light" | "dark"; half?: boolean }) {
  return (
    <span aria-hidden className={clsx(`theme-${mode} theme-${theme}`, "flex h-14 flex-col justify-end gap-1 bg-bg p-1.5", !half && "rounded-[8px]")}>
      <span className="flex items-center gap-1 rounded-[5px] bg-surface px-1.5 py-1 shadow-card">
        <span className="size-2 shrink-0 rounded-full bg-accent-fill" />
        <span className="text-caption leading-none font-semibold text-fg">Aa</span>
        {!half && <span className="truncate text-caption leading-none text-fg-2">9:30</span>}
      </span>
    </span>
  );
}

/** Font tiles, each written in its own font. */
export function FontPicker({ value, onChange }: { value: FontStyle; onChange: (v: FontStyle) => void }) {
  return (
    <div role="radiogroup" aria-label="Font" className="grid grid-cols-2 gap-2 sm:grid-cols-3">
      {FONTS.map((f) => {
        const on = value === f.id;
        return (
          <button
            key={f.id}
            type="button"
            role="radio"
            aria-checked={on}
            aria-label={f.label}
            onClick={() => onChange(f.id)}
            className={clsx(
              f.id !== "system" && `font-pick-${f.id}`,
              "flex items-baseline gap-2 rounded-control px-3 py-2 text-left transition-colors duration-150 ease-standard",
              on ? "bg-accent-soft text-fg ring-2 ring-accent" : "bg-fill text-fg hover:bg-fill-strong",
            )}
          >
            <span className="text-headline font-semibold">Aa</span>
            <span className="truncate text-body">{f.label}</span>
          </button>
        );
      })}
    </div>
  );
}
