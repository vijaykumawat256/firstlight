import { useMutation, useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { api, errorMessage, type CalEvent, type InviteSender } from "@/api";
import { Button, Dialog, TextField, useToasts } from "@/design";

/**
 * Share one of your own events. By default Firstlight sends nothing: it saves a calendar file to Downloads
 * and opens a draft in your own mail app, which you read and send yourself. If you turned on "Send
 * invitations from Firstlight" and have a Yahoo or iCloud mailbox, "Review and send" shows the exact message
 * and sends it only when you press Send.
 */
export function InviteDialog({
  event,
  invitees,
  onClose,
}: {
  event: CalEvent | null;
  invitees: string[];
  onClose: () => void;
}) {
  return event ? (
    <Body event={event} invitees={invitees} onClose={onClose} />
  ) : null;
}

function Body({
  event,
  invitees,
  onClose,
}: {
  event: CalEvent;
  invitees: string[];
  onClose: () => void;
}) {
  const push = useToasts((s) => s.push);
  const [to, setTo] = useState(invitees.join(", "));
  const run = useMutation({
    mutationFn: (openMail: boolean) =>
      api.prepareInvite(event.id, to, openMail),
    onSuccess: (r) => {
      onClose();
      push(
        r.mail_opened
          ? {
              title: "Your mail app is open",
              description:
                "Attach the calendar file from your Downloads folder, then send.",
            }
          : { title: "Calendar file saved", description: r.path },
      );
    },
  });
  const hasAddress = to.trim().length > 0;
  const senders = useQuery({ queryKey: ["invite-senders"], queryFn: () => api.inviteSenders() });
  const [reviewing, setReviewing] = useState(false);
  const [mailbox, setMailbox] = useState<string | null>(null);
  const sender: InviteSender | undefined = senders.data?.find((m) => m.id === mailbox) ?? senders.data?.[0];
  const preview = useQuery({ queryKey: ["invite-preview", event.id], queryFn: () => api.invitePreview(event.id), enabled: reviewing });
  const send = useMutation({
    mutationFn: () => api.sendInviteMail(event.id, to, sender!.id),
    onSuccess: () => {
      onClose();
      push({ title: "Invitation sent", description: `From ${sender?.address}` });
    },
  });
  if (reviewing && sender) {
    return (
      <Dialog
        open
        onOpenChange={(o) => !o && onClose()}
        title="Review before sending"
        description={`This is exactly what will be sent from ${sender.address}.`}
        footer={
          <>
            <Button disabled={send.isPending} onClick={() => setReviewing(false)}>Back</Button>
            <Button variant="primary" disabled={send.isPending || !preview.data} onClick={() => send.mutate()}>
              {send.isPending ? "Sending…" : "Send"}
            </Button>
          </>
        }
      >
        <div className="flex flex-col gap-3 pb-2">
          {send.isError && (
            <p role="alert" className="rounded-card bg-danger-soft px-3 py-2 text-body text-danger">{errorMessage(send.error)}</p>
          )}
          <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-body">
            <dt className="text-fg-2">To</dt>
            <dd className="break-words text-fg">{to}</dd>
            <dt className="text-fg-2">Subject</dt>
            <dd className="break-words text-fg">{preview.data?.subject ?? "…"}</dd>
          </dl>
          <pre className="max-h-48 overflow-auto whitespace-pre-wrap rounded-card bg-fill px-3 py-2 text-body text-fg" aria-label="Message text">{preview.data?.body ?? "Loading…"}</pre>
          <p className="text-footnote text-fg-2">The calendar file (.ics) is attached, so they can add the event to their calendar. Replies go to {sender.address}.</p>
        </div>
      </Dialog>
    );
  }
  return (
    <Dialog
      open
      onOpenChange={(o) => !o && onClose()}
      title="Send invitation"
      description={`Share “${event.title}” with people by email.`}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button disabled={run.isPending} onClick={() => run.mutate(false)}>
            Just save the file
          </Button>
          {sender && (
            <Button variant="primary" disabled={run.isPending || !hasAddress} onClick={() => setReviewing(true)}>
              Review and send
            </Button>
          )}
          <Button
            variant={sender ? "secondary" : "primary"}
            disabled={run.isPending || !hasAddress}
            onClick={() => run.mutate(true)}
          >
            {run.isPending ? "Preparing…" : "Open in my mail app"}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3 pb-2">
        {run.isError && (
          <p
            role="alert"
            className="rounded-card bg-danger-soft px-3 py-2 text-body text-danger"
          >
            {errorMessage(run.error)}
          </p>
        )}
        <TextField
          label="To"
          value={to}
          onChange={(e) => setTo(e.target.value)}
          placeholder="sam@example.com, kim@example.com"
          hint="Separate addresses with commas."
          autoComplete="off"
          spellCheck={false}
        />
        {senders.data && senders.data.length > 1 && (
          <label className="flex items-center justify-between gap-3 text-body text-fg">
            Send from
            <select className="rounded-control bg-fill px-2 py-1" value={sender?.id} onChange={(e) => setMailbox(e.target.value)}>
              {senders.data.map((m) => (
                <option key={m.id} value={m.id}>{m.label}: {m.address}</option>
              ))}
            </select>
          </label>
        )}
        <p className="text-footnote text-fg-2">
          {sender
            ? "“Review and send” shows the exact message first and sends it from your mailbox when you press Send. “Open in my mail app” saves a calendar file (.ics) to Downloads and opens a draft instead; attach the file, then press Send yourself."
            : "Firstlight doesn't send anything. It opens a draft in your mail app and saves a calendar file (.ics) to Downloads. Attach that file so they can add the event to their calendar, then press Send yourself."}
        </p>
      </div>
    </Dialog>
  );
}
