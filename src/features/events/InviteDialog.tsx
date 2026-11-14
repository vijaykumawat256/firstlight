import { useMutation } from "@tanstack/react-query";
import { useState } from "react";
import { api, errorMessage, type CalEvent } from "@/api";
import { Button, Dialog, TextField, useToasts } from "@/design";

/**
 * Share one of your own events. Firstlight sends nothing: it saves a calendar file to Downloads and
 * opens a draft in your own mail app, which you read and send yourself.
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
          <Button
            variant="primary"
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
        <p className="text-footnote text-fg-2">
          Firstlight doesn't send anything. It opens a draft in your mail app
          and saves a calendar file (.ics) to Downloads. Attach that file so
          they can add the event to their calendar, then press Send yourself.
        </p>
      </div>
    </Dialog>
  );
}
