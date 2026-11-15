import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { X } from "lucide-react";
import { useState } from "react";
import { api, errorMessage, type AiProvider, type AppSettings, type ImapPreset } from "@/api";
import { Button, Dialog, Segmented, Select, Switch, TextField, useToasts } from "@/design";
import { AI_PROVIDERS, aiProvider, hostOf } from "./aiProviders";
import { Group, NumberSelect, SettingRow } from "./controls";
import { useSettings } from "./useSettings";

/** The settings document comes back from the backend after an action; put it straight into the cache. */
function useSettingsCache() {
  const qc = useQueryClient();
  return (saved: AppSettings) => {
    qc.setQueryData(["settings"], saved);
    void qc.invalidateQueries({ queryKey: ["mail-digest"] });
    void qc.invalidateQueries({ queryKey: ["ai-summary"] });
  };
}

// ---------------------------------------------------------------------------------------- Email

export function EmailGroup() {
  const { settings: s, update } = useSettings();
  const push = useToasts((t) => t.push);
  const put = useSettingsCache();
  const [sender, setSender] = useState("");
  const [addMailbox, setAddMailbox] = useState(false);

  const accounts = useQuery({ queryKey: ["accounts"], queryFn: () => api.listAccounts() });
  const system = useQuery({ queryKey: ["system-status"], queryFn: () => api.systemCalendarStatus() });
  const mailAccounts = (accounts.data ?? []).filter((a) => a.provider === "google" || a.provider === "microsoft");

  const apple = useMutation({ mutationFn: () => api.enableAppleMail(), onSuccess: put, onError: (e) => push({ title: "Couldn't connect Mail", description: errorMessage(e), tone: "danger" }) });
  const account = useMutation({
    mutationFn: (id: string) => api.enableAccountMail(id),
    onSuccess: put,
    onError: (e) => push({ title: "Couldn't allow mail access", description: errorMessage(e), tone: "danger" }),
  });

  const removeMailbox = useMutation({
    mutationFn: (id: string) => api.removeImapMailbox(id),
    onSuccess: put,
    onError: (err) => push({ title: "Couldn't remove the mailbox", description: errorMessage(err), tone: "danger" }),
  });

  if (!s) return null;
  const e = s.email;
  const nothingChosen = e.enabled && !e.apple_mail && e.account_ids.length === 0 && e.imap.length === 0;
  const addSender = () => {
    const v = sender.trim();
    if (!v) return;
    update((d) => void (d.email.important_senders = [...d.email.important_senders, v]));
    setSender("");
  };

  return (
    <Group title="Email" footer="Off by default. Firstlight reads only each message's sender, subject and a short preview. It never stores whole messages, never loads images or links, and never changes, moves or sends mail.">
      <SettingRow title="Show email in the briefing" description="Unread count, invitations and mail from people who matter.">
        <Switch label="Show email in the briefing" checked={e.enabled} onCheckedChange={(v) => update((d) => void (d.email.enabled = v))} />
      </SettingRow>

      {e.enabled && (
        <>
          {nothingChosen && <SettingRow title="Choose where to read email from" description="Connect the Mail app or one of your accounts below." />}

          {system.data?.supported && (
            <SettingRow title="Mail app" description={e.apple_mail ? "Reading unread mail from the Mail app on this Mac." : "macOS will ask you to allow Firstlight to control Mail."}>
              {e.apple_mail ? (
                <Switch label="Read the Mail app" checked onCheckedChange={() => update((d) => void (d.email.apple_mail = false))} />
              ) : (
                <Button size="sm" variant="secondary" disabled={apple.isPending} onClick={() => apple.mutate()}>
                  {apple.isPending ? "Waiting…" : "Connect"}
                </Button>
              )}
            </SettingRow>
          )}

          {mailAccounts.map((a) => {
            const on = e.account_ids.includes(a.id);
            return (
              <SettingRow key={a.id} title={a.display_name} description={on ? `Reading unread mail from ${a.provider === "google" ? "Gmail" : "Outlook"}.` : "Needs extra permission to read mail. You'll be asked in your browser."}>
                {on ? (
                  <Switch label={`Read mail for ${a.display_name}`} checked onCheckedChange={() => update((d) => void (d.email.account_ids = d.email.account_ids.filter((id) => id !== a.id)))} />
                ) : (
                  <Button size="sm" variant="secondary" disabled={account.isPending} onClick={() => account.mutate(a.id)}>
                    {account.isPending && account.variables === a.id ? "Waiting…" : "Allow mail access"}
                  </Button>
                )}
              </SettingRow>
            );
          })}

          {e.imap.map((m) => (
            <SettingRow key={m.id} title={m.preset === "yahoo" ? "Yahoo Mail" : m.preset === "icloud" ? "iCloud Mail" : "Email"} description={`${m.address}${m.host ? ` (${m.host})` : ""} · reading unread mail.`}>
              <Button size="sm" variant="danger" disabled={removeMailbox.isPending} aria-label={`Remove ${m.address}`} onClick={() => removeMailbox.mutate(m.id)}>
                Remove
              </Button>
            </SettingRow>
          ))}
          <SettingRow title="Yahoo or iCloud mail" description="Sign in with an app-specific password. Firstlight only looks; it never changes your mail.">
            <Button size="sm" variant="secondary" disabled={e.imap.length >= 4} onClick={() => setAddMailbox(true)}>
              Add mailbox…
            </Button>
          </SettingRow>

          <SettingRow title="Important senders" description="Their mail is always shown. Use an address, @company.com, or a name. People who organise today's meetings count too." stack>
            <div className="flex w-full flex-col gap-2">
              {e.important_senders.length > 0 && (
                <ul className="flex flex-wrap gap-1.5" aria-label="Important senders">
                  {e.important_senders.map((p) => (
                    <li key={p} className="flex items-center gap-1 rounded-full bg-fill py-1 pl-3 pr-1 text-body text-fg">
                      {p}
                      <button type="button" aria-label={`Remove ${p}`} onClick={() => update((d) => void (d.email.important_senders = d.email.important_senders.filter((x) => x !== p)))} className="flex size-5 items-center justify-center rounded-full text-fg-2 hover:bg-fill-strong">
                        <X className="size-3" />
                      </button>
                    </li>
                  ))}
                </ul>
              )}
              <form
                className="flex items-end gap-2"
                onSubmit={(ev) => {
                  ev.preventDefault();
                  addSender();
                }}
              >
                <div className="min-w-0 flex-1">
                  <TextField label="Add a sender" placeholder="boss@company.com" value={sender} onChange={(ev) => setSender(ev.target.value)} maxLength={254} autoComplete="off" spellCheck={false} />
                </div>
                <Button size="md" variant="secondary" type="submit" disabled={!sender.trim()}>
                  Add
                </Button>
              </form>
            </div>
          </SettingRow>

          <SettingRow title="Look back">
            <NumberSelect label="Days of email" unit="days" options={[1, 3, 7, 14]} value={e.days_back} onChange={(n) => update((d) => void (d.email.days_back = n))} />
          </SettingRow>
        </>
      )}
      <ImapDialog open={addMailbox} onOpenChange={setAddMailbox} onDone={put} />
    </Group>
  );
}

