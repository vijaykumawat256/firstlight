import { useQuery } from "@tanstack/react-query";
import { CalendarX, CloudOff, RefreshCw, Settings as SettingsIcon } from "lucide-react";
import { useMemo, useState } from "react";
import { api } from "@/api";
import { Button, EmptyState, IconButton, Skeleton, Tooltip, useToasts } from "@/design";
import { SettingsSheet } from "@/features/settings/SettingsSheet";
import { formatDate, greeting } from "./format";
import { useNow, useOnline } from "./hooks";
import { AttentionSection, ClearDay, TodaySection, TomorrowSection, UpNextCard } from "./parts";
import { buildBriefing, startOfDay } from "./present";

function LoadingState() {
  return (
    <div className="flex flex-col gap-6" aria-busy aria-label="Loading your day">
      <Skeleton className="h-[108px] rounded-card" />
      <div className="flex flex-col gap-2">
        <Skeleton className="h-5 w-16" />
        <Skeleton className="h-[150px] rounded-card" />
      </div>
      <Skeleton className="h-[80px] rounded-card" />
    </div>
  );
}

export function BriefingScreen() {
  const now = useNow();
  const online = useOnline();
  const push = useToasts((s) => s.push);
  const [settingsOpen, setSettingsOpen] = useState(false);

  // Load from the start of today to the end of tomorrow.
  const dayKey = startOfDay(now).toDateString();
  const range = useMemo(
    () => [startOfDay(now).toISOString(), startOfDay(now, 2).toISOString()] as const,
    // eslint-disable-next-line react-hooks/exhaustive-deps -- recompute only when the day changes
    [dayKey],
  );
  const events = useQuery({ queryKey: ["events", ...range], queryFn: () => api.getEvents(range[0], range[1]) });
  const calendars = useQuery({ queryKey: ["calendars"], queryFn: () => api.getCalendars() });

  const colours = useMemo(() => new Map((calendars.data ?? []).map((c) => [c.id, c.colour])), [calendars.data]);
  const colourOf = (id: string) => colours.get(id) ?? "blue";
  const model = useMemo(() => buildBriefing(events.data ?? [], now), [events.data, now]);

  const [refreshing, setRefreshing] = useState(false);
  const refresh = () => {
    setRefreshing(true);
    api.refreshNow().catch(() => push({ title: "Couldn't refresh", tone: "danger" }));
    // The backend answers with a "sync finished" event that refetches everything; clear the spinner then.
    void api.onSyncFinished(() => setRefreshing(false)).then((off) => setTimeout(off, 15_000));
    setTimeout(() => setRefreshing(false), 15_000);
  };

  const join = (url: string) => {
    api.openUrl(url).catch(() => push({ title: "Couldn't open the link", tone: "danger" }));
  };

  const dayIsEmpty =
    model.allDayToday.length + model.past.length + model.remaining.length + model.tomorrow.timed.length + model.tomorrow.allDay.length === 0;

  return (
    <div className="flex h-full flex-col">
      {/* Draggable title-bar strip; the traffic lights sit at the left. */}
      <div data-tauri-drag-region className="flex h-11 shrink-0 items-center justify-end px-3">
        <Tooltip content="Refresh">
          <IconButton label="Refresh" onClick={refresh} disabled={refreshing}>
            <RefreshCw className={`size-[17px] ${refreshing ? "animate-spin" : ""}`} />
          </IconButton>
        </Tooltip>
        <Tooltip content="Settings">
          <IconButton label="Settings" onClick={() => setSettingsOpen(true)}>
            <SettingsIcon className="size-[18px]" />
          </IconButton>
        </Tooltip>
      </div>

      <main className="min-h-0 flex-1 overflow-y-auto px-5 pb-8">
        <header className="mb-6">
          <h1 className="text-display font-bold tracking-tight text-fg">{greeting(now)}</h1>
          <p className="mt-0.5 text-headline font-medium text-fg-2">{formatDate(now)}</p>
          {!events.isPending && !events.isError && !dayIsEmpty && (
            <p className="mt-3 text-headline text-fg">{model.summary}</p>
          )}
          {!online && (
            <p className="mt-3 flex items-center gap-1.5 text-footnote text-fg-2">
              <CloudOff className="size-3.5" aria-hidden /> Offline — showing saved events
            </p>
          )}
        </header>

        {events.isPending ? (
          <LoadingState />
        ) : events.isError ? (
          <EmptyState
            icon={<CalendarX />}
            title="Couldn't load your calendar"
            description="Something went wrong reading your events."
            action={
              <Button variant="secondary" leading={<RefreshCw className="size-4" />} onClick={() => void events.refetch()}>
                Try again
              </Button>
            }
          />
        ) : dayIsEmpty ? (
          <EmptyState icon={<CalendarX />} title="Your day is clear" description="No events today or tomorrow. Enjoy the quiet." />
        ) : (
          <div className="flex flex-col gap-6">
            {model.next ? (
              <UpNextCard item={model.next} inProgress={model.nextInProgress} now={now} colourOf={colourOf} onJoin={join} />
            ) : (
              <ClearDay />
            )}
            <AttentionSection conflicts={model.conflicts} needsResponse={model.needsResponse} />
            <TodaySection model={model} now={now} colourOf={colourOf} />
            <TomorrowSection model={model} colourOf={colourOf} />
          </div>
        )}
      </main>

      <SettingsSheet open={settingsOpen} onOpenChange={setSettingsOpen} />
    </div>
  );
}
