import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { BellRing, CalendarClock, Mail, Pencil } from "lucide-react";
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

const REPEAT_TEXT: Record<NonNullable<Task["repeat"]>, string> = {
  daily: "Every day",
  weekdays: "Every weekday",
  weekly: "Every week",
  monthly: "Every month",
  last_workday: "Last working day of the month",
};

/** Whole days since `iso`. */
export const daysSince = (iso: string) => Math.max(0, Math.floor((Date.now() - Date.parse(iso)) / 86_400_000));

/** The line under a task's title: when it is due, how long it has waited, where it came from. */
function meta(t: TaskView): string {
  const parts: string[] = [];
  if (t.waiting_on && !t.done_at) {
    const days = daysSince(t.waiting_since ?? t.created_at);
    parts.push(`Waiting on ${t.waiting_on}${days > 0 ? ` · ${days} ${days === 1 ? "day" : "days"}` : ""}`);
  }
  if (t.done_at) parts.push(`Done ${formatDate(new Date(t.done_at))}`);
  else if (t.days_late === 1) parts.push("From yesterday");
  else if (t.days_late > 1) parts.push(`Waiting ${t.days_late} days`);
  else {
    const due = dueText(t);
    if (due) parts.push(due);
  }
  if (t.minutes) parts.push(t.minutes >= 60 ? `${t.minutes / 60} h` : `${t.minutes} min`);
  if (t.repeat) parts.push(REPEAT_TEXT[t.repeat]);
  if (t.spent_minutes > 0) parts.push(`${t.spent_minutes >= 60 ? `${Math.floor(t.spent_minutes / 60)} h ${t.spent_minutes % 60 ? `${t.spent_minutes % 60} min ` : ""}` : `${t.spent_minutes} min `}spent`);
  if (t.from_meeting) parts.push(`From ${t.from_meeting}`);
  if (t.mail) parts.push(`Email from ${t.mail.from || "your inbox"}`);
  if (t.planned_event && !t.done_at) parts.push("Time set aside");
  return parts.join(" · ");
}

/** One task: tick it off, set time aside for it, or open it to change it. */
export function TaskRow({ task, onEdit, onPlan, onNudge }: { task: TaskView; onEdit: (t: TaskView) => void; onPlan?: (t: TaskView) => void; onNudge?: (t: TaskView) => void }) {
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
      {task.mail && task.mail.open.kind !== "none" && (
        <IconButton label={`Open the email for ${task.title}`} onClick={() => void api.openMailMessage(task.mail!.open).catch((e) => push({ title: errorMessage(e), tone: "danger" }))}>
          <Mail className="size-4" />
        </IconButton>
      )}
      {onNudge && task.waiting_on && !done && (
        <IconButton label={`Nudge ${task.waiting_on} about ${task.title}`} onClick={() => onNudge(task)}>
          <BellRing className="size-4" />
        </IconButton>
      )}
      {onPlan && !done && !task.waiting_on && (
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
