import { useQuery } from "@tanstack/react-query";
import { useId, useState } from "react";
import { api, errorMessage } from "@/api";
import { Button, Dialog, Segmented, TextField, useToasts } from "@/design";
import { SendBar } from "@/features/meetings/SendBar";

const EMAIL = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

/**
 * A standup or weekly update written from your tasks and calendar. The text can be changed before it is copied,
 * opened in your mail app or (when sending is on) sent. Nothing leaves the computer until you choose.
 */
export function StatusReportDialog({ initial = "daily", onClose }: { initial?: "daily" | "weekly"; onClose: () => void }) {
  const push = useToasts((s) => s.push);
  const textId = useId();
  const [kind, setKind] = useState<"daily" | "weekly">(initial);
  const report = useQuery({ queryKey: ["status-report", kind], queryFn: () => api.getStatusReport(kind), staleTime: 0 });
  // What you changed, per kind; until then the written text is shown.
  const [edits, setEdits] = useState<Partial<Record<"daily" | "weekly", string>>>({});
  const text = edits[kind] ?? report.data?.text ?? "";
  const setText = (t: string) => setEdits({ ...edits, [kind]: t });
  const [to, setTo] = useState("");
  const recipients = to
    .split(/[,;\s]+/)
    .map((s) => s.trim())
    .filter(Boolean);
  const valid = recipients.length > 0 && recipients.every((r) => EMAIL.test(r));
  const subject = report.data?.subject ?? "Update";
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()} title="Write an update" description="Written from your tasks and calendar. Change anything before you share it.">
      <div className="flex flex-col gap-3 pb-2">
        <Segmented
          fill
          label="Which update"
          value={kind}
          onValueChange={(v) => setKind(v as "daily" | "weekly")}
          options={[
            { value: "daily", label: "Standup" },
            { value: "weekly", label: "Weekly" },
          ]}
        />
        {report.isError ? (
          <p role="alert" className="rounded-card bg-danger-soft px-3 py-2 text-body text-danger">
            {errorMessage(report.error)}
          </p>
        ) : (
          <div className="flex flex-col gap-1.5">
            <label htmlFor={textId} className="text-footnote font-medium text-fg-2">
              {subject}
            </label>
            <textarea id={textId} rows={10} value={report.isPending ? "Writing…" : text} disabled={report.isPending} onChange={(e) => setText(e.target.value)} className="select-text rounded-card bg-fill px-3 py-2 font-mono text-footnote text-fg" />
          </div>
        )}
        <TextField label="Send to (optional)" type="email" value={to} autoComplete="off" placeholder="manager@company.com" onChange={(e) => setTo(e.target.value)} />
        {valid ? (
          <SendBar to={recipients} subject={subject} body={text} onDone={onClose} onCancel={onClose} />
        ) : (
          <div className="flex justify-end gap-2">
            <Button onClick={onClose}>Close</Button>
            <Button
              disabled={!text.trim()}
              onClick={() =>
                api
                  .openMailDraft(null, subject, text.slice(0, 1500))
                  .catch((e) => push({ title: errorMessage(e), tone: "danger" }))
              }
            >
              Open in mail app
            </Button>
            <Button
              variant="primary"
              disabled={!text.trim()}
              onClick={() =>
                navigator.clipboard
                  .writeText(text)
                  .then(() => push({ title: "Update copied" }))
                  .catch(() => push({ title: "Couldn't copy", tone: "danger" }))
              }
            >
              Copy
            </Button>
          </div>
        )}
      </div>
    </Dialog>
  );
}
