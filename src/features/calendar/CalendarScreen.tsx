import { useQuery } from "@tanstack/react-query";
import { ChevronLeft, ChevronRight, Plus } from "lucide-react";
import { useMemo, useState } from "react";
import { api, type CalEvent } from "@/api";
import { useNav } from "@/app/nav";
import { Button, IconButton, Segmented, Skeleton, useToasts } from "@/design";
import { EventDetail } from "@/features/briefing/EventDetail";
import { EventForm } from "@/features/events/EventForm";
import { useNow } from "@/features/briefing/hooks";
import { useSettings } from "@/features/settings/useSettings";
import { MonthGrid, monthWeeks } from "./MonthGrid";
import { TimeGrid } from "./TimeGrid";
import { localeFirstDay, sameDay, startOfLocalDay, startOfWeek } from "./layout";
import { isPhone } from "@/app/platform";
import { Fab } from "@/app/Fab";

type Mode = "day" | "week" | "month";

const dayFmt = new Intl.DateTimeFormat(undefined, { weekday: "long", day: "numeric", month: "long" });
const rangeFmt = new Intl.DateTimeFormat(undefined, { day: "numeric", month: "short" });
const yearFmt = new Intl.DateTimeFormat(undefined, { year: "numeric" });
const monthFmt = new Intl.DateTimeFormat(undefined, { month: "long", year: "numeric" });

