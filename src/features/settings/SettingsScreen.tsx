import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Bell, CalendarDays, ChevronLeft, Info, LockOpen, Mail, Palette, Palmtree, Settings2, ShieldCheck, Sparkles, Sunrise, type LucideIcon } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api, errorMessage, type DefaultView, type TextSize, type TimeFormat, type WeekStart } from "@/api";
import { useNav, type View } from "@/app/nav";
import { Button, CALENDAR_COLOURS, Dialog, ListGroup, Segmented, Select, Skeleton, Switch, calendarColour, useToasts } from "@/design";
import { FontPicker, ThemePicker } from "./LookPickers";
import { TimeOffSettings } from "./TimeOffSettings";
import { DndGroup } from "./DndGroup";
import { DayPicker, Group, HourSelect, NavRow, NumberSelect, SettingRow, ShortcutRecorder } from "./controls";
import { AiGroup, EmailGroup, SendingGroup, TaskSyncGroup } from "./EmailAiSettings";
import { COMMON_ZONES, zoneLabel } from "@/features/calendar/zones";
import { useDayStatus } from "@/features/tasks/DayCards";
import { useSettings } from "./useSettings";

const ACCENTS = ["system", ...CALENDAR_COLOURS.filter((c) => !["blue", "yellow", "brown"].includes(c))];

