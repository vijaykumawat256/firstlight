import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Copy, Target } from "lucide-react";
import { useMemo, useState } from "react";
import { api, errorMessage, type FreeSlot } from "@/api";
import { Dialog, IconButton, Segmented, useToasts } from "@/design";
import { formatDate, formatTime } from "@/features/briefing/format";
import { timeIn, zoneLabel } from "@/features/calendar/zones";
import { useSettings } from "@/features/settings/useSettings";

const LENGTHS = [
  { value: "15", label: "15 min" },
  { value: "30", label: "30 min" },
  { value: "60", label: "1 hour" },
  { value: "120", label: "2 hours" },
];
const SPANS = [
  { value: "7", label: "A week" },
  { value: "14", label: "Two weeks" },
];

const dayKey = (iso: string) => new Date(iso).toDateString();
const range = (s: FreeSlot) => `${formatDate(new Date(s.start))}, ${formatTime(new Date(s.start))} to ${formatTime(new Date(s.end))}`;

/** "Find a time": the free stretches in your working hours that are long enough. Reads your calendars only; nothing is sent. */
export function FindATime({ onClose }: { onClose: () => void }) {
  const push = useToasts((s) => s.push);
  const qc = useQueryClient();
  const zones = useSettings().settings?.appearance.extra_zones ?? [];
  const [minutes, setMinutes] = useState("30");
  const [days, setDays] = useState("7");
  const slots = useQuery({
    queryKey: ["free-times", minutes, days],
    queryFn: () => api.findFreeTimes(Number(minutes), Number(days)),
    staleTime: 0,
  });
  const byDay = useMemo(() => {
    const groups = new Map<string, FreeSlot[]>();
    for (const s of slots.data ?? []) groups.set(dayKey(s.start), [...(groups.get(dayKey(s.start)) ?? []), s]);
    return [...groups.values()];
  }, [slots.data]);

  const copy = (s: FreeSlot) => {
    navigator.clipboard
      .writeText(range(s))
      .then(() => push({ title: "Copied" }))
      .catch(() => push({ title: "Couldn't copy that", tone: "danger" }));
  };

  // Blocks the chosen length from the start of the stretch as focus time, kept on this device.
  const block = useMutation({
    mutationFn: (s: FreeSlot) => {
      const start = new Date(s.start);
      const end = new Date(Math.min(start.getTime() + Number(minutes) * 60_000, new Date(s.end).getTime()));
      return api.createEvent({
        title: "Focus",
        all_day: false,
        start: start.toISOString(),
        end: end.toISOString(),
        location: null,
        join_url: null,
        notes: null,
        repeat: null,
        reminder_minutes: null,
        invitees: [],
        focus: true,
      });
    },
    onSuccess: (e) => {
      void qc.invalidateQueries();
      const start = e.when.kind === "timed" ? new Date(e.when.start) : null;
      push({ title: start ? `Focus time added: ${formatDate(start)}, ${formatTime(start)}` : "Focus time added" });
    },
    onError: (e) => push({ title: errorMessage(e), tone: "danger" }),
  });

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()} title="Find a time" description="Free stretches in your working hours, weekdays only. The target blocks one for focus time.">
      <div className="flex flex-wrap gap-2 pb-3">
        <Segmented label="How long" value={minutes} onValueChange={setMinutes} options={LENGTHS} />
        <Segmented label="How far ahead" value={days} onValueChange={setDays} options={SPANS} />
      </div>
      {slots.isPending ? (
        <p className="text-body text-fg-2">Looking…</p>
      ) : slots.isError ? (
        <p className="text-body text-danger">Couldn't look that up. Try again.</p>
      ) : byDay.length === 0 ? (
        <p className="text-body text-fg-2">Nothing free that long in this period.</p>
      ) : (
        <ul className="flex flex-col gap-3">
          {byDay.map((group) => (
            <li key={group[0]?.start}>
              <h3 className="text-caption font-medium text-fg-2">{group[0] ? formatDate(new Date(group[0].start)) : ""}</h3>
              <ul className="mt-1 flex flex-col">
                {group.map((s) => (
                  <li key={s.start} className="flex items-center justify-between gap-2 py-1">
                    <span className="min-w-0 text-body text-fg">
                      {formatTime(new Date(s.start))} to {formatTime(new Date(s.end))}
                      {zones.map((z) => (
                        <span key={z} className="block text-footnote text-fg-2">
                          {timeIn(new Date(s.start), z)} to {timeIn(new Date(s.end), z)} in {zoneLabel(z)}
                        </span>
                      ))}
                    </span>
                    <span className="flex shrink-0 items-center gap-1">
                      <IconButton label={`Block ${LENGTHS.find((l) => l.value === minutes)?.label ?? ""} for focus from ${range(s)}`} disabled={block.isPending} onClick={() => block.mutate(s)}>
                        <Target className="size-4" />
                      </IconButton>
                      <IconButton label={`Copy ${range(s)}`} onClick={() => copy(s)}>
                        <Copy className="size-4" />
                      </IconButton>
                    </span>
                  </li>
                ))}
              </ul>
            </li>
          ))}
        </ul>
      )}
    </Dialog>
  );
}
