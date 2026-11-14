import { useQuery, useQueryClient } from "@tanstack/react-query";
import { MapPin, Video, X } from "lucide-react";
import { useEffect } from "react";
import { api } from "@/api";
import { Button, IconButton, calendarColour } from "@/design";
import { formatCountdown } from "./format";
import { useNow } from "./hooks";
import { timeRange } from "./summary";

/** The small panel that appears at the top-right of the screen when a meeting is about to start. */
export function UpNextPanel() {
  const qc = useQueryClient();
  const now = useNow(15_000);
  const briefing = useQuery({ queryKey: ["briefing", "upnext"], queryFn: () => api.getBriefing(), staleTime: 0 });
  const calendars = useQuery({ queryKey: ["calendars"], queryFn: () => api.getCalendars() });

  // The window is created once and reused; reload each time it is shown.
  useEffect(() => {
    let off: (() => void) | undefined;
    let cancelled = false;
    void api.onUpNextShown(() => void qc.invalidateQueries({ queryKey: ["briefing", "upnext"] })).then((u) => (cancelled ? u() : (off = u)));
    return () => {
      cancelled = true;
      off?.();
    };
  }, [qc]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && void api.dismissUpNext();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const next = briefing.data?.next ?? null;
  const colour = calendars.data?.find((c) => c.id === next?.event.calendar_id)?.colour ?? "blue";

  return (
    <div className="h-full p-2">
      <div className="flex h-full flex-col justify-between rounded-sheet bg-surface-raised p-4 shadow-float animate-fade-in">
        {next ? (
          <>
            <div className="flex gap-3">
              <span aria-hidden className="w-[3px] self-stretch rounded-full" style={{ background: calendarColour(colour) }} />
              <div className="min-w-0 flex-1">
                <p className="text-footnote font-semibold text-accent">{next.in_progress ? "Happening now" : `Up next · ${formatCountdown(new Date(next.start).getTime() - now.getTime())}`}</p>
                <p className="truncate text-headline font-semibold text-fg">{next.event.title}</p>
                <p className="text-footnote text-fg-2 tabular-nums">{timeRange(next.start, next.end)}</p>
                {next.event.location && (
                  <p className="flex items-center gap-1 text-footnote text-fg-2">
                    <MapPin className="size-3 shrink-0" aria-hidden />
                    <span className="truncate">{next.event.location}</span>
                  </p>
                )}
              </div>
              <IconButton label="Dismiss" className="-mr-1 -mt-1 size-7" onClick={() => void api.dismissUpNext()}>
                <X className="size-4" />
              </IconButton>
            </div>
            <div className="mt-2 flex items-center justify-end gap-2">
              <Button size="sm" variant="plain" onClick={() => void api.openBriefing()}>
                Open briefing
              </Button>
              {next.event.join_url && (
                <Button size="sm" variant="primary" leading={<Video className="size-3.5" aria-hidden />} onClick={() => void api.openUrl(next.event.join_url!)}>
                  Join
                </Button>
              )}
            </div>
          </>
        ) : (
          <p className="m-auto text-body text-fg-2">Nothing coming up.</p>
        )}
      </div>
    </div>
  );
}
