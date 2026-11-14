import { CalendarDays } from "lucide-react";
import { useState } from "react";
import { Button } from "@/design";
import { AddAccountSheet } from "./AddAccountSheet";

/** First-run screen: shown whenever no calendar is connected. */
export function Onboarding() {
  const [open, setOpen] = useState(false);
  return (
    <div className="flex h-full flex-col">
      <div data-tauri-drag-region className="h-11 shrink-0" />
      <main className="flex flex-1 flex-col items-center justify-center gap-6 px-8 pb-16 text-center animate-fade-in">
        <div className="flex size-20 items-center justify-center rounded-[22px] bg-accent text-accent-fg shadow-float">
          <CalendarDays className="size-10" strokeWidth={1.5} aria-hidden />
        </div>
        <div>
          <h1 className="text-largetitle font-bold tracking-tight text-fg">Welcome to Daybrief</h1>
          <p className="mx-auto mt-2 max-w-[30ch] text-headline text-fg-2">Your day at a glance, every time you unlock. Connect a calendar to begin.</p>
        </div>
        <Button variant="primary" size="md" className="min-w-44" onClick={() => setOpen(true)}>
          Add a calendar
        </Button>
        <p className="max-w-[34ch] text-footnote text-fg-3">Everything stays on this computer. Daybrief talks to your calendar provider directly and has no server of its own.</p>
      </main>
      <AddAccountSheet open={open} onOpenChange={setOpen} hasDemo={false} />
    </div>
  );
}
