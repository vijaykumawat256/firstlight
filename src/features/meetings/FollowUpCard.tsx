import { useQuery } from "@tanstack/react-query";
import { MessageSquareText } from "lucide-react";
import { useState } from "react";
import { api, errorMessage, type FollowUp } from "@/api";
import { Button, Card, useToasts } from "@/design";
import { formatTime } from "@/features/briefing/format";
import { NoteTasks } from "@/features/tasks/TaskDialogs";

const DISMISSED = "firstlight.followups.dismissed";

function readDismissed(): string[] {
  try {
    const v = JSON.parse(localStorage.getItem(DISMISSED) ?? "[]");
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string").slice(-50) : [];
  } catch {
    return [];
  }
}

/** The recap text, matching `followup::recap` (the person reads and edits it in their own mail app). */
export function recapOf(f: FollowUp): { subject: string; body: string } {
  let body = `Hi all,\n\nNotes from ${f.title}:\n\n${f.note.trim()}\n`;
  if (body.length > 1500) body = `${body.slice(0, 1499)}…`;
  return { subject: `Notes from ${f.title}`, body };
}

/** After a meeting you wrote notes in: turn them into tasks, or open a recap to everyone in your mail app. */
export function FollowUpCard() {
  const push = useToasts((s) => s.push);
  const items = useQuery({ queryKey: ["follow-ups"], queryFn: () => api.getFollowUps(), refetchInterval: 60_000, staleTime: 30_000 });
  const [dismissed, setDismissed] = useState<string[]>(readDismissed);
  const [lines, setLines] = useState<{ lines: string[]; meeting: string } | null>(null);
  const shown = (items.data ?? []).filter((f) => !dismissed.includes(`${f.event_id}@${f.end}`));
  if (shown.length === 0) return null;
  const dismiss = (f: FollowUp) => {
    const next = [...dismissed, `${f.event_id}@${f.end}`].slice(-50);
    setDismissed(next);
    try {
      localStorage.setItem(DISMISSED, JSON.stringify(next));
    } catch {
      // Remembering is a convenience; without storage it simply shows again next time.
    }
  };
  const recap = (f: FollowUp) => {
    const { subject, body } = recapOf(f);
    api.openMailDraft(f.people.join(", ") || null, subject, body).catch((e) => push({ title: errorMessage(e), tone: "danger" }));
  };
  return (
    <section aria-label="Follow up" className="flex flex-col gap-2">
      {shown.slice(0, 3).map((f) => (
        <Card key={f.event_id} className="flex items-start gap-3">
          <MessageSquareText className="mt-0.5 size-5 shrink-0 text-accent" aria-hidden />
          <div className="min-w-0 flex-1">
            <h2 className="text-headline font-semibold text-fg">Follow up on {f.title}</h2>
            <p className="text-footnote text-fg-2">Ended {formatTime(new Date(f.end))}. You wrote notes in it.</p>
            <div className="mt-2 flex flex-wrap gap-2">
              <Button size="sm" variant="secondary" onClick={() => void api.noteTaskLines(f.note).then((l) => (l.length ? setLines({ lines: l, meeting: f.title }) : push({ title: "No lines to turn into tasks" })))}>
                Make tasks
              </Button>
              <Button size="sm" variant="secondary" onClick={() => recap(f)}>
                {f.people.length ? `Email a recap to ${f.people.length === 1 ? "1 person" : `${f.people.length} people`}` : "Email a recap"}
              </Button>
              <Button size="sm" variant="plain" onClick={() => dismiss(f)} aria-label={`Done with ${f.title}`}>
                Done
              </Button>
            </div>
          </div>
        </Card>
      ))}
      {lines && <NoteTasks lines={lines.lines} meeting={lines.meeting} onClose={() => setLines(null)} />}
    </section>
  );
}
