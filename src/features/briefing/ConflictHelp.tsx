import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowRightLeft, Copy, Mail } from "lucide-react";
import { useState } from "react";
import { api, errorMessage, type CalEvent, type ConflictHelp as Help, type ConflictReason } from "@/api";
import { Button, Dialog, IconButton, useToasts } from "@/design";
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

/** Two meetings overlap: which is easier to move, why, and where it could go. Suggests only; you decide. */
export function ConflictSheet({ a, b, onClose }: { a: string; b: string; onClose: () => void }) {
  const push = useToasts((s) => s.push);
  const qc = useQueryClient();
  const [moveId, setMoveId] = useState<string | null>(null);
  const help = useQuery({ queryKey: ["conflict-help", a, b, moveId], queryFn: () => api.getConflictHelp(a, b, moveId), staleTime: 0, gcTime: 0 });
  const h = help.data ?? null;
  const own = useQuery({
    queryKey: ["own-event", h?.moved.id],
    queryFn: () => api.getOwnEvent(h!.moved.id),
    enabled: !!h?.yours,
  });
  const canMove = !!h?.yours && !!own.data && !own.data.input.repeat;

  const move = useMutation({
    mutationFn: async (o: Help["options"][number]) => {
      const rec = await api.getOwnEvent(h!.moved.id);
      if (!rec) throw new Error("That event no longer exists.");
      await api.updateEvent(h!.moved.id, { ...rec.input, start: o.start, end: o.end });
      return o;
    },
    onSuccess: (o) => {
      void qc.invalidateQueries();
      push({ title: "Moved", description: range(o) });
      onClose();
    },
    onError: (e) => push({ title: "Couldn't move it", description: errorMessage(e), tone: "danger" }),
  });
  const draft = useMutation({
    mutationFn: () => {
      const note = rescheduleNote(h!.moved, h!.options);
      return api.openMailDraft(organiserAddress(h!.moved), note.subject, note.body);
    },
    onError: (e) => push({ title: "Couldn't open your mail app", description: errorMessage(e), tone: "danger" }),
  });
  const copy = () => {
    const note = rescheduleNote(h!.moved, h!.options);
    navigator.clipboard
      .writeText(`${note.subject}\n\n${note.body}`)
      .then(() => push({ title: "Copied" }))
      .catch(() => push({ title: "Couldn't copy that", tone: "danger" }));
  };

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()} title="Sort out an overlap" description="A suggestion only. Nothing changes unless you choose it.">
      {help.isPending ? (
        <p className="text-body text-fg-2">Looking…</p>
      ) : !h ? (
        <p className="text-body text-fg-2">These no longer overlap.</p>
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

          {h.options.length === 0 ? (
            <p className="text-body text-fg-2">There is no free time that long in your working hours over the next two weeks.</p>
          ) : (
            <div>
              <h3 className="text-footnote font-semibold uppercase tracking-wide text-fg-2">Free times for it</h3>
              <ul aria-label="Free times" className="mt-1 flex flex-col">
                {h.options.map((o) => (
                  <li key={o.start} className="flex items-center justify-between gap-2 py-1.5">
                    <span className="text-body text-fg tabular-nums">{range(o)}</span>
                    {canMove && (
                      <Button variant="secondary" disabled={move.isPending} onClick={() => move.mutate(o)} aria-label={`Move here: ${range(o)}`}>
                        Move here
                      </Button>
                    )}
                  </li>
                ))}
              </ul>
              {h.yours && own.data && own.data.input.repeat && <p className="mt-1 text-footnote text-fg-2">It repeats, so change it from the event itself.</p>}
              {canMove && own.data && own.data.input.invitees.length > 0 && (
                <p className="mt-1 text-footnote text-fg-2">The people you invited aren't told automatically. Send the invitation again after moving it.</p>
              )}
            </div>
          )}

          {!h.yours && h.options.length > 0 && (
            <div>
              <h3 className="text-footnote font-semibold uppercase tracking-wide text-fg-2">Ask to move it</h3>
              <p className="mt-1 text-body text-fg-2">
                This one belongs to someone else's calendar, so you'll need to ask. The message lists these times and doesn't mention the other meeting.
              </p>
              <div className="mt-2 flex flex-wrap items-center gap-2">
                <Button variant="primary" leading={<Mail className="size-4" aria-hidden />} disabled={draft.isPending} onClick={() => draft.mutate()}>
                  Write the message
                </Button>
                <IconButton label="Copy the message" onClick={copy}>
                  <Copy className="size-4" />
                </IconButton>
              </div>
            </div>
          )}
        </div>
      )}
    </Dialog>
  );
}
