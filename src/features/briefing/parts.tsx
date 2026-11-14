import { AlertTriangle, CalendarCheck, Clock, MapPin, Video } from "lucide-react";
import clsx from "clsx";
import type { Briefing, BriefingItem, CalEvent, NextUp } from "@/api";
import { Badge, Button, Card, ListGroup, SectionHeader, calendarColour } from "@/design";
import { formatCountdown, formatDuration, formatTime } from "./format";
import { timeRange } from "./summary";

export type ColourOf = (calendarId: string) => string;
type Open = (event: CalEvent) => void;

const at = (iso: string) => new Date(iso);
const minutesBetween = (a: string, b: string) => (at(b).getTime() - at(a).getTime()) / 60_000;

/** Thin leading bar in the calendar's colour. */
function Bar({ colour, dim }: { colour: string; dim?: boolean }) {
  return <span aria-hidden className={clsx("w-[3px] self-stretch rounded-full", dim && "opacity-60")} style={{ background: calendarColour(colour) }} />;
}

const subtitleOf = (i: BriefingItem) => [i.event.location, formatDuration(minutesBetween(i.start, i.end))].filter(Boolean).join(" · ");

/* -------------------------------------------------------------- Up next */

export function UpNextCard({ next, now, colourOf, onJoin, onOpen }: { next: NextUp; now: Date; colourOf: ColourOf; onJoin: (url: string) => void; onOpen: Open }) {
  const label = next.in_progress
    ? `Happening now · ends ${formatCountdown(at(next.end).getTime() - now.getTime())}`
    : `Up next · ${formatCountdown(at(next.start).getTime() - now.getTime())}`;
  return (
    <Card className="animate-slide-up" aria-label="Up next">
      <div className="flex gap-3">
        <Bar colour={colourOf(next.event.calendar_id)} />
        <div className="min-w-0 flex-1">
          <p className="text-footnote font-semibold text-accent">{label}</p>
          <button type="button" onClick={() => onOpen(next.event)} className="mt-1 block w-full truncate text-left text-title font-semibold tracking-tight text-fg hover:underline">
            {next.event.title}
          </button>
          <p className="mt-0.5 text-body text-fg-2 tabular-nums">{timeRange(next.start, next.end)}</p>
          {next.event.location && (
            <p className="mt-1 flex items-center gap-1 text-body text-fg-2">
              <MapPin className="size-3.5 shrink-0" aria-hidden />
              <span className="truncate">{next.event.location}</span>
            </p>
          )}
          {next.event.join_url && (
            <Button variant="primary" className="mt-3" leading={<Video className="size-4" aria-hidden />} onClick={() => onJoin(next.event.join_url!)}>
              Join
            </Button>
          )}
        </div>
      </div>
    </Card>
  );
}

/* ------------------------------------------------------ Needs attention */

export function AttentionSection({ b }: { b: Briefing }) {
  const title = new Map(b.remaining.map((i) => [i.event.id, i.event.title]));
  const start = new Map(b.remaining.map((i) => [i.event.id, i.start]));
  const runs = b.back_to_back_runs;
  if (b.conflicts.length === 0 && b.needs_response.length === 0 && runs.length === 0) return null;
  return (
    <section aria-label="Needs attention">
      <SectionHeader title="Needs attention" />
      <ListGroup>
        {b.conflicts.map((c) => (
          <li key={`${c.a}-${c.b}`} className="flex items-start gap-3 px-4 py-3">
            <AlertTriangle className="mt-0.5 size-4 shrink-0 text-warning" aria-hidden />
            <p className="text-body text-fg">
              <span className="font-medium">{title.get(c.a)}</span> overlaps <span className="font-medium">{title.get(c.b)}</span>
              <span className="text-fg-2"> · {start.get(c.b) ? formatTime(at(start.get(c.b)!)) : ""} · {formatDuration(c.overlap_minutes)}</span>
            </p>
          </li>
        ))}
        {runs.map((run) => (
          <li key={run.join("-")} className="flex items-start gap-3 px-4 py-3">
            <Clock className="mt-0.5 size-4 shrink-0 text-fg-2" aria-hidden />
            <p className="text-body text-fg">
              {run.length} meetings back to back{start.get(run[0]!) ? <span className="text-fg-2"> from {formatTime(at(start.get(run[0]!)!))}</span> : null}
            </p>
          </li>
        ))}
        {b.needs_response.map((e) => (
          <li key={e.id} className="flex items-start gap-3 px-4 py-3">
            <Badge tone="accent">RSVP</Badge>
            <p className="text-body text-fg">
              <span className="font-medium">{e.title}</span>
              <span className="text-fg-2"> needs your response</span>
            </p>
          </li>
        ))}
      </ListGroup>
    </section>
  );
}

/* ------------------------------------------------------------- Timeline */

