import { useQuery } from "@tanstack/react-query";
import { CalendarDays, CalendarX, CloudOff, RefreshCw, Settings as SettingsIcon } from "lucide-react";
import { useNav } from "@/app/nav";
import { useMemo, useState } from "react";
import { api, type CalEvent } from "@/api";
import { Button, EmptyState, IconButton, Skeleton, Tooltip, useToasts } from "@/design";
import { formatDate, greeting } from "./format";
import { useNow, useOnline } from "./hooks";
import { AttentionSection, ClearDay, TodaySection, TomorrowSection, UpNextCard } from "./parts";
import { useSettings } from "@/features/settings/useSettings";
import { EventDetail } from "./EventDetail";
import { ClearWeek } from "./ClearWeek";
import { InboxSection } from "./InboxSection";
import { summarise } from "./summary";

function LoadingState() {
  return (
    <div className="flex flex-col gap-6" role="status" aria-busy>
      <span className="sr-only">Loading your day…</span>
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
  const go = useNav((s) => s.go);
  const { settings } = useSettings();
  const sections = settings?.sections ?? { tomorrow: true, free_time: true, attention: true };

  // The engine works out today and tomorrow; refetch each minute so past/next stay accurate while open.
  const briefing = useQuery({ queryKey: ["briefing"], queryFn: () => api.getBriefing(), refetchInterval: 60_000, staleTime: 15_000 });
  const calendars = useQuery({ queryKey: ["calendars"], queryFn: () => api.getCalendars() });
  // Optional AI summary. Never awaited: the normal summary shows at once and AI replaces it only if it arrives.
  const aiOn = settings?.ai.enabled === true;
  const ai = useQuery({ queryKey: ["ai-summary"], queryFn: () => api.getAiSummary(), enabled: aiOn, staleTime: 5 * 60_000, refetchInterval: 5 * 60_000, retry: false });
  const [detail, setDetail] = useState<CalEvent | null>(null);

  const colours = useMemo(() => new Map((calendars.data ?? []).map((c) => [c.id, c.colour])), [calendars.data]);
  const colourOf = (id: string) => colours.get(id) ?? "blue";

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

  const b = briefing.data;
  const dayIsEmpty = !!b && b.all_day_today.length + b.past.length + b.remaining.length + b.tomorrow.timed.length + b.tomorrow.all_day.length === 0;

  return (
    <div className="flex h-full flex-col">
      {/* Draggable title-bar strip; the traffic lights sit at the left. */}
      <div data-tauri-drag-region className="flex h-11 shrink-0 items-center justify-end px-3">
        <Tooltip content="Calendar">
          <IconButton label="Calendar" onClick={() => go("calendar")}>
            <CalendarDays className="size-[18px]" />
          </IconButton>
        </Tooltip>
        <Tooltip content="Refresh">
          <IconButton label="Refresh" onClick={refresh} disabled={refreshing}>
            <RefreshCw className={`size-[17px] ${refreshing ? "animate-spin" : ""}`} />
          </IconButton>
        </Tooltip>
        <Tooltip content="Settings">
          <IconButton label="Settings" onClick={() => go("settings")}>
            <SettingsIcon className="size-[18px]" />
          </IconButton>
        </Tooltip>
      </div>

      <main className="min-h-0 flex-1 overflow-y-auto px-5 pb-8 [&>*]:mx-auto [&>*]:w-full [&>*]:max-w-2xl">
        <header className="mb-6">
          <h1 className="text-display font-bold tracking-tight text-fg">{greeting(now)}</h1>
          <p className="mt-0.5 text-headline font-medium text-fg-2">{formatDate(now)}</p>
          {b && !dayIsEmpty && (
            <p className="mt-3 text-headline text-fg">{aiOn && ai.data ? ai.data : summarise(b, now)}</p>
          )}
          {b && !dayIsEmpty && aiOn && ai.data && <p className="mt-1 text-caption text-fg-3">Summary by AI</p>}
          {!online && (
            <p className="mt-3 flex items-center gap-1.5 text-footnote text-fg-2">
              <CloudOff className="size-3.5" aria-hidden /> Offline — showing saved events
            </p>
          )}
        </header>

        {briefing.isPending ? (
          <LoadingState />
        ) : briefing.isError || !b ? (
          <EmptyState
            icon={<CalendarX />}
            title="Couldn't load your calendar"
            description="Something went wrong reading your events."
            action={
              <Button variant="secondary" leading={<RefreshCw className="size-4" />} onClick={() => void briefing.refetch()}>
                Try again
              </Button>
            }
          />
        ) : dayIsEmpty ? (
          <ClearWeek colourOf={colourOf} onOpen={setDetail} />
        ) : (
          <div className="flex flex-col gap-6">
            {b.next ? <UpNextCard next={b.next} now={now} colourOf={colourOf} onJoin={join} onOpen={setDetail} /> : <ClearDay />}
            {sections.attention && <AttentionSection b={b} />}
            <TodaySection b={b} now={now} colourOf={colourOf} onOpen={setDetail} showFreeTime={sections.free_time} />
            {sections.tomorrow && <TomorrowSection b={b} colourOf={colourOf} onOpen={setDetail} />}
            {settings?.email.enabled && <InboxSection />}
          </div>
        )}
      </main>

      <EventDetail event={detail} calendars={calendars.data ?? []} onClose={() => setDetail(null)} onJoin={join} />
    </div>
  );
}
