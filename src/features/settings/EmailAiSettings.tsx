import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { X } from "lucide-react";
import { useState } from "react";
import { api, errorMessage, type AiProvider, type AppSettings } from "@/api";
import { Button, Dialog, Segmented, Switch, TextField, useToasts } from "@/design";
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

  const accounts = useQuery({ queryKey: ["accounts"], queryFn: () => api.listAccounts() });
  const system = useQuery({ queryKey: ["system-status"], queryFn: () => api.systemCalendarStatus() });
  const mailAccounts = (accounts.data ?? []).filter((a) => a.provider === "google" || a.provider === "microsoft");

  const apple = useMutation({ mutationFn: () => api.enableAppleMail(), onSuccess: put, onError: (e) => push({ title: "Couldn't connect Mail", description: errorMessage(e), tone: "danger" }) });
  const account = useMutation({
    mutationFn: (id: string) => api.enableAccountMail(id),
    onSuccess: put,
    onError: (e) => push({ title: "Couldn't allow mail access", description: errorMessage(e), tone: "danger" }),
  });

  if (!s) return null;
  const e = s.email;
  const nothingChosen = e.enabled && !e.apple_mail && e.account_ids.length === 0;
  const addSender = () => {
    const v = sender.trim();
    if (!v) return;
    update((d) => void (d.email.important_senders = [...d.email.important_senders, v]));
    setSender("");
  };

  return (
    <Group title="Email" footer="Off by default. Daybrief reads only each message's sender, subject and a short preview. It never stores whole messages, never loads images or links, and never changes, moves or sends mail.">
      <SettingRow title="Show email in the briefing" description="Unread count, invitations and mail from people who matter.">
        <Switch label="Show email in the briefing" checked={e.enabled} onCheckedChange={(v) => update((d) => void (d.email.enabled = v))} />
      </SettingRow>

      {e.enabled && (
        <>
          {nothingChosen && <SettingRow title="Choose where to read email from" description="Connect the Mail app or one of your accounts below." />}

          {system.data?.supported && (
            <SettingRow title="Mail app" description={e.apple_mail ? "Reading unread mail from the Mail app on this Mac." : "macOS will ask you to allow Daybrief to control Mail."}>
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
    </Group>
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
  const label = ai.provider === "anthropic" ? "Anthropic (Claude)" : "A model on this computer";

  return (
    <Group title="AI summaries" footer="Off by default. When on, a short summary of your day is written by an AI service you choose, using only the information you are shown before you agree. If it ever fails, the normal summary is used.">
      <SettingRow title="Smarter day summary" description="Written by AI instead of Daybrief's built-in summary.">
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
          {ai.provider === "anthropic" && status.data?.has_key && (
            <SettingRow title="API key" description="Saved in the system keychain.">
              <Button size="sm" variant="danger" disabled={forget.isPending} onClick={() => forget.mutate()}>
                Remove key and turn off
              </Button>
            </SettingRow>
          )}
        </>
      )}

      <ConsentDialog open={consent} onOpenChange={setConsent} onDone={(v) => (put(v), refresh())} hasKey={status.data?.has_key ?? false} />
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

function ConsentDialog({ open, onOpenChange, onDone, hasKey }: { open: boolean; onOpenChange: (o: boolean) => void; onDone: (s: AppSettings) => void; hasKey: boolean }) {
  const qc = useQueryClient();
  const { settings: s, update } = useSettings();
  const [provider, setProvider] = useState<AiProvider>("anthropic");
  const [key, setKey] = useState("");
  // Typed values stay drafts until "Turn on": saving half-typed addresses would have them "corrected" mid-keystroke.
  const [endpoint, setEndpoint] = useState<string | null>(null);
  const [model, setModel] = useState<string | null>(null);
  const enable = useMutation({
    mutationFn: async () => {
      const current = qc.getQueryData<AppSettings>(["settings"]);
      if (current) {
        const next = structuredClone(current);
        if (endpoint !== null) next.ai.endpoint = endpoint;
        if (model !== null) next.ai.model = model;
        // Saved first so the test request uses what the user typed.
        qc.setQueryData(["settings"], await api.setSettings(next));
      }
      return api.enableAi(provider, provider === "anthropic" ? key.trim() || null : null, true);
    },
    onSuccess: (saved) => {
      onDone(saved);
      setKey("");
      setEndpoint(null);
      setModel(null);
      onOpenChange(false);
    },
  });

  if (!s) return null;
  const shownEndpoint = endpoint ?? s.ai.endpoint;
  const where = provider === "anthropic" ? "Anthropic's servers (api.anthropic.com)" : `the model at ${shownEndpoint}, which runs on this computer`;
  const needsKey = provider === "anthropic" && !hasKey && key.trim().length < 10;

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        onOpenChange(o);
        if (!o) enable.reset();
      }}
      title="Turn on AI summaries?"
      description="Read exactly what will be shared, then decide."
      footer={
        <>
          <Button onClick={() => onOpenChange(false)}>Cancel</Button>
          <Button variant="primary" disabled={enable.isPending || needsKey} onClick={() => enable.mutate()}>
            {enable.isPending ? "Checking…" : "Turn on"}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4 pb-2">
        <Segmented
          label="Provider"
          value={provider}
          onValueChange={(v) => setProvider(v as AiProvider)}
          options={[
            { value: "anthropic", label: "Claude (your key)" },
            { value: "local", label: "On this computer" },
          ]}
        />

        {/* Where the data goes comes first, so it is the first thing read before agreeing. */}
        <p className="text-body text-fg">
          Your calendar summary will be sent to <span className="font-semibold">{where}</span> about every five minutes while the briefing is open. Meeting links, descriptions, attendees and message text are never included.
        </p>

        {enable.isError && (
          <p role="alert" className="rounded-card bg-danger-soft px-3 py-2 text-body text-danger">
            {errorMessage(enable.error)}
          </p>
        )}

        {provider === "anthropic" ? (
          <TextField
            label="Anthropic API key"
            type="password"
            value={key}
            onChange={(e) => setKey(e.target.value)}
            placeholder={hasKey ? "A key is already saved (leave blank to use it)" : "sk-ant-…"}
            hint="Stored in the system keychain. It is sent only to Anthropic, and never shown again."
            autoComplete="off"
          />
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
          <PreviewBox enabled={open} />
        </div>

        <TextField label="Model" value={model ?? s.ai.model} onChange={(e) => setModel(e.target.value)} spellCheck={false} autoComplete="off" />

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
