import { QueryClient, QueryClientProvider, useQuery, useQueryClient } from "@tanstack/react-query";
import { Suspense, lazy, useEffect, useState } from "react";
import { api } from "@/api";
import { useNav } from "@/app/nav";
import { Skeleton, ToastHost, TooltipProvider, useToasts } from "@/design";
const Gallery = lazy(() => import("@/design/Gallery").then((m) => ({ default: m.Gallery })));
const AboutScreen = lazy(() => import("@/features/about/AboutScreen").then((m) => ({ default: m.AboutScreen })));
const AccountsScreen = lazy(() => import("@/features/accounts/AccountsScreen").then((m) => ({ default: m.AccountsScreen })));
import { Onboarding } from "@/features/accounts/Onboarding";
import { SettingsApplier } from "@/features/settings/SettingsApplier";
const SettingsScreen = lazy(() => import("@/features/settings/SettingsScreen").then((m) => ({ default: m.SettingsScreen })));
import { BriefingScreen } from "@/features/briefing/BriefingScreen";
const CalendarScreen = lazy(() => import("@/features/calendar/CalendarScreen").then((m) => ({ default: m.CalendarScreen })));
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
    keep(api.onUpdateAvailable((version) => useToasts.getState().push({ title: `Firstlight ${version} is available`, description: "Open Settings, then About, to install it." })));
    return () => {
      cancelled = true;
      offs.forEach((off) => off());
    };
  }, [qc]);
}

function ScreenSkeleton() {
  return (
    <div className="flex h-full flex-col gap-4 p-5 pt-16" role="status" aria-busy>
      <span className="sr-only">Loading…</span>
      <Skeleton className="h-10 w-56" />
      <Skeleton className="h-28 rounded-card" />
    </div>
  );
}

function Shell() {
  useBackendEvents();
  const view = useNav((s) => s.view);
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: () => api.listAccounts() });

  if (accounts.isPending) {
    return (
      <div className="flex h-full flex-col gap-4 p-5 pt-16" role="status" aria-busy>
        <span className="sr-only">Loading…</span>
        <Skeleton className="h-10 w-56" />
        <Skeleton className="h-28 rounded-card" />
      </div>
    );
  }
  if ((accounts.data ?? []).length === 0) return <Onboarding />;
  // The briefing is what opens at every unlock, so it is in the main bundle. The screens reached from it load on demand.
  const screen = view === "accounts" ? <AccountsScreen /> : view === "calendar" ? <CalendarScreen /> : view === "settings" ? <SettingsScreen /> : view === "about" ? <AboutScreen /> : <BriefingScreen />;
  return <Suspense fallback={<ScreenSkeleton />}>{screen}</Suspense>;
}

export default function App() {
  const hash = useHash();
  return (
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>
        <ToastHost>
          <SettingsApplier>
            {hash === "#/upnext" ? <UpNextPanel /> : import.meta.env.DEV && hash === "#/gallery" ? <Suspense fallback={<ScreenSkeleton />}><Gallery /></Suspense> : <Shell />}
          </SettingsApplier>
        </ToastHost>
      </TooltipProvider>
    </QueryClientProvider>
  );
}
