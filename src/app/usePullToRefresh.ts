import { useEffect, useRef, useState, type RefObject } from "react";
import { isPhone } from "./platform";

const TRIGGER = 72;

/**
 * Phones only: pulling down from the top of a scrolling area refreshes, as phones' own apps do. Returns how far the
 * content is pulled (for the indicator) and whether a refresh is running.
 */
export function usePullToRefresh(ref: RefObject<HTMLElement | null>, onRefresh: () => Promise<unknown> | void) {
  const [pull, setPull] = useState(0);
  const [busy, setBusy] = useState(false);
  const start = useRef<number | null>(null);
  const latest = useRef(onRefresh);
  useEffect(() => {
    latest.current = onRefresh;
  });
  useEffect(() => {
    const el = ref.current;
    if (!isPhone || !el) return;
    let distance = 0;
    const down = (e: TouchEvent) => {
      start.current = el.scrollTop <= 0 ? (e.touches[0]?.clientY ?? null) : null;
      distance = 0;
    };
    const move = (e: TouchEvent) => {
      if (start.current === null) return;
      distance = Math.max(0, (e.touches[0]?.clientY ?? 0) - start.current);
      // Resistance, like a rubber band.
      setPull(Math.min(110, distance * 0.5));
    };
    const up = () => {
      if (start.current !== null && distance * 0.5 >= TRIGGER) {
        setBusy(true);
        void Promise.resolve(latest.current()).finally(() => setTimeout(() => setBusy(false), 600));
      }
      start.current = null;
      distance = 0;
      setPull(0);
    };
    el.addEventListener("touchstart", down, { passive: true });
    el.addEventListener("touchmove", move, { passive: true });
    el.addEventListener("touchend", up);
    el.addEventListener("touchcancel", up);
    return () => {
      el.removeEventListener("touchstart", down);
      el.removeEventListener("touchmove", move);
      el.removeEventListener("touchend", up);
      el.removeEventListener("touchcancel", up);
    };
  }, [ref]);
  return { pull, busy, ready: pull >= TRIGGER };
}
