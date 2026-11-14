import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Bell, CalendarDays, ChevronLeft, Info, LockOpen, Mail, Palette, Settings2, ShieldCheck, Sparkles, Sunrise, type LucideIcon } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api, errorMessage, type DefaultView, type ThemeChoice, type TimeFormat, type WeekStart } from "@/api";
import { useNav, type View } from "@/app/nav";
import { Button, CALENDAR_COLOURS, Dialog, ListGroup, Segmented, Select, Skeleton, Switch, calendarColour, useToasts } from "@/design";
import { DayPicker, Group, HourSelect, NavRow, NumberSelect, SettingRow, ShortcutRecorder } from "./controls";
import { AiGroup, EmailGroup } from "./EmailAiSettings";
import { useSettings } from "./useSettings";

const ACCENTS = ["system", ...CALENDAR_COLOURS.filter((c) => !["blue", "yellow", "brown"].includes(c))];

type Section = "general" | "appearance" | "briefing" | "appears" | "notifications" | "email" | "ai" | "privacy";
interface HomeItem {
  title: string;
  description: string;
  icon: LucideIcon;
  /** Opens a page of settings… */
  section?: Section;
  /** …or another screen. */
  view?: View;
}

/** The settings home: a few short clusters, each row opening one page. */
const HOME: HomeItem[][] = [
  [
    { section: "general", title: "General", description: "Start at login, shortcut, sync and updates", icon: Settings2 },
    { section: "appearance", title: "Appearance", description: "Theme, accent colour, time format and week start", icon: Palette },
  ],
  [
    { section: "briefing", title: "Briefing", description: "What the briefing shows and your working hours", icon: Sunrise },
    { section: "appears", title: "When it appears", description: "Unlock rules, days and quiet hours", icon: LockOpen },
    { section: "notifications", title: "Notifications", description: "Meeting heads-ups and daily summaries", icon: Bell },
  ],
  [
    { view: "accounts", title: "Accounts & calendars", description: "Connect accounts, choose calendars and colours", icon: CalendarDays },
    { section: "email", title: "Email", description: "Unread mail and invitations in the briefing", icon: Mail },
    { section: "ai", title: "AI summaries", description: "An optional smarter summary of your day", icon: Sparkles },
  ],
  [
    { section: "privacy", title: "Privacy & data", description: "Backup, crash reports and erasing everything", icon: ShieldCheck },
    { view: "about", title: "About Firstlight", description: "Version, help, privacy policy and licences", icon: Info },
  ],
];
const titleOf = (section: Section) => HOME.flat().find((i) => i.section === section)?.title ?? "Settings";

