import { useMutation, useQuery } from "@tanstack/react-query";
import { useId, useState } from "react";
import { api, errorMessage, type FreeSlot, type Task, type TaskInput, type TaskRepeat } from "@/api";
import { Button, Dialog, Segmented, Select, TextField, useToasts } from "@/design";
import { formatDate, formatTime } from "@/features/briefing/format";
import { SendBar } from "@/features/meetings/SendBar";
import { daysSince, useTaskChanged } from "./shared";

const pad = (n: number) => String(n).padStart(2, "0");
const ymd = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;

function inputOf(t: Task): TaskInput {
  const keep = { title: t.title, notes: t.notes, minutes: t.minutes, repeat: t.repeat, waiting_on: t.waiting_on };
  if (!t.due) return { ...keep, due_date: null, due_time: null };
  if (t.due.kind === "date") return { ...keep, due_date: t.due.date, due_time: null };
  const d = new Date(t.due.at);
  return { ...keep, due_date: ymd(d), due_time: `${pad(d.getHours())}:${pad(d.getMinutes())}` };
}

export const REPEATS: { value: TaskRepeat | "none"; label: string }[] = [
  { value: "none", label: "Doesn't repeat" },
  { value: "daily", label: "Every day" },
  { value: "weekdays", label: "Every weekday" },
  { value: "weekly", label: "Every week" },
  { value: "monthly", label: "Every month" },
  { value: "last_workday", label: "Last working day of the month" },
];

const LENGTHS = [
  { value: "none", label: "Not set" },
  { value: "15", label: "15 min" },
  { value: "30", label: "30 min" },
  { value: "60", label: "1 hour" },
  { value: "120", label: "2 hours" },
];

/** Change a task: title, day, time, how long it takes, notes. Or delete it. */
export function TaskForm({ task, onClose }: { task: Task; onClose: () => void }) {
  const push = useToasts((s) => s.push);
  const changed = useTaskChanged();
  const notesId = useId();
  const [d, setD] = useState<TaskInput>(() => inputOf(task));
  const save = useMutation({
    mutationFn: () => api.updateTask(task.id, d),
    onSuccess: () => {
      changed();
      onClose();
      push({ title: "Task updated" });
    },
  });
  const remove = useMutation({
    mutationFn: () => api.deleteTask(task.id),
    onSuccess: () => {
      changed();
      onClose();
      push({ title: "Task deleted" });
    },
    onError: (e) => push({ title: errorMessage(e), tone: "danger" }),
  });
  const minutes = d.minutes === null ? "none" : String(d.minutes);
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()} title="Change task" description="Kept only on this device.">
      <form
        className="flex flex-col gap-4 pb-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (d.title.trim() && !save.isPending) save.mutate();
        }}
      >
        {save.isError && (
          <p role="alert" className="rounded-card bg-danger-soft px-3 py-2 text-body text-danger">
            {errorMessage(save.error)}
          </p>
        )}
        <TextField label="Task" value={d.title} maxLength={300} autoComplete="off" onChange={(e) => setD({ ...d, title: e.target.value })} autoFocus />
        <div className="grid grid-cols-2 gap-3">
          <TextField label="Due" type="date" value={d.due_date ?? ""} onChange={(e) => setD({ ...d, due_date: e.target.value || null, due_time: e.target.value ? d.due_time : null })} />
          <TextField label="Time" type="time" value={d.due_time ?? ""} disabled={!d.due_date} onChange={(e) => setD({ ...d, due_time: e.target.value || null })} />
        </div>
        <div className="flex items-center justify-between gap-3">
          <span className="text-body text-fg">Repeats</span>
          <Select
            label="Repeats"
            value={d.repeat ?? "none"}
            onValueChange={(v) => {
              const repeat = v === "none" ? null : (v as TaskRepeat);
              // A repeating task needs a day: start it today when none is set.
              setD({ ...d, repeat, due_date: repeat && !d.due_date ? ymd(new Date()) : d.due_date });
            }}
            options={REPEATS}
          />
        </div>
        <TextField
          label="Waiting on"
          value={d.waiting_on ?? ""}
          maxLength={100}
          autoComplete="off"
          placeholder="Name or email address"
          hint="Someone else has the next step. It moves to Waiting, and you get a reminder to nudge them."
          onChange={(e) => setD({ ...d, waiting_on: e.target.value || null })}
        />
        <div className="flex flex-col gap-1.5">
          <span className="text-footnote font-medium text-fg-2">How long it takes</span>
          <Segmented label="How long it takes" value={LENGTHS.some((l) => l.value === minutes) ? minutes : "none"} onValueChange={(v) => setD({ ...d, minutes: v === "none" ? null : Number(v) })} options={LENGTHS} />
        </div>
        <div className="flex flex-col gap-1.5">
          <label htmlFor={notesId} className="text-footnote font-medium text-fg-2">
            Notes
          </label>
          <textarea id={notesId} rows={3} maxLength={4000} value={d.notes ?? ""} onChange={(e) => setD({ ...d, notes: e.target.value || null })} className="select-text rounded-card bg-fill px-3 py-2 text-body text-fg" />
        </div>
        {task.from_meeting && <p className="text-footnote text-fg-2">Written down in {task.from_meeting}.</p>}
        {task.mail && (
          <p className="text-footnote text-fg-2">
            From an email: “{task.mail.subject}”{task.mail.from ? `, ${task.mail.from}` : ""}.
          </p>
        )}
        <div className="flex justify-between gap-2">
          <Button variant="plain" onClick={() => remove.mutate()} disabled={remove.isPending}>
            Delete
          </Button>
          <div className="flex gap-2">
            <Button onClick={onClose}>Cancel</Button>
            <Button variant="primary" type="submit" disabled={!d.title.trim() || save.isPending}>
              {save.isPending ? "Saving…" : "Save"}
            </Button>
          </div>
        </div>
      </form>
    </Dialog>
  );
}

