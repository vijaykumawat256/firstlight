import * as RToggleGroup from "@radix-ui/react-toggle-group";
import clsx from "clsx";
import { useEffect, useState, type ReactNode } from "react";
import { Button, Select } from "@/design";
import { formatHour } from "@/features/briefing/format";
import { acceleratorFromEvent, prettyAccelerator } from "./accelerator";

/** One line of a settings group: a title, an optional explanation, and the control on the right. */
export function SettingRow({ title, description, children, stack }: { title: string; description?: string; children?: ReactNode; stack?: boolean }) {
  return (
    <li className={clsx("flex min-h-12 gap-3 px-4 py-2.5", stack ? "flex-col" : "items-center justify-between")}>
      <div className="min-w-0">
        <p className="text-headline font-medium text-fg">{title}</p>
        {description && <p className="mt-0.5 text-footnote text-fg-2">{description}</p>}
      </div>
      {children && <div className={clsx("shrink-0", stack && "self-start")}>{children}</div>}
    </li>
  );
}

export function HourSelect({ value, onChange, from, to, label }: { value: number; onChange: (h: number) => void; from: number; to: number; label: string }) {
  return (
    <Select
      label={label}
      value={String(value)}
      onValueChange={(v) => onChange(Number(v))}
      options={Array.from({ length: to - from + 1 }, (_, i) => ({ value: String(from + i), label: formatHour(from + i) }))}
    />
  );
}

export function NumberSelect({ value, onChange, options, label, unit }: { value: number; onChange: (n: number) => void; options: number[]; label: string; unit: string }) {
  // Keep an unusual stored value visible instead of showing an empty control.
  const all = options.includes(value) ? options : [...options, value].sort((a, b) => a - b);
  return <Select label={label} value={String(value)} onValueChange={(v) => onChange(Number(v))} options={all.map((n) => ({ value: String(n), label: `${n} ${unit}` }))} />;
}

const DAYS = [
  { full: "Monday", short: "M" },
  { full: "Tuesday", short: "T" },
  { full: "Wednesday", short: "W" },
  { full: "Thursday", short: "T" },
  { full: "Friday", short: "F" },
  { full: "Saturday", short: "S" },
  { full: "Sunday", short: "S" },
];

/** Seven toggles, Monday first, matching how the backend stores the days. */
export function DayPicker({ days, onChange }: { days: boolean[]; onChange: (d: boolean[]) => void }) {
  const on = DAYS.flatMap((d, i) => (days[i] ? [d.full] : []));
  return (
    <RToggleGroup.Root
      type="multiple"
      value={on}
      aria-label="Days"
      onValueChange={(vals) => onChange(DAYS.map((d) => vals.includes(d.full)))}
      className="flex gap-1"
    >
      {DAYS.map((d) => (
        <RToggleGroup.Item
          key={d.full}
          value={d.full}
          aria-label={d.full}
          className="flex size-8 items-center justify-center rounded-full bg-fill text-body font-medium text-fg-2 transition-colors duration-150 ease-standard data-[state=on]:bg-accent data-[state=on]:text-accent-fg"
        >
          {d.short}
        </RToggleGroup.Item>
      ))}
    </RToggleGroup.Root>
  );
}

/** Click, then press the keys you want. Escape cancels. */
export function ShortcutRecorder({ value, onChange }: { value: string | null; onChange: (s: string | null) => void }) {
  const [recording, setRecording] = useState(false);
  const [hint, setHint] = useState<string | null>(null);

  useEffect(() => {
    if (!recording) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        setRecording(false);
        setHint(null);
        return;
      }
      if (["Meta", "Control", "Alt", "Shift"].includes(e.key)) return; // wait for the real key
      const acc = acceleratorFromEvent(e);
      if (!acc) {
        setHint("Include Cmd, Ctrl or Alt, plus a letter, number or F-key.");
        return;
      }
      setRecording(false);
      setHint(null);
      onChange(acc);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [recording, onChange]);

  return (
    <div className="flex flex-col items-end gap-1">
      <div className="flex items-center gap-2">
        <Button size="sm" variant="secondary" aria-label="Global shortcut" aria-pressed={recording} onClick={() => setRecording((r) => !r)} className="min-w-28 tabular-nums">
          {recording ? "Press keys…" : value ? prettyAccelerator(value) : "Not set"}
        </Button>
        {value && !recording && (
          <Button size="sm" variant="plain" onClick={() => onChange(null)}>
            Clear
          </Button>
        )}
      </div>
      {recording && hint && (
        <p role="status" className="max-w-[26ch] text-right text-footnote text-fg-2">
          {hint}
        </p>
      )}
    </div>
  );
}
