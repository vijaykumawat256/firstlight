import { useMutation, useQuery } from "@tanstack/react-query";
import { ChevronLeft, ChevronRight, Search } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api, errorMessage } from "@/api";
import { useNav } from "@/app/nav";
import { Button, Card, Segmented, Skeleton } from "@/design";
import { Markdown } from "./markdown";
import { ProblemReportButton } from "./ProblemReport";
import privacyPolicy from "../../../docs/PRIVACY.md?raw";

type Tab = "help" | "privacy" | "licences";

const HELP: Array<{ title: string; body: string[] }> = [
  {
    title: "When does Firstlight appear?",
    body: [
      "The first time you unlock your computer each day, Firstlight opens your briefing: what is left today, what is next, and a look at tomorrow.",
      "On later unlocks it stays out of the way, and only shows a small reminder if a meeting is about to start. It never appears during quiet hours or on days you turn off. All of this is in Settings, under “When Firstlight appears”.",
    ],
  },
  {
    title: "Connecting calendars",
    body: [
      "Use Add a calendar. Google and Microsoft sign in through your browser, so Firstlight never sees your password. iCloud, Yahoo and others use an app password you create in your account. Any calendar link works too. On a Mac, you can also use the calendars already in your Calendar app.",
      "If the same meeting is in two accounts, it appears once.",
    ],
  },
  {
    title: "Reading the briefing",
    body: [
      "“Needs attention” lists overlapping meetings, runs of back-to-back meetings, and invitations you haven't answered. “Free” shows gaps long enough to be useful. Click any event for its details and a Join button when it has a call link.",
    ],
  },
  {
    title: "Email and AI (both optional)",
    body: [
      "Email is off until you turn it on. It shows an unread count, meeting invitations, and mail from the people you mark as important. It reads only the sender, subject and a short preview.",
      "AI summaries are off until you turn them on. You are shown exactly what would be sent before you agree.",
    ],
  },
  {
    title: "Keyboard",
    body: [
      "Everything can be used with the keyboard: Tab moves between controls, Space and Enter activate them, and Escape closes a panel. Set a global shortcut in Settings to show or hide Firstlight from any app.",
    ],
  },
];

