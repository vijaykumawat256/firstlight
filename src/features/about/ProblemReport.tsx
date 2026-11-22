import { useMutation, useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { api, errorMessage } from "@/api";
import { Button, Dialog, useToasts } from "@/design";

/**
 * "Report a problem": the whole report is shown (and can be edited) before anything happens. Saving puts a text file in
 * Downloads; when this build has a support address, "Save and email" also opens a draft in the person's mail app for
 * them to attach it to. Firstlight itself sends nothing.
 */
export function ProblemReportButton() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button size="sm" variant="plain" onClick={() => setOpen(true)}>
        Report a problem…
      </Button>
      {open && <ProblemReportDialog onClose={() => setOpen(false)} />}
    </>
  );
}

function ProblemReportDialog({ onClose }: { onClose: () => void }) {
  const push = useToasts((s) => s.push);
  const report = useQuery({ queryKey: ["problem-report"], queryFn: () => api.getProblemReport(), gcTime: 0 });
  const [edited, setEdited] = useState<string | null>(null);
  const text = edited ?? report.data?.text ?? "";
  const save = useMutation({
    mutationFn: async (email: boolean) => {
      const path = await api.saveProblemReport(text);
      const support = report.data?.support;
      if (email && support) {
        const file = path.split(/[\\/]/).pop() ?? path;
        await api.openMailDraft(support, "Firstlight problem report", `Hello,\n\nI've attached a problem report from Firstlight (${file}, in my Downloads folder).\n\nWhat happened:\n`);
      }
      return path;
    },
    onSuccess: (path, email) => {
      push({ title: email ? "Report saved: attach it to the email" : "Report saved", description: path });
      onClose();
    },
  });
  return (
    <Dialog
      open
      onOpenChange={(o) => !o && onClose()}
      title="Report a problem"
      description="Here is everything in the report. Add what happened at the top. It has no calendar, email or account content, and Firstlight doesn't send it: you attach it to an email yourself."
      footer={
        <>
          <Button variant={report.data?.support ? "secondary" : "primary"} disabled={!text || save.isPending} onClick={() => save.mutate(false)}>
            Save to Downloads
          </Button>
          {report.data?.support && (
            <Button variant="primary" disabled={!text || save.isPending} onClick={() => save.mutate(true)}>
              Save and email
            </Button>
          )}
        </>
      }
    >
      <label className="flex flex-col gap-1">
        <span className="sr-only">Problem report</span>
        <textarea
          value={text}
          onChange={(e) => setEdited(e.target.value)}
          rows={12}
          spellCheck={false}
          className="w-full select-text resize-y rounded-card bg-fill p-3 font-mono text-caption text-fg"
        />
      </label>
      {save.isError && (
        <p role="alert" className="mt-2 text-footnote text-danger">
          {errorMessage(save.error)}
        </p>
      )}
    </Dialog>
  );
}