export function CalendarScreen() {
  const go = useNav((s) => s.go);
  const push = useToasts((s) => s.push);
  const now = useNow(60_000);
  const { settings } = useSettings();
  const [mode, setMode] = useState<Mode>(() => useNav.getState().calendarMode);
  const [anchor, setAnchor] = useState(() => startOfLocalDay(useNav.getState().calendarDate ?? new Date()));
  const [detail, setDetail] = useState<CalEvent | null>(null);
  const [adding, setAdding] = useState(false);

  const days = useMemo(() => {
    if (mode === "month") {
      // The whole grid: from the first week's start to the last week's end, so spill-over days show their events too.
      const weeks = monthWeeks(anchor, localeFirstDay());
      const start = weeks[0]![0]!;
      return Array.from({ length: weeks.length * 7 }, (_, i) => startOfLocalDay(start, i));
    }
    const first = mode === "week" ? startOfWeek(anchor, localeFirstDay()) : anchor;
    return Array.from({ length: mode === "week" ? 7 : 1 }, (_, i) => startOfLocalDay(first, i));
  }, [mode, anchor]);

  const from = days[0]!;
  const to = startOfLocalDay(days[days.length - 1]!, 1);
  const events = useQuery({ queryKey: ["events", from.toISOString(), to.toISOString()], queryFn: () => api.getEvents(from.toISOString(), to.toISOString()) });
  const calendars = useQuery({ queryKey: ["calendars"], queryFn: () => api.getCalendars() });
  const colours = useMemo(() => new Map((calendars.data ?? []).map((c) => [c.id, c.colour])), [calendars.data]);

  const step = (dir: -1 | 1) =>
    setAnchor((a) => (mode === "month" ? new Date(a.getFullYear(), a.getMonth() + dir, 1) : startOfLocalDay(a, dir * (mode === "week" ? 7 : 1))));
  const isCurrent = mode === "month" ? anchor.getFullYear() === now.getFullYear() && anchor.getMonth() === now.getMonth() : days.some((d) => sameDay(d, now));
  const unit = mode === "day" ? "day" : mode === "week" ? "week" : "month";
  const title = mode === "month" ? monthFmt.format(anchor) : mode === "day" ? dayFmt.format(from) : `${rangeFmt.format(from)} – ${rangeFmt.format(days[6]!)}, ${yearFmt.format(days[6]!)}`;
  const join = (url: string) => api.openUrl(url).catch(() => push({ title: "Couldn't open the link", tone: "danger" }));

  return (
    <div className="flex h-full flex-col">
      {/* On a phone the tab bar takes you back, so there is no title strip (and no space lost to it). */}
      {isPhone ? (
        <div className="h-3 shrink-0" />
      ) : (
        <div data-tauri-drag-region className="flex h-11 shrink-0 items-center px-3 pl-20">
          <Button variant="plain" size="sm" leading={<ChevronLeft className="size-4" />} onClick={() => go("briefing")}>
            Briefing
          </Button>
        </div>
      )}
      {isPhone ? (
        // A phone: the title with Today and the arrows on one line, the view switch across the full width under it;
        // a new event is the round button at the bottom.
        <header className="flex shrink-0 flex-col gap-2.5 px-4 pb-2.5">
          <div className="flex items-center gap-1.5">
            <h1 aria-live="polite" className="min-w-0 flex-1 truncate text-title font-bold tracking-tight text-fg">
              {title}
            </h1>
            <IconButton label={`Previous ${unit}`} onClick={() => step(-1)}>
              <ChevronLeft className="size-5" />
            </IconButton>
            <Button variant="secondary" size="sm" disabled={isCurrent} onClick={() => setAnchor(startOfLocalDay(new Date()))}>
              Today
            </Button>
            <IconButton label={`Next ${unit}`} onClick={() => step(1)}>
              <ChevronRight className="size-5" />
            </IconButton>
          </div>
          <Segmented
            fill
            label="View"
            value={mode}
            onValueChange={(v) => setMode(v as Mode)}
            options={[
              { value: "day", label: "Day" },
              { value: "week", label: "Week" },
              { value: "month", label: "Month" },
            ]}
          />
          <Fab label="New event" onClick={() => setAdding(true)} />
        </header>
      ) : (
      <header className="flex shrink-0 flex-wrap items-center justify-between gap-2 px-5 pb-3">
        <h1 aria-live="polite" className="text-title font-semibold tracking-tight text-fg">
          {title}
        </h1>
        <div className="flex flex-wrap items-center gap-2 max-sm:w-full max-sm:justify-between">
          {isPhone ? (
            <IconButton label="New event" onClick={() => setAdding(true)}>
              <Plus className="size-5" />
            </IconButton>
          ) : (
            <Button size="sm" variant="secondary" leading={<Plus className="size-4" aria-hidden />} onClick={() => setAdding(true)}>
              New event
            </Button>
          )}
          <Segmented
            label="View"
            value={mode}
            onValueChange={(v) => setMode(v as Mode)}
            options={[
              { value: "day", label: "Day" },
              { value: "week", label: "Week" },
              { value: "month", label: "Month" },
            ]}
          />
          <div className="flex items-center">
            <IconButton label={`Previous ${unit}`} onClick={() => step(-1)}>
              <ChevronLeft className="size-[18px]" />
            </IconButton>
            <Button variant="plain" size="sm" disabled={isCurrent} onClick={() => setAnchor(startOfLocalDay(new Date()))}>
              Today
            </Button>
            <IconButton label={`Next ${unit}`} onClick={() => step(1)}>
              <ChevronRight className="size-[18px]" />
            </IconButton>
          </div>
        </div>
      </header>
      )}
      {events.isPending ? (
        <div className="flex flex-col gap-2 px-5" role="status" aria-busy>
          <span className="sr-only">Loading…</span>
          <Skeleton className="h-10" />
          <Skeleton className="h-64" />
        </div>
      ) : events.isError ? (
        <p className="px-5 text-body text-danger">Couldn’t load events. Try again in a moment.</p>
      ) : mode === "month" ? (
        <MonthGrid
          anchor={anchor}
          events={events.data}
          now={now}
          colourOf={(id) => colours.get(id) ?? "blue"}
          onPickDay={(d) => {
            setAnchor(startOfLocalDay(d));
            setMode("day");
          }}
          onOpen={setDetail}
        />
      ) : (
        <TimeGrid days={days} events={events.data} now={now} colourOf={(id) => colours.get(id) ?? "blue"} onOpen={setDetail} zones={settings?.appearance.extra_zones ?? []} />
      )}
      <EventForm open={adding} onOpenChange={setAdding} day={mode === "month" ? undefined : from} />
      <EventDetail event={detail} calendars={calendars.data ?? []} onClose={() => setDetail(null)} onJoin={join} />
    </div>
  );
}
