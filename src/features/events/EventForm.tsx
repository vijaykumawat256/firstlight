import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useId, useState } from "react";
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

/** Add an event of your own, or change one. It is saved only on this computer. */
export function EventForm({
  open,
  onOpenChange,
  initial,
  day,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  initial?: OwnEvent | null;
  day?: Date;
}) {
  // Mounted only while open, so every opening starts from a fresh draft.
  return open ? (
    <FormSheet onOpenChange={onOpenChange} initial={initial} day={day} />
  ) : null;
}

function FormSheet({
  onOpenChange,
  initial,
  day,
}: {
  onOpenChange: (o: boolean) => void;
  initial?: OwnEvent | null;
  day?: Date;
}) {
  const qc = useQueryClient();
  const push = useToasts((s) => s.push);
  const [d, setD] = useState<Draft>(() =>
    initial ? draftOf(initial.input) : blank(day ?? new Date()),
  );
  const notesId = useId();
  const editing = !!initial;

  const save = useMutation({
    mutationFn: () => {
      const input = inputOf(d);
      return initial
        ? api.updateEvent(initial.id, input)
        : api.createEvent(input);
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
  const repeating = d.repeat !== "none";

  return (
    <Sheet
      open
      onOpenChange={onOpenChange}
      title={editing ? "Edit event" : "New event"}
      description="Saved only on this computer. It isn't added to Google, Outlook or any other calendar."
      footer={
        <>
          <Button onClick={() => onOpenChange(false)}>Cancel</Button>
          <Button
            variant="primary"
            disabled={!ready || save.isPending}
            onClick={() => save.mutate()}
          >
            {save.isPending ? "Saving…" : editing ? "Save" : "Add event"}
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
        <TextField
          label="Title"
          value={d.title}
          onChange={(e) => set("title", e.target.value)}
          maxLength={200}
          autoComplete="off"
          autoFocus
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

        <div className="flex items-center justify-between gap-3">
          <span className="text-headline text-fg">Repeat</span>
          <Select
            label="Repeat"
            value={d.repeat}
            onValueChange={(v) => set("repeat", v)}
            options={repeatOptions(d.repeat)}
          />
        </div>
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
        <TextField
          label="Invite people"
          value={d.invitees}
          onChange={(e) => set("invitees", e.target.value)}
          placeholder="sam@example.com, kim@example.com"
          hint="Separate addresses with commas. Firstlight sends nothing itself: after saving, open the event and choose Send invitation to use your own mail app."
          autoComplete="off"
          spellCheck={false}
        />
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
