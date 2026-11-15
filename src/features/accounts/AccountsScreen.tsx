import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { AlertCircle, ChevronLeft, CloudOff, Plus } from "lucide-react";
import { useState } from "react";
import { api, errorMessage, type AccountView, type CalendarInfo } from "@/api";
import {
  Badge, Button, CALENDAR_COLOURS, Card, Dialog, IconButton, ListGroup, Skeleton, Switch, TextField, Tooltip, calendarColour, useToasts,
} from "@/design";
import { useNav } from "@/app/nav";
import { useSettings } from "@/features/settings/useSettings";
import { AddAccountSheet } from "./AddAccountSheet";
import { timeAgo } from "./time";

const PROVIDER_LABEL: Record<AccountView["provider"], string> = {
  fixture: "Sample data",
  google: "Google",
  microsoft: "Microsoft",
  caldav: "CalDAV",
  ics: "Calendar link",
  system: "This computer",
  local: "Your own events",
};

function CalendarRow({ cal, onChanged }: { cal: CalendarInfo; onChanged: () => void }) {
  const push = useToasts((s) => s.push);
  const [picking, setPicking] = useState(false);
  const visible = useMutation({
    mutationFn: (v: boolean) => api.setCalendarVisible(cal.id, v),
    onSuccess: onChanged,
    onError: (e) => push({ title: "Couldn't update calendar", description: errorMessage(e), tone: "danger" }),
  });
  const colour = useMutation({
    mutationFn: (c: string) => api.setCalendarColour(cal.id, c),
    onSuccess: () => {
      onChanged();
      setPicking(false);
    },
    onError: (e) => push({ title: "Couldn't change colour", description: errorMessage(e), tone: "danger" }),
  });

  return (
    <li className="flex min-h-12 items-center gap-3 px-4 py-2">
      <Tooltip content="Change colour">
        <button
          type="button"
          aria-label={`Colour for ${cal.name}`}
          onClick={() => setPicking(true)}
          className="flex size-6 shrink-0 items-center justify-center rounded-full transition-transform duration-150 ease-standard hover:scale-110"
        >
          <span className="size-3.5 rounded-full" style={{ background: calendarColour(cal.colour) }} />
        </button>
      </Tooltip>
      <span className="min-w-0 flex-1 truncate text-headline font-medium text-fg">{cal.name}</span>
      <Switch label={`Show ${cal.name}`} checked={cal.visible} onCheckedChange={(v) => visible.mutate(v)} />
      <Dialog open={picking} onOpenChange={setPicking} title="Calendar colour" description={cal.name}>
        <div className="grid grid-cols-5 gap-3 pb-3" role="radiogroup" aria-label="Colour">
          {CALENDAR_COLOURS.map((c) => (
            <button
              key={c}
              type="button"
              role="radio"
              aria-checked={cal.colour === c}
              aria-label={c}
              onClick={() => colour.mutate(c)}
              className="flex size-11 items-center justify-center rounded-full ring-offset-2 ring-offset-surface-raised transition-transform duration-150 ease-standard hover:scale-105 aria-checked:ring-2 aria-checked:ring-fg"
              style={{ background: calendarColour(c) }}
            />
          ))}
        </div>
      </Dialog>
    </li>
  );
}

function Status({ a }: { a: AccountView }) {
  if (a.status === "needs_attention") {
    return (
      <p className="flex items-start gap-1.5 text-footnote text-danger">
        <AlertCircle className="mt-px size-3.5 shrink-0" aria-hidden />{" "}
        {a.provider === "system" ? "Calendar access was turned off. Allow it in System Settings, then check again." : a.provider === "google" || a.provider === "microsoft" ? "Needs attention. Sign in again to keep it up to date." : "Needs attention. Add the calendar again with a fresh link or password."}
      </p>
    );
  }
  if (a.last_error === "offline") {
    return (
      <p className="flex items-start gap-1.5 text-footnote text-fg-2">
        <CloudOff className="mt-px size-3.5 shrink-0" aria-hidden /> Offline. Showing saved events from {timeAgo(a.last_synced_at)}.
      </p>
    );
  }
  if (a.last_error) return <p className="text-footnote text-warning">Last sync didn’t finish. Will retry.</p>;
  return <p className="text-footnote text-fg-2">Synced {timeAgo(a.last_synced_at)}</p>;
}

