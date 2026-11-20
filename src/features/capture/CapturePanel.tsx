import { useMutation } from "@tanstack/react-query";
import { CalendarPlus, ListPlus } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api, errorMessage } from "@/api";
import { Segmented } from "@/design";
import { formatDate, formatTime } from "@/features/briefing/format";
import { parseQuickAdd, parseQuickTask, quickToInput, taskLine } from "@/features/events/quickAdd";
import { quickToTaskInput, whereTask } from "@/features/tasks/shared";

type Kind = "task" | "event";

/**
 * The quick capture box (its own small window, opened by a shortcut from any app). One line becomes a task, or an
 * event of your own. Shows how it was read before Return; Escape closes it. Nothing is sent anywhere.
 */
export function CapturePanel() {
  const input = useRef<HTMLInputElement>(null);
  const [text, setText] = useState("");
  const [kind, setKind] = useState<Kind>("task");
  const [done, setDone] = useState<string | null>(null);
  const reset = () => {
    setText("");
    setDone(null);
    setKind("task");
    input.current?.focus();
  };
  useEffect(() => {
    input.current?.focus();
    let off: (() => void) | undefined;
    let cancelled = false;
    void api.onCaptureShown(reset).then((u) => (cancelled ? u() : (off = u)));
    // Clicking into another app hides the box, so it never floats over everything (or a screen share).
    const away = () => void api.hideCapture();
    window.addEventListener("blur", away);
    return () => {
      cancelled = true;
      off?.();
      window.removeEventListener("blur", away);
    };
  }, []);
  const now = new Date();
  // "task …" / "todo …" always means a task.
  const forced = taskLine(text);
  const asTask = parseQuickTask(forced ?? text, now);
  const asEvent = forced ? null : parseQuickAdd(text, now);
  const chosen: Kind = forced ? "task" : kind;
  const add = useMutation({
    mutationFn: async () => {
      if (chosen === "task") {
        const t = await api.addTask(quickToTaskInput(asTask!));
        return `Added to ${whereTask(t).text}: ${t.title}`;
      }
      const e = await api.createEvent(quickToInput(asEvent!));
      return `Added to your calendar: ${e.title}`;
    },
    onSuccess: (msg) => {
      setDone(msg);
      setText("");
      setTimeout(() => {
        setDone(null);
        void api.hideCapture();
      }, 900);
    },
  });
  const close = () => {
    setText("");
    void api.hideCapture();
  };
  const preview =
    chosen === "task"
      ? asTask && text.trim()
        ? `Task “${asTask.title}”${asTask.dueDate ? `, due ${formatDate(new Date(`${asTask.dueDate}T00:00:00`))}${asTask.dueTime ? ` at ${formatTime(new Date(`${asTask.dueDate}T${asTask.dueTime}`))}` : ""}` : ", for today"}`
        : "Type a task, like “call the bank friday 3pm”."
      : asEvent
        ? `Event “${asEvent.title}”, ${formatDate(asEvent.start)}${asEvent.allDay ? ", all day" : ` at ${formatTime(asEvent.start)}`}${asEvent.location ? `, ${asEvent.location}` : ""}`
        : text.trim()
          ? "Add a day or a time for an event, like “lunch with Riya friday 1pm”."
          : "Type an event, like “lunch with Riya friday 1pm”.";
  const ready = chosen === "task" ? !!asTask && !!text.trim() : !!asEvent;
  return (
    <main className="flex h-screen flex-col justify-center p-2" onKeyDown={(e) => e.key === "Escape" && close()}>
      <div className="flex flex-col gap-2.5 rounded-sheet bg-surface-raised p-4 shadow-float">
        <h1 className="sr-only">Quick capture</h1>
        <form
          className="flex items-center gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            if (ready && !add.isPending) add.mutate();
          }}
        >
          {chosen === "task" ? <ListPlus className="size-5 shrink-0 text-accent" aria-hidden /> : <CalendarPlus className="size-5 shrink-0 text-accent" aria-hidden />}
          <input
            ref={input}
            aria-label="Quick capture"
            value={text}
            maxLength={300}
            autoComplete="off"
            spellCheck={false}
            placeholder="Add a task or an event…"
            onChange={(e) => {
              setText(e.target.value);
              add.reset();
            }}
            onKeyDown={(e) => {
              // Tab switches between task and event, so hands stay on the keyboard.
              if (e.key === "Tab" && !forced) {
                e.preventDefault();
                setKind(kind === "task" ? "event" : "task");
              }
            }}
            className="min-w-0 flex-1 bg-transparent text-title text-fg outline-none placeholder:text-fg-3"
          />
        </form>
        <div className="flex items-center justify-between gap-3">
          <p role="status" className={`min-w-0 truncate text-footnote ${add.isError ? "text-danger" : "text-fg-2"}`}>
            {done ?? (add.isError ? errorMessage(add.error) : preview)}
          </p>
          {!forced && (
            <Segmented
              label="Add as"
              value={kind}
              onValueChange={(v) => setKind(v as Kind)}
              options={[
                { value: "task", label: "Task" },
                { value: "event", label: "Event" },
              ]}
            />
          )}
        </div>
        <p className="text-caption text-fg-3">Return adds · Tab switches task and event · Esc closes</p>
      </div>
    </main>
  );
}
