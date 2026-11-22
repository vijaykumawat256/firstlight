import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowRight, BellRing, CalendarClock, ChevronRight, Mail, Pencil, Timer } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { api, errorMessage, type Task, type TaskInput, type TaskView } from "@/api";
import { Button, Dialog, IconButton, TextField, useToasts } from "@/design";
import { isPhone } from "@/app/platform";
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

const pad2 = (n: number) => String(n).padStart(2, "0");
const todayYmd = () => {
  const d = new Date();
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
};

/** A typed line as a task. With no day in it, it goes on today's list unless `toToday` is false (adding to Someday). */
export function quickToTaskInput(q: QuickTask, toToday = true): TaskInput {
  return { title: q.title, notes: null, due_date: q.dueDate ?? (toToday ? todayYmd() : null), due_time: q.dueTime, minutes: null };
}

export type TaskTab = "today" | "upcoming" | "someday" | "waiting" | "done";

/** Which list a task shows in, and how to say so ("Today", "Upcoming, Fri 20 Nov", "Someday"). */
export function whereTask(t: Task): { tab: TaskTab; text: string } {
  if (t.done_at) return { tab: "done", text: "Done" };
  if (t.waiting_on) return { tab: "waiting", text: "Waiting" };
  if (!t.due) return { tab: "someday", text: "Someday" };
  const day = t.due.kind === "date" ? t.due.date : (() => {
    const d = new Date(t.due.at);
    return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
  })();
  if (day <= todayYmd()) return { tab: "today", text: "Today" };
  return { tab: "upcoming", text: `Upcoming, ${dueText(t)}` };
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
export function TaskRow({ task, onEdit, onPlan, onNudge, onTimer, fresh }: { task: TaskView; onEdit: (t: TaskView) => void; onPlan?: (t: TaskView) => void; onNudge?: (t: TaskView) => void; onTimer?: (t: TaskView) => void; fresh?: boolean }) {
  const row = useRef<HTMLLIElement>(null);
  useEffect(() => {
    if (fresh) row.current?.scrollIntoView({ block: "nearest" });
  }, [fresh]);
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
  const [menu, setMenu] = useState(false);
  // Phones: swipe right to tick it off, left to move it to tomorrow (both also in the options sheet).
  const [dx, setDx] = useState(0);
  const swipeFrom = useRef<{ x: number; y: number; sideways: boolean | null } | null>(null);
  const tomorrow = useMutation({
    mutationFn: () => {
      const d = new Date();
      d.setDate(d.getDate() + 1);
      const p = (n: number) => String(n).padStart(2, "0");
      return api.moveTasks([task.id], `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`);
    },
    onSuccess: () => {
      changed();
      push({ title: `Moved to tomorrow: ${task.title}` });
    },
    onError: (e) => push({ title: errorMessage(e), tone: "danger" }),
  });
  const openMail = () => void api.openMailMessage(task.mail!.open).catch((e) => push({ title: errorMessage(e), tone: "danger" }));
  if (isPhone) {
    // A phone: the whole row opens one sheet of big buttons instead of a line of small icons.
    const actions: { label: string; icon: ReactNode; run: () => void }[] = [
      { label: "Change", icon: <Pencil className="size-5" />, run: () => onEdit(task) },
      ...(onTimer && !done && !task.waiting_on ? [{ label: "Start a focus timer", icon: <Timer className="size-5" />, run: () => onTimer(task) }] : []),
      ...(onPlan && !done && !task.waiting_on ? [{ label: "Set time aside", icon: <CalendarClock className="size-5" />, run: () => onPlan(task) }] : []),
      ...(!done ? [{ label: "Move to tomorrow", icon: <ArrowRight className="size-5" />, run: () => tomorrow.mutate() }] : []),
      ...(onNudge && task.waiting_on && !done ? [{ label: `Nudge ${task.waiting_on}`, icon: <BellRing className="size-5" />, run: () => onNudge(task) }] : []),
      ...(task.mail && task.mail.open.kind !== "none" ? [{ label: "Open the email", icon: <Mail className="size-5" />, run: openMail }] : []),
    ];
    const SWIPE = 90;
    return (
      <li className="relative overflow-hidden">
        {/* What a swipe will do, revealed behind the row. */}
        {dx !== 0 && (
          <div aria-hidden className={`absolute inset-0 flex items-center px-5 text-headline font-semibold text-white ${dx > 0 ? "justify-start bg-success" : "justify-end bg-accent-fill"}`}>
            {dx > 0 ? (done ? "Not done" : "Done") : "Tomorrow"}
          </div>
        )}
      <div
        ref={row as unknown as React.RefObject<HTMLDivElement>}
        data-fresh={fresh || undefined}
        style={{ transform: dx ? `translateX(${dx}px)` : undefined }}
        onTouchStart={(e) => {
          const t = e.touches[0];
          if (t) swipeFrom.current = { x: t.clientX, y: t.clientY, sideways: null };
        }}
        onTouchMove={(e) => {
          const s = swipeFrom.current;
          const t = e.touches[0];
          if (!s || !t) return;
          const mx = t.clientX - s.x;
          const my = t.clientY - s.y;
          // Decide once whether this is a sideways swipe or the page scrolling.
          if (s.sideways === null && Math.abs(mx) + Math.abs(my) > 10) s.sideways = Math.abs(mx) > Math.abs(my) * 1.5;
          if (s.sideways) setDx(Math.max(-140, Math.min(140, done && mx < 0 ? 0 : mx)));
        }}
        onTouchEnd={() => {
          if (dx >= SWIPE) toggle.mutate(!done);
          else if (dx <= -SWIPE && !done) tomorrow.mutate();
          swipeFrom.current = null;
          setDx(0);
        }}
        className={`relative flex min-h-14 items-center gap-1 bg-surface pl-2 pr-3 data-[fresh]:bg-accent-soft ${dx ? "" : "transition-[transform,background-color] duration-200 ease-standard"}`}
      >
        <label className="flex size-11 shrink-0 items-center justify-center">
          <input
            type="checkbox"
            aria-label={`${done ? "Not done" : "Done"}: ${task.title}`}
            checked={done}
            disabled={toggle.isPending}
            onChange={(e) => toggle.mutate(e.target.checked)}
            className="size-6 cursor-pointer accent-accent"
          />
        </label>
        <button type="button" aria-label={`Options for ${task.title}`} onClick={() => setMenu(true)} className="flex min-w-0 flex-1 items-center gap-2 py-2.5 text-left">
          <span className="min-w-0 flex-1">
            <span className={`block break-words text-headline ${done ? "text-fg-2 line-through" : "text-fg"}`}>{task.title}</span>
            {line && <span className={`block text-footnote ${task.days_late > 0 && !done ? "text-warning" : "text-fg-2"}`}>{line}</span>}
          </span>
          <ChevronRight className="size-5 shrink-0 text-fg-3" aria-hidden />
        </button>
        <Dialog
          open={menu}
          onOpenChange={setMenu}
          title={task.title}
          description={line || undefined}
          footer={<Button onClick={() => setMenu(false)}>Close</Button>}
        >
          <div className="flex flex-col gap-2">
            {actions.map((a) => (
              <Button
                key={a.label}
                leading={a.icon}
                className="justify-start"
                onClick={() => {
                  setMenu(false);
                  a.run();
                }}
              >
                {a.label}
              </Button>
            ))}
          </div>
        </Dialog>
      </div>
      </li>
    );
  }
  return (
    <li ref={row} data-fresh={fresh || undefined} className="flex min-h-12 items-center gap-3 px-4 py-2 transition-colors duration-700 ease-standard data-[fresh]:bg-accent-soft">
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
        <IconButton label={`Open the email for ${task.title}`} onClick={openMail}>
          <Mail className="size-4" />
        </IconButton>
      )}
      {onNudge && task.waiting_on && !done && (
        <IconButton label={`Nudge ${task.waiting_on} about ${task.title}`} onClick={() => onNudge(task)}>
          <BellRing className="size-4" />
        </IconButton>
      )}
      {onTimer && !done && !task.waiting_on && (
        <IconButton label={`Start a focus timer on ${task.title}`} onClick={() => onTimer(task)}>
          <Timer className="size-4" />
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

/**
 * A one-line box to add a task: "call the bank friday 3pm". Shows where it will go before it is added, and says where
 * it went after. A line with no day goes on today's list (or Someday, with `toToday` false).
 */
export function AddTaskField({ label = "Add a task", autoFocus, toToday = true, onAdded }: { label?: string; autoFocus?: boolean; toToday?: boolean; onAdded?: (t: Task) => void }) {
  const push = useToasts((s) => s.push);
  const changed = useTaskChanged();
  const [text, setText] = useState("");
  const read = parseQuickTask(text, new Date());
  const add = useMutation({
    mutationFn: () => api.addTask(quickToTaskInput(read!, toToday)),
    onSuccess: (t) => {
      setText("");
      changed();
      push({ title: `Added to ${whereTask(t).text}`, description: t.title });
      onAdded?.(t);
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
        hint={read && text.trim() ? `Adds “${read.title}” to ${when ? `your list, due ${when}` : toToday ? "Today" : "Someday"}. Press Return.` : toToday ? "Goes on today's list unless you add a day, like “friday 3pm”." : "No day needed here; add one, like “friday”, to plan it."}
        autoComplete="off"
        autoFocus={autoFocus}
      />
    </form>
  );
}
