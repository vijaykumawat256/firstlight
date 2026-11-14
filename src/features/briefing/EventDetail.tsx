import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  Calendar as CalendarIcon,
  Copy,
  MapPin,
  Pencil,
  Trash2,
  User,
  Users,
  Video,
} from "lucide-react";
import { useState } from "react";
import { api, errorMessage, type CalEvent, type CalendarInfo } from "@/api";
import { EventForm, OWN_CALENDAR_ID } from "@/features/events/EventForm";
import { Badge, Button, Sheet, calendarColour, useToasts } from "@/design";
import { copyLink } from "./copyLink";
import { formatDate, formatTime } from "./format";

const RESPONSE: Record<CalEvent["my_response"], string | null> = {
  accepted: "You accepted",
  declined: "You declined",
  tentative: "You said maybe",
  needs_action: "Needs your response",
  none: null,
};

function when(e: CalEvent): { day: string; time: string } {
  if (e.when.kind === "all_day") {
    const start = new Date(`${e.when.start}T00:00:00`);
    const last = new Date(`${e.when.end}T00:00:00`);
    last.setDate(last.getDate() - 1);
    const multi = last > start;
    return {
      day: multi
        ? `${formatDate(start)} – ${formatDate(last)}`
        : formatDate(start),
      time: "All day",
    };
  }
  const s = new Date(e.when.start);
  const en = new Date(e.when.end);
  const sameDay = s.toDateString() === en.toDateString();
  return {
    day: formatDate(s),
    time: sameDay
      ? `${formatTime(s)} – ${formatTime(en)}`
      : `${formatTime(s)} – ${formatDate(en)} ${formatTime(en)}`,
  };
}

export function EventDetail({
  event,
  calendars,
  onClose,
  onJoin,
}: {
  event: CalEvent | null;
  calendars: CalendarInfo[];
  onClose: () => void;
  onJoin: (url: string) => void;
}) {
  const cal = event && calendars.find((c) => c.id === event.calendar_id);
  const others = event
    ? event.also_in
        .map((id) => calendars.find((c) => c.id === id)?.name)
        .filter(Boolean)
    : [];
  const t = event && when(event);
  const response = event && RESPONSE[event.my_response];
  const push = useToasts((s) => s.push);
  const qc = useQueryClient();
  const [editTarget, setEditTarget] = useState<CalEvent | null>(null);
  const [confirming, setConfirming] = useState(false);
  const remove = useMutation({
    mutationFn: (id: string) => api.deleteEvent(id),
    onSuccess: () => {
      setConfirming(false);
      onClose();
      void qc.invalidateQueries();
      push({ title: "Event deleted" });
    },
    onError: (e) =>
      push({
        title: "Couldn't delete the event",
        description: errorMessage(e),
        tone: "danger",
      }),
  });
  const own = event?.calendar_id === OWN_CALENDAR_ID;
  return (
    <>
      <Sheet
        open={event !== null}
        onOpenChange={(o) => {
          if (!o) {
            setConfirming(false);
            onClose();
          }
        }}
        title={event?.title ?? ""}
        description={t ? `${t.day} · ${t.time}` : undefined}
      >
        {event && (
          <div className="flex flex-col gap-4 pb-6">
            <div className="flex flex-wrap items-center gap-2">
              {event.status === "tentative" && <Badge>Tentative</Badge>}
              {event.status === "cancelled" && (
                <Badge tone="danger">Cancelled</Badge>
              )}
              {response && (
                <Badge
                  tone={
                    event.my_response === "needs_action" ? "accent" : "neutral"
                  }
                >
                  {response}
                </Badge>
              )}
              {event.recurrence_master_id && <Badge>Repeats</Badge>}
            </div>
            {event.join_url && (
              <div className="flex flex-wrap items-center gap-2">
                <Button
                  variant="primary"
                  leading={<Video className="size-4" aria-hidden />}
                  onClick={() => onJoin(event.join_url!)}
                >
                  Join
                </Button>
                <Button
                  variant="secondary"
                  leading={<Copy className="size-4" aria-hidden />}
                  onClick={() => void copyLink(event.join_url!, push)}
                >
                  Copy link
                </Button>
              </div>
            )}
            <dl className="flex flex-col gap-3 text-body">
              {event.location && (
                <div className="flex gap-3">
                  <MapPin
                    className="mt-0.5 size-4 shrink-0 text-fg-2"
                    aria-hidden
                  />
                  <dt className="sr-only">Location</dt>
                  <dd className="select-text text-fg">{event.location}</dd>
                </div>
              )}
              {event.organizer && (
                <div className="flex gap-3">
                  <User
                    className="mt-0.5 size-4 shrink-0 text-fg-2"
                    aria-hidden
                  />
                  <dt className="sr-only">Organiser</dt>
                  <dd className="text-fg">{event.organizer}</dd>
                </div>
              )}
              {cal && (
                <div className="flex gap-3">
                  <CalendarIcon
                    className="mt-0.5 size-4 shrink-0 text-fg-2"
                    aria-hidden
                  />
                  <dt className="sr-only">Calendar</dt>
                  <dd className="flex items-center gap-2 text-fg">
                    <span
                      className="size-2.5 rounded-full"
                      style={{ background: calendarColour(cal.colour) }}
                      aria-hidden
                    />
                    {cal.name}
                  </dd>
                </div>
              )}
              {others.length > 0 && (
                <div className="flex gap-3">
                  <Users
                    className="mt-0.5 size-4 shrink-0 text-fg-2"
                    aria-hidden
                  />
                  <dt className="sr-only">Also in</dt>
                  <dd className="text-fg-2">Also in {others.join(", ")}</dd>
                </div>
              )}
            </dl>
            {own && (
              <div className="flex flex-wrap items-center gap-2">
                <Button
                  variant="secondary"
                  leading={<Pencil className="size-4" aria-hidden />}
                  onClick={() => {
                    setEditTarget(event);
                    onClose();
                  }}
                >
                  Edit
                </Button>
                {confirming ? (
                  <>
                    <Button
                      variant="danger"
                      disabled={remove.isPending}
                      onClick={() => remove.mutate(event.id)}
                    >
                      {remove.isPending ? "Deleting…" : "Yes, delete it"}
                    </Button>
                    <Button
                      variant="plain"
                      onClick={() => setConfirming(false)}
                    >
                      Keep
                    </Button>
                  </>
                ) : (
                  <Button
                    variant="plain"
                    leading={<Trash2 className="size-4" aria-hidden />}
                    onClick={() => setConfirming(true)}
                  >
                    Delete
                  </Button>
                )}
              </div>
            )}
            {event.description && (
              // Event text is untrusted: shown as plain text only, never as HTML.
              <p className="max-h-48 select-text overflow-y-auto whitespace-pre-wrap break-words rounded-card bg-fill p-3 text-body text-fg-2">
                {event.description}
              </p>
            )}
          </div>
        )}
      </Sheet>
      <EventForm
        open={editTarget !== null}
        onOpenChange={(o) => !o && setEditTarget(null)}
        initial={editTarget}
      />
    </>
  );
}
