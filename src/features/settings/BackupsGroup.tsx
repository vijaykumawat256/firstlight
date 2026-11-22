import { useMutation, useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { api, errorMessage, type BackupInfo } from "@/api";
import { Button, Dialog } from "@/design";
import { Group, SettingRow } from "./controls";

const WHY: Record<BackupInfo["reason"], string> = {
  daily: "Daily copy",
  update: "Before an update",
  before_restore: "Before a restore",
};

const when = (iso: string) => new Date(iso).toLocaleString(undefined, { weekday: "short", day: "numeric", month: "short", hour: "numeric", minute: "2-digit" });

/**
 * Settings, Privacy & data: the automatic backups (daily and before each update; encrypted, on this device) and
 * putting one back. Restoring restarts the app; what is there now is kept as a backup of its own.
 */
export function BackupsGroup() {
  const list = useQuery({ queryKey: ["backups"], queryFn: () => api.listBackups() });
  const [chosen, setChosen] = useState<BackupInfo | null>(null);
  const restore = useMutation({ mutationFn: (name: string) => api.restoreBackup(name) });
  const backups = list.data ?? [];
  return (
    <>
      <Group
        title="Automatic backups"
        footer="Firstlight copies your tasks, notes, events and settings every day and before each update, and keeps the last 10. They stay on this device, encrypted like everything else. Your sign-ins and passwords stay in the system keychain and aren't part of them."
      >
        {backups.length === 0 ? (
          <SettingRow title="No backups yet" description="The first one is made within a minute of starting." />
        ) : (
          backups.slice(0, 5).map((b) => (
            <SettingRow key={b.name} title={when(b.taken_at)} description={WHY[b.reason]}>
              <Button size="sm" variant="secondary" aria-label={`Restore the copy from ${when(b.taken_at)}`} onClick={() => setChosen(b)}>
                Restore…
              </Button>
            </SettingRow>
          ))
        )}
      </Group>
      <Dialog
        open={chosen !== null}
        onOpenChange={(o) => {
          if (!o) {
            setChosen(null);
            restore.reset();
          }
        }}
        title="Restore this backup?"
        description={chosen ? `Firstlight restarts with your data as it was on ${when(chosen.taken_at)}. Anything changed since then is replaced. What you have now is kept as a backup first, so you can go back to it the same way.` : undefined}
        footer={
          <>
            <Button onClick={() => setChosen(null)}>Cancel</Button>
            <Button variant="primary" disabled={restore.isPending} onClick={() => chosen && restore.mutate(chosen.name)}>
              Restore and restart
            </Button>
          </>
        }
      >
        {restore.isError && (
          <p role="alert" className="text-footnote text-danger">
            {errorMessage(restore.error)}
          </p>
        )}
        {restore.isSuccess && <p role="status" className="text-footnote text-fg-2">Restarting…</p>}
      </Dialog>
    </>
  );
}
