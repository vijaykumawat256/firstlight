import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowRightLeft, Copy, Mail } from "lucide-react";
import { useState } from "react";
import { api, errorMessage, type CalEvent, type ConflictHelp as Help, type ConflictReason } from "@/api";
import { Button, Dialog, IconButton, useToasts } from "@/design";
import { InviteDialog } from "@/features/events/InviteDialog";
import { formatDate, formatDuration, formatTime } from "./format";

const REASON: Record<ConflictReason, string> = {
  yours: "You created it, so you can change it yourself",
  not_answered: "You haven't answered the invitation yet",
  tentative: "It's still tentative",
  shorter: "It's the shorter of the two",
  later: "It's the later one",
  your_choice: "You chose this one",
};

const range = (o: { start: string; end: string }) => `${formatDate(new Date(o.start))}, ${formatTime(new Date(o.start))} to ${formatTime(new Date(o.end))}`;

/** The first email address written in an organiser field ("Sam <sam@x.com>" or "sam@x.com"), if there is one. */
export function organiserAddress(e: CalEvent): string | null {
  return e.organizer?.match(/[^\s<>(),;"']+@[^\s<>(),;"']+\.[^\s<>(),;"']+/)?.[0] ?? null;
}

/** A polite note asking to move the meeting. It names no other meeting, so nothing private is shared. */
export function rescheduleNote(moved: CalEvent, options: Help["options"]): { subject: string; body: string } {
  const at = moved.when.kind === "timed" ? range(moved.when) : formatDate(new Date(`${moved.when.start}T00:00:00`));
  const lines = [
    "Hi,",
    "",
    `I have a clash at the time of “${moved.title}” (${at}). Could we move it to one of these instead?`,
    "",
    ...options.map((o) => `- ${range(o)}`),
    "",
    "If none of these suit, tell me what does.",
    "",
    "Thanks",
  ];
  return { subject: `Can we move “${moved.title}”?`, body: lines.join("\n") };
}

type Slot = { start: string; end: string };

/**
 * Where one meeting could go instead, and how to get it there: one click for an event you created (the whole
 * event, or just this day of a repeating one), or a note to the organiser for everyone else's.
 */
function MoveCard({ moved, options, yours, onMoved }: { moved: CalEvent; options: Slot[]; yours: boolean; onMoved: (made: CalEvent, invitees: string[]) => void }) {
  const push = useToasts((s) => s.push);
  const qc = useQueryClient();
  const own = useQuery({ queryKey: ["own-event", moved.id], queryFn: () => api.getOwnEvent(moved.id), enabled: yours });
  const rec = own.data ?? null;
  const repeats = !!rec?.input.repeat;
  // A repeating event saved into the Calendar app or a CalDAV account can't lose just one day from here.
  const mode: "whole" | "day" | null = !yours || !rec ? null : !repeats ? "whole" : rec.saved_to ? null : "day";

  const move = useMutation({
    mutationFn: async (o: Slot) => {
      if (!rec) throw new Error("That event no longer exists.");
      if (mode === "day") {
        // Make this day a separate event at the new time first; only then take the old day out of the series, so a
        // failure leaves the series exactly as it was.
        const made = await api.createEvent({ ...rec.input, repeat: null, start: o.start, end: o.end });
        await api.skipOccurrence(moved.id);
        return { made, o };
      }
      return { made: await api.updateEvent(moved.id, { ...rec.input, start: o.start, end: o.end }), o };
    },
    onSuccess: ({ made, o }) => {
      void qc.invalidateQueries();
      push({ title: mode === "day" ? "That day was moved" : "Moved", description: range(o) });
      onMoved(made, rec?.input.invitees ?? []);
    },
    onError: (e) => push({ title: "Couldn't move it", description: errorMessage(e), tone: "danger" }),
  });
  const draft = useMutation({
    mutationFn: () => {
      const note = rescheduleNote(moved, options);
      return api.openMailDraft(organiserAddress(moved), note.subject, note.body);
    },
    onError: (e) => push({ title: "Couldn't open your mail app", description: errorMessage(e), tone: "danger" }),
  });
  const copy = () => {
    const note = rescheduleNote(moved, options);
    navigator.clipboard
      .writeText(`${note.subject}\n\n${note.body}`)
      .then(() => push({ title: "Copied" }))
      .catch(() => push({ title: "Couldn't copy that", tone: "danger" }));
  };

  if (options.length === 0) return <p className="text-body text-fg-2">There is no free time that long in your working hours over the next two weeks.</p>;
  return (
    <div className="flex flex-col gap-2">
      <ul aria-label={`Free times for ${moved.title}`} className="flex flex-col">
        {options.map((o) => (
          <li key={o.start} className="flex items-center justify-between gap-2 py-1.5">
            <span className="text-body text-fg tabular-nums">{range(o)}</span>
            {mode && (
              <Button variant="secondary" disabled={move.isPending} onClick={() => move.mutate(o)} aria-label={`${mode === "day" ? "Move this day here" : "Move here"}: ${range(o)}`}>
                {mode === "day" ? "Move this day here" : "Move here"}
              </Button>
            )}
          </li>
        ))}
      </ul>
      {mode === "day" && <p className="text-footnote text-fg-2">This moves only this day. The rest of the series stays where it is.</p>}
      {yours && rec && repeats && !mode && <p className="text-footnote text-fg-2">It repeats and is saved in another calendar, so change it from the event itself.</p>}
      {mode && (rec?.input.invitees.length ?? 0) > 0 && <p className="text-footnote text-fg-2">After moving it you can send the people you invited an updated invitation.</p>}
      {!yours && (
        <div className="flex flex-wrap items-center gap-2">
          <Button variant="primary" leading={<Mail className="size-4" aria-hidden />} disabled={draft.isPending} onClick={() => draft.mutate()}>
            Write the message
          </Button>
          <IconButton label="Copy the message" onClick={copy}>
            <Copy className="size-4" />
          </IconButton>
          <p className="basis-full text-footnote text-fg-2">This one is in someone else's calendar, so you'll need to ask. The message lists these times and doesn't mention the other meeting.</p>
        </div>
      )}
    </div>
  );
}

