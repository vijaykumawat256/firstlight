import { CalendarDays, ListChecks, Settings as SettingsIcon, Sun, type LucideIcon } from "lucide-react";
import { useNav, type View } from "@/app/nav";

const TABS: { view: View; label: string; icon: LucideIcon; also?: View[] }[] = [
  { view: "briefing", label: "Today", icon: Sun },
  { view: "calendar", label: "Calendar", icon: CalendarDays },
  { view: "tasks", label: "Tasks", icon: ListChecks },
  { view: "settings", label: "Settings", icon: SettingsIcon, also: ["accounts", "about"] },
];

/**
 * Phones only: the four main places at the bottom, within reach of a thumb, as phones' own apps do. On a computer the
 * briefing's header buttons do this job.
 */
export function TabBar() {
  const view = useNav((s) => s.view);
  const go = useNav((s) => s.go);
  return (
    <nav aria-label="Main" className="shrink-0 border-t border-separator bg-surface/95 backdrop-blur-md">
      <ul className="mx-auto flex max-w-2xl px-1 py-1">
        {TABS.map((t) => {
          const current = t.view === view || (t.also ?? []).includes(view);
          const Icon = t.icon;
          return (
            <li key={t.view} className="flex-1">
              <button
                type="button"
                aria-current={current ? "page" : undefined}
                onClick={() => go(t.view)}
                className="flex min-h-13 w-full flex-col items-center justify-center gap-0.5 rounded-card text-caption font-medium text-fg-2 transition-colors duration-150 ease-standard active:scale-95 aria-[current=page]:font-semibold aria-[current=page]:text-accent"
              >
                <span className={`flex h-7 w-12 items-center justify-center rounded-full transition-colors duration-150 ${current ? "bg-accent-soft" : ""}`}>
                  <Icon className="size-[22px]" strokeWidth={current ? 2.25 : 1.75} aria-hidden />
                </span>
                {t.label}
              </button>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}
