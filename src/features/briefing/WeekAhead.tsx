import { useQuery } from "@tanstack/react-query";
import { api, type WeekAhead as Week, type WeekDay } from "@/api";
import { Card, SectionHeader } from "@/design";
import { formatDuration, formatTime } from "./format";

const day = (iso: string) => new Date(`${iso}T00:00:00`);
const weekday = (iso: string, style: "long" | "short") => day(iso).toLocaleDateString(undefined, { weekday: style });
const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

function dayLabel(d: WeekDay): string {
  const long = weekday(d.date, "long");
  if (d.meetings === 0) return d.all_day.length ? `${long}: ${d.all_day.join(", ")}` : `${long}: nothing scheduled`;
  return `${long}: ${plural(d.meetings, "meeting", "meetings")}, ${formatDuration(d.minutes)}${d.heavy ? ", a heavy day" : ""}`;
}

/** How the next seven days look: meeting time per day, the busiest and lightest, early starts, a block to focus. */
export function WeekAhead() {
  const week = useQuery({ queryKey: ["week-ahead"], queryFn: () => api.getWeekAhead(), staleTime: 60_000 });
  const w: Week | undefined = week.data;
  if (!w || (w.meetings === 0 && w.days.every((d) => d.all_day.length === 0))) return null;
  const most = Math.max(60, ...w.days.map((d) => d.minutes));
  const notes: string[] = [];
  if (w.busiest && w.meetings > 1) {
    const b = w.days.find((d) => d.date === w.busiest)!;
    notes.push(`Busiest: ${weekday(b.date, "long")}, ${formatDuration(b.minutes)} in meetings`);
  }
  if (w.lightest) notes.push(`Lightest: ${weekday(w.lightest, "long")}`);
  if (w.early_starts.length) notes.push(`Early start: ${w.early_starts.slice(0, 3).map((d) => weekday(d, "long")).join(", ")}`);
  if (w.focus) notes.push(`Best time to focus: ${weekday(w.focus.date, "long")}, ${formatTime(new Date(w.focus.start))} to ${formatTime(new Date(w.focus.end))}`);
  const back = w.days.reduce((n, d) => n + d.back_to_back_runs, 0);
  if (back) notes.push(`${plural(back, "stretch", "stretches")} of back-to-back meetings`);
  if (w.conflicts) notes.push(`${plural(w.conflicts, "overlap", "overlaps")} to sort out`);
  if (w.needs_response) notes.push(`${plural(w.needs_response, "invitation", "invitations")} to answer`);

  return (
    <section aria-label="Week ahead">
      <SectionHeader title="The week ahead" trailing={`${plural(w.meetings, "meeting", "meetings")} · ${formatDuration(w.minutes)}`} />
      <Card className="flex flex-col gap-3 p-4">
        <ol aria-label="Meeting time per day" className="flex h-16 items-end gap-1.5">
          {w.days.map((d) => (
            <li key={d.date} className="flex h-full flex-1 flex-col items-center justify-end gap-1" aria-label={dayLabel(d)}>
              <span
                aria-hidden
                className={`w-full rounded-sm ${d.heavy ? "bg-warning" : d.weekend ? "bg-fill-strong" : "bg-accent"}`}
                style={{ height: `${Math.max(6, (d.minutes / most) * 100)}%`, opacity: d.minutes === 0 ? 0.35 : 1 }}
              />
              <span aria-hidden className="text-caption text-fg-2">{weekday(d.date, "short").slice(0, 3)}</span>
            </li>
          ))}
        </ol>
        {notes.length > 0 && (
          <ul className="flex flex-col gap-1 text-body text-fg">
            {notes.map((n) => (
              <li key={n}>{n}</li>
            ))}
          </ul>
        )}
      </Card>
    </section>
  );
}
