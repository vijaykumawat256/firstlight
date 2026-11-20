import { useMutation, useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { StatusReportDialog } from "@/features/insights/StatusReport";
import { TimeReportSheet } from "@/features/insights/TimeReport";
import { BarChart3, Car, Moon, Palmtree, UtensilsCrossed } from "lucide-react";
import { useNav } from "@/app/nav";
import { useSettings } from "@/features/settings/useSettings";
import { api, errorMessage } from "@/api";
import { Button, Card, useToasts } from "@/design";
import { formatDate, formatTime } from "@/features/briefing/format";
import { useTaskChanged, useTaskLists } from "./shared";

const pad = (n: number) => String(n).padStart(2, "0");
/** The local date `days` from today, YYYY-MM-DD. */
const ymdIn = (days: number) => {
  const d = new Date();
  d.setDate(d.getDate() + days);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
};
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

/** "2 h 30 min", "45 min". */
const hours = (m: number) => (m < 60 ? `${m} min` : m % 60 ? `${Math.floor(m / 60)} h ${m % 60} min` : `${m / 60} h`);

/** In-person meetings the meeting before leaves too little time to get to (with the travel time you set). */
export function TravelCard() {
  const status = useDayStatus();
  const list = status.data?.travel ?? [];
  if (list.length === 0) return null;
  return (
    <section aria-label="Travel">
      <Card className="flex items-start gap-3">
        <Car className="mt-0.5 size-5 shrink-0 text-warning" aria-hidden />
        <div className="min-w-0 flex-1">
          <h2 className="text-headline font-semibold text-fg">Tight for travel</h2>
          <ul className="mt-1 flex flex-col gap-1">
            {list.map((w) => (
              <li key={w.event_id} className="text-body text-fg-2">
                <span className="text-fg">{w.title}</span> at {formatTime(new Date(w.start))}: {w.gap_minutes <= 0 ? "straight after" : `${w.gap_minutes} min after`} {w.previous_title}
              </li>
            ))}
          </ul>
        </div>
      </Card>
    </section>
  );
}

/** Friday afternoon and the weekend: the week so far next to last week. */
export function WeekReviewCard() {
  const status = useDayStatus();
  const review = useQuery({ queryKey: ["week-review"], queryFn: () => api.getWeekReview(), enabled: !!status.data?.review, staleTime: 5 * 60_000 });
  const [writing, setWriting] = useState(false);
  const [timeReport, setTimeReport] = useState(false);
  const r = review.data;
  if (!status.data?.review || !r) return null;
  const diff = r.meeting_minutes - r.previous_meeting_minutes;
  const compare = Math.abs(diff) < 30 ? "about the same as last week" : diff > 0 ? `${hours(diff)} more than last week` : `${hours(-diff)} less than last week`;
  return (
    <section aria-label="Your week">
      <Card className="flex items-start gap-3">
        <BarChart3 className="mt-0.5 size-5 shrink-0 text-accent" aria-hidden />
        <div className="min-w-0 flex-1">
          <h2 className="text-headline font-semibold text-fg">Your week</h2>
          <dl className="mt-1 grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 text-body">
            <dt className="text-fg-2">Meetings</dt>
            <dd className="text-fg">
              {hours(r.meeting_minutes)} in {r.meetings} {r.meetings === 1 ? "meeting" : "meetings"}, {compare}
            </dd>
            <dt className="text-fg-2">Focus</dt>
            <dd className="text-fg">{r.focus_minutes ? hours(r.focus_minutes) : "None set"}</dd>
            <dt className="text-fg-2">Tasks</dt>
            <dd className="text-fg">{r.tasks_done} done</dd>
            {r.busiest && (
              <>
                <dt className="text-fg-2">Busiest</dt>
                <dd className="text-fg">
                  {formatDate(new Date(`${r.busiest[0]}T00:00:00`))}, {hours(r.busiest[1])}
                </dd>
              </>
            )}
          </dl>
          <div className="mt-2 flex flex-wrap gap-2">
            <Button size="sm" variant="secondary" onClick={() => setWriting(true)}>
              Write a weekly update
            </Button>
            <Button size="sm" variant="plain" onClick={() => setTimeReport(true)}>
              Where my time went
            </Button>
          </div>
        </div>
      </Card>
      {writing && <StatusReportDialog initial="weekly" onClose={() => setWriting(false)} />}
      {timeReport && <TimeReportSheet onClose={() => setTimeReport(false)} />}
    </section>
  );
}

/** On a day off, say so; in the week before time off, show how many meetings fall in it and where to sort them out. */
export function TimeOffCard() {
  const offDays = useSettings().settings?.day.time_off;
  const status = useQuery({ queryKey: ["time-off", offDays], queryFn: () => api.getTimeOffStatus(), staleTime: 5 * 60_000, enabled: !!offDays && offDays.length > 0 });
  const go = useNav((s) => s.go);
  const setSection = useNav((s) => s.setSettingsSection);
  const st = status.data;
  if (!st || !offDays?.length) return null;
  const open = () => {
    go("settings");
    setSection("timeoff");
  };
  if (st.off_today) {
    return (
      <section aria-label="Time off">
        <Card className="flex items-start gap-3">
          <Palmtree className="mt-0.5 size-5 shrink-0 text-accent" aria-hidden />
          <div className="min-w-0 flex-1">
            <h2 className="text-headline font-semibold text-fg">You're off today</h2>
            <p className="text-body text-fg-2">{st.off_today.label ? `${st.off_today.label}. ` : ""}Only meeting heads-ups will come. Enjoy it.</p>
          </div>
        </Card>
      </section>
    );
  }
  const next = st.next;
  if (!next) return null;
  if (next.from > ymdIn(7)) return null;
  const inIt = st.clashes.filter((c) => c.off.from === next.from).length;
  return (
    <section aria-label="Time off">
      <Card className="flex items-start gap-3">
        <Palmtree className="mt-0.5 size-5 shrink-0 text-accent" aria-hidden />
        <div className="min-w-0 flex-1">
          <h2 className="text-headline font-semibold text-fg">Time off from {formatDate(new Date(`${next.from}T00:00:00`))}</h2>
          <p className="text-body text-fg-2">{inIt === 0 ? "No meetings while you're away." : inIt === 1 ? "1 meeting falls while you're away." : `${inIt} meetings fall while you're away.`}</p>
          <Button className="mt-2" size="sm" variant="secondary" onClick={open}>
            {inIt > 0 ? "Sort them out" : "Out-of-office reply"}
          </Button>
        </div>
      </Card>
    </section>
  );
}
