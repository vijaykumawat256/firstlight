import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useId, useState } from "react";
import { api, errorMessage, type CalEvent, type EventInput } from "@/api";
import { Button, Sheet, Switch, TextField, useToasts } from "@/design";

export const OWN_CALENDAR_ID = "local-events";

const pad = (n: number) => String(n).padStart(2, "0");
const dateValue = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
const timeValue = (d: Date) => `${pad(d.getHours())}:${pad(d.getMinutes())}`;

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
}

/** The next half hour on `day` (or today), and an hour after it. */
function blank(day: Date): Draft {
  const now = new Date();
  const start = new Date(day.getFullYear(), day.getMonth(), day.getDate(), now.getHours(), now.getMinutes() < 30 ? 30 : 60);
  if (dateValue(day) !== dateValue(now)) start.setHours(9, 0);
  const end = new Date(start.getTime() + 3_600_000);
  return { title: "", allDay: false, startDate: dateValue(start), startTime: timeValue(start), endDate: dateValue(end), endTime: timeValue(end), location: "", link: "", notes: "" };
}

function draftOf(e: CalEvent): Draft {
  if (e.when.kind === "all_day") {
    const last = new Date(`${e.when.end}T00:00:00`);
    last.setDate(last.getDate() - 1);
    return { ...blank(new Date()), title: e.title, allDay: true, startDate: e.when.start, endDate: dateValue(last), location: e.location ?? "", link: e.join_url ?? "", notes: e.description ?? "" };
  }
  const s = new Date(e.when.start);
  const en = new Date(e.when.end);
  return { title: e.title, allDay: false, startDate: dateValue(s), startTime: timeValue(s), endDate: dateValue(en), endTime: timeValue(en), location: e.location ?? "", link: e.join_url ?? "", notes: e.description ?? "" };
}

function inputOf(d: Draft): EventInput {
  const iso = (date: string, time: string) => new Date(`${date}T${time}`).toISOString();
  return {
    title: d.title,
    all_day: d.allDay,
    start: d.allDay ? d.startDate : iso(d.startDate, d.startTime),
    end: d.allDay ? d.endDate : iso(d.endDate, d.endTime),
    location: d.location.trim() || null,
    join_url: d.link.trim() || null,
    notes: d.notes.trim() || null,
  };
}

/** Add an event of your own, or change one. It is saved only on this computer. */
export function EventForm({ open, onOpenChange, initial, day }: { open: boolean; onOpenChange: (o: boolean) => void; initial?: CalEvent | null; day?: Date }) {
  // Mounted only while open, so every opening starts from a fresh draft.
  return open ? <FormSheet onOpenChange={onOpenChange} initial={initial} day={day} /> : null;
}

function FormSheet({ onOpenChange, initial, day }: { onOpenChange: (o: boolean) => void; initial?: CalEvent | null; day?: Date }) {
  const qc = useQueryClient();
  const push = useToasts((s) => s.push);
  const [d, setD] = useState<Draft>(() => (initial ? draftOf(initial) : blank(day ?? new Date())));
  const notesId = useId();
  const editing = !!initial;

  const save = useMutation({
    mutationFn: () => {
      const input = inputOf(d);
      return initial ? api.updateEvent(initial.id, input) : api.createEvent(input);
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
      if (k === "startDate" && next.endDate < next.startDate) next.endDate = next.startDate;
      return next;
    });
  const ready = d.title.trim().length > 0 && !!d.startDate && !!d.endDate && (d.allDay || (!!d.startTime && !!d.endTime));

  return (
    <Sheet
      open
      onOpenChange={onOpenChange}
      title={editing ? "Edit event" : "New event"}
      description="Saved only on this computer. It isn't added to Google, Outlook or any other calendar."
      footer={
        <>
          <Button onClick={() => onOpenChange(false)}>Cancel</Button>
          <Button variant="primary" disabled={!ready || save.isPending} onClick={() => save.mutate()}>
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
          <p role="alert" className="rounded-card bg-danger-soft px-3 py-2 text-body text-danger">
            {errorMessage(save.error)}
          </p>
        )}
        <TextField label="Title" value={d.title} onChange={(e) => set("title", e.target.value)} maxLength={200} autoComplete="off" autoFocus />
        <div className="flex items-center justify-between gap-3">
          <span className="text-headline text-fg">All day</span>
          <Switch label="All day" checked={d.allDay} onCheckedChange={(v) => set("allDay", v)} />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <TextField label={d.allDay ? "First day" : "Starts"} type="date" value={d.startDate} onChange={(e) => set("startDate", e.target.value)} />
          {!d.allDay && <TextField label="Start time" type="time" value={d.startTime} onChange={(e) => set("startTime", e.target.value)} />}
          <TextField label={d.allDay ? "Last day" : "Ends"} type="date" value={d.endDate} min={d.startDate} onChange={(e) => set("endDate", e.target.value)} />
          {!d.allDay && <TextField label="End time" type="time" value={d.endTime} onChange={(e) => set("endTime", e.target.value)} />}
        </div>
        <TextField label="Location" value={d.location} onChange={(e) => set("location", e.target.value)} maxLength={300} autoComplete="off" />
        <TextField label="Meeting link" type="url" value={d.link} onChange={(e) => set("link", e.target.value)} placeholder="https://" hint="Adds a Join button. Zoom, Meet, Teams or any https link." autoComplete="off" spellCheck={false} />
        <div className="flex flex-col gap-1.5">
          <label htmlFor={notesId} className="text-footnote font-medium text-fg-2">
            Notes
          </label>
          <textarea id={notesId} rows={3} maxLength={4000} value={d.notes} onChange={(e) => set("notes", e.target.value)} className="select-text rounded-card bg-fill px-3 py-2 text-headline text-fg placeholder:text-fg-3" />
        </div>
        <button type="submit" hidden />
      </form>
    </Sheet>
  );
}
