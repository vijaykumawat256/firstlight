import { ChevronLeft, ListChecks } from "lucide-react";
import { useState } from "react";
import type { TaskView } from "@/api";
import { useNav } from "@/app/nav";
import { Button, EmptyState, ListGroup, Segmented, Skeleton } from "@/design";
import { AddTaskField, TaskRow, useTaskLists } from "./shared";
import { PlanTask, TaskForm } from "./TaskDialogs";
import { PlanDay } from "./PlanDay";

type Tab = "today" | "upcoming" | "someday" | "done";

const EMPTY: Record<Tab, string> = {
  today: "Nothing to do today.",
  upcoming: "Nothing due after today.",
  someday: "No tasks without a day.",
  done: "Nothing done in the last 30 days.",
};

/** Your to-do list: Today (with tasks carried over from earlier days), Upcoming, Someday and Done. */
export function TasksScreen() {
  const go = useNav((s) => s.go);
  const lists = useTaskLists();
  const [tab, setTab] = useState<Tab>("today");
  const [editing, setEditing] = useState<TaskView | null>(null);
  const [planning, setPlanning] = useState<TaskView | null>(null);
  const [planDay, setPlanDay] = useState(false);
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
            <p className="mt-0.5 text-footnote text-fg-2">Kept only on this computer. Unfinished tasks stay in Today until they are done.</p>
          </header>
          <div className="flex flex-col gap-1">
            <AddTaskField autoFocus />
            <div className="flex justify-end">
              <Button variant="secondary" size="sm" onClick={() => setPlanDay(true)}>
                Plan my day
              </Button>
            </div>
          </div>
          <Segmented
            label="Which tasks"
            value={tab}
            onValueChange={(v) => setTab(v as Tab)}
            options={[
              { value: "today", label: `Today${count("today") ? ` ${count("today")}` : ""}` },
              { value: "upcoming", label: "Upcoming" },
              { value: "someday", label: "Someday" },
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
                <TaskRow key={t.id} task={t} onEdit={setEditing} onPlan={setPlanning} />
              ))}
            </ListGroup>
          )}
        </div>
      </main>
      {editing && <TaskForm task={editing} onClose={() => setEditing(null)} />}
      {planning && <PlanTask task={planning} onClose={() => setPlanning(null)} />}
      {planDay && <PlanDay onClose={() => setPlanDay(false)} />}
    </div>
  );
}
