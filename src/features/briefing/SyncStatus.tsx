import { useQuery } from "@tanstack/react-query";
import { AlertCircle, RefreshCw } from "lucide-react";
import { api } from "@/api";
import { useNav } from "@/app/nav";
import { timeAgo } from "@/features/accounts/time";
import { useSettings } from "@/features/settings/useSettings";

/**
 * One line under the date while sharing on the network is on: who this device last synced with and when, or what is
 * wrong. Tapping it opens Settings, Your computers.
 */
export function SyncStatus() {
  const { settings: s } = useSettings();
  const on = !!s?.tasks.lan_sync;
  const st = useQuery({ queryKey: ["sharing-status"], queryFn: () => api.sharingStatus(), enabled: on, refetchInterval: 10_000 });
  const go = useNav((n) => n.go);
  const setSection = useNav((n) => n.setSettingsSection);
  if (!on || !st.data) return null;
  const { last_ok, names, problem } = st.data;
  const who = names.length === 0 ? "your other device" : names.length === 1 ? names[0] : `${names[0]} and ${names.length - 1} more`;
  const text = problem ?? (last_ok ? `Synced with ${who} · ${timeAgo(last_ok)}` : "Looking for your other devices…");
  return (
    <button
      type="button"
      onClick={() => {
        go("settings");
        setSection("computers");
      }}
      className={`mt-2 flex items-center gap-1.5 text-left text-footnote ${problem ? "text-warning" : "text-fg-2"}`}
    >
      {problem ? <AlertCircle className="size-3.5 shrink-0" aria-hidden /> : <RefreshCw className="size-3.5 shrink-0" aria-hidden />}
      <span role="status">{text}</span>
    </button>
  );
}
