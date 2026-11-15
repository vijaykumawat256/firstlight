import { useMutation, useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { api, errorMessage, type CalEvent, type InviteSender } from "@/api";
import { Button, Dialog, TextField, useToasts } from "@/design";

/** The mailboxes people can be invited from. Empty means Firstlight can't send real invitations, so it offers none. */
export function useInviteSenders() {
  return useQuery({ queryKey: ["invite-senders"], queryFn: () => api.inviteSenders(), staleTime: 30_000 });
}

/**
 * Invite people to one of your own events. They get a real calendar invitation from your mailbox: Accept and
 * Decline appear in their calendar app and their answers come back to you. Firstlight shows the exact message
 * first and sends it only when you press Send.
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
  return event ? <Body event={event} invitees={invitees} onClose={onClose} /> : null;
}

function Body({ event, invitees, onClose }: { event: CalEvent; invitees: string[]; onClose: () => void }) {
  const push = useToasts((s) => s.push);
  const [to, setTo] = useState(invitees.join(", "));
  const senders = useInviteSenders();
  const [reviewing, setReviewing] = useState(false);
  const [mailbox, setMailbox] = useState<string | null>(null);
  const sender: InviteSender | undefined = senders.data?.find((m) => m.id === mailbox) ?? senders.data?.[0];
  const preview = useQuery({
    queryKey: ["invite-preview", event.id, sender?.id, "request"],
    queryFn: () => api.invitePreview(event.id, sender!.id, "request"),
    enabled: reviewing && !!sender,
  });
  const send = useMutation({
    mutationFn: () => api.sendInviteMail(event.id, to, sender!.id, "request"),
    onSuccess: () => {
      onClose();
      push({ title: "Invitation sent", description: `From ${sender?.address}` });
    },
  });
  const hasAddress = to.trim().length > 0;

  // Without a mailbox to send from there is nothing real to offer, so nothing is.
  if (!senders.isPending && !sender) {
    return (
      <Dialog open onOpenChange={(o) => !o && onClose()} title="Send invitation" description="Invitations can't be sent right now." footer={<Button onClick={onClose}>Close</Button>}>
        <p className="text-body text-fg-2">Turn on “Send invitations from Firstlight” in Settings, under Email, and connect a Yahoo or iCloud mailbox.</p>
      </Dialog>
    );
  }

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
          <p className="text-footnote text-fg-2">
            It goes out as a real calendar invitation: Accept and Decline appear in their calendar app, and their answers come back to {sender.address}.
          </p>
        </div>
      </Dialog>
    );
  }
  return (
    <Dialog
      open
      onOpenChange={(o) => !o && onClose()}
      title="Send invitation"
      description={`Invite people to “${event.title}”.`}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={!sender || !hasAddress} onClick={() => setReviewing(true)}>
            Review and send
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3 pb-2">
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
          They get a real calendar invitation from {sender?.address ?? "your mailbox"}. You see the exact message first, and nothing is sent until you press Send. If you change the event later, send it again and their calendar updates.
        </p>
      </div>
    </Dialog>
  );
}
