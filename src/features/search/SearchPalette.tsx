import { useQuery, useQueryClient } from "@tanstack/react-query";
import { CalendarDays, CalendarPlus, Globe, ListPlus, Search, Sparkles } from "lucide-react";
import { useEffect, useId, useMemo, useState } from "react";
import { api, type CalEvent } from "@/api";
import { useNav } from "@/app/nav";
import { Dialog, useToasts } from "@/design";
import { EventDetail } from "@/features/briefing/EventDetail";
import { formatDate, formatTime } from "@/features/briefing/format";
import { startOfLocalDay } from "@/features/calendar/layout";
import { describeQuick, EventForm } from "@/features/events/EventForm";
import { parseQuickAdd, parseQuickTask, taskLine, type QuickEvent, type QuickTask } from "@/features/events/quickAdd";
import { dueText, quickToTaskInput, whereTask } from "@/features/tasks/shared";
import { parseZoneQuestion, timeIn, zoneLabel, type ZoneAnswer } from "@/features/calendar/zones";
import { AskAnswer } from "./AskAnswer";
import { parseJump, searchEvents } from "./search";
import { useSearch } from "./store";

const DAY = 86_400_000;

const startOf = (e: CalEvent): Date => (e.when.kind === "all_day" ? new Date(`${e.when.start}T00:00:00`) : new Date(e.when.start));

type Row = { kind: "zone"; answer: ZoneAnswer } | { kind: "task"; task: QuickTask } | { kind: "add"; line: string; event: QuickEvent } | { kind: "ask"; question: string } | { kind: "day"; date: Date } | { kind: "event"; event: CalEvent };

