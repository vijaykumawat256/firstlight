import { useQuery } from "@tanstack/react-query";
import { CalendarPlus, ChevronRight, MailWarning } from "lucide-react";
import { api, type MailMessage, type MailProblem, type OpenTarget } from "@/api";
import { Button, ListGroup, SectionHeader, useToasts } from "@/design";
import { useNav } from "@/app/nav";
import { timeAgo } from "@/features/accounts/time";
import { formatDate, formatTime } from "./format";

function whenText(w: NonNullable<import("@/api").InvitationItem["when"]>): string {
  if (w.kind === "all_day") return formatDate(new Date(`${w.start}T00:00:00`));
  const s = new Date(w.start);
  return `${formatDate(s)} · ${formatTime(s)}`;
}

function problemText(p: MailProblem): { text: string; action?: "automation" | "settings" } {
  const where = p.source === "apple_mail" ? "Mail" : p.source;
  switch (p.kind) {
    case "permission":
      return p.source === "apple_mail"
        ? { text: "Allow Daybrief to control Mail to see your email here.", action: "automation" }
        : { text: `${where} needs mail permission. Allow it in Settings.`, action: "settings" };
    case "not_running":
      return { text: "Open the Mail app to see your email here." };
    case "reconnect":
      return { text: `Sign in to ${where} again to see its email.`, action: "settings" };
    case "offline":
      return { text: `Couldn't reach ${where}. Will try again.` };
    default:
      return { text: `Couldn't read email from ${where}.` };
  }
}

/** Email at a glance: unread count, invitations and mail from people who matter. Never a mail client. */
export function InboxSection() {
  const push = useToasts((s) => s.push);
  const go = useNav((s) => s.go);
  const digest = useQuery({ queryKey: ["mail-digest"], queryFn: () => api.getMailDigest(false), refetchInterval: 5 * 60_000, staleTime: 60_000, retry: false });
  const open = (target: OpenTarget) => api.openMailMessage(target).catch(() => push({ title: "Couldn't open the message", tone: "danger" }));

  const r = digest.data;
  if (!r?.enabled) return null;
  const d = r.digest;
  if (!d && r.problems.length === 0) return null;
  const nothing = d && d.important.length === 0 && d.invitations.length === 0;

  return (
    <section aria-label="Inbox">
      <SectionHeader title="Inbox" trailing={d ? `${d.unread_total} unread` : undefined} />
      <div className="flex flex-col gap-2">
        {r.problems.length > 0 && (
          <ListGroup>
            {r.problems.map((p) => {
              const { text, action } = problemText(p);
              return (
                <li key={`${p.source}-${p.kind}`} className="flex items-start gap-3 px-4 py-3">
                  <MailWarning className="mt-0.5 size-4 shrink-0 text-warning" aria-hidden />
                  <p className="min-w-0 flex-1 text-body text-fg">{text}</p>
                  {action === "automation" && (
                    <Button size="sm" variant="plain" onClick={() => void api.openPrivacySettings("automation")}>
                      Open Settings
                    </Button>
                  )}
                  {action === "settings" && (
                    <Button size="sm" variant="plain" onClick={() => go("settings")}>
                      Settings
                    </Button>
                  )}
                </li>
              );
            })}
          </ListGroup>
        )}

        {d && d.invitations.length > 0 && (
          <ListGroup aria-label="Invitations">
            {d.invitations.map((i) => (
              <li key={i.message.id}>
                <button type="button" onClick={() => open(i.message.open)} className="flex min-h-12 w-full items-center gap-3 px-4 py-2 text-left transition-colors duration-150 ease-standard hover:bg-fill active:bg-fill-strong">
                  <CalendarPlus className="size-4 shrink-0 text-accent" aria-hidden />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-headline font-medium text-fg">{i.title ?? i.message.subject}</span>
                    <span className="block truncate text-footnote text-fg-2">
                      Invitation from {i.organizer ?? i.message.sender_name}
                      {i.when ? ` · ${whenText(i.when)}` : ""}
                    </span>
                  </span>
                  <ChevronRight className="size-4 shrink-0 text-fg-3" aria-hidden />
                </button>
              </li>
            ))}
          </ListGroup>
        )}

        {d && d.important.length > 0 && (
          <ListGroup aria-label="Important email">
            {d.important.map((m) => (
              <MessageRow key={m.id} m={m} onOpen={() => open(m.open)} />
            ))}
          </ListGroup>
        )}

        {d && (nothing ? <p className="px-1 text-footnote text-fg-2">Nothing important in your inbox.{d.other_unread > 0 ? ` ${d.other_unread} other unread.` : ""}</p> : d.other_unread > 0 ? <p className="px-1 text-footnote text-fg-2">and {d.other_unread} more unread</p> : null)}
      </div>
    </section>
  );
}

function MessageRow({ m, onOpen }: { m: MailMessage; onOpen: () => void }) {
  return (
    <li>
      <button type="button" onClick={onOpen} className="flex min-h-12 w-full items-start gap-3 px-4 py-2.5 text-left transition-colors duration-150 ease-standard hover:bg-fill active:bg-fill-strong">
        <span className="min-w-0 flex-1">
          <span className="flex items-baseline justify-between gap-2">
            <span className="truncate text-headline font-medium text-fg">{m.sender_name}</span>
            <span className="shrink-0 text-footnote text-fg-3">{timeAgo(m.received)}</span>
          </span>
          <span className="block truncate text-body text-fg">{m.subject}</span>
          {m.snippet && <span className="block truncate text-footnote text-fg-2">{m.snippet}</span>}
        </span>
      </button>
    </li>
  );
}
