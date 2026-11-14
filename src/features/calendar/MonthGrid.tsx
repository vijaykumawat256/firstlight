import clsx from "clsx";
import { useMemo } from "react";
import type { CalEvent } from "@/api";
import { calendarColour } from "@/design";
import { formatTime } from "@/features/briefing/format";
import { isShown } from "./TimeGrid";
import { localeFirstDay, sameDay, startOfLocalDay, startOfWeek } from "./layout";

const MAX_CHIPS = 3;

/** Whether an event touches the given local day (all-day events end on an exclusive date). */
export function onDay(e: CalEvent, day: Date): boolean {
  const next = startOfLocalDay(day, 1);
  if (e.when.kind === "all_day") {
    const s = new Date(`${e.when.start}T00:00:00`);
    const en = new Date(`${e.when.end}T00:00:00`);
    return s < next && en > day;
  }
  return new Date(e.when.start) < next && new Date(e.when.end) > day;
}

/** The weeks shown for the month containing `anchor`: whole weeks, from the week start. */
export function monthWeeks(anchor: Date, firstDay: number): Date[][] {
  const first = new Date(anchor.getFullYear(), anchor.getMonth(), 1);
  const last = new Date(anchor.getFullYear(), anchor.getMonth() + 1, 0);
  const weeks: Date[][] = [];
  for (let w = startOfWeek(first, firstDay); w <= last; w = startOfLocalDay(w, 7)) {
    weeks.push(Array.from({ length: 7 }, (_, i) => startOfLocalDay(w, i)));
  }
  return weeks;
}

const weekday = new Intl.DateTimeFormat(undefined, { weekday: "short" });

export function MonthGrid({ anchor, events, now, colourOf, onPickDay, onOpen }: { anchor: Date; events: CalEvent[]; now: Date; colourOf: (calendarId: string) => string; onPickDay: (d: Date) => void; onOpen: (e: CalEvent) => void }) {
  const weeks = useMemo(() => monthWeeks(anchor, localeFirstDay()), [anchor]);
  const shown = useMemo(() => events.filter(isShown), [events]);
  return (
    <div role="grid" aria-label="Month" className="flex min-h-0 flex-1 flex-col overflow-y-auto px-3 pb-6">
      <div role="row" className="grid grid-cols-7 pb-1">
        {weeks[0]!.map((d) => (
          <div key={d.getDay()} role="columnheader" className="px-1 text-center text-caption font-medium text-fg-2">
            {weekday.format(d)}
          </div>
        ))}
      </div>
      <div className="flex flex-1 flex-col gap-px overflow-hidden rounded-card bg-separator">
        {weeks.map((week) => (
          <div key={week[0]!.toISOString()} role="row" className="grid min-h-24 flex-1 grid-cols-7 gap-px">
            {week.map((d) => {
              const todays = shown.filter((e) => onDay(e, d));
              const inMonth = d.getMonth() === anchor.getMonth();
              const today = sameDay(d, now);
              return (
                <div key={d.toISOString()} role="gridcell" className={clsx("flex min-w-0 flex-col gap-0.5 bg-surface p-1", !inMonth && "opacity-50")}>
                  <button
                    type="button"
                    onClick={() => onPickDay(d)}
                    aria-label={`${d.toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "long" })}, ${todays.length} ${todays.length === 1 ? "event" : "events"}`}
                    className={clsx("flex min-h-6 min-w-6 items-center justify-center self-start rounded-full px-1.5 text-footnote tabular-nums hover:bg-fill", today ? "bg-accent-fill font-semibold text-accent-fg" : "text-fg")}
                  >
                    {d.getDate()}
                  </button>
                  {todays.slice(0, MAX_CHIPS).map((e) => (
                    <button
                      key={e.id + d.toDateString()}
                      type="button"
                      onClick={() => onOpen(e)}
                      title={e.title}
                      className="flex min-h-6 min-w-0 items-center gap-1 rounded px-1 text-left text-caption text-fg hover:bg-fill"
                    >
                      <span aria-hidden className="size-1.5 shrink-0 rounded-full" style={{ background: calendarColour(colourOf(e.calendar_id)) }} />
                      <span className="truncate">
                        {e.when.kind === "timed" && sameDay(new Date(e.when.start), d) ? `${formatTime(new Date(e.when.start))} ` : ""}
                        {e.title}
                      </span>
                    </button>
                  ))}
                  {todays.length > MAX_CHIPS && (
                    <button type="button" onClick={() => onPickDay(d)} className="min-h-6 px-1 text-left text-caption text-fg-2 hover:underline">
                      +{todays.length - MAX_CHIPS} more
                    </button>
                  )}
                </div>
              );
            })}
          </div>
        ))}
      </div>
    </div>
  );
}
