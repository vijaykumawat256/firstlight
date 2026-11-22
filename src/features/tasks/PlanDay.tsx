import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { api, errorMessage } from "@/api";
import { Button, Dialog, Segmented, useToasts } from "@/design";
import { formatTime } from "@/features/briefing/format";
import { useTaskChanged } from "./shared";

/**
 * "Plan my day": suggestions for when today's tasks could be done, in today's free time. Worked out on this device,
 * or (when the person agreed to it) by their AI service, whose answer is checked against the free time first. Nothing
 * goes in the calendar until "Set time aside" is pressed.
 */
export function PlanDay({ onClose }: { onClose: () => void }) {
  const push = useToasts((s) => s.push);
  const qc = useQueryClient();
  const changed = useTaskChanged();
  const [useAi, setUseAi] = useState(false);
  const [consenting, setConsenting] = useState(false);
  const [skipped, setSkipped] = useState<Set<string>>(new Set());
  const plan = useQuery({ queryKey: ["day-plan", useAi], queryFn: () => api.planMyDay(useAi), staleTime: 0, gcTime: 0 });
  const preview = useQuery({ queryKey: ["day-plan-preview"], queryFn: () => api.dayPlanPreview(), enabled: consenting, staleTime: 0 });
  const agree = useMutation({
    mutationFn: () => api.enableAiPlanning(true),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["settings"] });
      setConsenting(false);
      setUseAi(true);
      void qc.invalidateQueries({ queryKey: ["day-plan"] });
    },
    onError: (e) => push({ title: errorMessage(e), tone: "danger" }),
  });
  const accept = useMutation({
    mutationFn: async () => {
      const chosen = (plan.data?.suggestions ?? []).filter((s) => !skipped.has(s.task_id));
      // One at a time, so the free time each one takes is gone before the next is placed.
      for (const s of chosen) await api.planTask(s.task_id, s.start, s.end);
      return chosen.length;
    },
    onSuccess: (n) => {
      changed();
      onClose();
      push({ title: n === 1 ? "Time set aside for 1 task" : `Time set aside for ${n} tasks` });
    },
    onError: (e) => {
      changed();
      push({ title: errorMessage(e), tone: "danger" });
    },
  });

  const p = plan.data;
  const chosen = (p?.suggestions ?? []).filter((s) => !skipped.has(s.task_id)).length;

  if (consenting) {
    return (
      <Dialog
        open
        onOpenChange={(o) => !o && onClose()}
        title="Plan with AI?"
        description="This is exactly what would be sent to your AI service: your task titles and how long they take, and today's free times. No meetings, people or notes."
        footer={
          <>
            <Button onClick={() => setConsenting(false)}>Not now</Button>
            <Button variant="primary" disabled={!preview.data || agree.isPending} onClick={() => agree.mutate()}>
              Agree and ask AI
            </Button>
          </>
        }
      >
        <pre aria-label="What would be sent" className="mb-3 max-h-60 overflow-auto whitespace-pre-wrap rounded-card bg-fill p-3 text-footnote text-fg-2">
          {preview.data ?? "Loading…"}
        </pre>
        <p className="text-footnote text-fg-2">You can withdraw this in Settings, AI. The suggestions are always checked against your free time on this device.</p>
      </Dialog>
    );
  }

  return (
    <Dialog
      open
      onOpenChange={(o) => !o && onClose()}
      title="Plan my day"
      description="Suggested times for today's tasks in your free time. Nothing is added until you choose."
      footer={
        <>
          <Button onClick={onClose}>Close</Button>
          <Button variant="primary" disabled={chosen === 0 || accept.isPending || plan.isPending} onClick={() => accept.mutate()}>
            {accept.isPending ? "Adding…" : "Set time aside"}
          </Button>
        </>
      }
    >
      {p?.ai_allowed && (
        <div className="pb-3">
          <Segmented
            fill
            label="Suggested by"
            value={useAi ? "ai" : "local"}
            onValueChange={(v) => setUseAi(v === "ai")}
            options={[
              { value: "local", label: "This device" },
              { value: "ai", label: "AI" },
            ]}
          />
        </div>
      )}
      {plan.isPending ? (
        <p className="text-body text-fg-2">{useAi ? "Asking AI…" : "Working it out…"}</p>
      ) : plan.isError || !p ? (
        <p className="text-body text-danger">Couldn't work out a plan. Try again.</p>
      ) : p.tasks === 0 ? (
        <p className="text-body text-fg-2">No tasks to plan. Tasks for today, and ones without a day, are planned.</p>
      ) : p.free_minutes === 0 ? (
        <p className="text-body text-fg-2">No free time left today in your working hours.</p>
      ) : (
        <>
          {p.problem && (
            <p role="status" className="mb-2 rounded-card bg-warning-soft px-3 py-2 text-footnote text-warning">
              {p.problem === "limit" ? "AI has been asked a lot this hour. Here is the plan worked out on this device." : "AI couldn't help this time. Here is the plan worked out on this device."}
            </p>
          )}
          {p.suggestions.length === 0 ? (
            <p className="text-body text-fg-2">None of your tasks fit in today's free time.</p>
          ) : (
            <ul aria-label="Suggestions" className="flex flex-col gap-1 pb-2">
              {p.suggestions.map((s) => (
                <li key={s.task_id}>
                  <label className="flex items-start gap-3 py-1 phone:min-h-11 phone:items-center">
                    <input
                      type="checkbox"
                      className="mt-1 size-4 shrink-0 accent-accent phone:mt-0 phone:size-5"
                      checked={!skipped.has(s.task_id)}
                      onChange={(e) => {
                        const next = new Set(skipped);
                        if (e.target.checked) next.delete(s.task_id);
                        else next.add(s.task_id);
                        setSkipped(next);
                      }}
                    />
                    <span className="min-w-0">
                      <span className="block break-words text-headline text-fg">{s.title}</span>
                      <span className="block text-footnote tabular-nums text-fg-2">
                        {formatTime(new Date(s.start))} to {formatTime(new Date(s.end))}
                      </span>
                    </span>
                  </label>
                </li>
              ))}
            </ul>
          )}
          <p className="pb-2 text-footnote text-fg-2">{p.by_ai ? "Suggested by AI and checked against your free time." : "Worked out on this device: tasks that have waited longest go first."}</p>
        </>
      )}
      {p?.ai_available && !p.ai_allowed && (
        <div className="pt-1">
          <Button variant="plain" size="sm" onClick={() => setConsenting(true)}>
            Ask AI instead…
          </Button>
        </div>
      )}
    </Dialog>
  );
}
