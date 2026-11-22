import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, Rocket } from "lucide-react";
import { useState } from "react";
import { api } from "@/api";
import { useNav } from "@/app/nav";
import { Button, Card, Segmented, Switch, useToasts } from "@/design";
import { useSettings } from "@/features/settings/useSettings";
import { isPhone } from "@/app/platform";

const KEY = "fl-setup";
type Steps = { fresh?: boolean; when?: boolean; hours?: boolean; hidden?: boolean };

function read(): Steps {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? "{}") as Steps;
  } catch {
    return {};
  }
}
function write(s: Steps) {
  try {
    localStorage.setItem(KEY, JSON.stringify(s));
  } catch {
    // Private window or storage blocked: the card simply shows again next time.
  }
}

function Tick({ on }: { on: boolean }) {
  return (
    <span aria-hidden className={`mt-0.5 flex size-5 shrink-0 items-center justify-center rounded-full ${on ? "bg-accent-fill text-accent-fg" : "border border-separator"}`}>
      {on && <Check className="size-3.5" strokeWidth={3} />}
    </span>
  );
}

/** Called by the first-run screen: only a new installation gets the set-up card (people already using it are set up). */
export function markFreshInstall() {
  const s = read();
  if (!s.fresh) write({ ...s, fresh: true });
}

const hour = (h: number) => new Date(2000, 0, 1, h).toLocaleTimeString(undefined, { hour: "numeric" });

/**
 * The three things that make Firstlight work from day one, on the briefing after the first calendar is connected:
 * open at sign-in (or it can't appear when you unlock), when it appears, and working hours (free time, lunch and
 * the end-of-day note depend on them). Gone once all three are done, or when hidden. Remembered on this device.
 */
export function GettingStarted() {
  const [steps, setSteps] = useState<Steps>(read);
  const { settings: s, update } = useSettings();
  const qc = useQueryClient();
  const push = useToasts((t) => t.push);
  const go = useNav((n) => n.go);
  const setSection = useNav((n) => n.setSettingsSection);
  const autostart = useQuery({ queryKey: ["autostart"], queryFn: () => api.getAutostart() });
  const toggle = useMutation({
    mutationFn: (on: boolean) => api.setAutostart(on),
    onSettled: () => void qc.invalidateQueries({ queryKey: ["autostart"] }),
    onError: () => push({ title: "Couldn't change Start at Login", tone: "danger" }),
  });
  const mark = (patch: Steps) => {
    const next = { ...steps, ...patch };
    setSteps(next);
    write(next);
  };
  // Its three steps (open at sign-in, on unlock, working hours) are about a computer.
  if (isPhone || !steps.fresh || steps.hidden || !s || autostart.isPending) return null;
  const loginOn = autostart.data === true;
  const done = [loginOn, !!steps.when, !!steps.hours];
  if (done.every(Boolean)) return null;

  return (
    <section aria-label="Get set up">
      <Card className="flex flex-col gap-4">
        <div className="flex items-start gap-3">
          <Rocket className="mt-0.5 size-5 shrink-0 text-accent" aria-hidden />
          <div className="min-w-0 flex-1">
            <h2 className="text-headline font-semibold text-fg">Get set up</h2>
            <p className="text-body text-fg-2">
              {done.filter(Boolean).length} of 3 done. Then Firstlight shows your day by itself each morning.
            </p>
          </div>
          <Button size="sm" variant="plain" onClick={() => mark({ hidden: true })}>
            Hide
          </Button>
        </div>

        <ol className="flex flex-col gap-4">
          <li className="flex items-start gap-3">
            <Tick on={loginOn} />
            <div className="min-w-0 flex-1">
              <p className="text-body font-medium text-fg">Open when you sign in</p>
              <p className="text-footnote text-fg-2">So it's there when you unlock your computer, without opening it yourself.</p>
            </div>
            <Switch label="Open Firstlight when you sign in" checked={loginOn} disabled={toggle.isPending} onCheckedChange={(v) => toggle.mutate(v)} />
          </li>

          <li className="flex items-start gap-3">
            <Tick on={!!steps.when} />
            <div className="flex min-w-0 flex-1 flex-col gap-2">
              <div>
                <p className="text-body font-medium text-fg">When should your day appear?</p>
                <p className="text-footnote text-fg-2">Later unlocks only show a small note when a meeting is about to start.</p>
              </div>
              <Segmented
                label="When your day appears"
                value={steps.when ? (s.unlock.open_every_time ? "every" : "first") : ""}
                onValueChange={(v) => {
                  if (!v) return;
                  update((d) => void (d.unlock.open_every_time = v === "every"));
                  mark({ when: true });
                }}
                options={[
                  { value: "first", label: "First unlock each day" },
                  { value: "every", label: "Every unlock" },
                ]}
              />
            </div>
          </li>

          <li className="flex items-start gap-3">
            <Tick on={!!steps.hours} />
            <div className="min-w-0 flex-1">
              <p className="text-body font-medium text-fg">
                Working hours: {hour(s.briefing.work_start_hour)} to {hour(s.briefing.work_end_hour)}
              </p>
              <p className="text-footnote text-fg-2">Free time, lunch and your end-of-day note use them.</p>
            </div>
            <div className="flex shrink-0 gap-1">
              <Button size="sm" variant="plain" onClick={() => mark({ hours: true })}>
                Looks right
              </Button>
              <Button
                size="sm"
                variant="secondary"
                onClick={() => {
                  mark({ hours: true });
                  go("settings");
                  setSection("briefing");
                }}
              >
                Change
              </Button>
            </div>
          </li>
        </ol>
      </Card>
    </section>
  );
}
