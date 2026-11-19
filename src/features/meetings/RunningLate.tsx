import { useState } from "react";
import { api, errorMessage, type CalEvent } from "@/api";
import { Button, Dialog, Segmented, useToasts } from "@/design";
import { formatTime } from "@/features/briefing/format";

/** Who would get a "running late" note: the organiser, or everyone on the guest list. */
export function lateRecipients(e: CalEvent, everyone: boolean): string[] {
  const organiser = e.attendees.filter((a) => a.organizer).map((a) => a.email);
  if (!everyone) return organiser;
  return [...new Set([...organiser, ...e.attendees.map((a) => a.email)])];
}

export function lateMessage(e: CalEvent, minutes: number): { subject: string; body: string } {
  const at = e.when.kind === "timed" ? ` at ${formatTime(new Date(e.when.start))}` : "";
  return {
    subject: `Running late: ${e.title}`,
    body: `Hi,\n\nI'm running about ${minutes} minutes late for ${e.title}${at}. Sorry, I'll be there as soon as I can.\n`,
  };
}

/** Pick how late you will be; the note opens in your own mail app, addressed to the organiser (or everyone). */
export function RunningLate({ event, onClose }: { event: CalEvent; onClose: () => void }) {
  const push = useToasts((s) => s.push);
  const [minutes, setMinutes] = useState("10");
  const [everyone, setEveryone] = useState(false);
  const to = lateRecipients(event, everyone);
  const { subject, body } = lateMessage(event, Number(minutes));
  const open = () =>
    api
      .openMailDraft(to.join(", ") || null, subject, body)
      .then(onClose)
      .catch((e) => push({ title: errorMessage(e), tone: "danger" }));
  const hasGuests = event.attendees.length > 0;
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()} title="Running late" description="A short note opens in your mail app for you to send. Firstlight sends nothing.">
      <div className="flex flex-col gap-3 pb-2">
        <Segmented
          label="How late"
          value={minutes}
          onValueChange={setMinutes}
          options={[
            { value: "5", label: "5 min" },
            { value: "10", label: "10 min" },
            { value: "15", label: "15 min" },
            { value: "30", label: "30 min" },
          ]}
        />
        {hasGuests && (
          <label className="flex items-center gap-2 text-body text-fg">
            <input type="checkbox" className="size-4 accent-accent" checked={everyone} onChange={(e) => setEveryone(e.target.checked)} />
            Tell everyone invited, not just the organiser
          </label>
        )}
        <p className="text-footnote text-fg-2">{to.length ? `To: ${to.join(", ")}` : "No address is known for this meeting: add one in your mail app."}</p>
        <pre aria-label="Message" className="whitespace-pre-wrap rounded-card bg-fill p-3 text-footnote text-fg-2">
          {body}
        </pre>
        <div className="flex justify-end gap-2">
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" onClick={() => void open()}>
            Open in mail app
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
