import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { ToastHost, TooltipProvider } from "@/design";
import { BriefingScreen } from "@/features/briefing/BriefingScreen";
import { Gallery } from "@/design/Gallery";

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

export default function App() {
  const hash = useHash();
  return (
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>
        <ToastHost>
          {import.meta.env.DEV && hash === "#/gallery" ? <Gallery /> : <BriefingScreen />}
        </ToastHost>
      </TooltipProvider>
    </QueryClientProvider>
  );
}