const IMAP_HELP: Record<ImapPreset, string> = {
  yahoo: "In Yahoo: Account info, then Account security, then Generate app password.",
  icloud: "At account.apple.com: Sign-In and Security, then App-Specific Passwords.",
  custom: "Use an app password if your provider offers one, and make sure IMAP is switched on for the account.",
};

const SENT_TO: Record<ImapPreset, string> = { yahoo: "Yahoo", icloud: "Apple", custom: "your mail server" };

function ImapDialog({ open, onOpenChange, onDone }: { open: boolean; onOpenChange: (o: boolean) => void; onDone: (s: AppSettings) => void }) {
  const [preset, setPreset] = useState<ImapPreset>("yahoo");
  const [address, setAddress] = useState("");
  const [password, setPassword] = useState("");
  const [host, setHost] = useState("");
  const add = useMutation({
    mutationFn: () => api.addImapMailbox(preset, address.trim(), password, preset === "custom" ? host.trim() : undefined),
    onSuccess: (saved) => {
      onDone(saved);
      setPassword("");
      setAddress("");
      setHost("");
      onOpenChange(false);
    },
  });
  const ready = address.includes("@") && password.trim().length > 0 && (preset !== "custom" || host.trim().includes("."));
  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        onOpenChange(o);
        if (!o) {
          add.reset();
          setPassword("");
        }
      }}
      title="Add a mailbox"
      description="Yahoo and iCloud need an app-specific password, not your normal one. Other providers work too."
      footer={
        <>
          <Button onClick={() => onOpenChange(false)}>Cancel</Button>
          <Button variant="primary" disabled={add.isPending || !ready} onClick={() => add.mutate()}>
            {add.isPending ? "Checking…" : "Connect"}
          </Button>
        </>
      }
    >
      <form
        className="flex flex-col gap-4 pb-2"
        onSubmit={(ev) => {
          ev.preventDefault();
          if (ready && !add.isPending) add.mutate();
        }}
      >
        <Segmented
          label="Mail provider"
          value={preset}
          onValueChange={(v) => setPreset(v as ImapPreset)}
          options={[
            { value: "yahoo", label: "Yahoo Mail" },
            { value: "icloud", label: "iCloud Mail" },
            { value: "custom", label: "Other" },
          ]}
        />
        {add.isError && (
          <p role="alert" className="rounded-card bg-danger-soft px-3 py-2 text-body text-danger">
            {errorMessage(add.error)}
          </p>
        )}
        <TextField label="Email address" type="email" value={address} onChange={(ev) => setAddress(ev.target.value)} placeholder={preset === "yahoo" ? "you@yahoo.com" : preset === "icloud" ? "you@icloud.com" : "you@example.com"} autoComplete="off" spellCheck={false} maxLength={254} />
        {preset === "custom" && (
          <TextField label="Mail server" value={host} onChange={(ev) => setHost(ev.target.value)} placeholder="imap.example.com" hint="Your provider's IMAP server. It must support encrypted IMAP on port 993." autoComplete="off" spellCheck={false} maxLength={253} />
        )}
        <TextField label="App-specific password" type="password" value={password} onChange={(ev) => setPassword(ev.target.value)} hint={`${IMAP_HELP[preset]} Stored in the system keychain and sent only to ${SENT_TO[preset]}.`} autoComplete="off" spellCheck={false} />
        <button type="submit" hidden />
      </form>
    </Dialog>
  );
}

