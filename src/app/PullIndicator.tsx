import { RefreshCw } from "lucide-react";

/** The little circle that appears while pulling down to refresh, and spins while refreshing. */
export function PullIndicator({ pull, busy, ready }: { pull: number; busy: boolean; ready: boolean }) {
  if (pull === 0 && !busy) return null;
  return (
    <div aria-hidden className="pointer-events-none flex justify-center overflow-hidden" style={{ height: busy ? 44 : pull }}>
      <span className={`mt-2 flex size-8 items-center justify-center rounded-full bg-surface shadow-card ${ready || busy ? "text-accent" : "text-fg-3"}`}>
        <RefreshCw className={`size-4 ${busy ? "animate-spin" : ""}`} style={busy ? undefined : { transform: `rotate(${pull * 3}deg)` }} />
      </span>
    </div>
  );
}
