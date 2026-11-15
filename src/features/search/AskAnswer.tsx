import { useQuery } from "@tanstack/react-query";
import { ChevronLeft, Copy, Sparkles } from "lucide-react";
import { api, type CalEvent, type CalendarAnswer, type FreeSlot } from "@/api";
import { Button, IconButton, useToasts } from "@/design";
import { formatDate, formatTime } from "@/features/briefing/format";

const day = (iso: string) => new Date(`${iso}T00:00:00`);
const startOf = (e: CalEvent): Date => (e.when.kind === "all_day" ? new Date(`${e.when.start}T00:00:00`) : new Date(e.when.start));

function headline(a: CalendarAnswer): string {
  const span = a.from === a.to ? formatDate(day(a.from)) : `${formatDate(day(a.from))} to ${formatDate(day(a.to))}`;
  if (a.kind === "free") {
    const n = a.slots.length;
    return n === 0 ? `Nothing free for ${a.minutes} minutes, ${span}.` : `${n} free ${n === 1 ? "stretch" : "stretches"} of ${a.minutes} minutes or more, ${span}.`;
  }
  const what = a.keywords.length ? ` matching “${a.keywords.join(" ")}”` : "";
  if (a.count === 0) return `No events${what}, ${span}.`;
  return `${a.count} ${a.count === 1 ? "event" : "events"}${what}, ${span}.`;
}

/** The answer to one question. Only the question went to the AI; everything shown is read from your own calendar here. */
export function AskAnswer({ question, onBack, onOpen }: { question: string; onBack: () => void; onOpen: (e: CalEvent) => void }) {
  const push = useToasts((s) => s.push);
  // Not kept: the question and its answer are dropped as soon as this closes.
  const reply = useQuery({ queryKey: ["ask", question], queryFn: () => api.askCalendar(question), gcTime: 0, staleTime: 0, retry: false });
  const a = reply.data?.answer ?? null;

  const copy = (s: FreeSlot) => {
    const text = `${formatDate(new Date(s.start))}, ${formatTime(new Date(s.start))} to ${formatTime(new Date(s.end))}`;
    navigator.clipboard
      .writeText(text)
      .then(() => push({ title: "Copied" }))
      .catch(() => push({ title: "Couldn't copy that", tone: "danger" }));
  };

  let body;
  if (reply.isPending) body = <p className="px-1 py-4 text-body text-fg-2">Working it out…</p>;
  else if (reply.isError) body = <p role="alert" className="px-1 py-4 text-body text-danger">Couldn’t ask. Try again.</p>;
  else if (!reply.data.enabled) body = <p className="px-1 py-4 text-body text-fg-2">Turn on AI summaries in Settings to ask questions.</p>;
  else if (reply.data.problem === "limit") body = <p role="alert" className="px-1 py-4 text-body text-danger">That’s a lot of questions for one hour. Try again later.</p>;
  else if (reply.data.problem === "ai") body = <p role="alert" className="px-1 py-4 text-body text-danger">Couldn’t reach the AI service. Check your key and connection in Settings.</p>;
  else if (!a) body = <p className="px-1 py-4 text-body text-fg-2">I couldn’t turn that into a search of your calendar. Try something like “when am I free for an hour this week?” or “when is my dentist appointment?”.</p>;
  else
    body = (
      <div className="flex flex-col gap-2">
        <p className="px-1 text-headline text-fg" role="status">{headline(a)}</p>
        {a.kind === "events" ? (
          <ul aria-label="Events" className="flex max-h-72 flex-col overflow-y-auto">
            {a.events.map((e) => (
              <li key={e.id}>
                <button type="button" onClick={() => onOpen(e)} className="flex min-h-12 w-full flex-col justify-center rounded-card px-3 py-2 text-left hover:bg-fill">
                  <span className="truncate text-headline text-fg">{e.title}</span>
                  <span className="truncate text-footnote text-fg-2 tabular-nums">
                    {formatDate(startOf(e))} · {e.when.kind === "all_day" ? "All day" : formatTime(startOf(e))}
                  </span>
                </button>
              </li>
            ))}
            {a.count > a.events.length && <li className="px-3 py-2 text-footnote text-fg-2">Showing the first {a.events.length}.</li>}
          </ul>
        ) : (
          <ul aria-label="Free times" className="flex max-h-72 flex-col overflow-y-auto">
            {a.slots.map((s) => (
              <li key={s.start} className="flex items-center justify-between gap-2 px-3 py-1">
                <span className="text-body text-fg tabular-nums">
                  {formatDate(new Date(s.start))}, {formatTime(new Date(s.start))} to {formatTime(new Date(s.end))}
                </span>
                <IconButton label={`Copy ${formatDate(new Date(s.start))} ${formatTime(new Date(s.start))}`} onClick={() => copy(s)}>
                  <Copy className="size-4" />
                </IconButton>
              </li>
            ))}
          </ul>
        )}
      </div>
    );

  return (
    <div className="flex flex-col gap-3 pb-2">
      <div className="flex items-center gap-2">
        <Button variant="secondary" leading={<ChevronLeft className="size-4" />} onClick={onBack}>
          Back
        </Button>
        <p className="flex min-w-0 items-center gap-1.5 text-body text-fg-2">
          <Sparkles className="size-4 shrink-0 text-accent" aria-hidden />
          <span className="truncate">{question}</span>
        </p>
      </div>
      {body}
      <p className="px-1 text-footnote text-fg-3">Only your question was sent to the AI. The answer is read from your calendar on this computer.</p>
    </div>
  );
}
