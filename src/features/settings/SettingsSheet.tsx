import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ChevronRight } from "lucide-react";
import { api } from "@/api";
import { useNav } from "@/app/nav";
import { ListGroup, Segmented, Sheet, Switch, useTheme, useToasts, type ThemeChoice } from "@/design";

/** Minimal Phase 1 settings. The full Settings screen is Phase 4. */
export function SettingsSheet({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  const qc = useQueryClient();
  const push = useToasts((s) => s.push);
  const theme = useTheme();
  const go = useNav((s) => s.go);
  const autostart = useQuery({ queryKey: ["autostart"], queryFn: () => api.getAutostart(), enabled: open });

  const toggleAutostart = async (enabled: boolean) => {
    try {
      await api.setAutostart(enabled);
      await qc.invalidateQueries({ queryKey: ["autostart"] });
    } catch {
      push({ title: "Couldn't change Start at Login", tone: "danger" });
    }
  };

  return (
    <Sheet open={open} onOpenChange={onOpenChange} title="Settings" description="More options arrive soon.">
      <ListGroup className="mb-4">
        <li>
          <button
            type="button"
            onClick={() => {
              onOpenChange(false);
              go("accounts");
            }}
            className="flex min-h-12 w-full items-center justify-between gap-3 px-4 py-2 text-left transition-colors duration-150 ease-standard hover:bg-fill active:bg-fill-strong"
          >
            <span className="text-headline font-medium text-fg">Accounts &amp; calendars</span>
            <ChevronRight className="size-4 text-fg-3" aria-hidden />
          </button>
        </li>
        <li className="flex min-h-12 items-center justify-between gap-3 px-4 py-2">
          <div>
            <p className="text-headline font-medium text-fg">Start at login</p>
            <p className="text-footnote text-fg-2">Open Daybrief quietly when you sign in.</p>
          </div>
          <Switch label="Start at login" checked={autostart.data ?? false} disabled={autostart.isPending} onCheckedChange={(v) => void toggleAutostart(v)} />
        </li>
        <li className="flex min-h-12 items-center justify-between gap-3 px-4 py-2">
          <p className="text-headline font-medium text-fg">Appearance</p>
          <Segmented
            label="Appearance"
            value={theme.choice}
            onValueChange={(v) => theme.set(v as ThemeChoice)}
            options={[
              { value: "system", label: "Auto" },
              { value: "light", label: "Light" },
              { value: "dark", label: "Dark" },
            ]}
          />
        </li>
      </ListGroup>
    </Sheet>
  );
}