function TimelineRow({ item, colourOf, dim, conflicted, onOpen }: { item: BriefingItem; colourOf: ColourOf; dim?: boolean; conflicted?: boolean; onOpen: Open }) {
  return (
    <li>
      <button
        type="button"
        onClick={() => onOpen(item.event)}
        className={clsx(
          "flex min-h-12 w-full items-stretch gap-3 px-4 py-2 text-left transition-[background-color,opacity] duration-150 ease-standard hover:bg-fill active:bg-fill-strong",
          dim && "opacity-50",
        )}
      >
        <span className="w-[68px] shrink-0 whitespace-nowrap pt-0.5 text-body font-medium text-fg-2 tabular-nums">{formatTime(at(item.start))}</span>
        <Bar colour={colourOf(item.event.calendar_id)} />
        <div className="min-w-0 flex-1">
          <p className="truncate text-headline font-medium text-fg">{item.event.title}</p>
          <p className="truncate text-footnote text-fg-2">{subtitleOf(item)}</p>
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          {conflicted && <AlertTriangle className="size-3.5 text-warning" aria-label="Conflict" />}
          {item.event.status === "tentative" && <Badge tone="neutral">Tentative</Badge>}
        </div>
      </button>
    </li>
  );
}

function NowMarker({ now }: { now: Date }) {
  return (
    <li aria-label={`Now, ${formatTime(now)}`} className="flex h-6 items-center gap-3 px-4 !border-t-0">
      <span className="w-[68px] shrink-0 whitespace-nowrap text-caption font-semibold text-accent tabular-nums">{formatTime(now)}</span>
      <span aria-hidden className="size-2 rounded-full bg-accent" />
      <span aria-hidden className="h-px flex-1 bg-accent" />
    </li>
  );
}

export function TodaySection({ b, now, colourOf, onOpen, showFreeTime = true }: { b: Briefing; now: Date; colourOf: ColourOf; onOpen: Open; showFreeTime?: boolean }) {
  const conflicted = new Set(b.conflicts.flatMap((c) => [c.a, c.b]));
  const hasTimed = b.past.length + b.remaining.length > 0;
  if (!hasTimed && b.all_day_today.length === 0) return null;
  const gaps = showFreeTime ? b.free_gaps.slice(0, 3) : [];
  return (
    <section aria-label="Today">
      <SectionHeader title="Today" trailing={b.all_day_today.length ? b.all_day_today.map((e) => e.title).join(" · ") : undefined} />
      {hasTimed && (
        <ListGroup>
          {b.past.map((i) => (
            <TimelineRow key={i.event.id} item={i} colourOf={colourOf} dim onOpen={onOpen} />
          ))}
          <NowMarker now={now} />
          {b.remaining.map((i) => (
            <TimelineRow key={i.event.id} item={i} colourOf={colourOf} conflicted={conflicted.has(i.event.id)} onOpen={onOpen} />
          ))}
        </ListGroup>
      )}
      {gaps.length > 0 && (
        <p className="mt-2 px-1 text-footnote text-fg-2">
          Free: {gaps.map((g) => `${formatTime(at(g.start))}–${formatTime(at(g.end))}`).join(", ")}
        </p>
      )}
    </section>
  );
}

/* ------------------------------------------------------------- Tomorrow */

export function TomorrowSection({ b, colourOf, onOpen }: { b: Briefing; colourOf: ColourOf; onOpen: Open }) {
  const { timed, all_day, early_start } = b.tomorrow;
  const count = timed.length + all_day.length;
  const shown = timed.slice(0, 3);
  return (
    <section aria-label="Tomorrow">
      <SectionHeader
        title="Tomorrow"
        trailing={count === 0 ? "Nothing scheduled" : `${count} ${count === 1 ? "event" : "events"}${timed[0] ? ` · from ${formatTime(at(timed[0].start))}` : ""}`}
      />
      {count > 0 && (
        <div className="rounded-card bg-fill px-4 py-1">
          {early_start && (
            <div className="pb-1 pt-2">
              <Badge tone="warning">Early start</Badge>
            </div>
          )}
          <ul className="divide-y divide-separator">
            {all_day.map((e) => (
              <li key={e.id} className="flex items-center gap-3 py-2">
                <span className="w-[68px] shrink-0 text-footnote text-fg-3">All day</span>
                <button type="button" onClick={() => onOpen(e)} className="truncate text-left text-body text-fg-2 hover:underline">{e.title}</button>
              </li>
            ))}
            {shown.map((i) => (
              <li key={i.event.id} className="flex items-center gap-3 py-2">
                <span className="w-[68px] shrink-0 whitespace-nowrap text-footnote text-fg-3 tabular-nums">{formatTime(at(i.start))}</span>
                <Bar colour={colourOf(i.event.calendar_id)} dim />
                <button type="button" onClick={() => onOpen(i.event)} className="truncate text-left text-body text-fg-2 hover:underline">{i.event.title}</button>
              </li>
            ))}
          </ul>
          {timed.length > shown.length && <p className="pb-2 pt-1 text-footnote text-fg-3">+{timed.length - shown.length} more</p>}
        </div>
      )}
    </section>
  );
}

export function ClearDay() {
  return (
    <div className="flex items-center gap-2 px-1 text-body text-fg-2">
      <CalendarCheck className="size-4 text-success" aria-hidden />
      Nothing left today.
    </div>
  );
}
