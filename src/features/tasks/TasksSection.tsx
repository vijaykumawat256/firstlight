import { useState } from "react";
import type { TaskView } from "@/api";
import { useNav } from "@/app/nav";
import { Button, ListGroup, SectionHeader } from "@/design";
import { AddTaskField, TaskRow, useTaskLists } from "./shared";
import { PlanTask, TaskForm } from "./TaskDialogs";

const SHOWN = 6;

/** Today's tasks in the briefing: due today, plus unfinished ones carried over from earlier days. */
export function TasksSection() {
  const go = useNav((s) => s.go);
  const lists = useTaskLists();
  const [editing, setEditing] = useState<TaskView | null>(null);
  const [planning, setPlanning] = useState<TaskView | null>(null);
  const l = lists.data;
  if (!l) return null;
  const today = l.today;
  const more = today.length - SHOWN;
  const trailing = l.done_today > 0 ? `${l.done_today} done today` : today.length > 0 ? `${today.length} to do` : undefined;
  return (
    <section aria-label="Tasks">
      <SectionHeader title="Tasks" trailing={trailing} />
      {today.length > 0 && (
        <ListGroup className="mb-2">
          {today.slice(0, SHOWN).map((t) => (
            <TaskRow key={t.id} task={t} onEdit={setEditing} onPlan={setPlanning} />
          ))}
        </ListGroup>
      )}
      <AddTaskField />
      <div className="mt-1 flex justify-end">
        <Button variant="plain" size="sm" onClick={() => go("tasks")}>
          {more > 0 ? `All tasks (${more} more today)` : "All tasks"}
        </Button>
      </div>
      {editing && <TaskForm task={editing} onClose={() => setEditing(null)} />}
      {planning && <PlanTask task={planning} onClose={() => setPlanning(null)} />}
    </section>
  );
}
