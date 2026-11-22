import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Copy, QrCode, ScanLine, Smartphone } from "lucide-react";
import { useState } from "react";
import { api, errorMessage, type PairingCode } from "@/api";
import { Button, Dialog, TextField, useToasts } from "@/design";
import { Group, SettingRow } from "./controls";

/** The QR picture as an image (never inserted as markup): an SVG made by the app itself, shown through a data address. */
const svgSrc = (svg: string) => `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;

/**
 * On a computer: "Add your phone". Shows a QR code the phone scans. It carries the sync code and this computer's
 * address on the home network, so the phone can reach it directly (a phone often can't use network-wide
 * announcements). Turns on sharing on the network here, since that is how the phone will reach it.
 */
export function AddPhone() {
  const qc = useQueryClient();
  const push = useToasts((s) => s.push);
  const [code, setCode] = useState<PairingCode | null>(null);
  const make = useMutation({
    mutationFn: () => api.pairingCode(),
    onSuccess: (c) => {
      setCode(c);
      void qc.invalidateQueries({ queryKey: ["settings"] });
    },
    onError: (e) => push({ title: errorMessage(e), tone: "danger" }),
  });
  return (
    <>
      <Group title="Your phone" footer="Your phone and this computer then share tasks, notes and events you made here, directly on your home or office network. Your calendars aren't copied: each device reads them itself.">
        <SettingRow title="Add your phone" description="Install Firstlight on your iPhone or Android phone, then scan a code shown here.">
          <Button size="sm" variant="secondary" leading={<QrCode className="size-4" />} disabled={make.isPending} onClick={() => make.mutate()}>
            {make.isPending ? "Making…" : "Show code"}
          </Button>
        </SettingRow>
      </Group>
      <Dialog
        open={code !== null}
        onOpenChange={(o) => !o && setCode(null)}
        title="Scan this with your phone"
        description="In Firstlight on your phone: Settings, Your computers, Scan the code."
        footer={
          <>
            <Button
              leading={<Copy className="size-4" />}
              onClick={() =>
                navigator.clipboard
                  .writeText(code?.text ?? "")
                  .then(() => push({ title: "Pairing text copied" }))
                  .catch(() => push({ title: "Couldn't copy", tone: "danger" }))
              }
            >
              Copy as text
            </Button>
            <Button variant="primary" onClick={() => setCode(null)}>
              Done
            </Button>
          </>
        }
      >
        {code && (
          <div className="flex flex-col items-center gap-3">
            <img src={svgSrc(code.svg)} alt="Pairing code for your phone" className="size-60 rounded-control bg-white p-2" />
            <p className="text-footnote text-fg-2">
              {code.addrs.length > 0
                ? `Your phone will find this computer at ${code.addrs.join(" or ")} while both are on the same network.`
                : "This computer isn't on a home or office network right now. The phone can still pair, and finds it once both are on the same network."}
            </p>
            <p className="text-footnote text-fg-2">Keep it private: the code opens what you share.</p>
          </div>
        )}
      </Dialog>
    </>
  );
}

/** On a phone: pair with a computer by scanning its code (or pasting the text, if the camera isn't allowed). */
export function PairWithComputer({ paired }: { paired: string[] }) {
  const qc = useQueryClient();
  const push = useToasts((s) => s.push);
  const [text, setText] = useState("");
  const pair = useMutation({
    mutationFn: (t: string) => api.pairWithComputer(t),
    onSuccess: (r) => {
      qc.setQueryData(["settings"], r.settings);
      void qc.invalidateQueries();
      setText("");
      push({ title: "Paired with your computer" });
    },
  });
  const scan = useMutation({
    mutationFn: () => api.scanQrCode(),
    onSuccess: (t) => t && pair.mutate(t),
  });
  const problem = scan.error ?? pair.error;
  return (
    <Group
      title="Your computer"
      footer="On your Mac or PC: Settings, Your computers, Add your phone. Pairing gives this phone the computer's sync code (replacing another one used here before) and where to find it on your network."
    >
      {paired.length > 0 && <SettingRow title="Paired" description={`Looking for your computer at ${paired.join(" or ")} while you're on the same network.`} />}
      <li className="flex flex-col gap-3 px-4 py-3">
        <Button variant="primary" leading={<ScanLine className="size-4" />} disabled={scan.isPending || pair.isPending} onClick={() => scan.mutate()}>
          {pair.isPending ? "Pairing…" : paired.length > 0 ? "Scan again" : "Scan the code"}
        </Button>
        <TextField label="Or paste the pairing text" value={text} placeholder="firstlight:pair?v=1&…" autoComplete="off" spellCheck={false} onChange={(e) => setText(e.target.value)} />
        <div className="flex justify-end">
          <Button leading={<Smartphone className="size-4" />} disabled={!text.trim() || pair.isPending} onClick={() => pair.mutate(text)}>
            Pair
          </Button>
        </div>
        {problem && (
          <p role="alert" className="rounded-card bg-danger-soft px-3 py-2 text-body text-danger">
            {errorMessage(problem)}
          </p>
        )}
      </li>
    </Group>
  );
}
