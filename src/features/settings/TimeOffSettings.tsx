import { useQuery } from "@tanstack/react-query";
import { Trash2 } from "lucide-react";
import { useState } from "react";
import { api, errorMessage, type TimeOff } from "@/api";
import { Button, IconButton, TextField, useToasts } from "@/design";
import { formatDate, formatTime } from "@/features/briefing/format";
import { Group, SettingRow } from "./controls";
import { useSettings } from "./useSettings";

const day = (ymd: string) => formatDate(new Date(`${ymd}T00:00:00`));
export const offText = (t: TimeOff) => (t.from === t.to ? day(t.from) : `${day(t.from)} to ${day(t.to)}`);

/** The day after a stretch ends (when you are back). */
function backOn(t: TimeOff): string {
  const d = new Date(`${t.to}T12:00:00`);
  d.setDate(d.getDate() + 1);
  while (d.getDay() === 0 || d.getDay() === 6) d.setDate(d.getDate() + 1);
  return formatDate(d);
}

/** An out-of-office reply for a stretch of time off. */
export function outOfOffice(t: TimeOff): string {
  return `Thanks for your message. I'm out of the office ${t.from === t.to ? `on ${day(t.from)}` : `from ${day(t.from)} to ${day(t.to)}`}${t.label ? ` (${t.label.toLowerCase()})` : ""} with limited access to email. I'll reply when I'm back on ${backOn(t)}.`;
}

/** A note to a meeting's organiser that you'll be away that day. */
export function awayNote(title: string, start: string, t: TimeOff): { subject: string; body: string } {
  return {
    subject: `Away for ${title}`,
    body: `Hi,\n\nI'll be out of the office ${t.from === t.to ? `on ${day(t.from)}` : `from ${day(t.from)} to ${day(t.to)}`}, so I can't make “${title}” on ${formatDate(new Date(start))} at ${formatTime(new Date(start))}. Could we move it, or could someone share notes afterwards?\n\nThanks!`,
  };
}

/** Settings, Time off: your days off, meetings on them, an out-of-office reply and the public holidays coming up. */
export function TimeOffSettings() {
  const push = useToasts((s) => s.push);
  const { settings: s, update } = useSettings();
  const status = useQuery({ queryKey: ["time-off", s?.day.time_off], queryFn: () => api.getTimeOffStatus(), staleTime: 0 });
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [label, setLabel] = useState("");
  if (!s) return null;
  const list = s.day.time_off;
  const add = (t: TimeOff) => {
    if (list.length >= 30) return push({ title: "That's the most stretches of time off kept. Remove an old one first.", tone: "danger" });
    update((d) => void (d.day.time_off = [...d.day.time_off, t].sort((a, b) => a.from.localeCompare(b.from))));
    push({ title: "Time off added", description: offText(t) });
  };
  const valid = !!from && (!to || to >= from);
  const st = status.data;
  const copy = (text: string) =>
    navigator.clipboard
      .writeText(text)
      .then(() => push({ title: "Copied" }))
      .catch(() => push({ title: "Couldn't copy", tone: "danger" }));
  return (
    <div className="flex flex-col gap-7">
      <Group title="Your days off" footer="On these days only meeting heads-ups come; summaries, wrap-up and nudges stay quiet. Kept on this computer.">
        {list.length === 0 && (
          <li className="px-4 py-3 text-body text-fg-2">No time off planned.</li>
        )}
        {list.map((t) => (
          <li key={`${t.from}-${t.to}`} className="flex min-h-12 items-center gap-3 px-4 py-2">
            <div className="min-w-0 flex-1">
              <div className="text-headline text-fg">{offText(t)}</div>
              {t.label && <div className="text-footnote text-fg-2">{t.label}</div>}
            </div>
            <IconButton label={`Remove time off ${offText(t)}`} onClick={() => update((d) => void (d.day.time_off = d.day.time_off.filter((x) => !(x.from === t.from && x.to === t.to))))}>
              <Trash2 className="size-4" />
            </IconButton>
          </li>
        ))}
        <li className="flex flex-col gap-3 px-4 py-3">
          <div className="grid grid-cols-2 gap-3">
            <TextField label="First day off" type="date" value={from} onChange={(e) => setFrom(e.target.value)} />
            <TextField label="Last day off" type="date" value={to} min={from || undefined} onChange={(e) => setTo(e.target.value)} />
          </div>
          <TextField label="What for (optional)" value={label} maxLength={60} placeholder="Holiday" onChange={(e) => setLabel(e.target.value)} />
          <div className="flex justify-end">
            <Button
              variant="primary"
              size="sm"
              disabled={!valid}
              onClick={() => {
                add({ from, to: to || from, label: label.trim() });
                setFrom("");
                setTo("");
                setLabel("");
              }}
            >
              Add time off
            </Button>
          </div>
        </li>
      </Group>

      {status.isError && (
        <p role="alert" className="rounded-card bg-danger-soft px-3 py-2 text-body text-danger">
          {errorMessage(status.error)}
        </p>
      )}

      {st && st.clashes.length > 0 && (
        <Group title="Meetings on your days off" footer="Write to the organiser before you go, or decline in your calendar.">
          {st.clashes.map((c) => (
            <li key={c.event_id} className="flex min-h-12 items-center gap-3 px-4 py-2">
              <div className="min-w-0 flex-1">
                <div className="truncate text-headline text-fg">{c.title}</div>
                <div className="text-footnote text-fg-2">
                  {formatDate(new Date(c.start))}, {formatTime(new Date(c.start))}
                </div>
              </div>
              <Button
                size="sm"
                variant="plain"
                onClick={() => {
                  const n = awayNote(c.title, c.start, c.off);
                  api.openMailDraft(c.organizer, n.subject, n.body).catch((e) => push({ title: errorMessage(e), tone: "danger" }));
                }}
              >
                {c.organizer ? "Write to organiser" : "Write a note"}
              </Button>
            </li>
          ))}
        </Group>
      )}

      {st?.next && (
        <Group title="Out-of-office reply">
          <li className="flex flex-col gap-2 px-4 py-3">
            <p className="text-body text-fg select-text">{outOfOffice(st.next)}</p>
            <div className="flex justify-end">
              <Button size="sm" variant="secondary" onClick={() => copy(outOfOffice(st.next!))}>
                Copy reply
              </Button>
            </div>
            <p className="text-footnote text-fg-2">Paste it into your email's automatic reply setting.</p>
          </li>
        </Group>
      )}

      <Group title="Public holidays coming up" footer={st && st.holidays.length === 0 ? "Add your country's holidays in Accounts & calendars to see them here." : undefined}>
        {st && st.holidays.length === 0 && <li className="px-4 py-3 text-body text-fg-2">None found in the next three months.</li>}
        {st?.holidays.map((h) => {
          const taken = list.some((t) => t.from <= h.date && h.date <= t.to);
          return (
            <SettingRow key={`${h.date}-${h.name}`} title={h.name} description={day(h.date)}>
              {taken ? (
                <span className="text-footnote text-fg-2">Day off</span>
              ) : (
                <Button size="sm" variant="plain" onClick={() => add({ from: h.date, to: h.date, label: h.name.slice(0, 60) })}>
                  Mark as day off
                </Button>
              )}
            </SettingRow>
          );
        })}
      </Group>
    </div>
  );
}
