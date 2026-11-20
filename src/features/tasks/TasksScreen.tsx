import { ChevronLeft, ListChecks } from "lucide-react";
import { useEffect, useState } from "react";
import type { TaskView } from "@/api";
import { useNav } from "@/app/nav";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api, errorMessage } from "@/api";
import { Button, EmptyState, ListGroup, Segmented, Skeleton, useToasts } from "@/design";
import { useSettings } from "@/features/settings/useSettings";
import { AddTaskField, TaskRow, useTaskLists, whereTask, type TaskTab } from "./shared";
import { MonthEndDialog, NudgeDialog, PlanTask, TaskForm } from "./TaskDialogs";
import { PlanDay } from "./PlanDay";
import { FocusTimerBar, StartTimer } from "./FocusTimer";
import { StatusReportDialog } from "@/features/insights/StatusReport";
import { TimeReportSheet } from "@/features/insights/TimeReport";

type Tab = TaskTab;

const EMPTY: Record<Tab, string> = {
  today: "Nothing to do today.",
  upcoming: "Nothing due after today.",
  someday: "No tasks without a day.",
  waiting: "Not waiting on anyone. Set “Waiting on” in a task when someone else has the next step.",
  done: "Nothing done in the last 30 days.",
};

/** Your to-do list: Today (with tasks carried over from earlier days), Upcoming, Someday and Done. */
export function TasksScreen() {
  const go = useNav((s) => s.go);
  const lists = useTaskLists();
  const [tab, setTab] = useState<Tab>(() => useNav.getState().tasksTab);
  const [editing, setEditing] = useState<TaskView | null>(null);
  const [planning, setPlanning] = useState<TaskView | null>(null);
  const [nudging, setNudging] = useState<TaskView | null>(null);
  const [timing, setTiming] = useState<TaskView | null>(null);
  const [planDay, setPlanDay] = useState(false);
  const [monthEnd, setMonthEnd] = useState(false);
  const [writing, setWriting] = useState(false);
  const [timeReport, setTimeReport] = useState(false);
  // The task just added: its list is shown and the row lights up for a moment.
  const [fresh, setFresh] = useState<string | null>(null);
  useEffect(() => {
    if (!fresh) return;
    const t = setTimeout(() => setFresh(null), 2500);
    return () => clearTimeout(t);
  }, [fresh]);
  const push = useToasts((s) => s.push);
  const qc = useQueryClient();
  const syncing = (useSettings().settings?.tasks.sync_accounts.length ?? 0) > 0;
  const syncNow = useMutation({
    mutationFn: () => api.syncTasksNow(),
    onSuccess: (r) => {
      void qc.invalidateQueries({ queryKey: ["tasks"] });
      if (r.problems.length) push({ title: `Couldn't sync with ${r.problems.map(([name, why]) => `${name} (${why})`).join(", ")}`, tone: "danger" });
      else push({ title: "Tasks are in step" });
    },
    onError: (e) => push({ title: errorMessage(e), tone: "danger" }),
  });
  const l = lists.data;
  const count = (t: Tab) => (l ? l[t].length : 0);
  const shown = l ? l[tab] : [];

  return (
    <div className="flex h-full flex-col">
      <div data-tauri-drag-region className="flex h-11 shrink-0 items-center px-3 pl-20">
        <Button variant="plain" size="sm" leading={<ChevronLeft className="size-4" />} onClick={() => go("briefing")}>
          Briefing
        </Button>
      </div>
      <main className="min-h-0 flex-1 overflow-y-auto px-5 pb-8 [&>*]:mx-auto [&>*]:w-full [&>*]:max-w-2xl">
        <div className="flex flex-col gap-4">
          <header>
            <h1 className="text-display font-bold tracking-tight text-fg">Tasks</h1>
            <p className="mt-0.5 text-footnote text-fg-2">
              {syncing ? "Kept in step with your task list online (Settings, General)." : "Kept only on this computer."} Unfinished tasks stay in Today until they are done.
            </p>
            {syncing && (
              <Button className="mt-1" size="sm" variant="plain" disabled={syncNow.isPending} onClick={() => syncNow.mutate()}>
                {syncNow.isPending ? "Syncing…" : "Sync now"}
              </Button>
            )}
          </header>
          <FocusTimerBar />
          <div className="flex flex-col gap-1">
            <AddTaskField
              autoFocus
              toToday={tab !== "someday"}
              onAdded={(t) => {
                setTab(whereTask(t).tab);
                setFresh(t.id);
              }}
            />
            <div className="flex flex-wrap justify-end gap-2">
              <Button variant="plain" size="sm" onClick={() => setTimeReport(true)}>
                My time
              </Button>
              <Button variant="plain" size="sm" onClick={() => setWriting(true)}>
                Write update
              </Button>
              <Button variant="plain" size="sm" onClick={() => setMonthEnd(true)}>
                Month-end
              </Button>
              <Button variant="secondary" size="sm" onClick={() => setPlanDay(true)}>
                Plan my day
              </Button>
            </div>
          </div>
          <Segmented
            label="Which tasks"
            fill
            value={tab}
            onValueChange={(v) => setTab(v as Tab)}
            options={[
              { value: "today", label: `Today${count("today") ? ` ${count("today")}` : ""}` },
              { value: "upcoming", label: "Upcoming" },
              { value: "someday", label: "Someday" },
              { value: "waiting", label: `Waiting${count("waiting") ? ` ${count("waiting")}` : ""}` },
              { value: "done", label: "Done" },
            ]}
          />
          {lists.isPending ? (
            <Skeleton className="h-32 rounded-card" />
          ) : shown.length === 0 ? (
            <EmptyState icon={<ListChecks />} title={EMPTY[tab]} />
          ) : (
            <ListGroup aria-label={`${tab} tasks`}>
              {shown.map((t) => (
                <TaskRow key={t.id} task={t} onEdit={setEditing} onPlan={setPlanning} onNudge={setNudging} onTimer={setTiming} fresh={t.id === fresh} />
              ))}
            </ListGroup>
          )}
        </div>
      </main>
      {editing && <TaskForm task={editing} onClose={() => setEditing(null)} />}
      {planning && <PlanTask task={planning} onClose={() => setPlanning(null)} />}
      {nudging && <NudgeDialog task={nudging} onClose={() => setNudging(null)} />}
      {timing && <StartTimer task={timing} onClose={() => setTiming(null)} />}
      {planDay && <PlanDay onClose={() => setPlanDay(false)} />}
      {writing && <StatusReportDialog onClose={() => setWriting(false)} />}
      {timeReport && <TimeReportSheet onClose={() => setTimeReport(false)} />}
      {monthEnd && l && (
        <MonthEndDialog existing={[...l.today, ...l.upcoming].filter((t) => t.repeat === "last_workday").map((t) => t.title)} onClose={() => setMonthEnd(false)} />
      )}
    </div>
  );
}
