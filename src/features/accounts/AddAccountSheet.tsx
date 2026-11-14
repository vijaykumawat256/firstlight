import { useMutation, useQueryClient } from "@tanstack/react-query";
import { CalendarDays, ChevronLeft, ChevronRight, Link2, Mail, Sparkles } from "lucide-react";
import { useState, type ReactNode } from "react";
import { api, errorMessage } from "@/api";
import { Badge, Button, ListGroup, Sheet, TextField } from "@/design";

type Step = "choose" | "ics";

const LINK_RE = /^(https|webcal):\/\/\S{4,}$/i;

function Choice({ icon, title, subtitle, soon, onClick }: { icon: ReactNode; title: string; subtitle: string; soon?: boolean; onClick?: () => void }) {
  return (
    <li>
      <button
        type="button"
        disabled={soon}
        onClick={onClick}
        className="flex min-h-14 w-full items-center gap-3 px-4 py-2.5 text-left transition-colors duration-150 ease-standard enabled:hover:bg-fill enabled:active:bg-fill-strong disabled:opacity-55"
      >
        <span className="flex size-9 shrink-0 items-center justify-center rounded-card bg-accent-soft text-accent [&>svg]:size-[18px]">{icon}</span>
        <span className="min-w-0 flex-1">
          <span className="block truncate text-headline font-medium text-fg">{title}</span>
          <span className="block truncate text-footnote text-fg-2">{subtitle}</span>
        </span>
        {soon ? <Badge>Soon</Badge> : <ChevronRight className="size-4 text-fg-3" aria-hidden />}
      </button>
    </li>
  );
}

export function AddAccountSheet({ open, onOpenChange, hasDemo }: { open: boolean; onOpenChange: (o: boolean) => void; hasDemo: boolean }) {
  const qc = useQueryClient();
  const [step, setStep] = useState<Step>("choose");
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [touched, setTouched] = useState(false);

  const close = (o: boolean) => {
    onOpenChange(o);
    if (!o) {
      // Reset after the closing animation so the content doesn't flash.
      setTimeout(() => {
        setStep("choose");
        setName("");
        setUrl("");
        setTouched(false);
        ics.reset();
        demo.reset();
      }, 250);
    }
  };

  const done = async () => {
    await qc.invalidateQueries();
    close(false);
  };

  const ics = useMutation({ mutationFn: () => api.addIcsAccount(name, url.trim()), onSuccess: done });
  const demo = useMutation({ mutationFn: () => api.addDemoAccount(), onSuccess: done });

  const linkError = touched && !LINK_RE.test(url.trim()) ? "Enter an https:// or webcal:// link." : undefined;

  const submit = () => {
    setTouched(true);
    if (LINK_RE.test(url.trim())) ics.mutate();
  };

  return (
    <Sheet
      open={open}
      onOpenChange={close}
      title={step === "choose" ? "Add a calendar" : "Calendar link"}
      description={step === "choose" ? "Choose how to connect." : "Paste the address of a published calendar."}
      footer={
        step === "ics" ? (
          <>
            <Button variant="secondary" leading={<ChevronLeft className="size-4" />} onClick={() => setStep("choose")} disabled={ics.isPending}>
              Back
            </Button>
            <Button variant="primary" onClick={submit} disabled={ics.isPending}>
              {ics.isPending ? "Connecting…" : "Add calendar"}
            </Button>
          </>
        ) : undefined
      }
    >
      {step === "choose" ? (
        <div className="flex flex-col gap-3 pb-4">
          <ListGroup>
            <Choice icon={<Link2 />} title="Calendar link" subtitle="Any published calendar (ICS or webcal)" onClick={() => setStep("ics")} />
            <Choice icon={<Mail />} title="Google" subtitle="Gmail and Google Workspace" soon />
            <Choice icon={<CalendarDays />} title="Microsoft" subtitle="Outlook.com and Microsoft 365" soon />
            <Choice icon={<CalendarDays />} title="iCloud, Yahoo and others" subtitle="CalDAV with an app password" soon />
          </ListGroup>
          {!hasDemo && (
            <ListGroup>
              <Choice icon={<Sparkles />} title="Try with sample data" subtitle="See how Daybrief looks. Removable any time." onClick={() => demo.mutate()} />
            </ListGroup>
          )}
          {demo.isError && <p role="alert" className="px-1 text-footnote text-danger">{errorMessage(demo.error)}</p>}
        </div>
      ) : (
        <form
          className="flex flex-col gap-4 pb-4"
          onSubmit={(e) => {
            e.preventDefault();
            submit();
          }}
        >
          <TextField label="Name" placeholder="Team holidays" value={name} onChange={(e) => setName(e.target.value)} maxLength={80} autoComplete="off" />
          <TextField
            label="Calendar address"
            placeholder="https://… or webcal://…"
            hint="In Google Calendar: Settings → your calendar → “Secret address in iCal format”. In Outlook: Publish calendar → ICS link."
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            onBlur={() => setTouched(true)}
            error={linkError}
            autoComplete="off"
            autoCapitalize="off"
            spellCheck={false}
            inputMode="url"
          />
          <p className="text-footnote text-fg-2">Private links work like a password. Daybrief keeps yours in the system keychain, never in a file.</p>
          {ics.isError && (
            <p role="alert" className="rounded-card bg-danger-soft px-3 py-2 text-body text-danger">
              {errorMessage(ics.error)}
            </p>
          )}
        </form>
      )}
    </Sheet>
  );
}