// ------------------------------------------------------------------------------------------- AI

export function AiGroup() {
  const { settings: s, update } = useSettings();
  const push = useToasts((t) => t.push);
  const put = useSettingsCache();
  const [consent, setConsent] = useState(false);
  const [preview, setPreview] = useState(false);
  const status = useQuery({ queryKey: ["ai-status"], queryFn: () => api.aiStatus(), staleTime: 0 });
  const refresh = () => void status.refetch();

  const off = useMutation({ mutationFn: () => api.disableAi(), onSuccess: (v) => (put(v), refresh()) });
  const forget = useMutation({ mutationFn: () => api.forgetAiKey(), onSuccess: (v) => (put(v), refresh(), push({ title: "Key removed" })) });

  if (!s) return null;
  const ai = s.ai;
  const label = aiProvider(ai.provider).label;

  return (
    <Group title="AI summaries" footer="Off by default. When on, a short summary of your day is written by an AI service you choose, using only the information you are shown before you agree. If it ever fails, the normal summary is used.">
      <SettingRow title="Smarter day summary" description="Written by AI instead of Firstlight's built-in summary.">
        <Switch
          label="AI summaries"
          checked={ai.enabled}
          onCheckedChange={(on) => (on ? setConsent(true) : off.mutate())}
        />
      </SettingRow>

      {ai.enabled && (
        <>
          <SettingRow title="Provided by" description={`${label} · ${ai.model}`}>
            <Button size="sm" variant="plain" onClick={() => setPreview(true)}>
              See what's sent
            </Button>
          </SettingRow>
          {s.email.enabled && (
            <SettingRow title="Include important email" description="Sender names and subjects only. Never previews or message text.">
              <Switch label="Include important email in AI summaries" checked={ai.include_email} onCheckedChange={(v) => update((d) => void (d.ai.include_email = v))} />
            </SettingRow>
          )}
          {status.data?.has_key && (
            <SettingRow title="API key" description={`Saved in the system keychain, for ${label}.`}>
              <Button size="sm" variant="danger" disabled={forget.isPending} onClick={() => forget.mutate()}>
                Remove key and turn off
              </Button>
            </SettingRow>
          )}
        </>
      )}

      <ConsentDialog open={consent} onOpenChange={setConsent} onDone={(v) => (put(v), refresh())} />
      <PreviewDialog open={preview} onOpenChange={setPreview} />
    </Group>
  );
}