/** Two or more meetings overlap: which is easier to move, why, and where it could go. Suggests only; you decide. */
export function ConflictSheet({ a, b, onClose }: { a: string; b: string; onClose: () => void }) {
  const [moveId, setMoveId] = useState<string | null>(null);
  // After moving an event you invited people to, offer to tell them.
  const [invite, setInvite] = useState<{ event: CalEvent; invitees: string[] } | null>(null);
  const help = useQuery({ queryKey: ["conflict-help", a, b, moveId], queryFn: () => api.getConflictHelp(a, b, moveId), staleTime: 0, gcTime: 0 });
  const h = help.data ?? null;
  const pile = h?.pile_up ?? null;

  const moved = (made: CalEvent, invitees: string[]) => {
    if (invitees.length > 0) setInvite({ event: made, invitees });
    else onClose();
  };
  if (invite) return <InviteDialog event={invite.event} invitees={invite.invitees} onClose={onClose} />;

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()} title="Sort out an overlap" description="A suggestion only. Nothing changes unless you choose it.">
      {help.isPending ? (
        <p className="text-body text-fg-2">Looking…</p>
      ) : !h ? (
        <p className="text-body text-fg-2">These no longer overlap.</p>
      ) : pile ? (
        <div className="flex flex-col gap-4 pb-2">
          <div>
            <p className="text-body text-fg" role="status">
              {pile.events.length} meetings overlap each other. Moving {pile.moves.length === 1 ? "one" : pile.moves.length} clears it.
            </p>
            <p className="mt-1 text-body text-fg-2">Keep: {pile.events.filter((e) => pile.keep.includes(e.id)).map((e) => e.title).join(", ")}</p>
          </div>
          {pile.moves.map((m) => (
            <div key={m.event.id}>
              <h3 className="text-headline text-fg">
                Move <span className="font-semibold">{m.event.title}</span>
              </h3>
              <MoveCard moved={m.event} options={m.options} yours={m.yours} onMoved={moved} />
            </div>
          ))}
        </div>
      ) : (
        <div className="flex flex-col gap-4 pb-2">
          <p className="text-body text-fg">
            <span className="font-medium">{h.kept.title}</span> and <span className="font-medium">{h.moved.title}</span> overlap for {formatDuration(h.overlap_minutes)}.
          </p>
          <div>
            <p className="text-headline text-fg" role="status">
              Move <span className="font-semibold">{h.moved.title}</span>
            </p>
            <ul className="mt-1 list-disc pl-5 text-body text-fg-2">
              {h.reasons.map((r) => (
                <li key={r}>{REASON[r]}</li>
              ))}
            </ul>
            <Button className="mt-2" variant="secondary" leading={<ArrowRightLeft className="size-4" aria-hidden />} onClick={() => setMoveId(h.kept.id)}>
              Move {h.kept.title} instead
            </Button>
          </div>
          <div>
            <h3 className="text-footnote font-semibold uppercase tracking-wide text-fg-2">Free times for it</h3>
            <MoveCard moved={h.moved} options={h.options} yours={h.yours} onMoved={moved} />
          </div>
        </div>
      )}
    </Dialog>
  );
}
