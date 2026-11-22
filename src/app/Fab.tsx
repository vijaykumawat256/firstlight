import { Plus } from "lucide-react";

/** Phones only: the round "new" button at the bottom right, above the tab bar, where a thumb reaches. */
export function Fab({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className="fixed right-5 bottom-[calc(5.25rem+env(safe-area-inset-bottom))] z-30 flex size-14 items-center justify-center rounded-full bg-accent-fill text-accent-fg shadow-float ring-1 ring-black/10 transition-transform duration-150 ease-standard active:scale-90"
    >
      <Plus className="size-7" aria-hidden />
    </button>
  );
}