function PreviewBox({ enabled }: { enabled: boolean }) {
  const preview = useQuery({ queryKey: ["ai-preview"], queryFn: () => api.aiPreview(), enabled, staleTime: 0, gcTime: 0 });
  return (
    <pre aria-label="Exactly what will be sent" className="max-h-56 select-text overflow-auto whitespace-pre-wrap break-words rounded-card bg-fill p-3 font-mono text-footnote text-fg">
      {preview.isPending ? "Loading…" : preview.isError ? "Couldn't prepare the preview." : preview.data}
    </pre>
  );
}

function PreviewDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange} title="What is sent" description="This is the complete text sent each time the summary refreshes." footer={<Button onClick={() => onOpenChange(false)}>Close</Button>}>
      <PreviewBox enabled={open} />
    </Dialog>
  );
}

function ConsentDialog({ open, onOpenChange, onDone }: { open: boolean; onOpenChange: (o: boolean) => void; onDone: (s: AppSettings) => void }) {
  // Mounted only while open, so each opening starts from what is saved.
  return open ? <ConsentBody onOpenChange={onOpenChange} onDone={onDone} /> : null;
}

function ConsentBody({ onOpenChange, onDone }: { onOpenChange: (o: boolean) => void; onDone: (s: AppSettings) => void }) {
  const qc = useQueryClient();
  const push = useToasts((t) => t.push);
  const { settings: s, update } = useSettings();
  const [provider, setProvider] = useState<AiProvider>(() => qc.getQueryData<AppSettings>(["settings"])?.ai.provider ?? "anthropic");
  const [key, setKey] = useState("");
  // Typed values stay drafts until "Turn on": saving half-typed addresses would have them "corrected" mid-keystroke.
  const [endpoint, setEndpoint] = useState<string | null>(null);
  const [customEndpoint, setCustomEndpoint] = useState<string | null>(null);
  const [model, setModel] = useState<string | null>(null);
  const saved = useQuery({ queryKey: ["ai-keys"], queryFn: () => api.aiKeys(), staleTime: 0 });
  const info = aiProvider(provider);
  const hasKey = (saved.data ?? []).includes(provider);

  const enable = useMutation({
    mutationFn: async () => {
      const current = qc.getQueryData<AppSettings>(["settings"]);
      if (current) {
        const next = structuredClone(current);
        next.ai.provider = provider;
        if (endpoint !== null) next.ai.endpoint = endpoint;
        if (customEndpoint !== null) next.ai.custom_endpoint = customEndpoint.trim();
        // Each service has its own model names: start from this one's default unless the user typed one.
        if (model !== null) next.ai.model = model;
        else if (current.ai.provider !== provider) next.ai.model = info.defaultModel;
        // Saved first so the test request uses what the user typed.
        qc.setQueryData(["settings"], await api.setSettings(next));
      }
      return api.enableAi(provider, info.keyLabel ? key.trim() || null : null, true);
    },
    onSuccess: (done) => {
      onDone(done);
      void qc.invalidateQueries({ queryKey: ["ai-keys"] });
      onOpenChange(false);
    },
  });
  const orSignIn = useMutation({
    mutationFn: () => api.signInOpenrouter(),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["ai-keys"] });
      push({ title: "Signed in to OpenRouter", description: "Now check what is sent and turn AI on." });
    },
  });

  if (!s) return null;
  const shownEndpoint = endpoint ?? s.ai.endpoint;
  const shownCustom = customEndpoint ?? s.ai.custom_endpoint;
  const shownModel = model ?? (s.ai.provider === provider ? s.ai.model : info.defaultModel);
  const destinationHost = provider === "custom" ? hostOf(shownCustom) : info.host;
  const where =
    provider === "local"
      ? `the model at ${shownEndpoint}, which runs on this computer`
      : destinationHost
        ? `${info.company}'s servers (${destinationHost})`
        : "the service whose address you enter below";
  const customOk = provider !== "custom" || !!hostOf(shownCustom);
  const needsKey = !!info.keyLabel && !hasKey && key.trim().length < 10;
  const close = (o: boolean) => {
    if (!o && orSignIn.isPending) void api.cancelOauth();
    onOpenChange(o);
  };

  return (
    <Dialog
      open
      onOpenChange={close}
      title="Turn on AI summaries?"
      description="Read exactly what will be shared, then decide."
      footer={
        <>
          <Button onClick={() => close(false)}>Cancel</Button>
          <Button variant="primary" disabled={enable.isPending || orSignIn.isPending || needsKey || !customOk} onClick={() => enable.mutate()}>
            {enable.isPending ? "Checking…" : "Turn on"}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4 pb-2">
        <div className="flex items-center justify-between gap-3">
          <span className="text-headline text-fg">Provider</span>
          <Select
            label="Provider"
            value={provider}
            onValueChange={(v) => {
              setProvider(v as AiProvider);
              setKey("");
              setModel(null);
              enable.reset();
              orSignIn.reset();
            }}
            options={AI_PROVIDERS.map((p) => ({ value: p.id, label: p.label }))}
          />
        </div>

        {/* Where the data goes comes first, so it is the first thing read before agreeing. */}
        <p className="text-body text-fg">
          Your calendar summary will be sent to <span className="font-semibold">{where}</span> about every five minutes while the briefing is open. Meeting links, descriptions, attendees and message text are never included.
        </p>

        {(enable.isError || orSignIn.isError) && (
          <p role="alert" className="rounded-card bg-danger-soft px-3 py-2 text-body text-danger">
            {errorMessage(enable.error ?? orSignIn.error)}
          </p>
        )}

        {provider === "custom" && (
          <TextField
            label="Service address"
            value={shownCustom}
            onChange={(e) => setCustomEndpoint(e.target.value)}
            placeholder="https://api.example.com/v1"
            hint="Any service that speaks the OpenAI chat protocol. It must start with https://. Your key and the summary go to this address only."
            spellCheck={false}
            autoComplete="off"
          />
        )}

        {info.keyLabel ? (
          <div className="flex flex-col gap-2">
            <TextField
              label={info.keyLabel}
              type="password"
              value={key}
              onChange={(e) => setKey(e.target.value)}
              placeholder={hasKey ? "A key is already saved (leave blank to use it)" : info.keyPlaceholder}
              hint={`Stored in the system keychain. It is sent only to ${destinationHost ?? "that service"}, and never shown again.`}
              autoComplete="off"
            />
            {info.keyUrl && (
              <Button size="sm" variant="plain" className="self-start" onClick={() => void api.openUrl(info.keyUrl!)}>
                Get a key
              </Button>
            )}
            {provider === "openrouter" && (
              <div className="flex flex-col gap-1.5 rounded-card bg-fill p-3">
                <p className="text-footnote text-fg-2">Or skip the key: sign in to OpenRouter in your browser and allow Firstlight. It receives a key for your own account and nothing else.</p>
                {orSignIn.isPending ? (
                  <Button size="sm" variant="secondary" className="self-start" onClick={() => void api.cancelOauth()}>
                    Waiting for your browser… Cancel
                  </Button>
                ) : (
                  <Button size="sm" variant="secondary" className="self-start" onClick={() => orSignIn.mutate()}>
                    Sign in with OpenRouter
                  </Button>
                )}
              </div>
            )}
          </div>
        ) : (
          <TextField
            label="Model address"
            value={shownEndpoint}
            onChange={(e) => setEndpoint(e.target.value)}
            hint="Ollama, LM Studio or similar running on this computer. Only addresses on this computer are accepted."
            spellCheck={false}
            autoComplete="off"
          />
        )}

        <div>
          <p className="mb-1.5 text-footnote font-medium text-fg-2">Exactly what will be sent</p>
          <PreviewBox enabled />
        </div>

        <TextField label="Model" value={shownModel} onChange={(e) => setModel(e.target.value)} hint={provider === "local" ? undefined : "Model names change over time. If this one is no longer offered, the check below says so."} spellCheck={false} autoComplete="off" />

        {s.email.enabled && (
          <div className="flex items-center justify-between gap-3">
            <span className="text-body text-fg">Include important email (sender and subject)</span>
            <Switch label="Include important email" checked={s.ai.include_email} onCheckedChange={(v) => (update((d) => void (d.ai.include_email = v)), void qc.invalidateQueries({ queryKey: ["ai-preview"] }))} />
          </div>
        )}
      </div>
    </Dialog>
  );
}
