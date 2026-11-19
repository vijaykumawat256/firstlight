import { useMutation, useQuery } from "@tanstack/react-query";
import { useId, useState } from "react";
import { api, errorMessage, type FreeSlot, type Task, type TaskInput } from "@/api";
import { Button, Dialog, Segmented, TextField, useToasts } from "@/design";
import { formatDate, formatTime } from "@/features/briefing/format";
import { useTaskChanged } from "./shared";

const pad = (n: number) => String(n).padStart(2, "0");
const ymd = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;

function inputOf(t: Task): TaskInput {
  if (!t.due) return { title: t.title, notes: t.notes, due_date: null, due_time: null, minutes: t.minutes };
  if (t.due.kind === "date") return { title: t.title, notes: t.notes, due_date: t.due.date, due_time: null, minutes: t.minutes };
  const d = new Date(t.due.at);
  return { title: t.title, notes: t.notes, due_date: ymd(d), due_time: `${pad(d.getHours())}:${pad(d.getMinutes())}`, minutes: t.minutes };
}

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
    <Dialog open onOpenChange={(o) => !o && onClose()} title="Change task" description="Kept only on this computer.">
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
    <Dialog open onOpenChange={(o) => !o && onClose()} title="Set time aside" description={`For “${task.title}”. It is added to your calendar on this computer only.`}>
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
