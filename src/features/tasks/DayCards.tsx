import { useMutation, useQuery } from "@tanstack/react-query";
import { Moon, UtensilsCrossed } from "lucide-react";
import { api, errorMessage } from "@/api";
import { Button, Card, useToasts } from "@/design";
import { formatTime } from "@/features/briefing/format";
import { useTaskChanged, useTaskLists } from "./shared";

const pad = (n: number) => String(n).padStart(2, "0");
const tomorrowYmd = () => {
  const d = new Date();
  d.setDate(d.getDate() + 1);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
};

export function useDayStatus() {
  return useQuery({ queryKey: ["day-status"], queryFn: () => api.getDayStatus(), refetchInterval: 60_000, staleTime: 30_000 });
}

/** From the wrap-up time until midnight: what got done, and the tasks left, each easy to move to tomorrow. */
export function WrapUpCard() {
  const push = useToasts((s) => s.push);
  const changed = useTaskChanged();
  const status = useDayStatus();
  const lists = useTaskLists();
  const move = useMutation({
    mutationFn: (ids: string[]) => api.moveTasks(ids, tomorrowYmd()),
    onSuccess: (_, ids) => {
      changed();
      push({ title: ids.length === 1 ? "Moved to tomorrow" : `${ids.length} tasks moved to tomorrow` });
    },
    onError: (e) => push({ title: errorMessage(e), tone: "danger" }),
  });
  const l = lists.data;
  if (!status.data?.wrap_up || !l || l.today.length + l.done_today === 0) return null;
  const left = l.today;
  return (
    <section aria-label="Wrap up">
      <Card className="flex flex-col gap-3">
        <div className="flex items-start gap-3">
          <Moon className="mt-0.5 size-5 shrink-0 text-accent" aria-hidden />
          <div className="min-w-0">
            <h2 className="text-headline font-semibold text-fg">Wrap up your day</h2>
            <p className="text-body text-fg-2">
              {l.done_today === 1 ? "1 task done" : `${l.done_today} tasks done`}
              {left.length === 0 ? ". Nothing left for today." : `, ${left.length} left.`}
            </p>
          </div>
        </div>
        {left.length > 0 && (
          <>
            <ul className="flex flex-col gap-1">
              {left.slice(0, 8).map((t) => (
                <li key={t.id} className="flex items-center justify-between gap-2">
                  <span className="min-w-0 truncate text-body text-fg">{t.title}</span>
                  <Button size="sm" variant="plain" disabled={move.isPending} onClick={() => move.mutate([t.id])} aria-label={`Move ${t.title} to tomorrow`}>
                    Tomorrow
                  </Button>
                </li>
              ))}
            </ul>
            {left.length > 1 && (
              <Button variant="secondary" disabled={move.isPending} onClick={() => move.mutate(left.map((t) => t.id))}>
                Move all {left.length} to tomorrow
              </Button>
            )}
          </>
        )}
      </Card>
    </section>
  );
}

/** When meetings leave less time than you asked for between your lunch hours, say so, and offer to keep what is left. */
export function LunchCard() {
  const push = useToasts((s) => s.push);
  const changed = useTaskChanged();
  const status = useDayStatus();
  const block = useMutation({
    mutationFn: ([start, end]: [string, string]) =>
      api.createEvent({ title: "Lunch", all_day: false, start, end, location: null, join_url: null, notes: null, repeat: null, reminder_minutes: null, invitees: [] }),
    onSuccess: () => {
      changed();
      push({ title: "Lunch is in your calendar" });
    },
    onError: (e) => push({ title: errorMessage(e), tone: "danger" }),
  });
  const w = status.data?.lunch;
  if (!w) return null;
  const between = `between ${formatTime(new Date(w.from))} and ${formatTime(new Date(w.to))}`;
  return (
    <section aria-label="Lunch">
      <Card className="flex items-start gap-3">
        <UtensilsCrossed className="mt-0.5 size-5 shrink-0 text-warning" aria-hidden />
        <div className="min-w-0 flex-1">
          <h2 className="text-headline font-semibold text-fg">Little time for lunch</h2>
          <p className="text-body text-fg-2">
            {w.free_minutes === 0 ? `Meetings fill everything ${between}.` : `Only ${w.free_minutes} free minutes ${between}.`}
          </p>
          {w.longest && (
            <Button className="mt-2" size="sm" variant="secondary" disabled={block.isPending} onClick={() => block.mutate(w.longest!)}>
              Keep {formatTime(new Date(w.longest[0]))} to {formatTime(new Date(w.longest[1]))} for lunch
            </Button>
          )}
        </div>
      </Card>
    </section>
  );
}
