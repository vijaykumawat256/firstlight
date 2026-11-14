import * as RDialog from "@radix-ui/react-dialog";
import * as RTooltip from "@radix-ui/react-tooltip";
import * as RToast from "@radix-ui/react-toast";
import { X } from "lucide-react";
import clsx from "clsx";
import { create } from "zustand";
import type { ReactNode } from "react";
import { IconButton } from "./controls";

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
}: OverlayProps & { contentClass: string }) {
  return (
    <RDialog.Root open={open} onOpenChange={onOpenChange}>
      <RDialog.Portal>
        <RDialog.Overlay className="fixed inset-0 z-40 animate-fade-in bg-overlay" />
        <RDialog.Content className={clsx("fixed z-50 flex flex-col bg-surface-raised shadow-float", contentClass)}>
          <div className="flex items-start justify-between gap-3 px-5 pb-2 pt-5">
            <div>
              <RDialog.Title className="text-title font-semibold text-fg">{title}</RDialog.Title>
              {description ? (
                <RDialog.Description className="mt-1 text-body text-fg-2">{description}</RDialog.Description>
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
          <div className="min-h-0 flex-1 overflow-y-auto px-5 py-3">{children}</div>
          {footer && <div className="flex justify-end gap-2 px-5 pb-5 pt-3">{footer}</div>}
        </RDialog.Content>
      </RDialog.Portal>
    </RDialog.Root>
  );
}

/** Centred modal for decisions that interrupt the flow. Prefer a Sheet for everything else. */
export function Dialog(props: OverlayProps) {
  return (
    <OverlayShell
      {...props}
      contentClass="left-1/2 top-1/2 w-[min(92vw,380px)] max-h-[80vh] -translate-x-1/2 -translate-y-1/2 animate-pop-in rounded-sheet"
    />
  );
}

/** Bottom sheet that slides up over the current screen. */
export function Sheet(props: OverlayProps) {
  return (
    <OverlayShell
      {...props}
      contentClass="inset-x-0 bottom-0 max-h-[85vh] animate-sheet-in rounded-t-sheet"
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
      <RToast.Viewport className="fixed inset-x-0 bottom-0 z-[60] m-0 flex list-none flex-col gap-2 p-4 outline-none" />
    </RToast.Provider>
  );
}
