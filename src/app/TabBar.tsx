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
    <nav aria-label="Main" className="shrink-0 border-t border-separator bg-surface">
      <ul className="mx-auto flex max-w-2xl">
        {TABS.map((t) => {
          const current = t.view === view || (t.also ?? []).includes(view);
          const Icon = t.icon;
          return (
            <li key={t.view} className="flex-1">
              <button
                type="button"
                aria-current={current ? "page" : undefined}
                onClick={() => go(t.view)}
                className="flex min-h-14 w-full flex-col items-center justify-center gap-0.5 text-caption font-medium text-fg-2 aria-[current=page]:text-accent"
              >
                <Icon className="size-6" strokeWidth={current ? 2.2 : 1.8} aria-hidden />
                {t.label}
              </button>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}
