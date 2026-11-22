import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2, Smartphone } from "lucide-react";
import { useEffect, useState } from "react";
import { api, errorMessage } from "@/api";
import { Button, Dialog, useToasts } from "@/design";
import { timeAgo } from "@/features/accounts/time";
import { Group, SettingRow } from "./controls";

const spaced = (pin: string) => `${pin.slice(0, 3)} ${pin.slice(3)}`;
const clock = (s: number) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;

/**
 * On a computer: "Add a phone". Shows a 6-digit code the phone types; the page follows the pairing until the phone has
 * really connected (or the code runs out), so "Paired" is only said when it is true.
 */
export function AddPhone() {
  const qc = useQueryClient();
  const push = useToasts((s) => s.push);
  const [open, setOpen] = useState(false);
  const start = useMutation({
    mutationFn: () => api.startPairing(),
    onSuccess: () => {
      setOpen(true);
      void qc.invalidateQueries({ queryKey: ["pairing"] });
      void qc.invalidateQueries({ queryKey: ["settings"] });
    },
    onError: (e) => push({ title: errorMessage(e), tone: "danger" }),
  });
  const status = useQuery({ queryKey: ["pairing"], queryFn: () => api.pairingStatus(), enabled: open, refetchInterval: open ? 1000 : false });
  const st = status.data ?? start.data;
  useEffect(() => {
    if (st?.state === "paired") void qc.invalidateQueries({ queryKey: ["settings"] });
  }, [st?.state, qc]);
  const close = () => {
    setOpen(false);
    void api.stopPairing();
  };
  return (
    <>
      <Group title="Your phone" footer="Your phone and this computer then keep tasks, notes and the events you add in step whenever both are on the same Wi-Fi. Calendars aren't copied: each device reads them itself.">
        <SettingRow title="Add a phone" description="Install Firstlight on your iPhone or Android phone first.">
          <Button size="sm" variant="secondary" leading={<Smartphone className="size-4" />} disabled={start.isPending} onClick={() => start.mutate()}>
            {start.isPending ? "Starting…" : "Add phone"}
          </Button>
        </SettingRow>
      </Group>
      <Dialog
        open={open}
        onOpenChange={(o) => !o && close()}
        title={st?.state === "paired" ? "Phone paired" : "Pair your phone"}
        description={st?.state === "waiting" ? "On your phone, open Firstlight, then Settings, Your computers, and type this code." : undefined}
        footer={
          st?.state === "expired" || st?.state === "too_many_tries" ? (
            <>
              <Button onClick={close}>Cancel</Button>
              <Button variant="primary" onClick={() => start.mutate()}>
                New code
              </Button>
            </>
          ) : (
            <Button variant={st?.state === "paired" ? "primary" : "secondary"} onClick={close}>
              {st?.state === "paired" ? "Done" : "Cancel"}
            </Button>
          )
        }
      >
        {st?.state === "waiting" && (
          <div className="flex flex-col items-center gap-2 py-2">
            <p aria-label={`Pairing code ${st.pin.split("").join(" ")}`} className="font-mono text-display font-semibold tracking-[0.2em] text-fg select-text">
              {spaced(st.pin)}
            </p>
            <p className="text-footnote text-fg-2" aria-live="off">
              Works for {clock(st.seconds_left)}. Waiting for your phone…
            </p>
          </div>
        )}
        {st?.state === "paired" && (
          <p role="status" className="flex items-center gap-2 text-headline text-fg">
            <CheckCircle2 className="size-6 shrink-0 text-success" aria-hidden />
            Paired with {st.name}. Changes now go between them while both are on the same Wi-Fi.
          </p>
        )}
        {st?.state === "expired" && (
          <p role="alert" className="text-body text-fg-2">
            That code ran out. Make a new one.
          </p>
        )}
        {st?.state === "too_many_tries" && (
          <p role="alert" className="text-body text-danger">
            Too many wrong codes were typed, so that code stopped working. Make a new one.
          </p>
        )}
      </Dialog>
    </>
  );
}

/** On a phone: type the 6 digits a computer shows. */
export function PairWithComputer({ paired }: { paired: boolean }) {
  const qc = useQueryClient();
  const push = useToasts((s) => s.push);
  const [code, setCode] = useState("");
  const [done, setDone] = useState<string | null>(null);
  const [again, setAgain] = useState(false);
  const lan = useQuery({ queryKey: ["lan-status"], queryFn: () => api.lanSyncStatus(), enabled: paired, refetchInterval: 15_000 });
  const pair = useMutation({
    mutationFn: () => api.pairWithCode(code),
    onSuccess: (r) => {
      qc.setQueryData(["settings"], r.settings);
      void qc.invalidateQueries();
      setDone(r.name);
      setAgain(false);
      setCode("");
      push({ title: `Paired with ${r.name}` });
    },
  });
  const digits = code.replace(/\D/g, "").slice(0, 6);
  const peer = lan.data?.peers[0];

  if (paired && !again) {
    return (
      <Group title="Your computer">
        <li className="flex flex-col gap-3 px-4 py-4">
          <p role="status" className="flex items-start gap-3 text-headline text-fg">
            <CheckCircle2 className="mt-0.5 size-6 shrink-0 text-success" aria-hidden />
            <span>
              {done ? `Paired with ${done}` : "Paired with your computer"}
              <span className="mt-1 block text-body font-normal text-fg-2">
                {peer ? `Last shared with ${peer.name} ${timeAgo(peer.written_at)}.` : "Your changes go across while both are on the same Wi-Fi and Firstlight is open here."}
              </span>
            </span>
          </p>
          <Button className="self-start" onClick={() => setAgain(true)}>
            Pair again
          </Button>
        </li>
      </Group>
    );
  }
  return (
    <Group title="Pair with your computer">
      <li className="flex flex-col gap-4 px-4 py-4">
        <ol className="flex list-decimal flex-col gap-1 pl-5 text-body text-fg-2">
          <li>On your Mac or PC: Firstlight, Settings, Your computers, Add phone.</li>
          <li>Type the 6 digits it shows.</li>
        </ol>
        <label className="flex flex-col gap-1.5">
          <span className="text-footnote font-medium text-fg-2">Pairing code</span>
          <input
            value={digits.length > 3 ? spaced(digits) : digits}
            onChange={(e) => setCode(e.target.value)}
            inputMode="numeric"
            autoComplete="one-time-code"
            placeholder="000 000"
            maxLength={7}
            className="h-14 rounded-control border border-separator bg-surface px-4 text-center font-mono text-title tracking-[0.3em] text-fg outline-none focus:border-accent"
          />
        </label>
        <Button variant="primary" size="md" disabled={digits.length !== 6 || pair.isPending} onClick={() => pair.mutate()}>
          {pair.isPending ? "Looking for your computer…" : "Pair"}
        </Button>
        {pair.isError && (
          <p role="alert" className="rounded-card bg-danger-soft px-3 py-2 text-body text-danger">
            {errorMessage(pair.error)}
          </p>
        )}
        <p className="text-footnote text-fg-2">Pairing gives this phone your computer's sync code, so only your devices can read what is shared. The first time, allow Firstlight to find devices on your local network.</p>
      </li>
    </Group>
  );
}
