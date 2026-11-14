import { AlertTriangle, CalendarCheck, MapPin, Video } from "lucide-react";
import clsx from "clsx";
import type { CalEvent } from "@/api";
import { Badge, Button, Card, ListGroup, SectionHeader, calendarColour } from "@/design";
import { formatCountdown, formatTime } from "./format";
import { durationLabel, type BriefingModel, type Conflict, type TimedItem } from "./present";

export type ColourOf = (calendarId: string) => string;

/** Thin leading bar in the calendar's colour. */
function Bar({ colour, dim }: { colour: string; dim?: boolean }) {
  return (
    <span
      aria-hidden
      className={clsx("w-[3px] self-stretch rounded-full", dim && "opacity-60")}
      style={{ background: calendarColour(colour) }}
    />
  );
}

const subtitleOf = (t: TimedItem) =>
  [t.event.location, durationLabel(t)].filter(Boolean).join(" · ");

/* -------------------------------------------------------------- Up next */

export function UpNextCard({
  item,
  inProgress,
  now,
  colourOf,
  onJoin,
}: {
  item: TimedItem;
  inProgress: boolean;
  now: Date;
  colourOf: ColourOf;
  onJoin: (url: string) => void;
}) {
  const label = inProgress ? `Happening now · ends ${formatCountdown(item.end.getTime() - now.getTime())}` : `Up next · ${formatCountdown(item.start.getTime() - now.getTime())}`;
  return (
    <Card className="animate-slide-up" aria-label="Up next">
      <div className="flex gap-3">
        <Bar colour={colourOf(item.event.calendar_id)} />
        <div className="min-w-0 flex-1">
          <p className="text-footnote font-semibold text-accent">{label}</p>
          <h2 className="mt-1 truncate text-title font-semibold tracking-tight text-fg">{item.event.title}</h2>
          <p className="mt-0.5 text-body text-fg-2 tabular-nums">
            {formatTime(item.start)} – {formatTime(item.end)}
          </p>
          {item.event.location && (
            <p className="mt-1 flex items-center gap-1 text-body text-fg-2">
              <MapPin className="size-3.5 shrink-0" aria-hidden />
              <span className="truncate">{item.event.location}</span>
            </p>
          )}
          {item.event.join_url && (
            <Button
              variant="primary"
              className="mt-3"
              leading={<Video className="size-4" aria-hidden />}
              onClick={() => onJoin(item.event.join_url!)}
            >
              Join
            </Button>
          )}
        </div>
      </div>
    </Card>
  );
}

/* ------------------------------------------------------ Needs attention */

export function AttentionSection({ conflicts, needsResponse }: { conflicts: Conflict[]; needsResponse: CalEvent[] }) {
  if (conflicts.length === 0 && needsResponse.length === 0) return null;
  return (
    <section aria-label="Needs attention">
      <SectionHeader title="Needs attention" />
      <ListGroup>
        {conflicts.map((c) => (
          <li key={`${c.a.event.id}-${c.b.event.id}`} className="flex items-start gap-3 px-4 py-3">
            <AlertTriangle className="mt-0.5 size-4 shrink-0 text-warning" aria-hidden />
            <p className="text-body text-fg">
              <span className="font-medium">{c.a.event.title}</span> overlaps{" "}
              <span className="font-medium">{c.b.event.title}</span>
              <span className="text-fg-2"> · {formatTime(c.b.start)}</span>
            </p>
          </li>
        ))}
        {needsResponse.map((e) => (
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

function TimelineRow({ item, colourOf, dim, conflicted }: { item: TimedItem; colourOf: ColourOf; dim?: boolean; conflicted?: boolean }) {
  return (
    <li className={clsx("flex min-h-12 items-stretch gap-3 px-4 py-2", dim && "opacity-50")}>
      <span className="w-[68px] whitespace-nowrap shrink-0 pt-0.5 text-body font-medium text-fg-2 tabular-nums">{formatTime(item.start)}</span>
      <Bar colour={colourOf(item.event.calendar_id)} />
      <div className="min-w-0 flex-1">
        <p className="truncate text-headline font-medium text-fg">{item.event.title}</p>
        <p className="truncate text-footnote text-fg-2">{subtitleOf(item)}</p>
      </div>
      <div className="flex shrink-0 items-center gap-1.5">
        {conflicted && <AlertTriangle className="size-3.5 text-warning" aria-label="Conflict" />}
        {item.event.status === "tentative" && <Badge tone="neutral">Tentative</Badge>}
      </div>
    </li>
  );
}

function NowMarker({ now }: { now: Date }) {
  return (
    <li aria-label={`Now, ${formatTime(now)}`} className="flex h-6 items-center gap-3 px-4 !border-t-0">
      <span className="w-[68px] whitespace-nowrap shrink-0 text-caption font-semibold text-accent tabular-nums">{formatTime(now)}</span>
      <span aria-hidden className="size-2 rounded-full bg-accent" />
      <span aria-hidden className="h-px flex-1 bg-accent" />
    </li>
  );
}

export function TodaySection({ model, now, colourOf }: { model: BriefingModel; now: Date; colourOf: ColourOf }) {
  const conflicted = new Set(model.conflicts.flatMap((c) => [c.a.event.id, c.b.event.id]));
  const hasTimed = model.past.length + model.remaining.length > 0;
  if (!hasTimed && model.allDayToday.length === 0) return null;
  return (
    <section aria-label="Today">
      <SectionHeader title="Today" trailing={model.allDayToday.length ? model.allDayToday.map((e) => e.title).join(" · ") : undefined} />
      {hasTimed && (
        <ListGroup>
          {model.past.map((t) => (
            <TimelineRow key={t.event.id} item={t} colourOf={colourOf} dim />
          ))}
          <NowMarker now={now} />
          {model.remaining.map((t) => (
            <TimelineRow key={t.event.id} item={t} colourOf={colourOf} conflicted={conflicted.has(t.event.id)} />
          ))}
        </ListGroup>
      )}
    </section>
  );
}

/* ------------------------------------------------------------- Tomorrow */

export function TomorrowSection({ model, colourOf }: { model: BriefingModel; colourOf: ColourOf }) {
  const { timed, allDay, earlyStart } = model.tomorrow;
  const count = timed.length + allDay.length;
  const shown = timed.slice(0, 3);
  return (
    <section aria-label="Tomorrow">
      <SectionHeader
        title="Tomorrow"
        trailing={count === 0 ? "Nothing scheduled" : `${count} ${count === 1 ? "event" : "events"}${timed[0] ? ` · from ${formatTime(timed[0].start)}` : ""}`}
      />
      {count > 0 && (
        <div className="rounded-card bg-fill px-4 py-1">
          {earlyStart && (
            <div className="pb-1 pt-2">
              <Badge tone="warning">Early start</Badge>
            </div>
          )}
          <ul className="divide-y divide-separator">
            {allDay.map((e) => (
              <li key={e.id} className="flex items-center gap-3 py-2">
                <span className="w-[68px] whitespace-nowrap shrink-0 text-footnote text-fg-3">All day</span>
                <span className="truncate text-body text-fg-2">{e.title}</span>
              </li>
            ))}
            {shown.map((t) => (
              <li key={t.event.id} className="flex items-center gap-3 py-2">
                <span className="w-[68px] whitespace-nowrap shrink-0 text-footnote text-fg-3 tabular-nums">{formatTime(t.start)}</span>
                <Bar colour={colourOf(t.event.calendar_id)} dim />
                <span className="truncate text-body text-fg-2">{t.event.title}</span>
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