function Updates() {
  const info = useQuery({ queryKey: ["app-info"], queryFn: () => api.getAppInfo(), staleTime: Infinity });
  const check = useMutation({ mutationFn: () => api.checkForUpdate() });
  const install = useMutation({ mutationFn: () => api.installUpdate() });
  const status = check.data;

  if (!info.data) return <Skeleton className="h-16 rounded-card" />;
  return (
    <Card>
      <p className="text-headline font-semibold text-fg">{info.data.name}</p>
      <p className="text-body text-fg-2">
        Version {info.data.version} · {info.data.os} {info.data.arch}
      </p>
      <div className="mt-3 flex flex-wrap items-center gap-3">
        <Button size="sm" variant="secondary" disabled={check.isPending || !info.data.updates_configured} onClick={() => check.mutate()}>
          {check.isPending ? "Checking…" : "Check for updates"}
        </Button>
        {!info.data.updates_configured && <p className="text-footnote text-fg-2">Updates aren't set up in this build.</p>}
        {status?.state === "up_to_date" && <p role="status" className="text-footnote text-fg-2">You're up to date.</p>}
        {status?.state === "failed" && <p role="status" className="text-footnote text-danger">Couldn't check. Try again later.</p>}
        {status?.state === "available" && (
          <>
            <p role="status" className="text-footnote text-fg">Version {status.version} is available.</p>
            <Button size="sm" variant="primary" disabled={install.isPending} onClick={() => install.mutate()}>
              {install.isPending ? "Installing…" : "Install and restart"}
            </Button>
          </>
        )}
        <ProblemReportButton />
      </div>
      {install.isError && <p role="alert" className="mt-2 text-footnote text-danger">{errorMessage(install.error)}</p>}
    </Card>
  );
}

interface LicenceData {
  generated: string;
  texts: string[];
  rust: Entry[];
  npm: Entry[];
}
interface Entry {
  n: string;
  v: string;
  l: string;
  r: string | null;
  t: number;
}

function Licences() {
  const [data, setData] = useState<LicenceData | null>(null);
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState<string | null>(null);

  // A large file that is only needed here: loaded when this tab opens.
  useEffect(() => {
    let alive = true;
    void import("@/assets/licences.json").then((m) => alive && setData(m.default as unknown as LicenceData));
    return () => {
      alive = false;
    };
  }, []);

  const all = useMemo(() => (data ? [...data.npm.map((e) => ({ ...e, kind: "JavaScript" })), ...data.rust.map((e) => ({ ...e, kind: "Rust" }))] : []), [data]);
  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return q ? all.filter((e) => `${e.n} ${e.l}`.toLowerCase().includes(q)) : all;
  }, [all, query]);

  if (!data) return <Skeleton className="h-40 rounded-card" />;
  return (
    <div className="flex flex-col gap-3">
      <p className="text-body text-fg-2">
        Firstlight is built with {all.length} open-source packages. Their licences, as of {data.generated}:
      </p>
      <label className="relative block">
        <span className="sr-only">Search licences</span>
        <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-fg-3" aria-hidden />
        <input
          type="search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search packages or licences"
          className="h-9 w-full select-text rounded-card bg-fill pl-9 pr-3 text-headline text-fg placeholder:text-fg-3"
        />
      </label>
      <p className="text-footnote text-fg-2" role="status">
        {shown.length} {shown.length === 1 ? "package" : "packages"}
      </p>
      <ul className="divide-y divide-separator overflow-hidden rounded-card bg-surface shadow-card">
        {shown.map((e) => {
          const key = `${e.kind}-${e.n}-${e.v}`;
          const isOpen = open === key;
          return (
            <li key={key}>
              <button type="button" aria-expanded={isOpen} onClick={() => setOpen(isOpen ? null : key)} className="flex min-h-11 w-full items-center gap-3 px-4 py-2 text-left hover:bg-fill">
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-body font-medium text-fg">
                    {e.n} <span className="font-normal text-fg-2">{e.v}</span>
                  </span>
                  <span className="block truncate text-footnote text-fg-2">{e.l}</span>
                </span>
                <ChevronRight className={`size-4 shrink-0 text-fg-3 transition-transform duration-150 ease-standard ${isOpen ? "rotate-90" : ""}`} aria-hidden />
              </button>
              {isOpen && (
                <div className="bg-fill px-4 py-3">
                  {e.t >= 0 ? (
                    <pre className="max-h-64 select-text overflow-auto whitespace-pre-wrap break-words font-mono text-caption text-fg-2">{data.texts[e.t]}</pre>
                  ) : (
                    <p className="text-footnote text-fg-2">
                      Licence: {e.l}. This package does not include its licence file; the full text is in the project's repository{e.r ? `: ${e.r}` : "."}
                    </p>
                  )}
                </div>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

export function AboutScreen() {
  const go = useNav((s) => s.go);
  const [tab, setTab] = useState<Tab>("help");
  return (
    <div className="flex h-full flex-col">
      <div data-tauri-drag-region className="flex h-11 shrink-0 items-center px-3 pl-20">
        <Button variant="plain" size="sm" leading={<ChevronLeft className="size-4" />} onClick={() => go("settings")}>
          Settings
        </Button>
      </div>
      <main className="min-h-0 flex-1 overflow-y-auto px-5 pb-10 [&>*]:mx-auto [&>*]:w-full [&>*]:max-w-2xl">
        <h1 className="mb-4 text-largetitle font-bold tracking-tight text-fg">About</h1>
        <div className="flex flex-col gap-5">
          <Updates />
          <Segmented
            label="About sections"
            value={tab}
            onValueChange={(v) => setTab(v as Tab)}
            options={[
              { value: "help", label: "Help" },
              { value: "privacy", label: "Privacy" },
              { value: "licences", label: "Licences" },
            ]}
          />
          {tab === "help" && (
            <div className="flex flex-col gap-5">
              {HELP.map((h) => (
                <section key={h.title} aria-label={h.title} className="flex flex-col gap-1.5">
                  <h2 className="text-headline font-semibold text-fg">{h.title}</h2>
                  {h.body.map((p) => (
                    <p key={p} className="text-body text-fg-2">{p}</p>
                  ))}
                </section>
              ))}
            </div>
          )}
          {tab === "privacy" && (
            <section aria-label="Privacy policy" className="flex flex-col gap-3">
              <h2 className="text-title font-semibold text-fg">Privacy policy</h2>
              <Markdown source={privacyPolicy} />
            </section>
          )}
          {tab === "licences" && <Licences />}
        </div>
      </main>
    </div>
  );
}