/** Off by default: Firstlight only reads this account until the user lets it add events. */
function WriteSwitch({ account }: { account: AccountView }) {
  const { settings: s, update } = useSettings();
  const writable = account.calendars.some((c) => !c.read_only);
  if (!s) return null;
  const on = s.general.writable_accounts.includes(account.id);
  return (
    <div className="flex items-start justify-between gap-3 px-1">
      <div className="min-w-0">
        <p className="text-subheadline font-medium text-fg">Let Firstlight add events to this account</p>
        <p className="mt-0.5 text-footnote text-fg-2">
          {writable
            ? "Off by default. When on, the New event form can save into this account's calendars, and Firstlight can change or remove only the events it added. Everything already there stays untouched. The server may not send invitations for these."
            : "The server doesn't let this account's calendars be changed, so there is nothing to turn on."}
        </p>
      </div>
      <Switch
        label={`Add events to ${account.display_name}`}
        checked={on && writable}
        disabled={!writable}
        onCheckedChange={(v) =>
          update((d) => {
            d.general.writable_accounts = v ? [...new Set([...d.general.writable_accounts, account.id])] : d.general.writable_accounts.filter((x) => x !== account.id);
          })
        }
      />
    </div>
  );
}

function AccountCard({ account, onChanged }: { account: AccountView; onChanged: () => void }) {
  const push = useToasts((s) => s.push);
  const [renaming, setRenaming] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [name, setName] = useState(account.display_name);

  const rename = useMutation({
    mutationFn: () => api.renameAccount(account.id, name),
    onSuccess: () => {
      onChanged();
      setRenaming(false);
    },
    onError: (e) => push({ title: "Couldn't rename", description: errorMessage(e), tone: "danger" }),
  });
  const reconnect = useMutation({
    mutationFn: () => api.addOauthAccount(account.provider as "google" | "microsoft"),
    onSuccess: onChanged,
    onError: (e) => push({ title: "Couldn't reconnect", description: errorMessage(e), tone: "danger" }),
  });
  const recheck = useMutation({
    mutationFn: () => api.addSystemAccount(),
    onSuccess: onChanged,
    onError: (e) => push({ title: "Still no access", description: errorMessage(e), tone: "danger" }),
  });
  const isSystem = account.provider === "system";
  const isOauth = account.provider === "google" || account.provider === "microsoft";
  const remove = useMutation({
    mutationFn: () => api.removeAccount(account.id),
    onSuccess: () => {
      onChanged();
      setConfirming(false);
    },
    onError: (e) => push({ title: "Couldn't remove account", description: errorMessage(e), tone: "danger" }),
  });

  return (
    <section aria-label={account.display_name} className="flex flex-col gap-2">
      <div className="flex items-start justify-between gap-3 px-1">
        <div className="min-w-0">
          <h2 className="truncate text-headline font-semibold text-fg">{account.display_name}</h2>
          <div className="mt-0.5 flex items-center gap-2">
            <Badge tone={account.status === "needs_attention" ? "danger" : "neutral"}>{PROVIDER_LABEL[account.provider]}</Badge>
          </div>
        </div>
        <div className="flex shrink-0 gap-1">
          {isSystem && account.status === "needs_attention" && (
            <>
              <Button variant="plain" size="sm" onClick={() => void api.openPrivacySettings("calendars")}>
                Open Settings
              </Button>
              <Button variant="primary" size="sm" disabled={recheck.isPending} onClick={() => recheck.mutate()}>
                {recheck.isPending ? "Checking…" : "Check again"}
              </Button>
            </>
          )}
          {isOauth && account.status === "needs_attention" && (
            <Button variant="primary" size="sm" disabled={reconnect.isPending} onClick={() => reconnect.mutate()}>
              {reconnect.isPending ? "Waiting…" : "Reconnect"}
            </Button>
          )}
          <Button variant="plain" size="sm" onClick={() => { setName(account.display_name); setRenaming(true); }}>
            Rename
          </Button>
          <Button variant="plain" size="sm" className="!text-danger hover:!bg-danger-soft" onClick={() => setConfirming(true)}>
            Remove
          </Button>
        </div>
      </div>
      <div className="px-1">
        <Status a={account} />
      </div>
      <ListGroup>
        {account.calendars.map((c) => (
          <CalendarRow key={c.id} cal={c} onChanged={onChanged} />
        ))}
      </ListGroup>
      {account.provider === "caldav" && <WriteSwitch account={account} />}

      <Dialog
        open={renaming}
        onOpenChange={setRenaming}
        title="Rename"
        footer={
          <>
            <Button onClick={() => setRenaming(false)}>Cancel</Button>
            <Button variant="primary" disabled={rename.isPending} onClick={() => rename.mutate()}>Save</Button>
          </>
        }
      >
        <form onSubmit={(e) => { e.preventDefault(); rename.mutate(); }}>
          <TextField label="Name" value={name} maxLength={80} onChange={(e) => setName(e.target.value)} autoFocus />
        </form>
      </Dialog>

      <Dialog
        open={confirming}
        onOpenChange={setConfirming}
        title={`Remove “${account.display_name}”?`}
        description="Its events are deleted from this computer, and its saved link or sign-in is removed from the keychain. Your calendar itself is untouched."
        footer={
          <>
            <Button onClick={() => setConfirming(false)}>Cancel</Button>
            <Button variant="danger" disabled={remove.isPending} onClick={() => remove.mutate()}>
              {remove.isPending ? "Removing…" : "Remove"}
            </Button>
          </>
        }
      >
        <span className="sr-only">Confirm removal</span>
      </Dialog>
    </section>
  );
}

