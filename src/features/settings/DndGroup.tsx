import { useMutation, useQuery } from "@tanstack/react-query";
import { CheckCircle2, Circle } from "lucide-react";
import { api, errorMessage } from "@/api";
import { Button, Switch, useToasts } from "@/design";
import { Group, SettingRow } from "./controls";
import { useSettings } from "./useSettings";

function Step({ found, children }: { found: boolean; children: React.ReactNode }) {
  return (
    <li className="flex items-start gap-2 text-body text-fg">
      {found ? <CheckCircle2 className="mt-0.5 size-4 shrink-0 text-success" aria-label="Done" /> : <Circle className="mt-0.5 size-4 shrink-0 text-fg-3" aria-label="Not yet" />}
      <span>{children}</span>
    </li>
  );
}

/**
 * Do Not Disturb during meetings (Mac only). macOS lets the person's own Shortcuts set Focus, so this walks them
 * through making two one-step shortcuts, shows which exist, and lets them try both.
 */
export function DndGroup() {
  const push = useToasts((s) => s.push);
  const { settings: s, update } = useSettings();
  const status = useQuery({ queryKey: ["dnd-status"], queryFn: () => api.dndStatus(), staleTime: 0, refetchOnWindowFocus: true });
  const tryIt = useMutation({
    mutationFn: (on: boolean) => api.tryDnd(on),
    onSuccess: (_, on) => push({ title: on ? "Do Not Disturb is on" : "Do Not Disturb is off" }),
    onError: (e) => push({ title: errorMessage(e), tone: "danger" }),
  });
  const st = status.data;
  if (!s || !st?.available) return null;
  const ready = st.on_found && st.off_found;
  return (
    <Group title="Do Not Disturb" footer="Firstlight turns it off only if it turned it on, and again if the app quits during a meeting.">
      <SettingRow title="Do Not Disturb in meetings" description="Turns your Mac's Do Not Disturb on while a meeting, a focus block or the focus timer runs, so chats and mail stay quiet.">
        <Switch label="Do Not Disturb in meetings" checked={s.day.dnd_meetings} disabled={!ready && !s.day.dnd_meetings} onCheckedChange={(v) => update((d) => void (d.day.dnd_meetings = v))} />
      </SettingRow>
      <li className="flex flex-col gap-2 px-4 py-3">
        <p className="text-footnote font-medium text-fg-2">{ready ? "Set up: both shortcuts found." : "One-time setup in the Shortcuts app"}</p>
        <ol className="flex flex-col gap-1.5">
          <Step found={st.on_found}>
            A shortcut named <strong>{st.on_name}</strong> with one action: Set Focus, Do Not Disturb, Turn On.
          </Step>
          <Step found={st.off_found}>
            A shortcut named <strong>{st.off_name}</strong> with one action: Set Focus, Do Not Disturb, Turn Off.
          </Step>
          <Step found={st.status_found}>
            Optional: <strong>{st.status_name}</strong> with one action: Get Current Focus. With it, a Focus you turned on yourself is left alone.
          </Step>
        </ol>
        {ready && !st.status_found && <p className="text-footnote text-fg-2">Without the third shortcut Firstlight can't tell a Focus you turned on yourself, so it may turn Do Not Disturb off when a meeting ends.</p>}
        <div className="flex flex-wrap justify-end gap-2">
          {!ready && (
            <>
              <Button size="sm" variant="plain" onClick={() => void status.refetch()}>
                Check again
              </Button>
              <Button size="sm" variant="secondary" onClick={() => api.openShortcutsApp().catch((e) => push({ title: errorMessage(e), tone: "danger" }))}>
                Open Shortcuts
              </Button>
            </>
          )}
          {ready && (
            <>
              <Button size="sm" variant="plain" disabled={tryIt.isPending} onClick={() => tryIt.mutate(true)}>
                Try: turn on
              </Button>
              <Button size="sm" variant="plain" disabled={tryIt.isPending} onClick={() => tryIt.mutate(false)}>
                Try: turn off
              </Button>
            </>
          )}
        </div>
      </li>
    </Group>
  );
}
