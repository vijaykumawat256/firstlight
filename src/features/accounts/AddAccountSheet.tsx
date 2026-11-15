import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CalendarDays, ChevronLeft, ChevronRight, ExternalLink, Flag, KeyRound, Laptop, Link2, Mail, Sparkles } from "lucide-react";
import { useState, type ReactNode } from "react";
import { api, errorMessage } from "@/api";
import { Badge, Button, ListGroup, Select, Sheet, TextField } from "@/design";
import { HOLIDAY_CALENDARS } from "./holidays";
import { CALDAV_PRESETS, OTHER_SERVER } from "./presets";

type Step = "choose" | "ics" | "caldav" | "oauth" | "system" | "holidays";
type OAuthProvider = "google" | "microsoft";
const OAUTH_LABEL: Record<OAuthProvider, string> = { google: "Google", microsoft: "Microsoft" };

const LINK_RE = /^(https|webcal):\/\/\S{4,}$/i;

function Choice({ icon, title, subtitle, soon, badge, onClick }: { icon: ReactNode; title: string; subtitle: string; soon?: boolean; badge?: string; onClick?: () => void }) {
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
        {soon ? <Badge>{badge ?? "Soon"}</Badge> : <ChevronRight className="size-4 text-fg-3" aria-hidden />}
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
  const [preset, setPreset] = useState<string>(CALDAV_PRESETS[0]!.id);
  const [server, setServer] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [oauthProvider, setOauthProvider] = useState<OAuthProvider>("google");
  const available = useQuery({ queryKey: ["oauth-available"], queryFn: () => api.oauthAvailable(), enabled: open, staleTime: Infinity });

  const close = (o: boolean) => {
    // Closing while we wait for the browser aborts the sign-in so nothing lingers.
    if (!o && step === "oauth" && oauth.isPending) void api.cancelOauth();
    onOpenChange(o);
    if (!o) {
      // Reset after the closing animation so the content doesn't flash.
      setTimeout(() => {
        setStep("choose");
        setName("");
        setUrl("");
        setTouched(false);
        setPreset(CALDAV_PRESETS[0]!.id);
        setServer("");
        setUsername("");
        setPassword("");
        ics.reset();
        dav.reset();
        oauth.reset();
        sys.reset();
        demo.reset();
        holiday.reset();
      }, 250);
    }
  };

  const done = async () => {
    await qc.invalidateQueries();
    close(false);
  };

  const ics = useMutation({ mutationFn: () => api.addIcsAccount(name, url.trim()), onSuccess: done });
  const presetInfo = CALDAV_PRESETS.find((p) => p.id === preset);
  const serverUrl = presetInfo ? presetInfo.server : server.trim();
  const dav = useMutation({ mutationFn: () => api.addCaldavAccount(name, serverUrl, username.trim(), password), onSuccess: done });
  const oauth = useMutation({ mutationFn: (p: OAuthProvider) => api.addOauthAccount(p), onSuccess: done });
  const startOauth = (p: OAuthProvider) => {
    setOauthProvider(p);
    setStep("oauth");
    oauth.mutate(p);
  };
  const cancelOauth = () => {
    void api.cancelOauth();
    oauth.reset();
    setStep("choose");
  };
  const system = useQuery({ queryKey: ["system-status"], queryFn: () => api.systemCalendarStatus(), enabled: open });
  const sys = useMutation({
    mutationFn: () => api.addSystemAccount(),
    onSuccess: done,
    onSettled: () => void system.refetch(),
  });
  const demo = useMutation({ mutationFn: () => api.addDemoAccount(), onSuccess: done });
  const holiday = useMutation({ mutationFn: (h: (typeof HOLIDAY_CALENDARS)[number]) => api.addIcsAccount(`Holidays: ${h.label}`, h.url), onSuccess: done });

  const linkError = touched && !LINK_RE.test(url.trim()) ? "Enter an https:// or webcal:// link." : undefined;

  const submit = () => {
    setTouched(true);
    if (step === "caldav") {
      if (serverOk && username.trim() && password) dav.mutate();
    } else if (LINK_RE.test(url.trim())) ics.mutate();
  };

  const serverOk = /^https:\/\/\S{4,}$/i.test(serverUrl);
  const busy = ics.isPending || dav.isPending;

  return (
    <Sheet
      open={open}
      onOpenChange={close}
      title={step === "choose" ? "Add a calendar" : step === "system" ? (system.data?.name ?? "Calendar app") : step === "holidays" ? "Public holidays" : step === "ics" ? "Calendar link" : step === "oauth" ? `Sign in with ${OAUTH_LABEL[oauthProvider]}` : "iCloud, Yahoo and others"}
      description={step === "choose" ? "Choose how to connect." : step === "system" ? "Use the calendars already on this computer." : step === "holidays" ? "Choose a country. Its holidays appear alongside your events." : step === "ics" ? "Paste the address of a published calendar." : step === "oauth" ? "Firstlight only asks to view your calendar." : "Sign in with an app password."}
      footer={
        step === "system" ? (
          <>
            <Button variant="secondary" leading={<ChevronLeft className="size-4" />} onClick={() => setStep("choose")} disabled={sys.isPending}>
              Back
            </Button>
            <Button variant="primary" onClick={() => sys.mutate()} disabled={sys.isPending}>
              {sys.isPending ? (system.data?.name === "Outlook" ? "Reading Outlook…" : "Waiting for permission…") : system.data?.access === "denied" ? "Check again" : system.data?.name === "Outlook" ? "Connect Outlook" : "Allow access"}
            </Button>
          </>
        ) : step === "oauth" ? (
          oauth.isPending ? (
            <Button variant="secondary" onClick={cancelOauth}>
              Cancel
            </Button>
          ) : (
            <>
              <Button variant="secondary" onClick={() => setStep("choose")}>
                Back
              </Button>
              <Button variant="primary" onClick={() => oauth.mutate(oauthProvider)}>
                Try again
              </Button>
            </>
          )
        ) : step === "holidays" ? (
          <Button variant="secondary" leading={<ChevronLeft className="size-4" />} onClick={() => setStep("choose")} disabled={holiday.isPending}>
            Back
          </Button>
        ) : step !== "choose" ? (
          <>
            <Button variant="secondary" leading={<ChevronLeft className="size-4" />} onClick={() => setStep("choose")} disabled={busy}>
              Back
            </Button>
            <Button variant="primary" onClick={submit} disabled={busy}>
              {busy ? "Connecting…" : "Add calendar"}
            </Button>
          </>
        ) : undefined
      }
    >
      {step === "choose" ? (
        <div className="flex flex-col gap-3 pb-4">
          {system.data?.supported && (
            <ListGroup>
              <Choice
                icon={<Laptop />}
                title={system.data.name === "Outlook" ? "Outlook on this computer" : "Calendar app on this computer"}
                subtitle={system.data.name === "Outlook" ? "Every account already in Outlook: Microsoft 365, Exchange, Gmail, IMAP…" : "Everything already in Calendar: iCloud, Gmail, Outlook, Yahoo…"}
                onClick={() => setStep("system")}
              />
            </ListGroup>
          )}
          <ListGroup>
            <Choice icon={<Link2 />} title="Calendar link" subtitle="Any published calendar (ICS or webcal)" onClick={() => setStep("ics")} />
            <Choice icon={<Flag />} title="Public holidays" subtitle="Pick a country; nothing to sign in to" onClick={() => setStep("holidays")} />
            <Choice
              icon={<Mail />}
              title="Google"
              subtitle="Gmail and Google Workspace"
              soon={!available.data?.google}
              badge="Setup needed"
              onClick={() => startOauth("google")}
            />
            <Choice
              icon={<CalendarDays />}
              title="Microsoft"
              subtitle="Outlook.com and Microsoft 365"
              soon={!available.data?.microsoft}
              badge="Setup needed"
              onClick={() => startOauth("microsoft")}
            />
            <Choice icon={<KeyRound />} title="iCloud, Yahoo and others" subtitle="CalDAV with an app password" onClick={() => setStep("caldav")} />
          </ListGroup>
          {!hasDemo && (
            <ListGroup>
              <Choice icon={<Sparkles />} title="Try with sample data" subtitle="See how Firstlight looks. Removable any time." onClick={() => demo.mutate()} />
            </ListGroup>
          )}
          {demo.isError && <p role="alert" className="px-1 text-footnote text-danger">{errorMessage(demo.error)}</p>}
        </div>
      ) : step === "holidays" ? (
        <div className="flex flex-col gap-3 pb-4">
          {holiday.isError && (
            <p role="alert" className="rounded-card bg-danger-soft px-3 py-2 text-body text-danger">
              {errorMessage(holiday.error)}
            </p>
          )}
          <ListGroup>
            {HOLIDAY_CALENDARS.map((h) => (
              <li key={h.id}>
                <button
                  type="button"
                  disabled={holiday.isPending}
                  onClick={() => holiday.mutate(h)}
                  className="flex min-h-12 w-full items-center justify-between gap-3 px-4 py-2.5 text-left text-headline text-fg transition-colors duration-150 ease-standard enabled:hover:bg-fill enabled:active:bg-fill-strong disabled:opacity-55"
                >
                  {h.label}
                  {holiday.isPending && holiday.variables?.id === h.id ? <span className="text-footnote text-fg-2">Adding…</span> : <ChevronRight className="size-4 text-fg-3" aria-hidden />}
                </button>
              </li>
            ))}
          </ListGroup>
          <p className="px-1 text-footnote text-fg-2">These are public calendars published by Google. Birthdays from your Contacts already appear if you connect the Calendar app on this computer.</p>
        </div>
      ) : step === "system" ? (
        <div className="flex flex-col gap-4 pb-4">
          {sys.isError && (
            <p role="alert" className="rounded-card bg-danger-soft px-3 py-2 text-body text-danger">
              {errorMessage(sys.error)}
            </p>
          )}
          {sys.isPending && system.data?.name === "Outlook" && (
            <div role="status" className="flex items-center gap-3 rounded-card bg-fill px-3 py-2.5">
              <div className="size-5 shrink-0 animate-spin rounded-full border-2 border-fill-strong border-t-accent" aria-hidden />
              <p className="text-body text-fg">Reading your calendars from Outlook. This can take up to a minute the first time.</p>
            </div>
          )}
          {system.data?.name === "Outlook" ? (
            <>
              <p className="text-body text-fg">
                Firstlight will read the calendars in Outlook on this computer, so every account you've added there shows up with no extra sign-in.
              </p>
              <ul className="flex flex-col gap-1.5 text-body text-fg-2">
                <li>• This works with the classic Outlook. Outlook may open in the background the first time.</li>
                <li>• Firstlight only reads. It never changes or deletes events.</li>
                <li>• The new Outlook can't be read this way. Connect that account with Sign in with Microsoft instead.</li>
              </ul>
            </>
          ) : (
            <>
              <p className="text-body text-fg">
                Firstlight will read the calendars in your Calendar app, so any account you've added to this computer shows up here with no extra sign-in.
              </p>
              <ul className="flex flex-col gap-1.5 text-body text-fg-2">
                <li>• macOS will ask you to allow access. Choose <span className="font-medium text-fg">Allow Full Access</span>.</li>
                <li>• Firstlight only reads unless you turn on adding events in Settings.</li>
                <li>• Birthdays and subscribed calendars start hidden; you can turn them on.</li>
              </ul>
            </>
          )}
          {system.data?.name !== "Outlook" && (sys.isError || system.data?.access === "denied") && (
            <button type="button" onClick={() => void api.openPrivacySettings("calendars")} className="flex items-center gap-1.5 self-start text-body text-accent hover:underline">
              Open System Settings <ExternalLink className="size-3.5" aria-hidden />
            </button>
          )}
        </div>
      ) : step === "oauth" ? (
        <div className="flex flex-col items-center gap-4 px-2 pb-8 pt-4 text-center" aria-live="polite">
          {oauth.isError ? (
            <p role="alert" className="w-full rounded-card bg-danger-soft px-3 py-2 text-left text-body text-danger">
              {errorMessage(oauth.error)}
            </p>
          ) : (
            <>
              <div className="size-10 animate-spin rounded-full border-[3px] border-fill-strong border-t-accent" aria-hidden />
              <div>
                <p className="text-headline font-semibold text-fg">Finish signing in in your browser</p>
                <p className="mt-1 text-body text-fg-2">Choose your {OAUTH_LABEL[oauthProvider]} account and allow calendar access. Then come back here.</p>
              </div>
            </>
          )}
          <p className="max-w-[34ch] text-footnote text-fg-3">Firstlight never sees your password, only a revocable permission to read your calendar.</p>
        </div>
      ) : step === "caldav" ? (
        <form
          className="flex flex-col gap-4 pb-4"
          onSubmit={(e) => {
            e.preventDefault();
            submit();
          }}
        >
          {dav.isError && (
            <p role="alert" className="rounded-card bg-danger-soft px-3 py-2 text-body text-danger">
              {errorMessage(dav.error)}
            </p>
          )}
          <div className="flex flex-col gap-1.5">
            <span className="text-footnote font-medium text-fg-2">Provider</span>
            <Select
              label="Provider"
              value={preset}
              onValueChange={setPreset}
              options={[...CALDAV_PRESETS.map((p) => ({ value: p.id, label: p.label })), { value: OTHER_SERVER, label: "Other server…" }]}
            />
          </div>
          {!presetInfo && (
            <TextField
              label="Server address"
              placeholder="https://caldav.example.com"
              value={server}
              onChange={(e) => setServer(e.target.value)}
              error={touched && !serverOk ? "Enter the full https:// address." : undefined}
              autoComplete="off"
              spellCheck={false}
            />
          )}
          <TextField label="Name" placeholder={presetInfo?.label ?? "My calendar"} value={name} onChange={(e) => setName(e.target.value)} maxLength={80} autoComplete="off" />
          <TextField
            label="Username"
            placeholder={presetInfo?.usernameHint ?? "Username or email"}
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            error={touched && !username.trim() ? "Enter your username." : undefined}
            autoComplete="off"
            autoCapitalize="off"
            spellCheck={false}
          />
          <TextField
            label="App password"
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            error={touched && !password ? "Enter the app password." : undefined}
            hint="Not your normal password. It only works for calendars and can be revoked any time."
            autoComplete="off"
          />
          {presetInfo && (
            <button
              type="button"
              onClick={() => void api.openUrl(presetInfo.helpUrl)}
              className="flex items-center gap-1.5 self-start text-body text-accent hover:underline"
            >
              {presetInfo.helpLabel} <ExternalLink className="size-3.5" aria-hidden />
            </button>
          )}
          <p className="text-footnote text-fg-2">Firstlight keeps your password in the system keychain, never in a file.</p>
        </form>
      ) : (
        <form
          className="flex flex-col gap-4 pb-4"
          onSubmit={(e) => {
            e.preventDefault();
            submit();
          }}
        >
          {ics.isError && (
            <p role="alert" className="rounded-card bg-danger-soft px-3 py-2 text-body text-danger">
              {errorMessage(ics.error)}
            </p>
          )}
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
          <p className="text-footnote text-fg-2">Private links work like a password. Firstlight keeps yours in the system keychain, never in a file.</p>
        </form>
      )}
    </Sheet>
  );
}
