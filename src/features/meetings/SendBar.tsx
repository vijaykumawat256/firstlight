import { useMutation, useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { api, errorMessage } from "@/api";
import { Button, Select, useToasts } from "@/design";

/**
 * The buttons under a note the person has just read in full: Send (from a mailbox or account they allowed, when
 * sending is turned on) or Open in mail app. Nothing is sent without a press of Send.
 */
export function SendBar({ to, subject, body, onDone, onCancel }: { to: string[]; subject: string; body: string; onDone: () => void; onCancel: () => void }) {
  const push = useToasts((s) => s.push);
  const senders = useQuery({ queryKey: ["message-senders"], queryFn: () => api.messageSenders(), staleTime: 0 });
  const list = senders.data ?? [];
  const [from, setFrom] = useState<string | null>(null);
  const chosen = from ?? list[0]?.id ?? null;
  const send = useMutation({
    mutationFn: () => api.sendMessage(chosen!, to.join(", "), subject, body),
    onSuccess: () => {
      push({ title: to.length === 1 ? `Sent to ${to[0]}` : `Sent to ${to.length} people` });
      onDone();
    },
  });
  const draft = () =>
    api
      .openMailDraft(to.join(", ") || null, subject, body)
      .then(onDone)
      .catch((e) => push({ title: errorMessage(e), tone: "danger" }));
  const canSend = list.length > 0 && to.length > 0;
  return (
    <div className="flex flex-col gap-2">
      {send.isError && (
        <p role="alert" className="rounded-card bg-danger-soft px-3 py-2 text-footnote text-danger">
          {errorMessage(send.error)}
        </p>
      )}
      {canSend && list.length > 1 && (
        <div className="flex items-center justify-between gap-3">
          <span className="text-body text-fg">From</span>
          <Select label="Send from" value={chosen ?? ""} onValueChange={setFrom} options={list.map((s) => ({ value: s.id, label: `${s.address} (${s.label})` }))} />
        </div>
      )}
      {canSend && list.length === 1 && <p className="text-footnote text-fg-2">From {list[0]!.address}</p>}
      <div className="flex flex-wrap justify-end gap-2">
        <Button onClick={onCancel}>Cancel</Button>
        <Button variant={canSend ? "secondary" : "primary"} onClick={() => void draft()}>
          Open in mail app
        </Button>
        {canSend && (
          <Button variant="primary" disabled={send.isPending} onClick={() => send.mutate()}>
            {send.isPending ? "Sending…" : "Send"}
          </Button>
        )}
      </div>
    </div>
  );
}
