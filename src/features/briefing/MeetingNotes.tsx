import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useId, useRef, useState } from "react";
import { api, errorMessage, type CalEvent, type MeetingNote } from "@/api";
import { Button } from "@/design";
import { NoteTasks } from "@/features/tasks/TaskDialogs";
import { formatDate, formatTime } from "./format";

const SAVE_AFTER_MS = 800;
const MAX_CHARS = 10_000;

/**
 * Your private notes for this meeting. One note per meeting: every time it happens (a repeating meeting, or the next
 * one with the same title) shows the same note, so what you wrote last time is there when it comes round again.
 * Saved as you type, kept only on this device, never sent anywhere (not to AI, not to the calendar).
 */
export function MeetingNotes({ event }: { event: CalEvent }) {
  const id = useId();
  const qc = useQueryClient();
  const key = ["meeting-note", event.id];
  const note = useQuery({ queryKey: key, queryFn: () => api.getMeetingNote(event.id), staleTime: 0 });
  const [text, setText] = useState<string | null>(null);
  const [lines, setLines] = useState<string[] | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const latest = useRef<string | null>(null);

  const save = useMutation({
    mutationFn: (t: string) => api.saveMeetingNote(event.id, t),
    onSuccess: (saved: MeetingNote | null) => qc.setQueryData(key, saved),
  });

  // What is stored, kept in a ref so the save-on-close below sees the latest without re-subscribing.
  const stored = useRef("");
  useEffect(() => {
    stored.current = note.data?.text ?? "";
  }, [note.data]);
  const { mutate } = save;
  const flush = useCallback(() => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
    if (latest.current !== null && latest.current !== stored.current) mutate(latest.current);
    latest.current = null;
  }, [mutate]);
  // Whatever was typed is kept when the sheet closes before the pause runs out.
  useEffect(() => flush, [flush]);

  if (note.isPending) return null;
  const shown = text ?? note.data?.text ?? "";
  const status = save.isPending
    ? "Saving…"
    : save.isError
      ? `Not saved: ${errorMessage(save.error)}`
      : note.data
        ? `Saved ${formatDate(new Date(note.data.updated_at))}, ${formatTime(new Date(note.data.updated_at))}`
        : "Only on this device. It comes back every time this meeting does.";

  return (
    <section className="flex flex-col gap-1.5">
      <label htmlFor={id} className="text-footnote font-semibold uppercase tracking-wide text-fg-2">
        Your notes
      </label>
      <textarea
        id={id}
        rows={3}
        maxLength={MAX_CHARS}
        value={shown}
        placeholder="Points to raise, what was agreed, what to follow up…"
        onChange={(e) => {
          setText(e.target.value);
          latest.current = e.target.value;
          if (timer.current) clearTimeout(timer.current);
          timer.current = setTimeout(flush, SAVE_AFTER_MS);
        }}
        onBlur={flush}
        className="select-text rounded-card bg-fill px-3 py-2 text-body text-fg placeholder:text-fg-3"
      />
      <div className="flex items-center justify-between gap-2">
        <p role="status" className={`text-footnote ${save.isError ? "text-danger" : "text-fg-2"}`}>
          {status}
        </p>
        {shown.trim() && (
          <Button variant="plain" size="sm" onClick={() => void api.noteTaskLines(shown).then((l) => l.length > 0 && setLines(l))}>
            Make tasks
          </Button>
        )}
      </div>
      {lines && <NoteTasks lines={lines} meeting={event.title} onClose={() => setLines(null)} />}
    </section>
  );
}
