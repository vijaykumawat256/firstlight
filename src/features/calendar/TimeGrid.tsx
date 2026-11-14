import clsx from "clsx";
import { useEffect, useMemo, useRef } from "react";
import type { CalEvent } from "@/api";
import { calendarColour } from "@/design";
import { formatTime } from "@/features/briefing/format";
import { layoutColumn, sameDay, startOfLocalDay } from "./layout";

const HOUR = 52; // px per hour
const GUTTER = 48; // px for the hour labels

// Hour-only labels ("10 AM", or "10" on a 24-hour clock) fit the narrow gutter and read like Apple's Calendar.
const hourFmt = new Intl.DateTimeFormat(undefined, { hour: "numeric" });
const hourLabel = (h: number) => hourFmt.format(new Date(2000, 0, 1, h));
const dayName = new Intl.DateTimeFormat(undefined, { weekday: "short" });

/** Events a calendar view shows: cancelled and declined stay hidden, as in the briefing. */
export const isShown = (e: CalEvent) => e.status !== "cancelled" && e.my_response !== "declined";

function allDayOn(events: CalEvent[], day: Date): CalEvent[] {
  const key = `${day.getFullYear()}-${String(day.getMonth() + 1).padStart(2, "0")}-${String(day.getDate()).padStart(2, "0")}`;
  return events.filter((e) => e.when.kind === "all_day" && e.when.start <= key && key < e.when.end);
}

interface Props {
  days: Date[];
  events: CalEvent[];
  now: Date;
  colourOf: (calendarId: string) => string;
  onOpen: (e: CalEvent) => void;
}

export function TimeGrid({ days, events, now, colourOf, onOpen }: Props) {
  const scroller = useRef<HTMLDivElement>(null);
  const shown = useMemo(() => events.filter(isShown), [events]);
  const timed = useMemo(() => shown.filter((e) => e.when.kind === "timed"), [shown]);
  const compact = days.length > 1;
  const hasAllDay = days.some((d) => allDayOn(shown, d).length > 0);

  // Open scrolled to just before "now" (or 8 AM when looking at another day).
  const showsToday = days.some((d) => sameDay(d, now));
  useEffect(() => {
    const top = (showsToday ? Math.max(0, now.getHours() - 1) : 8) * HOUR;
    scroller.current?.scrollTo({ top });
    // Only when the range changes, not every minute.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [days[0]?.getTime(), days.length]);

  return (
    <div className="flex min-h-0 flex-1 flex-col" role="grid" aria-label={compact ? "Week" : "Day"}>
      {/* Day headers */}
      <div className="flex shrink-0 border-b border-separator" style={{ paddingLeft: GUTTER }}>
        {days.map((d) => {
          const today = sameDay(d, now);
          return (
            <div key={d.getTime()} role="columnheader" className="flex-1 py-1.5 text-center">
              <span className={clsx("text-caption font-medium", today ? "text-accent" : "text-fg-2")}>{dayName.format(d)}</span>
              <span className={clsx("mx-auto mt-0.5 flex size-6 items-center justify-center rounded-full text-headline font-semibold tabular-nums", today ? "bg-accent text-accent-fg" : "text-fg")}>
                {d.getDate()}
              </span>
            </div>
          );
        })}
      </div>

      {/* All-day strip */}
      {hasAllDay && (
        <div className="flex shrink-0 border-b border-separator py-1" style={{ paddingLeft: GUTTER }}>
          {days.map((d) => (
            <div key={d.getTime()} className="flex min-w-0 flex-1 flex-col gap-0.5 px-0.5">
              {allDayOn(shown, d).map((e) => (
                <button
                  key={e.id}
                  type="button"
                  onClick={() => onOpen(e)}
                  className="truncate rounded-[5px] px-1.5 py-0.5 text-left text-caption font-medium text-fg"
                  style={{ background: `color-mix(in srgb, ${calendarColour(colourOf(e.calendar_id))} 22%, transparent)` }}
                >
                  {e.title}
                </button>
              ))}
            </div>
          ))}
        </div>
      )}

      {/* Hours */}
      <div ref={scroller} className="min-h-0 flex-1 overflow-y-auto overflow-x-auto">
        <div className="relative flex" style={{ height: HOUR * 24, minWidth: compact ? 7 * 56 + GUTTER : undefined }}>
          <div className="relative shrink-0" style={{ width: GUTTER }} aria-hidden>
            {Array.from({ length: 24 }, (_, h) => (
              <span key={h} className="absolute right-2 -translate-y-1/2 whitespace-nowrap text-caption text-fg-3 tabular-nums" style={{ top: h * HOUR, display: h === 0 ? "none" : undefined }}>
                {hourLabel(h)}
              </span>
            ))}
          </div>
          <div className="relative flex flex-1">
            {/* hour lines */}
            {Array.from({ length: 24 }, (_, h) => (
              <div key={h} aria-hidden className="pointer-events-none absolute inset-x-0 border-t border-separator" style={{ top: h * HOUR }} />
            ))}
            {days.map((d) => {
              const dayStart = startOfLocalDay(d);
              const items = timed.map((e) => ({ id: e.id, start: new Date((e.when as { start: string }).start), end: new Date((e.when as { end: string }).end) }));
              const placed = layoutColumn(items, d);
              const byId = new Map(timed.map((e) => [e.id, e]));
              const today = sameDay(d, now);
              return (
                <div key={d.getTime()} role="gridcell" aria-label={d.toDateString()} className={clsx("relative flex-1 border-l border-separator first:border-l-0", today && "bg-accent-soft/30")}>
                  {placed.map((p) => {
                    const e = byId.get(p.id)!;
                    const colour = calendarColour(colourOf(e.calendar_id));
                    return (
                      <button
                        key={p.id}
                        type="button"
                        onClick={() => onOpen(e)}
                        title={e.title}
                        className={clsx(
                          "absolute overflow-hidden rounded-[6px] border-l-[3px] px-1.5 py-0.5 text-left transition-[filter] duration-150 ease-standard hover:brightness-95",
                          e.status === "tentative" && "opacity-70",
                        )}
                        style={{
                          top: (p.top / 60) * HOUR,
                          height: Math.max((p.height / 60) * HOUR - 2, 18),
                          left: `calc(${(p.col / p.cols) * 100}% + 1px)`,
                          width: `calc(${100 / p.cols}% - 2px)`,
                          background: `color-mix(in srgb, ${colour} 18%, var(--surface))`,
                          borderLeftColor: colour,
                        }}
                      >
                        <span className="block truncate text-caption font-semibold text-fg">{e.title}</span>
                        {!compact && p.height >= 40 && (
                          <span className="block truncate text-caption text-fg-2 tabular-nums">{formatTime(new Date((e.when as { start: string }).start))}</span>
                        )}
                      </button>
                    );
                  })}
                  {today && (
                    <div aria-label={`Now, ${formatTime(now)}`} className="pointer-events-none absolute inset-x-0 z-10 flex items-center" style={{ top: ((now.getHours() * 60 + now.getMinutes()) / 60) * HOUR }}>
                      <span className="-ml-1 size-2 rounded-full bg-accent" />
                      <span className="h-px flex-1 bg-accent" />
                    </div>
                  )}
                  <span className="sr-only">{dayStart.toDateString()}</span>
                </div>
              );
            })}
          </div>
        </div>
      </div>
    </div>
  );
}
