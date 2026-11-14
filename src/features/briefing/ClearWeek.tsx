import { useQuery } from "@tanstack/react-query";
import { CalendarX } from "lucide-react";
import { api, type CalEvent } from "@/api";
import { useNav } from "@/app/nav";
import { Button, EmptyState, ListGroup, SectionHeader, calendarColour } from "@/design";
import { isShown } from "@/features/calendar/TimeGrid";
import { startOfLocalDay } from "@/features/calendar/layout";
import { formatTime } from "./format";
import type { ColourOf } from "./parts";

const dayFmt = new Intl.DateTimeFormat(undefined, { weekday: "long", day: "numeric", month: "short" });
const DAYS_AHEAD = 14;
const MAX_SHOWN = 6;

const startOf = (e: CalEvent) => (e.when.kind === "all_day" ? new Date(`${e.when.start}T00:00:00`) : new Date(e.when.start));

/** Shown when today and tomorrow are both empty: what is coming up in the next two weeks instead. */
export function ClearWeek({ colourOf, onOpen }: { colourOf: ColourOf; onOpen: (e: CalEvent) => void }) {
  const openCalendar = useNav((s) => s.openCalendar);
  const from = startOfLocalDay(new Date(), 2);
  const to = startOfLocalDay(new Date(), DAYS_AHEAD);
  const upcoming = useQuery({ queryKey: ["clear-week", from.toISOString()], queryFn: () => api.getEvents(from.toISOString(), to.toISOString()) });
  const events = (upcoming.data ?? []).filter(isShown).sort((a, b) => startOf(a).getTime() - startOf(b).getTime());

  if (events.length === 0) {
    return (
      <EmptyState
        icon={<CalendarX />}
        title="Your day is clear"
        description="Nothing today or tomorrow, and nothing in the next two weeks."
        action={
          <Button variant="secondary" onClick={() => openCalendar("month")}>
            Open the month
          </Button>
        }
      />
    );
  }
  return (
    <div className="flex flex-col gap-4">
      <div>
        <h2 className="text-headline font-semibold text-fg">Nothing today or tomorrow</h2>
        <p className="mt-1 text-body text-fg-2">Here is what is coming up.</p>
      </div>
      <section aria-label="Coming up">
        <SectionHeader title="Coming up" />
        <ListGroup>
          {events.slice(0, MAX_SHOWN).map((e) => (
            <li key={e.id}>
              <button type="button" onClick={() => onOpen(e)} className="flex min-h-12 w-full items-center gap-3 px-4 py-2.5 text-left transition-colors duration-150 ease-standard hover:bg-fill active:bg-fill-strong">
                <span aria-hidden className="w-[3px] self-stretch rounded-full" style={{ background: calendarColour(colourOf(e.calendar_id)) }} />
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-headline font-medium text-fg">{e.title}</span>
                  <span className="block text-footnote text-fg-2 tabular-nums">
                    {dayFmt.format(startOf(e))} · {e.when.kind === "all_day" ? "All day" : formatTime(startOf(e))}
                  </span>
                </span>
              </button>
            </li>
          ))}
        </ListGroup>
      </section>
      <div className="flex gap-2">
        <Button variant="secondary" onClick={() => openCalendar("week")}>
          See the week
        </Button>
        <Button variant="plain" onClick={() => openCalendar("month")}>
          See the month
        </Button>
      </div>
    </div>
  );
}