type Section = "general" | "appearance" | "briefing" | "appears" | "notifications" | "timeoff" | "email" | "ai" | "privacy";
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
    { section: "appearance", title: "Appearance", description: "Theme, font, text size, accent colour, time format and week start", icon: Palette },
  ],
  [
    { section: "briefing", title: "Briefing", description: "What the briefing shows and your working hours", icon: Sunrise },
    { section: "appears", title: "When it appears", description: "Unlock rules, days and quiet hours", icon: LockOpen },
    { section: "notifications", title: "Notifications", description: "Meeting heads-ups and daily summaries", icon: Bell },
    { section: "timeoff", title: "Time off", description: "Days off, holidays and an out-of-office reply", icon: Palmtree },
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
  const mapsAvailable = useDayStatus().data?.maps_available ?? false;

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
  const exportEvents = useMutation({
    mutationFn: () => api.exportOwnEvents(),
    onSuccess: (r) => push({ title: `${r.count} ${r.count === 1 ? "event" : "events"} exported`, description: r.path }),
    onError: (e) => push({ title: "Couldn't export your events", description: errorMessage(e), tone: "danger" }),
  });
  const copy = useMutation({
    mutationFn: async () => navigator.clipboard.writeText(await api.exportSettings()),
    onSuccess: () => push({ title: "Settings copied" }),
    onError: () => push({ title: "Couldn't copy to the clipboard", tone: "danger" }),
  });

  const sysCal = useQuery({ queryKey: ["system-status"], queryFn: () => api.systemCalendarStatus() });
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
      <main key={section ?? "home"} className="min-h-0 flex-1 overflow-y-auto px-5 pb-10 [&>*]:mx-auto [&>*]:w-full [&>*]:max-w-2xl">
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
            <TaskSyncGroup />
            <Group title="Event templates" footer="1:1, Interview, Standup and Kickoff are built in. Make your own with “Keep as a template” in the new-event form; a title that starts with a template's name (“1:1 with Sam”) picks it up.">
              {s.templates.length === 0 ? (
                <SettingRow title="Your templates" description="None yet." />
              ) : (
                s.templates.map((t) => (
                  <SettingRow key={t.name} title={t.name} description={`${t.minutes} min${t.agenda.trim() ? " · with an agenda" : ""}`}>
                    <Button size="sm" variant="plain" aria-label={`Delete template ${t.name}`} onClick={() => update((d) => void (d.templates = d.templates.filter((x) => x.name !== t.name)))}>
                      Delete
                    </Button>
                  </SettingRow>
                ))
              )}
            </Group>
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
                    { value: "month", label: "Month view" },
                  ]}
                />
              </SettingRow>
              <SettingRow title="Show in the Dock or taskbar" description="Off keeps Firstlight in the menu bar or system tray only. You can always open it from there or with your shortcut.">
                <Switch label="Show in the Dock or taskbar" checked={s.general.show_in_dock} onCheckedChange={(v) => update((d) => void (d.general.show_in_dock = v))} />
              </SettingRow>
              <SettingRow title="Show or hide the briefing" description="A shortcut that works from any app.">
                <ShortcutRecorder value={s.general.global_shortcut} onChange={(v) => update((d) => void (d.general.global_shortcut = v))} />
              </SettingRow>
              <SettingRow title="Quick capture" description="Opens a small box over any app: type a task or an event and press Return.">
                <ShortcutRecorder label="Quick capture shortcut" value={s.general.capture_shortcut} onChange={(v) => update((d) => void (d.general.capture_shortcut = v))} />
              </SettingRow>
              <SettingRow title="Join the next meeting" description="Opens the video link of the meeting that is starting soon or running.">
                <ShortcutRecorder label="Join shortcut" value={s.general.join_shortcut} onChange={(v) => update((d) => void (d.general.join_shortcut = v))} />
              </SettingRow>
              <SettingRow title="Next meeting in the menu bar" description="Shows its name and a countdown next to the icon. Anyone who can see your screen can read it.">
                <Switch label="Show the next meeting in the menu bar" checked={s.general.tray_next_meeting} onCheckedChange={(v) => update((d) => void (d.general.tray_next_meeting = v))} />
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
            <Group title="Other time zones" footer="Shown beside your own hours in the day and week views, and in Find a time. In Search, type “3pm in London” to see it in your time.">
              {[0, 1].map((i) => (
                <SettingRow key={i} title={i === 0 ? "First" : "Second"}>
                  <Select
                    label={i === 0 ? "First other time zone" : "Second other time zone"}
                    value={s.appearance.extra_zones[i] ?? "none"}
                    onValueChange={(v) =>
                      update((d) => {
                        const z = [...d.appearance.extra_zones];
                        if (v === "none") z.splice(i, 1);
                        else z[i] = v;
                        d.appearance.extra_zones = z.filter(Boolean).slice(0, 2);
                      })
                    }
                    options={[{ value: "none", label: "None" }, ...COMMON_ZONES.map((z) => ({ value: z, label: zoneLabel(z) }))]}
                  />
                </SettingRow>
              ))}
            </Group>
            <Group title="Look">
              <SettingRow title="Theme" description="Auto follows your computer's light or dark setting. The theme colours every screen, window and popup." stack>
                <ThemePicker value={s.appearance.theme} onChange={(v) => update((d) => void (d.appearance.theme = v))} />
              </SettingRow>
              <SettingRow title="Font" description="Uses fonts already on your computer; where one is missing, the closest one is used." stack>
                <FontPicker value={s.appearance.font} onChange={(v) => update((d) => void (d.appearance.font = v))} />
              </SettingRow>
              <SettingRow title="Text size" stack>
                <Segmented
                  label="Text size"
                  value={s.appearance.text_size}
                  onValueChange={(v) => update((d) => void (d.appearance.text_size = v as TextSize))}
                  options={[
                    { value: "small", label: "Small" },
                    { value: "default", label: "Normal" },
                    { value: "large", label: "Large" },
                    { value: "larger", label: "Larger" },
                  ]}
                />
              </SettingRow>
              <SettingRow title="Time-of-day background" description="A soft tint behind the app that changes from morning to night.">
                <Switch label="Time-of-day background" checked={s.appearance.ambient} onCheckedChange={(v) => update((d) => void (d.appearance.ambient = v))} />
              </SettingRow>
              <SettingRow title="Accent colour" description="Default follows your Mac's accent colour." stack>
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
            <Group title="On unlock" footer="Normally the first unlock of the day opens your briefing, and later unlocks only show a small reminder when a meeting is about to start. Turn on the first option to open it every time, including when the computer starts.">
              <SettingRow title="Open on every unlock and start" description="Quiet days, quiet hours and full screen still apply.">
                <Switch label="Open on every unlock and start" checked={s.unlock.open_every_time} onCheckedChange={(v) => update((d) => void (d.unlock.open_every_time = v))} />
              </SettingRow>
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
              <SettingRow title="While another app is full screen" description="Stay quiet during presentations, films and games.">
                <Switch label="Stay quiet while another app is full screen" checked={s.unlock.skip_when_fullscreen} onCheckedChange={(v) => update((d) => void (d.unlock.skip_when_fullscreen = v))} />
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
              {s.notify.evening && (
                <SettingRow title="Week preview on Sundays" description="On Sunday evening, a look at the whole week ahead in place of the note about tomorrow: how many meetings, the busiest day, early starts.">
                  <Switch label="Week preview on Sundays" checked={s.notify.week_preview} onCheckedChange={(v) => update((d) => void (d.notify.week_preview = v))} />
                </SettingRow>
              )}
              <SettingRow title="Task reminders" description="A notification when a task with a time comes due.">
                <Switch label="Task reminders" checked={s.day.task_reminders} onCheckedChange={(v) => update((d) => void (d.day.task_reminders = v))} />
              </SettingRow>
              <SettingRow title="Nudge reminders" description="When a task has waited on someone this long, a reminder to nudge them (weekdays, working hours).">
                <Select
                  label="Nudge reminders"
                  value={String(s.day.waiting_nudge_days)}
                  onValueChange={(v) => update((d) => void (d.day.waiting_nudge_days = Number(v)))}
                  options={[
                    { value: "0", label: "Never" },
                    { value: "2", label: "Every 2 days" },
                    { value: "3", label: "Every 3 days" },
                    { value: "5", label: "Every 5 days" },
                    { value: "7", label: "Every week" },
                  ]}
                />
              </SettingRow>
            </Group>
            <DndGroup />
            <Group title="Your day" footer="These follow your days and quiet hours, and wait while focus time runs.">
              <SettingRow title="Wrap up your day" description="At the end of the day: tasks done and left, and how tomorrow starts. The briefing then offers to move what is left to tomorrow.">
                <Switch label="Wrap up your day" checked={s.day.wrap_up} onCheckedChange={(v) => update((d) => void (d.day.wrap_up = v))} />
              </SettingRow>
              {s.day.wrap_up && (
                <SettingRow title="Wrap-up time">
                  <input
                    type="time"
                    aria-label="Wrap-up time"
                    value={`${String(s.day.wrap_up_hour).padStart(2, "0")}:${String(s.day.wrap_up_minute).padStart(2, "0")}`}
                    onChange={(e) => {
                      const [h, m] = e.target.value.split(":").map(Number);
                      if (Number.isFinite(h) && Number.isFinite(m)) update((d) => void ((d.day.wrap_up_hour = h!), (d.day.wrap_up_minute = m!)));
                    }}
                    className="h-8 rounded-control bg-fill px-2 text-body text-fg"
                  />
                </SettingRow>
              )}
              <SettingRow title="Break after long meetings" description="When a run of meetings with no real break ends, a nudge to step away.">
                <Switch label="Break after long meetings" checked={s.day.break_nudge} onCheckedChange={(v) => update((d) => void (d.day.break_nudge = v))} />
              </SettingRow>
              {s.day.break_nudge && (
                <SettingRow title="After">
                  <Select
                    label="Break after"
                    value={String(s.day.break_after_minutes)}
                    onValueChange={(v) => update((d) => void (d.day.break_after_minutes = Number(v)))}
                    options={[90, 120, 150, 180, 240].map((m) => ({ value: String(m), label: m % 60 ? `${Math.floor(m / 60)} h ${m % 60} min` : `${m / 60} hours` }))}
                  />
                </SettingRow>
              )}
              <SettingRow title="Room for lunch" description="On weekdays, the briefing warns when meetings leave too little time for lunch.">
                <Switch label="Room for lunch" checked={s.day.lunch_check} onCheckedChange={(v) => update((d) => void (d.day.lunch_check = v))} />
              </SettingRow>
              {s.day.lunch_check && (
                <SettingRow title="Lunch between">
                  <div className="flex items-center gap-1">
                    <Select label="Lunch from" value={String(s.day.lunch_from_hour)} onValueChange={(v) => update((d) => void (d.day.lunch_from_hour = Number(v)))} options={[11, 12, 13].map((h) => ({ value: String(h), label: `${h}:00` }))} />
                    <Select label="Lunch until" value={String(s.day.lunch_to_hour)} onValueChange={(v) => update((d) => void (d.day.lunch_to_hour = Number(v)))} options={[13, 14, 15].map((h) => ({ value: String(h), label: `${h}:00` }))} />
                  </div>
                </SettingRow>
              )}
              <SettingRow title="Follow up after meetings" description="After a meeting you wrote notes in: turn them into tasks or email a recap.">
                <Switch label="Follow up after meetings" checked={s.day.follow_ups} onCheckedChange={(v) => update((d) => void (d.day.follow_ups = v))} />
              </SettingRow>
              <SettingRow title="Travel time" description="For meetings held somewhere (not online): a “Leave now” notice and a warning when the meeting before leaves too little time. Your own estimate; no addresses are looked up.">
                <Select
                  label="Travel time"
                  value={String(s.day.travel_minutes)}
                  onValueChange={(v) => update((d) => void (d.day.travel_minutes = Number(v)))}
                  options={[0, 10, 15, 30, 45, 60, 90].map((m) => ({ value: String(m), label: m === 0 ? "Off" : `${m} min` }))}
                />
              </SettingRow>
              {mapsAvailable && (
                <SettingRow title="Trip times from Apple Maps" description="Instead of your own estimate, ask Apple Maps how long the drive takes (plus 5 minutes). Sends only the two places to Apple. Off by default.">
                  <Switch label="Trip times from Apple Maps" checked={s.day.travel_maps} onCheckedChange={(v) => update((d) => void (d.day.travel_maps = v))} />
                </SettingRow>
              )}
              {mapsAvailable && s.day.travel_maps && (
                <SettingRow title="First trip starts from" description="Your office, say. Later trips start from the meeting before." stack>
                  <TravelFrom value={s.day.travel_from} onSave={(v) => update((d) => void (d.day.travel_from = v))} />
                </SettingRow>
              )}
              <SettingRow title="Weekly review" description="On Friday afternoon: time in meetings next to last week, focus time and tasks done. Numbers only.">
                <Switch label="Weekly review" checked={s.day.weekly_review} onCheckedChange={(v) => update((d) => void (d.day.weekly_review = v))} />
              </SettingRow>
              <SettingRow title="Stop-work reminder" description="A note when your working hours end.">
                <Switch label="Stop-work reminder" checked={s.day.stop_work} onCheckedChange={(v) => update((d) => void (d.day.stop_work = v))} />
              </SettingRow>
            </Group>
          </div>
        )}
        {section === "email" && (
          <div className="flex flex-col gap-7">
            <EmailGroup />
            <SendingGroup />
          </div>
        )}
        {section === "ai" && <AiGroup />}
        {section === "timeoff" && <TimeOffSettings />}
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

            {sysCal.data?.supported && sysCal.data.writable && (
              <Group title="Your calendars">
                <SettingRow
                  title="Let Firstlight add events to the Calendar app"
                  description="Off by default. When on, the New event form can save into a calendar in your Calendar app, and only events Firstlight created there can be changed or removed. Anything already in your calendars stays untouched. The Mac asks your permission first."
                >
                  <Switch
                    label="Add events to the Calendar app"
                    checked={s.general.write_to_calendar_app}
                    onCheckedChange={(v) => update((d) => void (d.general.write_to_calendar_app = v))}
                  />
                </SettingRow>
              </Group>
            )}

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

            <Group title="Your data" footer="Firstlight has no server and no account. Your events stay on this computer, your passwords and sign-ins live in the system keychain, and nothing is sent anywhere except to your own calendar providers, and (only if you turn those on) your own mail provider, AI service and mailbox.">
              <SettingRow title="Export my own events" description="The events you added yourself, as a calendar file you can open in any calendar app.">
                <Button size="sm" variant="secondary" disabled={exportEvents.isPending} onClick={() => exportEvents.mutate()}>
                  Export…
                </Button>
              </SettingRow>
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
          placeholder='{ "format": "firstlight-settings", … }'
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

/** The starting place for the first trip of the day, saved when you leave the field. */
function TravelFrom({ value, onSave }: { value: string; onSave: (v: string) => void }) {
  const [text, setText] = useState(value);
  return (
    <input
      aria-label="First trip starts from"
      value={text}
      maxLength={200}
      placeholder="1 Main Street, London"
      onChange={(e) => setText(e.target.value)}
      onBlur={() => text.trim() !== value && onSave(text.trim())}
      className="h-9 w-full rounded-control bg-fill px-3 text-body text-fg placeholder:text-fg-3"
    />
  );
}
