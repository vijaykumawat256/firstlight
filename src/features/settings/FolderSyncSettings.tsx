import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Copy, Laptop, Wifi } from "lucide-react";
import { useState } from "react";
import { api, errorMessage, type FolderSyncReport } from "@/api";
import { Button, Segmented, Switch, TextField, useToasts } from "@/design";
import { timeAgo } from "@/features/accounts/time";
import { Group, SettingRow } from "./controls";
import { useSettings } from "./useSettings";

/**
 * Settings, Your computers: share tasks, meeting notes, your own events, templates and days off between your Mac and
 * PC through a folder you already sync. Encrypted with a sync code only your computers have; no server, no account.
 */
export function FolderSyncSettings() {
  const push = useToasts((s) => s.push);
  const qc = useQueryClient();
  const { settings: s } = useSettings();
  const choices = useQuery({ queryKey: ["folder-choices"], queryFn: () => api.folderSyncChoices(), staleTime: 60_000 });
  const sharing = s?.tasks.folder_sync ?? s?.tasks.cloud_sync ?? (s?.tasks.lan_sync ? "lan" : null);
  const lanStatus = useQuery({ queryKey: ["lan-status"], queryFn: () => api.lanSyncStatus(), enabled: !!s?.tasks.lan_sync, refetchInterval: 15_000 });
  const code = useQuery({ queryKey: ["folder-code", sharing], queryFn: () => api.folderSyncCode(), enabled: !!sharing });
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: () => api.listAccounts() });
  const cloudAccounts = (accounts.data ?? []).filter((a) => a.provider === "google" || a.provider === "microsoft");
  const [where, setWhere] = useState<"folder" | "cloud" | "lan">("folder");
  const [cloudAccount, setCloudAccount] = useState("");
  const [folder, setFolder] = useState("");
  const [joinCode, setJoinCode] = useState("");
  const [last, setLast] = useState<FolderSyncReport | null>(null);
  const done = (r: FolderSyncReport) => {
    setLast(r);
    void qc.invalidateQueries();
  };
  const start = useMutation({
    mutationFn: () =>
      where === "cloud" ? api.enableCloudSync(cloudAccount, joinCode.trim() || null) : where === "lan" ? api.enableLanSync(joinCode.trim() || null) : api.enableFolderSync(folder, joinCode.trim() || null),
    onSuccess: (r) => {
      qc.setQueryData(["settings"], r.settings);
      done(r.report);
      push({ title: "Sharing between your computers is on" });
    },
  });
  const now = useMutation({
    mutationFn: () => api.syncFolderNow(),
    onSuccess: (r) => {
      done(r);
      push({ title: r.applied === 0 ? "Everything is up to date" : r.applied === 1 ? "1 change from your other computers" : `${r.applied} changes from your other computers` });
    },
    onError: (e) => push({ title: errorMessage(e), tone: "danger" }),
  });
  const lan = useMutation({
    mutationFn: (on: boolean) => (on ? api.enableLanSync(null).then((r) => r.settings) : api.disableLanSync()),
    onSuccess: (saved) => qc.setQueryData(["settings"], saved),
    onError: (e) => push({ title: errorMessage(e), tone: "danger" }),
  });
  const stop = useMutation({
    mutationFn: async () => {
      if (s?.tasks.lan_sync) await api.disableLanSync();
      if (s?.tasks.cloud_sync) return api.disableCloudSync();
      return api.disableFolderSync();
    },
    onSuccess: (saved) => {
      qc.setQueryData(["settings"], saved);
      setLast(null);
      push({ title: "Sharing stopped on this computer" });
    },
  });
  if (!s) return null;
  const storeName = (provider: string) => (provider === "google" ? "Google Drive" : "OneDrive");
  const cloudOn = s.tasks.cloud_sync ? cloudAccounts.find((a) => a.id === s.tasks.cloud_sync) : undefined;
  const place = s.tasks.folder_sync ?? (s.tasks.cloud_sync ? (cloudOn ? `${storeName(cloudOn.provider)} of ${cloudOn.email ?? cloudOn.display_name}` : "Your cloud storage") : null);
  const on = place ?? (s.tasks.lan_sync ? "This network only" : null);
  if (!on) {
    const list = choices.data ?? [];
    return (
      <div className="flex flex-col gap-7">
        <Group title="Share between your computers" footer="Your tasks, meeting notes, events you made here, templates and days off. Each computer keeps an encrypted file there; only your computers have the code to open them. Use the same place on every computer. No server of ours, no new account.">
          <li className="flex flex-col gap-3 px-4 py-3">
            <Segmented
              label="Where to keep the files"
              value={where}
              onValueChange={(v) => v && setWhere(v as "folder" | "cloud" | "lan")}
              options={[
                { value: "folder", label: "Synced folder" },
                { value: "cloud", label: "Cloud account" },
                { value: "lan", label: "Same network" },
              ]}
            />
            {where === "lan" ? (
              <p className="text-body text-fg-2">
                Your devices find each other on the same Wi-Fi or office network and swap changes directly, encrypted with your sync code. Nothing goes through the internet. They share only while both are on the same network; add a folder or cloud account later to share from anywhere.
              </p>
            ) : where === "folder" ? (
              <>
                <fieldset className="flex flex-col gap-2">
                  <legend className="mb-1 text-footnote font-medium text-fg-2">A folder you already sync (the same one on every computer)</legend>
                  {list.length === 0 && <p className="text-footnote text-fg-2">No iCloud Drive, OneDrive, Dropbox or Google Drive folder found. Type the folder's full path below.</p>}
                  {list.map((c) => (
                    <label key={c.path} className="flex items-center gap-3 text-body text-fg">
                      <input type="radio" name="folder" className="size-4 accent-accent" checked={folder === c.path} onChange={() => setFolder(c.path)} />
                      <span className="min-w-0">
                        <span className="block">{c.label}</span>
                        <span className="block truncate text-footnote text-fg-2">{c.path}</span>
                      </span>
                    </label>
                  ))}
                </fieldset>
                <TextField label="Or another folder" value={list.some((c) => c.path === folder) ? "" : folder} placeholder={list[0]?.path ?? "/Users/you/Dropbox"} autoComplete="off" onChange={(e) => setFolder(e.target.value)} />
              </>
            ) : (
              <fieldset className="flex flex-col gap-2">
                <legend className="mb-1 text-footnote font-medium text-fg-2">The Google Drive or OneDrive that keeps the files (the same account on every computer)</legend>
                {cloudAccounts.length === 0 && <p className="text-footnote text-fg-2">Connect a Google or Microsoft account first (Settings, Accounts &amp; calendars).</p>}
                {cloudAccounts.map((a) => (
                  <label key={a.id} className="flex items-center gap-3 text-body text-fg">
                    <input type="radio" name="cloud" className="size-4 accent-accent" checked={cloudAccount === a.id} onChange={() => setCloudAccount(a.id)} />
                    <span className="min-w-0">
                      <span className="block">{storeName(a.provider)}</span>
                      <span className="block truncate text-footnote text-fg-2">{a.email ?? a.display_name}</span>
                    </span>
                  </label>
                ))}
                <p className="text-footnote text-fg-2">Works without the Drive or OneDrive app. Firstlight asks the account once for a folder only it can see; it can't see your other files.</p>
              </fieldset>
            )}
            <TextField label="Sync code (only when another computer already shares)" value={joinCode} placeholder="FL1-…" autoComplete="off" spellCheck={false} onChange={(e) => setJoinCode(e.target.value)} hint="Leave empty on your first computer: a new code is made for you." />
            {start.isError && (
              <p role="alert" className="rounded-card bg-danger-soft px-3 py-2 text-body text-danger">
                {errorMessage(start.error)}
              </p>
            )}
            <div className="flex justify-end">
              <Button variant="primary" disabled={(where === "folder" ? !folder.trim() : where === "cloud" ? !cloudAccount : false) || start.isPending} onClick={() => start.mutate()}>
                {start.isPending ? "Starting…" : "Start sharing"}
              </Button>
            </div>
          </li>
        </Group>
      </div>
    );
  }
  return (
    <div className="flex flex-col gap-7">
      <Group title="Sharing" footer="Changes are shared each time calendars refresh, and when you press Sync now. Your cloud service moves the files between computers, so allow it a minute.">
        <SettingRow title={s.tasks.cloud_sync ? "Kept in" : s.tasks.folder_sync ? "Folder" : "Sharing"} description={on} />
        {place && (
          <SettingRow title="Also on the same network" description="Faster when your devices are on the same Wi-Fi or office network, and works when the internet is down.">
            <Switch label="Also share on the same network" checked={s.tasks.lan_sync} disabled={lan.isPending} onCheckedChange={(v) => lan.mutate(v)} />
          </SettingRow>
        )}
        <li className="flex flex-col gap-2 px-4 py-3">
          <span className="text-footnote font-medium text-fg-2">Sync code: type it on your other computers</span>
          <div className="flex items-center gap-2">
            <code className="min-w-0 flex-1 break-all rounded-control bg-fill px-3 py-2 font-mono text-footnote text-fg select-text">{code.data ?? "…"}</code>
            <Button
              size="sm"
              variant="secondary"
              leading={<Copy className="size-4" />}
              disabled={!code.data}
              onClick={() =>
                navigator.clipboard
                  .writeText(code.data ?? "")
                  .then(() => push({ title: "Sync code copied" }))
                  .catch(() => push({ title: "Couldn't copy", tone: "danger" }))
              }
            >
              Copy
            </Button>
          </div>
          <p className="text-footnote text-fg-2">Keep it private: anyone with the code and the folder can read what is shared.</p>
        </li>
      </Group>
      {s.tasks.lan_sync && (
        <Group title="On this network" footer="Looked for every 30 seconds. Your Mac may ask once to allow Local Network access, and Windows to allow it on private networks.">
          {(lanStatus.data?.peers ?? []).length === 0 ? (
            <li className="px-4 py-3 text-body text-fg-2">No other device found yet. Turn on sharing on the same network, with the same code, on your other computer.</li>
          ) : (
            lanStatus.data!.peers.map((p) => (
              <li key={"lan" + p.name} className="flex min-h-12 items-center gap-3 px-4 py-2">
                <Wifi className="size-4 shrink-0 text-fg-2" aria-hidden />
                <span className="min-w-0 flex-1 truncate text-headline text-fg">{p.name}</span>
                <span className="shrink-0 text-footnote text-fg-2">changed {timeAgo(p.written_at)}</span>
              </li>
            ))
          )}
        </Group>
      )}
      {place && <Group title="Your other computers">
        {(last?.peers ?? []).length === 0 ? (
          <li className="px-4 py-3 text-body text-fg-2">{last ? "None yet. Turn sharing on with this code on your other computer." : "Press Sync now to look."}</li>
        ) : (
          last!.peers.map((p) => (
            <li key={p.name + p.written_at} className="flex min-h-12 items-center gap-3 px-4 py-2">
              <Laptop className="size-4 shrink-0 text-fg-2" aria-hidden />
              <span className="min-w-0 flex-1 truncate text-headline text-fg">{p.name}</span>
              <span className="shrink-0 text-footnote text-fg-2">{timeAgo(p.written_at)}</span>
            </li>
          ))
        )}
        {last && last.unreadable > 0 && <li className="px-4 py-3 text-footnote text-warning">{last.unreadable === 1 ? "1 file" : `${last.unreadable} files`} in the folder use a different sync code and were left alone.</li>}
      </Group>}
      <div className="flex justify-end gap-2">
        <Button variant="plain" disabled={stop.isPending} onClick={() => stop.mutate()}>
          Stop sharing
        </Button>
        {place && (
          <Button variant="primary" disabled={now.isPending} onClick={() => now.mutate()}>
            {now.isPending ? "Syncing…" : "Sync now"}
          </Button>
        )}
      </div>
    </div>
  );
}