export function SettingsScreen() {
  const section = useNav((n) => n.settingsSection) as Section | null;
  const setSection = useNav((n) => n.setSettingsSection);
  const heading = useRef<HTMLHeadingElement>(null);
  const first = useRef(true);
  // Moving between the list and a page replaces everything on screen, so put keyboard and screen-reader focus on the new title.
  useEffect(() => {
    if (first.current) first.current = false;
    else heading.current?.focus();
  }, [section]);

  const go = useNav((s) => s.go);
  const qc = useQueryClient();
  const push = useToasts((s) => s.push);
  const { settings: s, update } = useSettings();

  const autostart = useQuery({ queryKey: ["autostart"], queryFn: () => api.getAutostart() });
  const toggleAutostart = useMutation({
    mutationFn: (enabled: boolean) => api.setAutostart(enabled),
    onSettled: () => void qc.invalidateQueries({ queryKey: ["autostart"] }),
    onError: () => push({ title: "Couldn't change Start at Login", tone: "danger" }),
  });

  const [importing, setImporting] = useState(false);
  const [importText, setImportText] = useState("");
  const importMutation = useMutation({
    mutationFn: () => api.importSettings(importText),
    onSuccess: (saved) => {
      qc.setQueryData(["settings"], saved);
      void qc.invalidateQueries({ queryKey: ["briefing"] });
      setImporting(false);
      setImportText("");
      push({ title: "Settings imported" });
    },
  });
  const exportFile = useMutation({
    mutationFn: () => api.saveSettingsExport(),
    onSuccess: (path) => push({ title: "Settings saved", description: path }),
    onError: (e) => push({ title: "Couldn't save the file", description: errorMessage(e), tone: "danger" }),
  });
  const copy = useMutation({
    mutationFn: async () => navigator.clipboard.writeText(await api.exportSettings()),
    onSuccess: () => push({ title: "Settings copied" }),
    onError: () => push({ title: "Couldn't copy to the clipboard", tone: "danger" }),
  });

  const crash = useQuery({ queryKey: ["crash-report"], queryFn: () => api.getLastCrashReport() });
  const [reportOpen, setReportOpen] = useState(false);
  const clearReports = useMutation({
    mutationFn: () => api.clearCrashReports(),
    onSuccess: () => {
      setReportOpen(false);
      void crash.refetch();
    },
  });
  const [erasing, setErasing] = useState(false);
  const erase = useMutation({
    mutationFn: () => api.eraseAllData(),
    onSuccess: async () => {
      setErasing(false);
      await qc.invalidateQueries();
      go("briefing");
      push({ title: "Everything was erased" });
    },
    onError: (e) => push({ title: "Couldn't erase everything", description: errorMessage(e), tone: "danger" }),
  });

  return (
    <div className="flex h-full flex-col">
      <div data-tauri-drag-region className="flex h-11 shrink-0 items-center px-3 pl-20">
        <Button variant="plain" size="sm" leading={<ChevronLeft className="size-4" />} onClick={() => (section ? setSection(null) : go("briefing"))}>
          {section ? "Settings" : "Briefing"}
        </Button>
      </div>
      <main key={section ?? "home"} className="min-h-0 flex-1 overflow-y-auto px-5 pb-10">
        <h1 ref={heading} tabIndex={-1} className="mb-6 text-largetitle font-bold tracking-tight text-fg outline-none">
          {section ? titleOf(section) : "Settings"}
        </h1>
        {!s && (
          <div className="flex flex-col gap-3" aria-busy>
            <Skeleton className="h-5 w-24" />
            <Skeleton className="h-40 rounded-card" />
          </div>
        )}
        {s && (
          <>
        {section === null && (
          <div className="flex flex-col gap-5">
            {HOME.map((cluster) => (
              <ListGroup key={cluster.map((i) => i.title).join()}>
                {cluster.map((item) => (
                  <NavRow key={item.title} icon={item.icon} title={item.title} description={item.description} onClick={() => (item.view ? go(item.view) : setSection(item.section ?? null))} />
                ))}
              </ListGroup>
            ))}
          </div>
        )}
        {section === "general" && (
          <div className="flex flex-col gap-7">
            <Group title="Startup">
              <SettingRow title="Start at login" description="Open Firstlight quietly when you sign in.">
                <Switch label="Start at login" checked={autostart.data ?? false} disabled={autostart.isPending} onCheckedChange={(v) => toggleAutostart.mutate(v)} />
              </SettingRow>
              <SettingRow title="Opens to">
                <Select
                  label="Opens to"
                  value={s.general.default_view}
                  onValueChange={(v) => update((d) => void (d.general.default_view = v as DefaultView))}
                  options={[
                    { value: "briefing", label: "Briefing" },
                    { value: "day", label: "Day view" },
                    { value: "week", label: "Week view" },
                  ]}
                />
              </SettingRow>
              <SettingRow title="Show or hide the briefing" description="A shortcut that works from any app.">
                <ShortcutRecorder value={s.general.global_shortcut} onChange={(v) => update((d) => void (d.general.global_shortcut = v))} />
              </SettingRow>
            </Group>

            <Group title="Sync">
              <SettingRow title="Refresh calendars every">
                <NumberSelect label="Refresh interval" unit="min" options={[1, 5, 10, 15, 30, 60]} value={s.general.sync_interval_minutes} onChange={(n) => update((d) => void (d.general.sync_interval_minutes = n))} />
              </SettingRow>
            </Group>

            <Group title="Updates">
              <SettingRow title="Check for updates" description="About once a day. Sends only the app's version and your operating system.">
                <Switch label="Check for updates automatically" checked={s.general.check_for_updates} onCheckedChange={(v) => update((d) => void (d.general.check_for_updates = v))} />
              </SettingRow>
              {s.general.check_for_updates && (
                <SettingRow title="Update channel" description="Beta gets new things sooner, with more rough edges.">
                  <Segmented
                    label="Update channel"
                    value={s.general.update_channel}
                    onValueChange={(v) => update((d) => void (d.general.update_channel = v as "stable" | "beta"))}
                    options={[
                      { value: "stable", label: "Stable" },
                      { value: "beta", label: "Beta" },
                    ]}
                  />
                </SettingRow>
              )}
            </Group>
          </div>
        )}
        {section === "appearance" && (
          <div className="flex flex-col gap-7">
            <Group title="Look">
              <SettingRow title="Theme">
                <Segmented
                  label="Theme"
                  value={s.appearance.theme}
                  onValueChange={(v) => update((d) => void (d.appearance.theme = v as ThemeChoice))}
                  options={[
                    { value: "system", label: "Auto" },
                    { value: "light", label: "Light" },
                    { value: "dark", label: "Dark" },
                  ]}
                />
              </SettingRow>
              <SettingRow title="Accent colour" stack>
                <div role="radiogroup" aria-label="Accent colour" className="flex flex-wrap gap-2.5">
                  {ACCENTS.map((c) => (
                    <button
                      key={c}
                      type="button"
                      role="radio"
                      aria-checked={s.appearance.accent === c}
                      aria-label={c === "system" ? "Default" : c}
                      onClick={() => update((d) => void (d.appearance.accent = c))}
                      className="size-7 rounded-full ring-offset-2 ring-offset-surface transition-transform duration-150 ease-standard hover:scale-110 aria-checked:ring-2 aria-checked:ring-fg"
                      style={{ background: c === "system" ? "#007aff" : calendarColour(c) }}
                    />
                  ))}
                </div>
              </SettingRow>
            </Group>

            <Group title="Date and time">
              <SettingRow title="Time format">
                <Segmented
                  label="Time format"
                  value={s.general.time_format}
                  onValueChange={(v) => update((d) => void (d.general.time_format = v as TimeFormat))}
                  options={[
                    { value: "auto", label: "Auto" },
                    { value: "h12", label: "12-hour" },
                    { value: "h24", label: "24-hour" },
                  ]}
                />
              </SettingRow>
              <SettingRow title="Week starts on">
                <Select
                  label="Week starts on"
                  value={s.general.week_start}
                  onValueChange={(v) => update((d) => void (d.general.week_start = v as WeekStart))}
                  options={[
                    { value: "auto", label: "Automatic" },
                    { value: "monday", label: "Monday" },
                    { value: "sunday", label: "Sunday" },
                    { value: "saturday", label: "Saturday" },
                  ]}
                />
              </SettingRow>
            </Group>
          </div>
        )}
        {section === "briefing" && (
          <div className="flex flex-col gap-7">
            <Group title="Sections" footer="Choose what the briefing shows. Cancelled events are never shown.">
              <SettingRow title="Needs attention" description="Overlaps, back-to-back runs and unanswered invitations.">
                <Switch label="Show needs attention" checked={s.sections.attention} onCheckedChange={(v) => update((d) => void (d.sections.attention = v))} />
              </SettingRow>
              <SettingRow title="Free time" description="Gaps between today's meetings.">
                <Switch label="Show free time" checked={s.sections.free_time} onCheckedChange={(v) => update((d) => void (d.sections.free_time = v))} />
              </SettingRow>
              <SettingRow title="Tomorrow">
                <Switch label="Show tomorrow" checked={s.sections.tomorrow} onCheckedChange={(v) => update((d) => void (d.sections.tomorrow = v))} />
              </SettingRow>
              <SettingRow title="Declined events" description="Show events you have said no to.">
                <Switch label="Show declined events" checked={s.briefing.show_declined} onCheckedChange={(v) => update((d) => void (d.briefing.show_declined = v))} />
              </SettingRow>
            </Group>

            <Group title="Your day">
              <SettingRow title="Working day starts">
                <HourSelect label="Working day starts" from={0} to={12} value={s.briefing.work_start_hour} onChange={(h) => update((d) => void (d.briefing.work_start_hour = h))} />
              </SettingRow>
              <SettingRow title="Working day ends">
                <HourSelect label="Working day ends" from={13} to={23} value={s.briefing.work_end_hour} onChange={(h) => update((d) => void (d.briefing.work_end_hour = h))} />
              </SettingRow>
              <SettingRow title="Mention free time longer than">
                <NumberSelect label="Minimum free time" unit="min" options={[15, 30, 45, 60, 90]} value={s.briefing.min_gap_minutes} onChange={(n) => update((d) => void (d.briefing.min_gap_minutes = n))} />
              </SettingRow>
              <SettingRow title="Flag tomorrow's early start before">
                <HourSelect label="Early start before" from={5} to={11} value={s.briefing.early_hour} onChange={(h) => update((d) => void (d.briefing.early_hour = h))} />
              </SettingRow>
            </Group>
          </div>
        )}
        {section === "appears" && (
          <div className="flex flex-col gap-7">
            <Group title="On unlock" footer="The first unlock of the day opens your briefing. Later unlocks only show a small reminder when a meeting is about to start.">
              <SettingRow title="First unlock after">
                <HourSelect label="First unlock after" from={0} to={12} value={s.unlock.first_unlock_after_hour} onChange={(h) => update((d) => void (d.unlock.first_unlock_after_hour = h))} />
              </SettingRow>
              <SettingRow title="Remind me about meetings starting within">
                <NumberSelect label="Reminder window" unit="min" options={[5, 10, 15, 30, 60]} value={s.unlock.up_next_lead_minutes} onChange={(n) => update((d) => void (d.unlock.up_next_lead_minutes = n))} />
              </SettingRow>
              <SettingRow title="At least this long between appearances">
                <NumberSelect label="Minimum time between appearances" unit="min" options={[0, 15, 30, 60, 120]} value={s.unlock.min_interval_minutes} onChange={(n) => update((d) => void (d.unlock.min_interval_minutes = n))} />
              </SettingRow>
            </Group>

            <Group title="Quiet times">
              <SettingRow title="Days" description="It stays quiet on the others." stack>
                <DayPicker days={s.unlock.days} onChange={(days) => update((d) => void (d.unlock.days = days))} />
              </SettingRow>
              <SettingRow title="Quiet hours" description="Nothing appears, and no summaries are sent.">
                <Switch
                  label="Quiet hours"
                  checked={s.unlock.quiet_hours !== null}
                  onCheckedChange={(on) => update((d) => void (d.unlock.quiet_hours = on ? [22, 6] : null))}
                />
              </SettingRow>
              {s.unlock.quiet_hours && (
                <SettingRow title="From and until">
                  <div className="flex items-center gap-2">
                    <HourSelect label="Quiet hours from" from={0} to={23} value={s.unlock.quiet_hours[0]} onChange={(h) => update((d) => void (d.unlock.quiet_hours = [h, d.unlock.quiet_hours?.[1] ?? 6]))} />
                    <span className="text-footnote text-fg-2">to</span>
                    <HourSelect label="Quiet hours until" from={0} to={23} value={s.unlock.quiet_hours[1]} onChange={(h) => update((d) => void (d.unlock.quiet_hours = [d.unlock.quiet_hours?.[0] ?? 22, h]))} />
                  </div>
                </SettingRow>
              )}
            </Group>
          </div>
        )}
        {section === "notifications" && (
          <div className="flex flex-col gap-7">
            <Group title="Meetings" footer="Heads-ups still arrive during quiet hours, so an early meeting is never missed.">
              <SettingRow title="Show event names" description="Turn off to keep names and places out of notifications, which appear on the lock screen and in screen shares.">
                <Switch label="Show event names in notifications" checked={!s.notify.hide_titles} onCheckedChange={(v) => update((d) => void (d.notify.hide_titles = !v))} />
              </SettingRow>
              <SettingRow title="Before each event">
                <Switch label="Notify before events" checked={s.notify.before_event} onCheckedChange={(v) => update((d) => void (d.notify.before_event = v))} />
              </SettingRow>
              {s.notify.before_event && (
                <SettingRow title="How early">
                  <NumberSelect label="Notify how early" unit="min" options={[5, 10, 15, 30]} value={s.notify.before_minutes} onChange={(n) => update((d) => void (d.notify.before_minutes = n))} />
                </SettingRow>
              )}
            </Group>

            <Group title="Daily summaries">
              <SettingRow title="Morning briefing" description="A short summary of your day.">
                <Switch label="Morning briefing" checked={s.notify.morning} onCheckedChange={(v) => update((d) => void (d.notify.morning = v))} />
              </SettingRow>
              {s.notify.morning && (
                <SettingRow title="Morning briefing time">
                  <HourSelect label="Morning briefing time" from={4} to={11} value={s.notify.morning_hour} onChange={(h) => update((d) => void (d.notify.morning_hour = h))} />
                </SettingRow>
              )}
              <SettingRow title="Evening look-ahead" description="A heads-up about tomorrow.">
                <Switch label="Evening look-ahead" checked={s.notify.evening} onCheckedChange={(v) => update((d) => void (d.notify.evening = v))} />
              </SettingRow>
              {s.notify.evening && (
                <SettingRow title="Evening look-ahead time">
                  <HourSelect label="Evening look-ahead time" from={12} to={22} value={s.notify.evening_hour} onChange={(h) => update((d) => void (d.notify.evening_hour = h))} />
                </SettingRow>
              )}
            </Group>
          </div>
        )}
        {section === "email" && <EmailGroup />}
        {section === "ai" && <AiGroup />}
        {section === "privacy" && (
          <div className="flex flex-col gap-7">
            <Group title="Backup">
              <SettingRow title="Back up your settings" description="Settings only. No accounts, links or passwords are included.">
                <div className="flex gap-1">
                  <Button size="sm" variant="secondary" onClick={() => exportFile.mutate()}>
                    Save to Downloads
                  </Button>
                  <Button size="sm" variant="plain" onClick={() => copy.mutate()}>
                    Copy
                  </Button>
                </div>
              </SettingRow>
              <SettingRow title="Restore settings" description="From a file you saved earlier.">
                <Button size="sm" variant="secondary" onClick={() => setImporting(true)}>
                  Import…
                </Button>
              </SettingRow>
            </Group>

            <Group title="Crash reports">
              <SettingRow title="Crash reports" description="If Firstlight stops unexpectedly, save a short report on this computer. It contains no calendar or email information and is never sent anywhere.">
                <Switch label="Save crash reports" checked={s.privacy.crash_reports} onCheckedChange={(v) => update((d) => void (d.privacy.crash_reports = v))} />
              </SettingRow>
              {crash.data && (
                <SettingRow title="Last crash report">
                  <Button size="sm" variant="secondary" onClick={() => setReportOpen(true)}>
                    View…
                  </Button>
                </SettingRow>
              )}
            </Group>

            <Group title="Your data" footer="Firstlight has no server and no account. Your events stay on this computer, your passwords and sign-ins live in the system keychain, and nothing is sent anywhere except to your own calendar providers.">
              <SettingRow title="Erase all data" description="Removes every account, saved password and link, event and setting from this computer.">
                <Button size="sm" variant="danger" onClick={() => setErasing(true)}>
                  Erase…
                </Button>
              </SettingRow>
            </Group>
          </div>
        )}
          </>
        )}
      </main>

      <Dialog
        open={importing}
        onOpenChange={(o) => {
          setImporting(o);
          if (!o) importMutation.reset();
        }}
        title="Import settings"
        description="Paste the contents of a Firstlight settings file."
        footer={
          <>
            <Button onClick={() => setImporting(false)}>Cancel</Button>
            <Button variant="primary" disabled={!importText.trim() || importMutation.isPending} onClick={() => importMutation.mutate()}>
              Import
            </Button>
          </>
        }
      >
        <label className="sr-only" htmlFor="import-text">
          Settings file contents
        </label>
        <textarea
          id="import-text"
          value={importText}
          onChange={(e) => setImportText(e.target.value)}
          spellCheck={false}
          rows={7}
          className="w-full select-text resize-none rounded-card bg-fill p-3 font-mono text-footnote text-fg placeholder:text-fg-3"
          placeholder='{ "format": "daybrief-settings", … }'
        />
        {importMutation.isError && (
          <p role="alert" className="mt-2 text-body text-danger">
            {errorMessage(importMutation.error)}
          </p>
        )}
      </Dialog>

      <Dialog
        open={reportOpen}
        onOpenChange={setReportOpen}
        title="Last crash report"
        description="This is everything in the report. You can share it when asking for help."
        footer={
          <>
            <Button variant="danger" disabled={clearReports.isPending} onClick={() => clearReports.mutate()}>
              Delete
            </Button>
            <Button onClick={() => void navigator.clipboard.writeText(crash.data ?? "").then(() => push({ title: "Copied" }))}>Copy</Button>
          </>
        }
      >
        <pre aria-label="Crash report" className="max-h-56 select-text overflow-auto whitespace-pre-wrap break-words rounded-card bg-fill p-3 font-mono text-footnote text-fg">{crash.data}</pre>
      </Dialog>

      <Dialog
        open={erasing}
        onOpenChange={setErasing}
        title="Erase all data?"
        description="This removes every connected account, saved password and calendar link, all cached events, and all settings from this computer. Your calendars themselves are not touched. It cannot be undone."
        footer={
          <>
            <Button onClick={() => setErasing(false)}>Cancel</Button>
            <Button variant="danger" disabled={erase.isPending} onClick={() => erase.mutate()}>
              {erase.isPending ? "Erasing…" : "Erase everything"}
            </Button>
          </>
        }
      >
        <span className="sr-only">Confirm erasing all data</span>
      </Dialog>
    </div>
  );
}
