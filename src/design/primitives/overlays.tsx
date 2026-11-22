import * as RDialog from "@radix-ui/react-dialog";
import * as RTooltip from "@radix-ui/react-tooltip";
import * as RToast from "@radix-ui/react-toast";
import { X } from "lucide-react";
import clsx from "clsx";
import { create } from "zustand";
import { useEffect, useState, type CSSProperties, type ReactNode } from "react";
import { isPhone } from "@/app/platform";
import { IconButton } from "./controls";

/**
 * Tracks the phone's visual viewport while a sheet or dialog is open so the iOS/Android soft keyboard never pushes
 * the header off the top of the screen or covers the footer buttons.
 */
function useVisualViewport(active: boolean) {
  const [vp, setVp] = useState<{ top: number; height: number; keyboard: boolean } | null>(null);
  useEffect(() => {
    if (!active || !isPhone || typeof window === "undefined" || !window.visualViewport) return;
    const vv = window.visualViewport;
    const sync = () => {
      const keyboard = vv.height < window.innerHeight - 100;
      setVp({ top: Math.max(0, Math.round(vv.offsetTop)), height: Math.round(vv.height), keyboard });
    };
    vv.addEventListener("resize", sync);
    vv.addEventListener("scroll", sync);
    return () => {
      vv.removeEventListener("resize", sync);
      vv.removeEventListener("scroll", sync);
    };
  }, [active]);
  return active ? vp : null;
}

/* ------------------------------------------------------- Dialog & Sheet */

interface OverlayProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: string;
  children: ReactNode;
  footer?: ReactNode;
}

function OverlayShell({
  open,
  onOpenChange,
  title,
  description,
  children,
  footer,
  contentClass,
  headerClass,
}: OverlayProps & { contentClass: string; headerClass?: string }) {
  const vp = useVisualViewport(open);
  // Only override geometry while the soft keyboard is actually open: otherwise CSS classes control full-screen vs bottom-sheet.
  const phoneStyle: CSSProperties | undefined =
    isPhone && vp?.keyboard ? { top: vp.top, bottom: "auto", height: vp.height, maxHeight: vp.height } : undefined;
  return (
    <RDialog.Root open={open} onOpenChange={onOpenChange}>
      <RDialog.Portal>
        <RDialog.Overlay className="fixed inset-0 z-40 animate-fade-in bg-overlay" />
        <RDialog.Content style={phoneStyle} className={clsx("fixed z-50 flex flex-col bg-surface-raised shadow-float", contentClass)}>
          {/* Subtle iOS-style grabber on phone bottom sheets (hidden on full-screen sheets and desktop). */}
          <div aria-hidden className="mx-auto mt-2.5 hidden h-1 w-9 shrink-0 rounded-full bg-fill-strong phone:block phone:group-has-[input:not([type=checkbox]):not([type=radio]),textarea]/dialog:hidden phone:group-[.is-sheet]/dialog:hidden" />
          <div className={clsx("flex items-start justify-between gap-3 border-b border-separator/70 px-5 pb-3 pt-4", vp?.keyboard && "phone:py-2.5", headerClass)}>
            <div className="min-w-0">
              <RDialog.Title className="text-title font-semibold text-fg">{title}</RDialog.Title>
              {description ? (
                <RDialog.Description className="mt-1 text-body text-fg-2 phone:text-footnote">{description}</RDialog.Description>
              ) : (
                <RDialog.Description className="sr-only">{title}</RDialog.Description>
              )}
            </div>
            <RDialog.Close asChild>
              <IconButton label="Close">
                <X className="size-4" />
              </IconButton>
            </RDialog.Close>
          </div>
          <div className={clsx("min-h-0 flex-1 overflow-y-auto px-5 py-3", !footer && !vp?.keyboard && "phone:pb-[max(1.25rem,env(safe-area-inset-bottom))]")}>{children}</div>
          {/* On a phone the buttons share the width, big enough for a thumb, above the home bar (or flush above the keyboard). */}
          {footer && (
            <div className={clsx("flex flex-wrap justify-end gap-2 px-5 pt-3 phone:border-t phone:border-separator phone:[&>*]:flex-1", vp?.keyboard ? "pb-3" : "pb-5 phone:pb-[max(1.25rem,env(safe-area-inset-bottom))]")}>{footer}</div>
          )}
        </RDialog.Content>
      </RDialog.Portal>
    </RDialog.Root>
  );
}

