import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useId, useState } from "react";
import { useInviteSenders } from "@/features/events/InviteDialog";
import { parseQuickAdd, type QuickEvent } from "@/features/events/quickAdd";
import { formatDate, formatTime } from "@/features/briefing/format";
import { allTemplates, templateFor } from "@/features/events/templates";
import { useSettings } from "@/features/settings/useSettings";
import type { EventTemplate } from "@/api";
import { api, errorMessage, type EventInput, type OwnEvent } from "@/api";
import {
  Button,
  Segmented,
  Select,
  Sheet,
  Switch,
  TextField,
  useToasts,
} from "@/design";

export const OWN_CALENDAR_ID = "local-events";

const pad = (n: number) => String(n).padStart(2, "0");
const dateValue = (d: Date) =>
  `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
const timeValue = (d: Date) => `${pad(d.getHours())}:${pad(d.getMinutes())}`;

type Every = "daily" | "weekly" | "monthly" | "yearly";
const UNIT: Record<Every, string> = {
  daily: "days",
  weekly: "weeks",
  monthly: "months",
  yearly: "years",
};

/** The choices in the "Repeat" menu. A repeat the menu has no word for (every 3 months) is shown as it is. */
function repeatKey(every: Every, interval: number): string {
  if (interval === 1) return every;
  if (every === "weekly" && interval === 2) return "biweekly";
  return `${every}:${interval}`;
}

interface Draft {
  title: string;
  allDay: boolean;
  startDate: string;
  startTime: string;
  endDate: string;
  endTime: string;
  location: string;
  link: string;
  notes: string;
  /** "none", or a repeatKey. */
  repeat: string;
  /** Last day it repeats, or "" for no end. */
  until: string;
  /** Minutes before, or "none". */
  reminder: string;
  invitees: string;
  /** Focus time (timed events kept on this device only). */
  focus: boolean;
}

/** The fields a quick-add line fills in; everything else in the draft stays as it was. */
function fromQuick(p: Draft, q: QuickEvent): Draft {
  const last = q.allDay ? new Date(q.end.getFullYear(), q.end.getMonth(), q.end.getDate() - 1) : q.end;
  return {
    ...p,
    title: q.title,
    allDay: q.allDay,
    startDate: dateValue(q.start),
    startTime: q.allDay ? p.startTime : timeValue(q.start),
    endDate: dateValue(last),
    endTime: q.allDay ? p.endTime : timeValue(q.end),
    location: q.location ?? "",
  };
}

/** Sets the end so the event lasts `minutes` (timed events). */
function withLength(p: Draft, minutes: number): Draft {
  if (p.allDay || !p.startDate || !p.startTime) return p;
  const end = new Date(new Date(`${p.startDate}T${p.startTime}`).getTime() + minutes * 60_000);
  return { ...p, endDate: dateValue(end), endTime: timeValue(end) };
}

/** How long a timed draft lasts, in minutes (30 when it can't be read). */
function lengthOf(d: Draft): number {
  const m = (new Date(`${d.endDate}T${d.endTime}`).getTime() - new Date(`${d.startDate}T${d.startTime}`).getTime()) / 60_000;
  return Number.isFinite(m) && m >= 5 && m <= 480 ? Math.round(m) : 30;
}

/** "Fri 20 Nov, 1:00 PM to 2:00 PM · Cafe Nero", so the person can see how the line was read. */
export function describeQuick(q: QuickEvent): string {
  const when = q.allDay ? `${formatDate(q.start)}, all day` : `${formatDate(q.start)}, ${formatTime(q.start)} to ${formatTime(q.end)}`;
  return `${q.title} · ${when}${q.location ? ` · ${q.location}` : ""}`;
}

/** The next half hour on `day` (or today), and an hour after it. */
function blank(day: Date): Draft {
  const now = new Date();
  const start = new Date(
    day.getFullYear(),
    day.getMonth(),
    day.getDate(),
    now.getHours(),
    now.getMinutes() < 30 ? 30 : 60,
  );
  if (dateValue(day) !== dateValue(now)) start.setHours(9, 0);
  const end = new Date(start.getTime() + 3_600_000);
  return {
    title: "",
    allDay: false,
    startDate: dateValue(start),
    startTime: timeValue(start),
    endDate: dateValue(end),
    endTime: timeValue(end),
    location: "",
    link: "",
    notes: "",
    repeat: "none",
    until: "",
    reminder: "none",
    invitees: "",
    focus: false,
  };
}

function draftOf(i: EventInput): Draft {
  const common = {
    title: i.title,
    location: i.location ?? "",
    link: i.join_url ?? "",
    notes: i.notes ?? "",
    repeat: i.repeat ? repeatKey(i.repeat.every, i.repeat.interval) : "none",
    until: i.repeat?.until ?? "",
    reminder: i.reminder_minutes === null ? "none" : String(i.reminder_minutes),
    invitees: i.invitees.join(", "),
    focus: i.focus ?? false,
  };
  if (i.all_day)
    return {
      ...blank(new Date()),
      ...common,
      allDay: true,
      startDate: i.start,
      endDate: i.end,
    };
  const s = new Date(i.start);
  const en = new Date(i.end);
  return {
    ...common,
    allDay: false,
    startDate: dateValue(s),
    startTime: timeValue(s),
    endDate: dateValue(en),
    endTime: timeValue(en),
  };
}

function inputOf(d: Draft): EventInput {
  const iso = (date: string, time: string) =>
    new Date(`${date}T${time}`).toISOString();
  let repeat: EventInput["repeat"] = null;
  if (d.repeat !== "none") {
    const [name, n] =
      d.repeat === "biweekly" ? ["weekly", "2"] : d.repeat.split(":");
    repeat = {
      every: name as Every,
      interval: Number(n ?? 1),
      until: d.until || null,
    };
  }
  return {
    title: d.title,
    all_day: d.allDay,
    start: d.allDay ? d.startDate : iso(d.startDate, d.startTime),
    end: d.allDay ? d.endDate : iso(d.endDate, d.endTime),
    location: d.location.trim() || null,
    join_url: d.link.trim() || null,
    notes: d.notes.trim() || null,
    repeat,
    reminder_minutes:
      d.allDay || d.reminder === "none" ? null : Number(d.reminder),
    invitees: d.invitees.split(/[\s,;]+/).filter(Boolean),
    focus: !d.allDay && d.focus,
  };
}

const REMINDERS: { value: string; label: string }[] = [
  { value: "none", label: "No reminder" },
  { value: "5", label: "5 minutes before" },
  { value: "10", label: "10 minutes before" },
  { value: "15", label: "15 minutes before" },
  { value: "30", label: "30 minutes before" },
  { value: "60", label: "1 hour before" },
  { value: "120", label: "2 hours before" },
  { value: "1440", label: "1 day before" },
];

function repeatOptions(current: string): { value: string; label: string }[] {
  const base = [
    { value: "none", label: "Doesn't repeat" },
    { value: "daily", label: "Every day" },
    { value: "weekly", label: "Every week" },
    { value: "biweekly", label: "Every 2 weeks" },
    { value: "monthly", label: "Every month" },
    { value: "yearly", label: "Every year" },
  ];
  if (base.some((o) => o.value === current)) return base;
  const [every, n] = current.split(":");
  return [
    ...base,
    { value: current, label: `Every ${n} ${UNIT[every as Every] ?? ""}` },
  ];
}

/** Add an event of your own, or change one. It is saved only on this device. */
export function EventForm({
  open,
  onOpenChange,
  initial,
  day,
  detach,
  quick,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  initial?: OwnEvent | null;
  day?: Date;
  /** The id of one day of a repeating event: only that day changes (see FormSheet). */
  detach?: string;
  /** A line typed elsewhere (search) to start a new event from, as if typed into Quick add. */
  quick?: string;
}) {
  // Mounted only while open, so every opening starts from a fresh draft.
  return open ? (
    <FormSheet
      onOpenChange={onOpenChange}
      initial={initial}
      day={day}
      detach={detach}
      quick={quick}
    />
  ) : null;
}

/**
 * With `detach` (the id of one day of a repeating event) the form makes a separate one-off event from
 * these details and removes that day from the series, so only that day changes.
 */
function FormSheet({
  onOpenChange,
  initial,
  day,
  detach,
  quick,
}: {
  onOpenChange: (o: boolean) => void;
  initial?: OwnEvent | null;
  day?: Date;
  detach?: string;
  quick?: string;
}) {
  const qc = useQueryClient();
  const push = useToasts((s) => s.push);
  const { settings, update: updateSettings } = useSettings();
  const templates = allTemplates(settings?.templates ?? []);
  const [line, setLine] = useState(quick ?? "");
  const [understood, setUnderstood] = useState<QuickEvent | null>(() => (quick ? parseQuickAdd(quick, new Date()) : null));
  // The template chosen (by name), whose agenda goes into the meeting's private note once the event is added.
  const [template, setTemplate] = useState<EventTemplate | null>(() => (understood ? templateFor(understood.title, templates) : null));
  const [d, setD] = useState<Draft>(() => {
    if (initial) return draftOf(initial.input);
    const start = blank(day ?? new Date());
    if (!understood) return start;
    const filled = fromQuick(start, understood);
    const t = templateFor(understood.title, templates);
    return t && !understood.lengthGiven ? withLength(filled, t.minutes) : filled;
  });
  const quickAdd = !initial && !detach;
  const notesId = useId();
  const editing = !!initial && !detach;
  // Where a NEW event goes: only here, or into a calendar in the Calendar app. The menu has more than "this
  // computer" only when the user turned that on in Settings and allowed access.
  const [destination, setDestination] = useState("local");
  const writable = useQuery({ queryKey: ["writable-calendars"], queryFn: () => api.writableCalendars(), staleTime: 0 });
  const choices = !initial && !detach ? (writable.data ?? []) : [];
  // The field to invite people appears only when real invitations can go out; otherwise only who is already invited shows.
  const canInvite = (useInviteSenders().data?.length ?? 0) > 0;
  // Focus time is read from this device's copy, so it is offered only for events kept here.
  const canFocus = !d.allDay && destination === "local" && !initial?.saved_to;

  const save = useMutation({
    mutationFn: async () => {
      const input = { ...inputOf(d), focus: canFocus && d.focus };
      if (detach) {
        // Create the one-off first: if that fails, the series is left exactly as it was.
        const made = await api.createEvent(input);
        await api.skipOccurrence(detach);
        return made;
      }
      if (initial) return api.updateEvent(initial.id, input);
      const made = await api.createEvent(input, destination === "local" ? undefined : destination);
      // The template's agenda starts the meeting's private note, unless that meeting already has one.
      if (template?.agenda.trim()) {
        const existing = await api.getMeetingNote(made.id).catch(() => null);
        if (!existing) await api.saveMeetingNote(made.id, template.agenda).catch(() => null);
      }
      return made;
    },
    onSuccess: () => {
      void qc.invalidateQueries();
      onOpenChange(false);
      push({ title: editing ? "Event updated" : "Event added" });
    },
  });
  const set = <K extends keyof Draft>(k: K, v: Draft[K]) =>
    setD((p) => {
      const next = { ...p, [k]: v };
      // Moving the start past the end drags the end along, so the dates never cross by accident.
      if (k === "startDate" && next.endDate < next.startDate)
        next.endDate = next.startDate;
      return next;
    });
  const ready =
    d.title.trim().length > 0 &&
    !!d.startDate &&
    !!d.endDate &&
    (d.allDay || (!!d.startTime && !!d.endTime));
  const repeating = d.repeat !== "none" && !detach;
  const typeLine = (text: string) => {
    setLine(text);
    const q = parseQuickAdd(text, new Date());
    setUnderstood(q);
    if (!q) return;
    const t = templateFor(q.title, templates);
    setTemplate(t);
    setD((p) => {
      const filled = fromQuick(p, q);
      return t && !q.lengthGiven ? withLength(filled, t.minutes) : filled;
    });
  };
  const pickTemplate = (name: string) => {
    const t = templates.find((x) => x.name === name) ?? null;
    setTemplate(t);
    if (t) setD((p) => withLength({ ...p, title: p.title.trim() ? p.title : t.name }, t.minutes));
  };
  const saveAsTemplate = () => {
    const name = d.title.trim().slice(0, 60);
    if (!name) return;
    updateSettings((s) => {
      s.templates = [{ name, minutes: d.allDay ? 30 : lengthOf(d), agenda: d.notes.trim().slice(0, 2000) }, ...s.templates.filter((t) => t.name.toLowerCase() !== name.toLowerCase())].slice(0, 20);
    });
    push({ title: `Kept “${name}” as a template` });
  };

  return (
    <Sheet
      open
      onOpenChange={onOpenChange}
      title={detach ? "Edit this day" : editing ? "Edit event" : "New event"}
      description={
        detach
          ? "Only this day changes. The rest of the repeat stays as it is."
          : "Saved only on this device. It isn't added to Google, Outlook or any other calendar."
      }
      footer={
        <>
          <Button onClick={() => onOpenChange(false)}>Cancel</Button>
          <Button
            variant="primary"
            disabled={!ready || save.isPending}
            onClick={() => save.mutate()}
          >
            {save.isPending ? "Saving…" : editing || detach ? "Save" : "Add event"}
          </Button>
        </>
      }
    >
      <form
        className="flex flex-col gap-4 pb-4"
        onSubmit={(ev) => {
          ev.preventDefault();
          if (ready && !save.isPending) save.mutate();
        }}
      >
        {save.isError && (
          <p
            role="alert"
            className="rounded-card bg-danger-soft px-3 py-2 text-body text-danger"
          >
            {errorMessage(save.error)}
          </p>
        )}
        {choices.length > 0 && (
          <div className="flex items-center justify-between gap-3">
            <span className="text-headline text-fg">Save to</span>
            <Select
              label="Save to"
              value={destination}
              onValueChange={setDestination}
              options={[
                { value: "local", label: "This device only" },
                ...choices.map((c) => ({ value: c.id, label: c.source ? `${c.name} (${c.source})` : c.name })),
              ]}
            />
          </div>
        )}
        {destination !== "local" && choices.length > 0 && (
          <p className="text-footnote text-fg-2">It is added to that calendar in your Calendar app, so it appears on your other devices too.{canInvite ? " People you invite are not emailed by saving: use Send invitation for that." : ""}</p>
        )}
        {initial?.saved_to && !detach && <p className="text-footnote text-fg-2">This event is in your Calendar app, so changes you make here are made there too.</p>}
        {quickAdd && (
          <TextField
            label="Quick add"
            value={line}
            onChange={(e) => typeLine(e.target.value)}
            maxLength={300}
            placeholder="Lunch with Riya friday 1pm at Cafe Nero"
            hint={
              understood
                ? `Understood: ${describeQuick(understood)}. Press Return to add it, or change anything below.`
                : line.trim()
                  ? "Add a day or a time, such as “tomorrow 3pm” or “dec 24”."
                  : "Type it in one go and the details below fill themselves in."
            }
            autoComplete="off"
            autoFocus
          />
        )}
        {quickAdd && (
          <div className="flex items-center justify-between gap-3">
            <span className="text-headline text-fg">Template</span>
            <Select
              label="Template"
              value={template?.name ?? "none"}
              onValueChange={(v) => (v === "none" ? setTemplate(null) : pickTemplate(v))}
              options={[{ value: "none", label: "None" }, ...templates.map((t) => ({ value: t.name, label: `${t.name} (${t.minutes} min)` }))]}
            />
          </div>
        )}
        {template?.agenda.trim() && quickAdd && <p className="text-footnote text-fg-2">Its agenda goes into your private notes for this meeting.</p>}
        <TextField
          label="Title"
          value={d.title}
          onChange={(e) => set("title", e.target.value)}
          maxLength={200}
          autoComplete="off"
          autoFocus={!quickAdd}
        />
        <div className="flex items-center justify-between gap-3">
          <span className="text-headline text-fg">All day</span>
          <Switch
            label="All day"
            checked={d.allDay}
            onCheckedChange={(v) => set("allDay", v)}
          />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <TextField
            label={d.allDay ? "First day" : "Starts"}
            type="date"
            value={d.startDate}
            onChange={(e) => set("startDate", e.target.value)}
          />
          {!d.allDay && (
            <TextField
              label="Start time"
              type="time"
              value={d.startTime}
              onChange={(e) => set("startTime", e.target.value)}
            />
          )}
          <TextField
            label={d.allDay ? "Last day" : "Ends"}
            type="date"
            value={d.endDate}
            min={d.startDate}
            onChange={(e) => set("endDate", e.target.value)}
          />
          {!d.allDay && (
            <TextField
              label="End time"
              type="time"
              value={d.endTime}
              onChange={(e) => set("endTime", e.target.value)}
            />
          )}
        </div>

        {!detach && (
          <div className="flex items-center justify-between gap-3">
            <span className="text-headline text-fg">Repeat</span>
            <Select
              label="Repeat"
              value={d.repeat}
              onValueChange={(v) => set("repeat", v)}
              options={repeatOptions(d.repeat)}
            />
          </div>
        )}
        {repeating && (
          <div className="flex flex-col gap-3">
            <div className="flex items-center justify-between gap-3">
              <span className="text-headline text-fg">Ends</span>
              <Segmented
                label="Repeat ends"
                value={d.until ? "date" : "never"}
                onValueChange={(v) =>
                  set("until", v === "never" ? "" : d.until || d.startDate)
                }
                options={[
                  { value: "never", label: "Never" },
                  { value: "date", label: "On a date" },
                ]}
              />
            </div>
            {d.until && (
              <TextField
                label="Last day it repeats"
                type="date"
                value={d.until}
                min={d.startDate}
                onChange={(e) => set("until", e.target.value)}
              />
            )}
          </div>
        )}
        {canFocus && (
          <div className="flex flex-col gap-1">
            <div className="flex items-center justify-between gap-3">
              <span className="text-headline text-fg">Focus time</span>
              <Switch label="Focus time" checked={d.focus} onCheckedChange={(v) => set("focus", v)} />
            </div>
            <p className="text-footnote text-fg-2">While it runs, Firstlight holds back its summaries and doesn't open when you unlock. Meeting heads-ups and reminders still come.</p>
          </div>
        )}
        {!d.allDay && (
          <div className="flex items-center justify-between gap-3">
            <span className="text-headline text-fg">Remind me</span>
            <Select
              label="Reminder"
              value={d.reminder}
              onValueChange={(v) => set("reminder", v)}
              options={REMINDERS}
            />
          </div>
        )}

        <TextField
          label="Location"
          value={d.location}
          onChange={(e) => set("location", e.target.value)}
          maxLength={300}
          autoComplete="off"
        />
        <TextField
          label="Meeting link"
          type="url"
          value={d.link}
          onChange={(e) => set("link", e.target.value)}
          placeholder="https://"
          hint="Adds a Join button. Zoom, Meet, Teams or any https link."
          autoComplete="off"
          spellCheck={false}
        />
        {canInvite ? (
          <TextField
            label="Invite people"
            value={d.invitees}
            onChange={(e) => set("invitees", e.target.value)}
            placeholder="sam@example.com, kim@example.com"
            hint="Separate addresses with commas. They get a real calendar invitation, but nothing is sent when you save: after saving, open the event and choose Send invitation."
            autoComplete="off"
            spellCheck={false}
          />
        ) : (
          d.invitees.trim() !== "" && <p className="text-footnote text-fg-2">Invited: {d.invitees}</p>
        )}
        <div className="flex flex-col gap-1.5">
          <label
            htmlFor={notesId}
            className="text-footnote font-medium text-fg-2"
          >
            Notes
          </label>
          <textarea
            id={notesId}
            rows={3}
            maxLength={4000}
            value={d.notes}
            onChange={(e) => set("notes", e.target.value)}
            className="select-text rounded-card bg-fill px-3 py-2 text-headline text-fg placeholder:text-fg-3"
          />
        </div>
        {!d.allDay && d.title.trim() && (
          <div>
            <Button variant="plain" size="sm" onClick={saveAsTemplate}>
              Keep as a template
            </Button>
          </div>
        )}
        {editing && repeating && (
          <p className="text-footnote text-fg-2">
            Changes apply to every repeat of this event.
          </p>
        )}
        <button type="submit" hidden />
      </form>
    </Sheet>
  );
}