export function AccountsScreen() {
  const qc = useQueryClient();
  const go = useNav((s) => s.go);
  const [adding, setAdding] = useState(false);
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: () => api.listAccounts() });
  const changed = () => void qc.invalidateQueries();

  return (
    <div className="flex h-full flex-col">
      <div data-tauri-drag-region className="flex h-11 shrink-0 items-center justify-between px-3 pl-20">
        <Button variant="plain" size="sm" leading={<ChevronLeft className="size-4" />} onClick={() => go("briefing")}>
          Briefing
        </Button>
        <Tooltip content="Add calendar">
          <IconButton label="Add calendar" onClick={() => setAdding(true)}>
            <Plus className="size-[18px]" />
          </IconButton>
        </Tooltip>
      </div>
      <main className="min-h-0 flex-1 overflow-y-auto px-5 pb-8 [&>*]:mx-auto [&>*]:w-full [&>*]:max-w-2xl">
        <h1 className="mb-6 text-largetitle font-bold tracking-tight text-fg">Accounts</h1>
        {accounts.isPending ? (
          <div className="flex flex-col gap-3" aria-busy>
            <Skeleton className="h-5 w-32" />
            <Skeleton className="h-24 rounded-card" />
          </div>
        ) : accounts.isError ? (
          <Card><p className="text-body text-danger">Couldn’t load your accounts.</p></Card>
        ) : (
          <div className="flex flex-col gap-7">
            {accounts.data.map((a) => (
              <AccountCard key={a.id} account={a} onChanged={changed} />
            ))}
            <Button variant="secondary" leading={<Plus className="size-4" />} onClick={() => setAdding(true)} className="self-start">
              Add a calendar
            </Button>
          </div>
        )}
      </main>
      <AddAccountSheet open={adding} onOpenChange={setAdding} hasDemo={accounts.data?.some((a) => a.provider === "fixture") ?? false} />
    </div>
  );
}