/** Set time aside for a task: pick one of the free stretches in your working hours. */
export function PlanTask({ task, onClose }: { task: Task; onClose: () => void }) {
  const push = useToasts((s) => s.push);
  const changed = useTaskChanged();
  const [minutes, setMinutes] = useState(String(task.minutes ?? 30));
  const options = LENGTHS.filter((l) => l.value !== "none");
  if (!options.some((o) => o.value === minutes)) options.push({ value: minutes, label: `${minutes} min` });
  const slots = useQuery({ queryKey: ["free-times", minutes, "7"], queryFn: () => api.findFreeTimes(Number(minutes), 7), staleTime: 0 });
  const plan = useMutation({
    mutationFn: (s: FreeSlot) => {
      const start = new Date(s.start);
      return api.planTask(task.id, start.toISOString(), new Date(start.getTime() + Number(minutes) * 60_000).toISOString());
    },
    onSuccess: () => {
      changed();
      onClose();
      push({ title: "Time set aside in your calendar" });
    },
    onError: (e) => push({ title: errorMessage(e), tone: "danger" }),
  });
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()} title="Set time aside" description={`For “${task.title}”. It is added to your calendar on this device only.`}>
      <div className="pb-3">
        <Segmented label="How long" value={minutes} onValueChange={setMinutes} options={options} />
      </div>
      {slots.isPending ? (
        <p className="text-body text-fg-2">Looking…</p>
      ) : (slots.data ?? []).length === 0 ? (
        <p className="text-body text-fg-2">Nothing free that long in the next week.</p>
      ) : (
        <ul className="flex flex-col gap-1">
          {(slots.data ?? []).slice(0, 8).map((s) => (
            <li key={s.start}>
              <Button className="w-full justify-start" disabled={plan.isPending} onClick={() => plan.mutate(s)}>
                {formatDate(new Date(s.start))}, {formatTime(new Date(s.start))}
              </Button>
            </li>
          ))}
        </ul>
      )}
    </Dialog>
  );
}

/** Turn lines of a meeting note into tasks: tick the ones that are things to do. */
export function NoteTasks({ lines, meeting, onClose }: { lines: string[]; meeting: string; onClose: () => void }) {
  const push = useToasts((s) => s.push);
  const changed = useTaskChanged();
  const [picked, setPicked] = useState<boolean[]>(() => lines.map(() => true));
  const add = useMutation({
    mutationFn: () => api.addTasksFromNote(lines.filter((_, i) => picked[i]), meeting),
    onSuccess: (made) => {
      changed();
      onClose();
      push({ title: made.length === 1 ? "1 task added" : `${made.length} tasks added` });
    },
    onError: (e) => push({ title: errorMessage(e), tone: "danger" }),
  });
  const count = picked.filter(Boolean).length;
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()} title="Make tasks" description="Tick the lines that are things to do. They go on your task list.">
      <ul className="flex max-h-72 flex-col gap-2 overflow-y-auto pb-3">
        {lines.map((l, i) => (
          <li key={i}>
            <label className="flex items-start gap-3 text-body text-fg">
              <input type="checkbox" className="mt-1 size-4 accent-accent" checked={picked[i]} onChange={(e) => setPicked(picked.map((p, j) => (j === i ? e.target.checked : p)))} />
              <span className="break-words">{l}</span>
            </label>
          </li>
        ))}
      </ul>
      <div className="flex justify-end gap-2 pb-2">
        <Button onClick={onClose}>Cancel</Button>
        <Button variant="primary" disabled={count === 0 || add.isPending} onClick={() => add.mutate()}>
          {count === 1 ? "Add 1 task" : `Add ${count} tasks`}
        </Button>
      </div>
    </Dialog>
  );
}

const EMAIL = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

