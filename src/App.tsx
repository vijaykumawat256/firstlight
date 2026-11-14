import { QueryClient, QueryClientProvider, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { api } from "@/api";
import { useNav } from "@/app/nav";
import { Skeleton, ToastHost, TooltipProvider } from "@/design";
import { Gallery } from "@/design/Gallery";
import { AccountsScreen } from "@/features/accounts/AccountsScreen";
import { Onboarding } from "@/features/accounts/Onboarding";
import { BriefingScreen } from "@/features/briefing/BriefingScreen";
import { UpNextPanel } from "@/features/briefing/UpNextPanel";

const queryClient = new QueryClient({
  defaultOptions: { queries: { staleTime: 30_000, refetchOnWindowFocus: true, retry: 1 } },
});

function useHash() {
  const [hash, setHash] = useState(window.location.hash);
  useEffect(() => {
    const on = () => setHash(window.location.hash);
    window.addEventListener("hashchange", on);
    return () => window.removeEventListener("hashchange", on);
  }, []);
  return hash;
}

/** Refresh every screen when the computer unlocks or a sync finishes. */
function useBackendEvents() {
  const qc = useQueryClient();
  useEffect(() => {
    const offs: Array<() => void> = [];
    let cancelled = false;
    const keep = (p: Promise<() => void>) =>
      void p.then((off) => (cancelled ? off() : offs.push(off)));
    keep(api.onUnlock(() => void qc.invalidateQueries()));
    keep(api.onSyncFinished(() => void qc.invalidateQueries()));
    return () => {
      cancelled = true;
      offs.forEach((off) => off());
    };
  }, [qc]);
}

function Shell() {
  useBackendEvents();
  const view = useNav((s) => s.view);
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: () => api.listAccounts() });

  if (accounts.isPending) {
    return (
      <div className="flex h-full flex-col gap-4 p-5 pt-16" aria-busy aria-label="Loading">
        <Skeleton className="h-10 w-56" />
        <Skeleton className="h-28 rounded-card" />
      </div>
    );
  }
  if ((accounts.data ?? []).length === 0) return <Onboarding />;
  return view === "accounts" ? <AccountsScreen /> : <BriefingScreen />;
}

export default function App() {
  const hash = useHash();
  return (
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>
        <ToastHost>
          {hash === "#/upnext" ? <UpNextPanel /> : import.meta.env.DEV && hash === "#/gallery" ? <Gallery /> : <Shell />}
        </ToastHost>
      </TooltipProvider>
    </QueryClientProvider>
  );
}
