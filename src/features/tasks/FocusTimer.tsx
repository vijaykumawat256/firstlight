import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Timer } from "lucide-react";
import { useEffect, useState } from "react";
import { api, errorMessage, type Task } from "@/api";
import { Button, Dialog, Segmented, useToasts } from "@/design";

export const TIMER_KEY = ["focus-timer"];

export function useFocusTimer() {
  return useQuery({ queryKey: TIMER_KEY, queryFn: () => api.getFocusTimer(), refetchInterval: 15_000, staleTime: 5_000 });
}

const mmss = (ms: number) => {
  const s = Math.max(0, Math.ceil(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
};

/** While a focus timer runs: what it is on, the time left, and Stop. Hidden otherwise. */
export function FocusTimerBar() {
  const push = useToasts((s) => s.push);
  const qc = useQueryClient();
  const timer = useFocusTimer();
  const t = timer.data;
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!t) return;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [t]);
  const left = t ? Date.parse(t.ends_at) - now : 0;
  // When it runs out, ask again (the app counts the minutes and suggests a break).
  useEffect(() => {
    if (t && left <= 0) void qc.invalidateQueries();
  }, [t, left, qc]);
  const stop = useMutation({
    mutationFn: () => api.stopFocusTimer(),
    onSuccess: (m) => {
      void qc.invalidateQueries();
      push({ title: m > 0 ? `${m} min counted on the task` : "Timer stopped" });
    },
    onError: (e) => push({ title: errorMessage(e), tone: "danger" }),
  });
  if (!t || left <= 0) return null;
  return (
    <section aria-label="Focus timer" className="flex items-center gap-3 rounded-card bg-accent-soft px-4 py-2.5">
      <Timer className="size-5 shrink-0 text-accent" aria-hidden />
      <div className="min-w-0 flex-1">
        <p className="truncate text-headline font-semibold text-fg">{t.title}</p>
        <p className="text-footnote text-fg-2">
          <span className="font-mono tabular-nums" aria-hidden>
            {mmss(left)}
          </span>
          <span className="sr-only">{Math.ceil(left / 60_000)} minutes</span> left · reminders wait until it ends
        </p>
      </div>
      <Button size="sm" variant="secondary" disabled={stop.isPending} onClick={() => stop.mutate()}>
        Stop
      </Button>
    </section>
  );
}

/** Start a focus timer on a task. */
export function StartTimer({ task, onClose }: { task: Task; onClose: () => void }) {
  const push = useToasts((s) => s.push);
  const qc = useQueryClient();
  const [minutes, setMinutes] = useState("25");
  const start = useMutation({
    mutationFn: () => api.startFocusTimer(task.id, Number(minutes)),
    onSuccess: () => {
      void qc.invalidateQueries();
      onClose();
      push({ title: `Focus timer: ${minutes} min` });
    },
    onError: (e) => push({ title: errorMessage(e), tone: "danger" }),
  });
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()} title="Focus timer" description={`On “${task.title}”. Firstlight's own reminders wait until it ends (meeting heads-ups still come), then it suggests a break.`}>
      <div className="flex flex-col gap-4 pb-2">
        <Segmented
          label="How long"
          fill
          value={minutes}
          onValueChange={setMinutes}
          options={[
            { value: "15", label: "15 min" },
            { value: "25", label: "25 min" },
            { value: "50", label: "50 min" },
            { value: "90", label: "90 min" },
          ]}
        />
        <div className="flex justify-end gap-2">
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={start.isPending} onClick={() => start.mutate()}>
            Start
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
