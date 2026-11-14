import { useQuery } from "@tanstack/react-query";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { useMemo, useState } from "react";
import { api, type CalEvent } from "@/api";
import { useNav } from "@/app/nav";
import { Button, IconButton, Segmented, Skeleton, useToasts } from "@/design";
import { EventDetail } from "@/features/briefing/EventDetail";
import { useNow } from "@/features/briefing/hooks";
import { TimeGrid } from "./TimeGrid";
import { localeFirstDay, sameDay, startOfLocalDay, startOfWeek } from "./layout";

type Mode = "day" | "week";

const dayFmt = new Intl.DateTimeFormat(undefined, { weekday: "long", day: "numeric", month: "long" });
const rangeFmt = new Intl.DateTimeFormat(undefined, { day: "numeric", month: "short" });
const yearFmt = new Intl.DateTimeFormat(undefined, { year: "numeric" });

export function CalendarScreen() {
  const go = useNav((s) => s.go);
  const push = useToasts((s) => s.push);
  const now = useNow(60_000);
  const [mode, setMode] = useState<Mode>("day");
  const [anchor, setAnchor] = useState(() => startOfLocalDay(new Date()));
  const [detail, setDetail] = useState<CalEvent | null>(null);

  const days = useMemo(() => {
    const first = mode === "week" ? startOfWeek(anchor, localeFirstDay()) : anchor;
    return Array.from({ length: mode === "week" ? 7 : 1 }, (_, i) => startOfLocalDay(first, i));
  }, [mode, anchor]);

  const from = days[0]!;
  const to = startOfLocalDay(days[days.length - 1]!, 1);
  const events = useQuery({ queryKey: ["events", from.toISOString(), to.toISOString()], queryFn: () => api.getEvents(from.toISOString(), to.toISOString()) });
  const calendars = useQuery({ queryKey: ["calendars"], queryFn: () => api.getCalendars() });
  const colours = useMemo(() => new Map((calendars.data ?? []).map((c) => [c.id, c.colour])), [calendars.data]);

  const step = (dir: -1 | 1) => setAnchor((a) => startOfLocalDay(a, dir * (mode === "week" ? 7 : 1)));
  const isCurrent = days.some((d) => sameDay(d, now));
  const title = mode === "day" ? dayFmt.format(from) : `${rangeFmt.format(from)} – ${rangeFmt.format(days[6]!)}, ${yearFmt.format(days[6]!)}`;
  const join = (url: string) => api.openUrl(url).catch(() => push({ title: "Couldn't open the link", tone: "danger" }));

  return (
    <div className="flex h-full flex-col">
      <div data-tauri-drag-region className="flex h-11 shrink-0 items-center px-3 pl-20">
        <Button variant="plain" size="sm" leading={<ChevronLeft className="size-4" />} onClick={() => go("briefing")}>
          Briefing
        </Button>
      </div>
      <header className="flex shrink-0 flex-wrap items-center justify-between gap-2 px-5 pb-3">
        <h1 aria-live="polite" className="text-title font-semibold tracking-tight text-fg">
          {title}
        </h1>
        <div className="flex items-center gap-2">
          <Segmented
            label="View"
            value={mode}
            onValueChange={(v) => setMode(v as Mode)}
            options={[
              { value: "day", label: "Day" },
              { value: "week", label: "Week" },
            ]}
          />
          <div className="flex items-center">
            <IconButton label={mode === "day" ? "Previous day" : "Previous week"} onClick={() => step(-1)}>
              <ChevronLeft className="size-[18px]" />
            </IconButton>
            <Button variant="plain" size="sm" disabled={isCurrent} onClick={() => setAnchor(startOfLocalDay(new Date()))}>
              Today
            </Button>
            <IconButton label={mode === "day" ? "Next day" : "Next week"} onClick={() => step(1)}>
              <ChevronRight className="size-[18px]" />
            </IconButton>
          </div>
        </div>
      </header>
      {events.isPending ? (
        <div className="flex flex-col gap-2 px-5" aria-busy aria-label="Loading">
          <Skeleton className="h-10" />
          <Skeleton className="h-64" />
        </div>
      ) : events.isError ? (
        <p className="px-5 text-body text-danger">Couldn’t load events. Try again in a moment.</p>
      ) : (
        <TimeGrid days={days} events={events.data} now={now} colourOf={(id) => colours.get(id) ?? "blue"} onOpen={setDetail} />
      )}
      <EventDetail event={detail} calendars={calendars.data ?? []} onClose={() => setDetail(null)} onJoin={join} />
    </div>
  );
}
