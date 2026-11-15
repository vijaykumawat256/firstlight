import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { api, type BriefingItem, type CalEvent } from "@/api";
import { Button, ListGroup, SectionHeader } from "@/design";
import { onDay } from "@/features/calendar/MonthGrid";
import { startOfLocalDay } from "@/features/calendar/layout";
import { isShown } from "@/features/calendar/TimeGrid";
import { TimelineRow, type ColourOf } from "./parts";

const STEP = 7;
const MAX_DAYS = 56; // a single request may cover about two months
const dayFmt = new Intl.DateTimeFormat(undefined, { weekday: "long", month: "long", day: "numeric" });

/** The same event as a briefing row: its start and end as instants. */
function asItem(e: CalEvent): BriefingItem | null {
  if (e.when.kind !== "timed") return null;
  return { event: e, start: e.when.start, end: e.when.end };
}

/**
 * The days after tomorrow, on request: "Show the next 7 days", then 7 more each time, up to eight weeks.
 * Closed by default so the briefing stays short; days with nothing on are left out.
 */
export function UpcomingDays({ colourOf, onOpen, onJoin }: { colourOf: ColourOf; onOpen: (e: CalEvent) => void; onJoin: (url: string) => void }) {
  const [days, setDays] = useState(0);
  const today = startOfLocalDay(new Date());
  const from = startOfLocalDay(today, 2);
  const to = startOfLocalDay(today, 2 + Math.max(days, 1));
  const events = useQuery({
    queryKey: ["upcoming-days", from.toISOString(), days],
    queryFn: () => api.getEvents(from.toISOString(), to.toISOString()),
    enabled: days > 0,
  });

  if (days === 0) {
    return (
      <div>
        <Button variant="plain" onClick={() => setDays(STEP)}>
          Show the next {STEP} days
        </Button>
      </div>
    );
  }

  const groups = Array.from({ length: days }, (_, i) => startOfLocalDay(from, i))
    .map((day) => ({
      day,
      events: (events.data ?? [])
        .filter((e) => isShown(e) && onDay(e, day))
        .sort((a, b) => Number(b.when.kind === "all_day") - Number(a.when.kind === "all_day") || a.when.start.localeCompare(b.when.start)),
    }))
    .filter((g) => g.events.length > 0);

  return (
    <section aria-label="Coming up">
      <SectionHeader title="Coming up" trailing={`the ${days} days after tomorrow`} />
      {events.isPending ? (
        <p className="px-1 text-footnote text-fg-2" role="status">
          Loading…
        </p>
      ) : events.isError ? (
        <p className="px-1 text-body text-danger">Couldn’t load those days. Try again in a moment.</p>
      ) : groups.length === 0 ? (
        <p className="px-1 text-body text-fg-2">Nothing scheduled in these {days} days.</p>
      ) : (
        <div className="flex flex-col gap-4">
          {groups.map(({ day, events: list }) => (
            <section key={day.toISOString()} aria-label={dayFmt.format(day)}>
              <h3 className="mb-1.5 px-1 text-footnote font-semibold text-fg-2">{dayFmt.format(day)}</h3>
              <ListGroup>
                {list.map((e) => {
                  const item = asItem(e);
                  return item ? (
                    <TimelineRow key={e.id} item={item} colourOf={colourOf} onOpen={onOpen} onJoin={onJoin} />
                  ) : (
                    <li key={e.id}>
                      <button type="button" onClick={() => onOpen(e)} className="flex min-h-12 w-full items-center gap-3 px-4 py-2 text-left transition-colors duration-150 ease-standard hover:bg-fill active:bg-fill-strong">
                        <span className="w-[68px] shrink-0 text-body font-medium text-fg-2">All day</span>
                        <span className="truncate text-headline font-medium text-fg">{e.title}</span>
                      </button>
                    </li>
                  );
                })}
              </ListGroup>
            </section>
          ))}
        </div>
      )}
      <div className="mt-2 flex flex-wrap items-center gap-1">
        {days < MAX_DAYS && (
          <Button variant="plain" onClick={() => setDays((d) => Math.min(d + STEP, MAX_DAYS))}>
            Show {STEP} more days
          </Button>
        )}
        <Button variant="plain" onClick={() => setDays(0)}>
          Hide
        </Button>
      </div>
    </section>
  );
}
