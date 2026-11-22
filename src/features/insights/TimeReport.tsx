import { useMutation, useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { api, errorMessage, type TimeTotal } from "@/api";
import { Button, Segmented, Sheet, useToasts } from "@/design";
import { formatDate } from "@/features/briefing/format";

type Range = "this-week" | "last-week" | "this-month" | "last-month";

const pad = (n: number) => String(n).padStart(2, "0");
const ymd = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;

/** The two days (inclusive) of a range, worked out from today. Weeks start on Monday, like the weekly review. */
export function rangeDays(r: Range, now = new Date()): [string, string] {
  const day = new Date(now.getFullYear(), now.getMonth(), now.getDate(), 12);
  const monday = new Date(day);
  monday.setDate(day.getDate() - ((day.getDay() + 6) % 7));
  if (r === "this-week") return [ymd(monday), ymd(day)];
  if (r === "last-week") {
    const from = new Date(monday);
    from.setDate(monday.getDate() - 7);
    const to = new Date(monday);
    to.setDate(monday.getDate() - 1);
    return [ymd(from), ymd(to)];
  }
  if (r === "this-month") return [ymd(new Date(day.getFullYear(), day.getMonth(), 1, 12)), ymd(day)];
  return [ymd(new Date(day.getFullYear(), day.getMonth() - 1, 1, 12)), ymd(new Date(day.getFullYear(), day.getMonth(), 0, 12))];
}

/** "2 h 30 min", "45 min". */
export const hours = (m: number) => (m < 60 ? `${m} min` : m % 60 ? `${Math.floor(m / 60)} h ${m % 60} min` : `${m / 60} h`);

function Totals({ title, list }: { title: string; list: TimeTotal[] }) {
  if (list.length === 0) return null;
  const max = Math.max(...list.map((t) => t.minutes), 1);
  return (
    <section aria-label={title} className="flex flex-col gap-1.5">
      <h3 className="text-footnote font-semibold uppercase tracking-wide text-fg-2">{title}</h3>
      <ul className="flex flex-col gap-1.5">
        {list.map((t) => (
          <li key={t.name} className="flex flex-col gap-0.5">
            <div className="flex items-baseline justify-between gap-3 text-body">
              <span className="min-w-0 truncate text-fg">{t.name}</span>
              <span className="shrink-0 text-fg-2">
                {hours(t.minutes)}
                {t.count > 1 ? ` · ${t.count}` : ""}
              </span>
            </div>
            <div className="h-1.5 rounded-full bg-fill" aria-hidden>
              <div className="h-1.5 rounded-full bg-accent-fill" style={{ width: `${Math.max(4, (t.minutes / max) * 100)}%` }} />
            </div>
          </li>
        ))}
      </ul>
    </section>
  );
}

/** Where my time went: meeting and focus time for a week or month, and a spreadsheet for timesheets. */
export function TimeReportSheet({ onClose }: { onClose: () => void }) {
  const push = useToasts((s) => s.push);
  const [range, setRange] = useState<Range>("this-week");
  const [from, to] = rangeDays(range);
  const report = useQuery({ queryKey: ["time-report", from, to], queryFn: () => api.getTimeReport(from, to), staleTime: 60_000 });
  const save = useMutation({
    mutationFn: () => api.exportTimeReport(from, to),
    onSuccess: (path) => push({ title: "Saved to Downloads", description: path.split(/[\\/]/).pop() }),
    onError: (e) => push({ title: errorMessage(e), tone: "danger" }),
  });
  const r = report.data;
  const maxDay = r ? Math.max(...r.by_day.map(([, m]) => m), 1) : 1;
  return (
    <Sheet
      open
      onOpenChange={(o) => !o && onClose()}
      title="Where my time went"
      description="Meetings and focus time from your calendars. Worked out on this device."
      footer={
        <>
          <Button onClick={onClose}>Close</Button>
          <Button variant="primary" disabled={!r || r.rows.length === 0 || save.isPending} onClick={() => save.mutate()}>
            {save.isPending ? "Saving…" : "Save as spreadsheet"}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4 pb-4">
        <Segmented
          label="Which days"
          fill
          value={range}
          onValueChange={(v) => setRange(v as Range)}
          options={[
            { value: "this-week", label: "This week" },
            { value: "last-week", label: "Last week" },
            { value: "this-month", label: "This month" },
            { value: "last-month", label: "Last month" },
          ]}
        />
        {report.isError ? (
          <p role="alert" className="rounded-card bg-danger-soft px-3 py-2 text-body text-danger">
            {errorMessage(report.error)}
          </p>
        ) : !r ? (
          <p className="text-body text-fg-2">Adding it up…</p>
        ) : (
          <>
            <dl className="grid grid-cols-3 gap-2">
              {[
                ["Meetings", hours(r.meeting_minutes)],
                ["Focus", r.focus_minutes ? hours(r.focus_minutes) : "None"],
                ["Count", String(r.meetings)],
              ].map(([k, v]) => (
                <div key={k} className="rounded-card bg-fill px-3 py-2">
                  <dt className="text-footnote text-fg-2">{k}</dt>
                  <dd className="text-headline font-semibold text-fg">{v}</dd>
                </div>
              ))}
            </dl>
            {r.by_day.length <= 31 && (
              <div role="img" aria-label={`Meeting time per day: ${r.by_day.filter(([, m]) => m > 0).map(([d, m]) => `${formatDate(new Date(`${d}T00:00:00`))} ${hours(m)}`).join(", ") || "none"}`} className="flex h-20 items-end gap-0.5">
                {r.by_day.map(([d, m]) => (
                  <div key={d} className="flex-1 rounded-t-control bg-accent-fill" style={{ height: `${m === 0 ? 2 : Math.max(6, (m / maxDay) * 100)}%`, opacity: m === 0 ? 0.25 : 1 }} />
                ))}
              </div>
            )}
            <Totals title="By calendar" list={r.by_calendar} />
            <Totals title="By project" list={r.by_project} />
            <Totals title="Biggest meetings" list={r.by_title} />
            {r.rows.length === 0 && <p className="text-body text-fg-2">No meetings in these days.</p>}
            {r.by_project.length === 0 && r.rows.length > 0 && <p className="text-footnote text-fg-2">Tip: put a project in a title, like “[Acme] Weekly sync” or “#acme”, to see time per project.</p>}
          </>
        )}
      </div>
    </Sheet>
  );
}
