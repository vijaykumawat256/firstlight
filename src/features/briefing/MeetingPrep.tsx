import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { AlertTriangle, ArrowRight, History, Mail, Repeat } from "lucide-react";
import { api, type CalEvent, type MailMessage, type When } from "@/api";
import { formatDate, formatTime } from "./format";

const whenText = (w: When): string => {
  if (w.kind === "all_day") return formatDate(new Date(`${w.start}T00:00:00`));
  const s = new Date(w.start);
  return `${formatDate(s)}, ${formatTime(s)}`;
};

/** "5 minutes", "1 hour", "1 hour 15 minutes". */
function span(minutes: number): string {
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  const hours = h ? `${h} ${h === 1 ? "hour" : "hours"}` : "";
  const mins = m ? `${m} ${m === 1 ? "minute" : "minutes"}` : "";
  return [hours, mins].filter(Boolean).join(" ");
}

function Line({ icon, children, tone }: { icon: React.ReactNode; children: React.ReactNode; tone?: "warn" }) {
  return (
    <li className={`flex gap-3 ${tone === "warn" ? "text-warning" : "text-fg"}`}>
      <span className="mt-0.5 shrink-0" aria-hidden>
        {icon}
      </span>
      <span className="min-w-0 text-body">{children}</span>
    </li>
  );
}

/**
 * What is worth knowing before this meeting, from your own calendar: what happened the last times, what is right
 * before and after, anything overlapping, and unread mail that looks related. Worked out on this device (no AI,
 * no network). Shown only when there is something to say.
 */
export function MeetingPrep({ event }: { event: CalEvent }) {
  // The time is read once, when this opens: a meeting that ended more than an hour ago has nothing left to prepare.
  const [openedAt] = useState(() => Date.now());
  const upcoming = event.when.kind === "timed" && new Date(event.when.end).getTime() > openedAt - 3_600_000 && event.status !== "cancelled";
  const prep = useQuery({
    queryKey: ["meeting-prep", event.id],
    queryFn: () => api.getMeetingPrep(event.id),
    enabled: upcoming,
    staleTime: 30_000,
  });
  const p = prep.data;
  if (!upcoming || !p) return null;
  const nothing = p.earlier.length === 0 && !p.next_time && !p.before && !p.after && p.overlaps.length === 0 && p.mail.length === 0;
  if (nothing) return null;

  const openMail = (m: MailMessage) => {
    if (m.open.kind !== "none") void api.openMailMessage(m.open);
  };

  return (
    <section aria-label="Prepare" className="flex flex-col gap-3 rounded-card border border-separator p-3">
      <h3 className="text-footnote font-semibold uppercase tracking-wide text-fg-2">Prepare</h3>
      <ul className="flex flex-col gap-2.5">
        {p.overlaps.map((o) => (
          <Line key={o.id} icon={<AlertTriangle className="size-4" />} tone="warn">
            Overlaps with {o.title} for {span(o.minutes)}
          </Line>
        ))}
        {p.before && (
          <Line icon={<ArrowRight className="size-4" />}>
            {p.before.gap_minutes === 0 ? "Straight after" : `${span(p.before.gap_minutes)} after`} {p.before.title}
          </Line>
        )}
        {p.after && (
          <Line icon={<ArrowRight className="size-4" />}>
            {p.after.gap_minutes === 0 ? "Straight into" : `${span(p.after.gap_minutes)} before`} {p.after.title}
          </Line>
        )}
        {p.earlier.map((e, i) => (
          <Line key={e.id} icon={<History className="size-4" />}>
            <span className="font-medium">{i === 0 ? "Last time" : "Before that"}, {whenText(e.when)}</span>
            {e.notes ? <span className="block select-text break-words text-fg-2">{e.notes}</span> : <span className="block text-fg-3">No notes were written.</span>}
          </Line>
        ))}
        {p.next_time && (
          <Line icon={<Repeat className="size-4" />}>Next time: {whenText(p.next_time.when)}</Line>
        )}
        {p.mail.map((m) => (
          <Line key={m.id} icon={<Mail className="size-4" />}>
            {m.open.kind !== "none" ? (
              <button type="button" onClick={() => openMail(m)} className="text-left text-accent hover:underline">
                {m.subject || "(no subject)"}
              </button>
            ) : (
              <span>{m.subject || "(no subject)"}</span>
            )}
            <span className="block text-footnote text-fg-2">Unread, from {m.sender_name || m.sender_address}</span>
          </Line>
        ))}
      </ul>
    </section>
  );
}