/**
 * Centred modal for decisions that interrupt the flow. Prefer a Sheet for everything else. On a phone it rises from the
 * bottom; one with a text field fills the screen, so the keyboard never covers the field or the buttons.
 */
export function Dialog(props: OverlayProps) {
  return (
    <OverlayShell
      {...props}
      contentClass="group/dialog left-1/2 top-1/2 w-[min(92vw,380px)] max-h-[80vh] -translate-x-1/2 -translate-y-1/2 animate-pop-in rounded-sheet phone:inset-x-0 phone:top-auto phone:bottom-0 phone:w-full phone:max-h-[92dvh] phone:translate-x-0 phone:translate-y-0 phone:rounded-b-none phone:animate-sheet-in phone:has-[input:not([type=checkbox]):not([type=radio]),textarea]:top-0 phone:has-[input:not([type=checkbox]):not([type=radio]),textarea]:max-h-none phone:has-[input:not([type=checkbox]):not([type=radio]),textarea]:rounded-none"
      headerClass="phone:group-has-[input:not([type=checkbox]):not([type=radio]),textarea]/dialog:pt-[max(1.25rem,env(safe-area-inset-top))]"
    />
  );
}

/** Bottom sheet that slides up over the current screen. */
export function Sheet(props: OverlayProps) {
  return (
    <OverlayShell
      {...props}
      // On a phone a sheet is a full page, so the keyboard never covers its fields or buttons.
      contentClass="group/dialog is-sheet inset-x-0 bottom-0 max-h-[85vh] animate-sheet-in rounded-t-sheet phone:top-0 phone:max-h-none phone:rounded-none"
      headerClass="phone:pt-[max(1rem,env(safe-area-inset-top))]"
    />
  );
}

/* --------------------------------------------------------------- Tooltip */

export function TooltipProvider({ children }: { children: ReactNode }) {
  return <RTooltip.Provider delayDuration={400}>{children}</RTooltip.Provider>;
}

export function Tooltip({ content, children }: { content: string; children: ReactNode }) {
  return (
    <RTooltip.Root>
      <RTooltip.Trigger asChild>{children}</RTooltip.Trigger>
      <RTooltip.Portal>
        <RTooltip.Content
          sideOffset={6}
          className="z-50 animate-fade-in rounded-control bg-fg px-2 py-1 text-footnote font-medium text-bg shadow-float"
        >
          {content}
        </RTooltip.Content>
      </RTooltip.Portal>
    </RTooltip.Root>
  );
}

/* ----------------------------------------------------------------- Toast */

interface ToastItem {
  id: number;
  title: string;
  description?: string;
  tone: "neutral" | "danger";
}

interface ToastStore {
  items: ToastItem[];
  push: (t: Omit<ToastItem, "id" | "tone"> & { tone?: ToastItem["tone"] }) => void;
  remove: (id: number) => void;
}

let nextToastId = 1;

export const useToasts = create<ToastStore>((set) => ({
  items: [],
  push: (t) => set((s) => ({ items: [...s.items, { tone: "neutral", ...t, id: nextToastId++ }] })),
  remove: (id) => set((s) => ({ items: s.items.filter((i) => i.id !== id) })),
}));

export function ToastHost({ children }: { children: ReactNode }) {
  const { items, remove } = useToasts();
  return (
    <RToast.Provider swipeDirection="down" duration={4000}>
      {children}
      {items.map((t) => (
        <RToast.Root
          key={t.id}
          onOpenChange={(open) => !open && remove(t.id)}
          className="animate-slide-up rounded-card bg-surface-raised px-4 py-3 shadow-float data-[state=closed]:opacity-0"
        >
          <RToast.Title className={clsx("text-body font-semibold", t.tone === "danger" ? "text-danger" : "text-fg")}>
            {t.title}
          </RToast.Title>
          {t.description && <RToast.Description className="text-footnote text-fg-2">{t.description}</RToast.Description>}
        </RToast.Root>
      ))}
      {/* On a phone, above the tab bar and the home bar. */}
      <RToast.Viewport className="fixed inset-x-0 bottom-0 z-[60] m-0 flex list-none flex-col gap-2 p-4 outline-none phone:bottom-[calc(4.5rem+env(safe-area-inset-bottom))]" />
    </RToast.Provider>
  );
}