/** The text of a polite nudge to someone a task is waiting on. */
export function nudgeMessage(t: Task, days: number): { subject: string; body: string } {
  const who = t.waiting_on && !EMAIL.test(t.waiting_on) ? t.waiting_on.split(" ")[0] : null;
  return {
    subject: `Checking in: ${t.title}`,
    body: `Hi${who ? ` ${who}` : ""},\n\nJust checking in on “${t.title}”${days >= 2 ? ` from ${days} days ago` : ""}. Is there anything you need from me?\n\nThanks!`,
  };
}

/** Nudge someone a task is waiting on: the full message first, then send it, open it in your mail app, or copy it. */
export function NudgeDialog({ task, onClose }: { task: Task; onClose: () => void }) {
  const push = useToasts((s) => s.push);
  const [days] = useState(() => (task.waiting_since ? daysSince(task.waiting_since) : 0));
  const { subject, body } = nudgeMessage(task, days);
  const to = task.waiting_on && EMAIL.test(task.waiting_on) ? [task.waiting_on] : [];
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()} title="Nudge" description={`Waiting on ${task.waiting_on ?? "someone"}${days > 0 ? ` for ${days} ${days === 1 ? "day" : "days"}` : ""}.`}>
      <div className="flex flex-col gap-3 pb-2">
        <div className="rounded-card bg-fill px-3 py-2">
          <p className="text-footnote font-medium text-fg-2">{subject}</p>
          <p className="whitespace-pre-wrap text-body text-fg select-text">{body}</p>
        </div>
        {to.length > 0 ? (
          <SendBar to={to} subject={subject} body={body} onDone={onClose} onCancel={onClose} />
        ) : (
          <div className="flex justify-end gap-2">
            <Button onClick={onClose}>Close</Button>
            <Button
              variant="primary"
              onClick={() =>
                navigator.clipboard
                  .writeText(body)
                  .then(() => push({ title: "Copied" }))
                  .catch(() => push({ title: "Couldn't copy", tone: "danger" }))
              }
            >
              Copy message
            </Button>
          </div>
        )}
      </div>
    </Dialog>
  );
}

/** This month's last Monday-to-Friday day, or next month's once this one has passed. YYYY-MM-DD. */
export function nextLastWorkday(now = new Date()): string {
  const last = (y: number, m: number) => {
    const d = new Date(y, m + 1, 0, 12);
    while (d.getDay() === 0 || d.getDay() === 6) d.setDate(d.getDate() - 1);
    return d;
  };
  let d = last(now.getFullYear(), now.getMonth());
  if (ymd(d) < ymd(now)) d = last(now.getFullYear(), now.getMonth() + 1);
  return ymd(d);
}

const MONTH_END = ["Submit your timesheet", "File your expenses", "Send the monthly report", "Plan next month"];

/** Ready-made month-end reminders: tasks due on the last working day of every month, at 10:00. */
export function MonthEndDialog({ existing, onClose }: { existing: string[]; onClose: () => void }) {
  const push = useToasts((s) => s.push);
  const changed = useTaskChanged();
  const offered = MONTH_END.filter((t) => !existing.includes(t));
  const [picked, setPicked] = useState<string[]>(() => offered.slice(0, 2));
  const [own, setOwn] = useState("");
  const titles = [...picked, ...(own.trim() ? [own.trim()] : [])];
  const due = nextLastWorkday();
  const add = useMutation({
    mutationFn: async () => {
      for (const title of titles) await api.addTask({ title, notes: null, due_date: due, due_time: "10:00", minutes: null, repeat: "last_workday" });
    },
    onSuccess: () => {
      changed();
      onClose();
      push({ title: titles.length === 1 ? "Month-end reminder added" : `${titles.length} month-end reminders added` });
    },
    onError: (e) => push({ title: errorMessage(e), tone: "danger" }),
  });
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()} title="Month-end reminders" description={`Tasks that come back on the last working day of every month, at 10:00. The first is ${formatDate(new Date(`${due}T00:00:00`))}.`}>
      <div className="flex flex-col gap-3 pb-2">
        {offered.length > 0 ? (
          <ul className="flex flex-col gap-2">
            {offered.map((t) => (
              <li key={t}>
                <label className="flex items-center gap-3 text-body text-fg">
                  <input type="checkbox" className="size-4 accent-accent" checked={picked.includes(t)} onChange={(e) => setPicked(e.target.checked ? [...picked, t] : picked.filter((p) => p !== t))} />
                  {t}
                </label>
              </li>
            ))}
          </ul>
        ) : (
          <p className="text-body text-fg-2">You have all the usual ones already.</p>
        )}
        <TextField label="Another one" value={own} maxLength={300} autoComplete="off" placeholder="Pay the team's software bills" onChange={(e) => setOwn(e.target.value)} />
        <div className="flex justify-end gap-2">
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={titles.length === 0 || add.isPending} onClick={() => add.mutate()}>
            {titles.length === 1 ? "Add 1 reminder" : `Add ${titles.length} reminders`}
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
