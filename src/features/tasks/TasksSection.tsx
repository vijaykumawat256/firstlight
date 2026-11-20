import { useState } from "react";
import type { TaskView } from "@/api";
import { useNav } from "@/app/nav";
import { Button, ListGroup, SectionHeader } from "@/design";
import { AddTaskField, TaskRow, useTaskLists } from "./shared";
import { PlanTask, TaskForm } from "./TaskDialogs";
import { PlanDay } from "./PlanDay";
import { FocusTimerBar, StartTimer } from "./FocusTimer";

const SHOWN = 6;

/** Today's tasks in the briefing: due today, plus unfinished ones carried over from earlier days. */
export function TasksSection() {
  const go = useNav((s) => s.go);
  const openTasks = useNav((s) => s.openTasks);
  const [fresh, setFresh] = useState<string | null>(null);
  const lists = useTaskLists();
  const [editing, setEditing] = useState<TaskView | null>(null);
  const [planning, setPlanning] = useState<TaskView | null>(null);
  const [planDay, setPlanDay] = useState(false);
  const [timing, setTiming] = useState<TaskView | null>(null);
  const l = lists.data;
  if (!l) return null;
  const today = l.today;
  const more = today.length - SHOWN;
  const trailing = l.done_today > 0 ? `${l.done_today} done today` : today.length > 0 ? `${today.length} to do` : undefined;
  return (
    <section aria-label="Tasks">
      <SectionHeader title="Tasks" trailing={trailing} />
      <div className="mb-2 empty:hidden">
        <FocusTimerBar />
      </div>
      {today.length > 0 && (
        <ListGroup className="mb-2">
          {today.slice(0, SHOWN).map((t) => (
            <TaskRow key={t.id} task={t} onEdit={setEditing} onPlan={setPlanning} onTimer={setTiming} fresh={t.id === fresh} />
          ))}
        </ListGroup>
      )}
      <AddTaskField
        onAdded={(t) => {
          setFresh(t.id);
          setTimeout(() => setFresh((f) => (f === t.id ? null : f)), 2500);
        }}
      />
      <div className="mt-1 flex flex-wrap justify-end gap-1">
        {l.waiting.length > 0 && (
          <Button variant="plain" size="sm" onClick={() => openTasks("waiting")}>
            Waiting ({l.waiting.length})
          </Button>
        )}
        {l.someday.length > 0 && (
          <Button variant="plain" size="sm" onClick={() => openTasks("someday")}>
            No day ({l.someday.length})
          </Button>
        )}
        {today.length + l.someday.length > 0 && (
          <Button variant="plain" size="sm" onClick={() => setPlanDay(true)}>
            Plan my day
          </Button>
        )}
        <Button variant="plain" size="sm" onClick={() => go("tasks")}>
          {more > 0 ? `All tasks (${more} more today)` : "All tasks"}
        </Button>
      </div>
      {editing && <TaskForm task={editing} onClose={() => setEditing(null)} />}
      {planning && <PlanTask task={planning} onClose={() => setPlanning(null)} />}
      {planDay && <PlanDay onClose={() => setPlanDay(false)} />}
      {timing && <StartTimer task={timing} onClose={() => setTiming(null)} />}
    </section>
  );
}