/** Press ⌘K (Ctrl+K) anywhere: find an event by name or place, or type a date to jump to that day. */
export function SearchHost() {
  const { open, setOpen } = useSearch();
  const push = useToasts((s) => s.push);
  const qc = useQueryClient();
  const openCalendar = useNav((s) => s.openCalendar);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const [detail, setDetail] = useState<CalEvent | null>(null);
  const [asked, setAsked] = useState<string | null>(null);
  const [adding, setAdding] = useState<string | null>(null);
  const listId = useId();

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && !e.altKey && e.key.toLowerCase() === "k") {
        e.preventDefault();
        useSearch.getState().setOpen(!useSearch.getState().open);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // The window the app keeps (30 days back, 90 ahead). Fetched when the palette opens; each request is
  // limited to about two months, so it comes in two parts.
  const events = useQuery({
    queryKey: ["search-events", open],
    enabled: open,
    staleTime: 0,
    queryFn: async () => {
      const day = startOfLocalDay(new Date());
      const edge = (n: number) => new Date(day.getTime() + n * DAY).toISOString();
      const [a, b] = await Promise.all([api.getEvents(edge(-30), edge(30)), api.getEvents(edge(30), edge(90))]);
      return [...a, ...b];
    },
  });
  const calendars = useQuery({ queryKey: ["calendars"], queryFn: () => api.getCalendars(), enabled: open });
  const ai = useQuery({ queryKey: ["ai-status"], queryFn: () => api.aiStatus(), enabled: open });
  const colours = useMemo(() => new Map((calendars.data ?? []).map((c) => [c.id, c.name])), [calendars.data]);

  const now = new Date();
  const rows: Row[] = useMemo(() => {
    const jump = parseJump(query, new Date());
    // A line with a title and a day or time can become a new event ("lunch with Riya friday 1pm").
    // "task …" or "todo …" adds to the task list instead.
    const zone = parseZoneQuestion(query, new Date());
    const asTask = taskLine(query);
    const task = asTask ? parseQuickTask(asTask, new Date()) : null;
    const quick = jump || asTask || zone ? null : parseQuickAdd(query, new Date());
    const found = searchEvents(events.data ?? [], query, new Date());
    // A question needs AI to be on and agreed to. It looks like one when it has a few words or ends with "?".
    const words = query.trim().split(/\s+/).filter(Boolean).length;
    const canAsk = !!ai.data?.enabled && ai.data.consented && (words >= 3 || query.trim().endsWith("?"));
    return [
      ...(zone ? [{ kind: "zone" as const, answer: zone }] : []),
      ...(task ? [{ kind: "task" as const, task }] : []),
      ...(quick ? [{ kind: "add" as const, line: query.trim(), event: quick }] : []),
      ...(canAsk ? [{ kind: "ask" as const, question: query.trim() }] : []),
      ...(jump ? [{ kind: "day" as const, date: jump }] : []),
      ...found.map((event) => ({ kind: "event" as const, event })),
    ];
  }, [events.data, query, ai.data]);

  const close = () => {
    setOpen(false);
    setAsked(null);
    setQuery("");
    setActive(0);
  };
  const choose = (row: Row | undefined) => {
    if (!row) return;
    if (row.kind === "ask") {
      setAsked(row.question);
      return;
    }
    if (row.kind === "zone") return; // the answer is the row itself
    close();
    if (row.kind === "task") {
      api
        .addTask(quickToTaskInput(row.task))
        .then((t) => {
          void qc.invalidateQueries();
          push({ title: `Added to ${whereTask(t).text}`, description: row.task.title });
        })
        .catch(() => push({ title: "Couldn't add that task", tone: "danger" }));
      return;
    }
    if (row.kind === "add") {
      // Opened after this key press has finished, so the Return that chose the row cannot also submit the new form.
      setTimeout(() => setAdding(row.line), 0);
      return;
    }
    if (row.kind === "day") openCalendar("day", row.date);
    else setDetail(row.event);
  };
  const join = (url: string) => api.openUrl(url).catch(() => push({ title: "Couldn't open the link", tone: "danger" }));

  return (
    <>
      <Dialog open={open} onOpenChange={(o) => (o ? setOpen(true) : close())} title="Search" description="Find an event by name or place, type a date such as “friday” or “24 dec”, or type a new event such as “lunch with Riya friday 1pm”, or a task such as “task call the bank friday”.">
        {asked ? (
          <AskAnswer
            question={asked}
            onBack={() => setAsked(null)}
            onOpen={(e) => {
              close();
              setDetail(e);
            }}
          />
        ) : (
        <div className="flex flex-col gap-3 pb-2">
          <div className="flex items-center gap-2 rounded-card bg-fill px-3">
            <Search className="size-4 shrink-0 text-fg-2" aria-hidden />
            <input
              autoFocus
              role="combobox"
              aria-label="Search events"
              aria-expanded={rows.length > 0}
              aria-controls={listId}
              aria-activedescendant={rows.length > 0 ? `${listId}-${active}` : undefined}
              aria-autocomplete="list"
              value={query}
              onChange={(e) => {
                setQuery(e.target.value);
                setActive(0);
              }}
              onKeyDown={(e) => {
                if (e.key === "ArrowDown") {
                  e.preventDefault();
                  setActive((a) => Math.min(a + 1, rows.length - 1));
                } else if (e.key === "ArrowUp") {
                  e.preventDefault();
                  setActive((a) => Math.max(a - 1, 0));
                } else if (e.key === "Enter") {
                  e.preventDefault();
                  choose(rows[active]);
                }
              }}
              placeholder="Search events, or type a date"
              autoComplete="off"
              spellCheck={false}
              className="h-10 min-w-0 flex-1 select-text bg-transparent text-headline text-fg placeholder:text-fg-3"
            />
          </div>
          {events.isError && (
            <p role="alert" className="text-body text-danger">
              Couldn’t load your events. Try again in a moment.
            </p>
          )}
          {rows.length === 0 && !events.isPending && !events.isError ? (
            <p className="px-1 py-4 text-center text-body text-fg-2">{query.trim() ? "Nothing matches." : "Nothing coming up."}</p>
          ) : (
            <ul id={listId} role="listbox" aria-label={query.trim() ? "Results" : "Coming up"} className="flex max-h-72 flex-col overflow-y-auto phone:max-h-none">
              {rows.map((r, i) => (
                <li
                  key={r.kind === "zone" ? "zone" : r.kind === "task" ? "task" : r.kind === "add" ? "add" : r.kind === "ask" ? "ask" : r.kind === "day" ? "day" : r.event.id}
                  id={`${listId}-${i}`}
                  role="option"
                  aria-selected={i === active}
                  onMouseEnter={() => setActive(i)}
                  onClick={() => choose(r)}
                  className={`flex min-h-12 cursor-pointer items-center gap-3 rounded-card px-3 py-2 ${i === active ? "bg-fill" : ""}`}
                >
                  {r.kind === "zone" ? (
                    <>
                      <Globe className="size-4 shrink-0 text-accent" aria-hidden />
                      <span className="min-w-0 text-headline text-fg">
                        {timeIn(r.answer.at, r.answer.zone)} in {zoneLabel(r.answer.zone)} is {formatTime(r.answer.at)} here
                        {new Date().toDateString() !== r.answer.at.toDateString() ? `, ${formatDate(r.answer.at)}` : ""}
                      </span>
                    </>
                  ) : r.kind === "task" ? (
                    <>
                      <ListPlus className="size-4 shrink-0 text-accent" aria-hidden />
                      <span className="min-w-0 truncate text-headline text-fg">
                        Add task: {r.task.title}
                        {r.task.dueDate ? ` · ${dueText({ due: r.task.dueTime ? { kind: "at", at: new Date(`${r.task.dueDate}T${r.task.dueTime}`).toISOString() } : { kind: "date", date: r.task.dueDate } } as Parameters<typeof dueText>[0])}` : ""}
                      </span>
                    </>
                  ) : r.kind === "add" ? (
                    <>
                      <CalendarPlus className="size-4 shrink-0 text-accent" aria-hidden />
                      <span className="min-w-0 truncate text-headline text-fg">Add: {describeQuick(r.event)}</span>
                    </>
                  ) : r.kind === "ask" ? (
                    <>
                      <Sparkles className="size-4 shrink-0 text-accent" aria-hidden />
                      <span className="min-w-0 truncate text-headline text-fg">Ask: {r.question}</span>
                    </>
                  ) : r.kind === "day" ? (
                    <>
                      <CalendarDays className="size-4 shrink-0 text-accent" aria-hidden />
                      <span className="min-w-0 text-headline text-fg">Go to {formatDate(r.date)}</span>
                    </>
                  ) : (
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-headline text-fg">{r.event.title}</span>
                      <span className="block truncate text-footnote text-fg-2 tabular-nums">
                        {formatDate(startOf(r.event))} · {r.event.when.kind === "all_day" ? "All day" : formatTime(startOf(r.event))}
                        {colours.get(r.event.calendar_id) ? ` · ${colours.get(r.event.calendar_id)}` : ""}
                        {startOf(r.event) < now && r.event.when.kind === "timed" && new Date(r.event.when.end) < now ? " · past" : ""}
                      </span>
                    </span>
                  )}
                </li>
              ))}
            </ul>
          )}
        </div>
        )}
      </Dialog>
      <EventForm open={adding !== null} onOpenChange={(o) => !o && setAdding(null)} quick={adding ?? undefined} />
      <EventDetail event={detail} calendars={calendars.data ?? []} onClose={() => setDetail(null)} onJoin={join} />
    </>
  );
}
