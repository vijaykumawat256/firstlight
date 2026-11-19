import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CalendarClock, Pencil } from "lucide-react";
import { useState } from "react";
import { api, errorMessage, type Task, type TaskInput, type TaskView } from "@/api";
import { IconButton, TextField, useToasts } from "@/design";
import { formatDate, formatTime } from "@/features/briefing/format";
import { parseQuickTask, type QuickTask } from "@/features/events/quickAdd";

export const TASKS_KEY = ["tasks"];

export function useTaskLists() {
  return useQuery({ queryKey: TASKS_KEY, queryFn: () => api.listTasks(), staleTime: 10_000, refetchInterval: 60_000 });
}

/** Anything that changes tasks also refreshes the briefing and the calendar (a planned task adds an event). */
export function useTaskChanged() {
  const qc = useQueryClient();
  return () => void qc.invalidateQueries();
}

/** "Today, 3:00 PM", "Fri 20 Nov", or null when the task has no day. */
export function dueText(t: Task): string | null {
  if (!t.due) return null;
  if (t.due.kind === "date") return formatDate(new Date(`${t.due.date}T00:00:00`));
  const d = new Date(t.due.at);
  return `${formatDate(d)}, ${formatTime(d)}`;
}

export function quickToTaskInput(q: QuickTask): TaskInput {
  return { title: q.title, notes: null, due_date: q.dueDate, due_time: q.dueTime, minutes: null };
}

/** The line under a task's title: when it is due, how long it has waited, where it came from. */
function meta(t: TaskView): string {
  const parts: string[] = [];
  if (t.done_at) parts.push(`Done ${formatDate(new Date(t.done_at))}`);
  else if (t.days_late === 1) parts.push("From yesterday");
  else if (t.days_late > 1) parts.push(`Waiting ${t.days_late} days`);
  else {
    const due = dueText(t);
    if (due) parts.push(due);
  }
  if (t.minutes) parts.push(t.minutes >= 60 ? `${t.minutes / 60} h` : `${t.minutes} min`);
  if (t.from_meeting) parts.push(`From ${t.from_meeting}`);
  if (t.planned_event && !t.done_at) parts.push("Time set aside");
  return parts.join(" · ");
}

/** One task: tick it off, set time aside for it, or open it to change it. */
export function TaskRow({ task, onEdit, onPlan }: { task: TaskView; onEdit: (t: TaskView) => void; onPlan?: (t: TaskView) => void }) {
  const push = useToasts((s) => s.push);
  const changed = useTaskChanged();
  const toggle = useMutation({
    mutationFn: (done: boolean) => api.setTaskDone(task.id, done),
    onSuccess: (t) => {
      changed();
      if (t.done_at) push({ title: `Done: ${t.title}` });
    },
    onError: (e) => push({ title: errorMessage(e), tone: "danger" }),
  });
  const done = !!task.done_at;
  const line = meta(task);
  return (
    <li className="flex min-h-12 items-center gap-3 px-4 py-2">
      <input
        type="checkbox"
        aria-label={`${done ? "Not done" : "Done"}: ${task.title}`}
        checked={done}
        disabled={toggle.isPending}
        onChange={(e) => toggle.mutate(e.target.checked)}
        className="size-5 shrink-0 cursor-pointer accent-accent"
      />
      <div className="min-w-0 flex-1">
        <div className={`break-words text-headline ${done ? "text-fg-2 line-through" : "text-fg"}`}>{task.title}</div>
        {line && <div className={`text-footnote ${task.days_late > 0 && !done ? "text-warning" : "text-fg-2"}`}>{line}</div>}
      </div>
      {onPlan && !done && (
        <IconButton label={`Set time aside for ${task.title}`} onClick={() => onPlan(task)}>
          <CalendarClock className="size-4" />
        </IconButton>
      )}
      <IconButton label={`Change ${task.title}`} onClick={() => onEdit(task)}>
        <Pencil className="size-4" />
      </IconButton>
    </li>
  );
}

/** A one-line box to add a task: "call the bank friday 3pm". Shows how it was read before it is added. */
export function AddTaskField({ label = "Add a task", autoFocus }: { label?: string; autoFocus?: boolean }) {
  const push = useToasts((s) => s.push);
  const changed = useTaskChanged();
  const [text, setText] = useState("");
  const read = parseQuickTask(text, new Date());
  const add = useMutation({
    mutationFn: () => api.addTask(quickToTaskInput(read!)),
    onSuccess: () => {
      setText("");
      changed();
    },
    onError: (e) => push({ title: errorMessage(e), tone: "danger" }),
  });
  const when = read?.dueDate ? `${formatDate(new Date(`${read.dueDate}T00:00:00`))}${read.dueTime ? `, ${formatTime(new Date(`${read.dueDate}T${read.dueTime}`))}` : ""}` : null;
  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        if (read && !add.isPending) add.mutate();
      }}
    >
      <TextField
        label={label}
        value={text}
        onChange={(e) => setText(e.target.value)}
        maxLength={300}
        placeholder="Call the bank friday 3pm"
        hint={read && text.trim() ? `Adds “${read.title}”${when ? `, due ${when}` : ""}. Press Return.` : "Add a day or time if it has one."}
        autoComplete="off"
        autoFocus={autoFocus}
      />
    </form>
  );
}
