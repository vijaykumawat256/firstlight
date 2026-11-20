//! The person's working day beyond meetings: task reminders, the end-of-day wrap-up, a nudge to take a break after a
//! long run of meetings, a warning when meetings leave no room for lunch, and an optional stop-work reminder.
//! Settings are part of the one settings document (`settings.day`). Everything here is pure and tested; the notifier
//! shows what [`due`] returns and records it so nothing repeats.

use crate::briefing::notify::{Kind, Notification, NotifySettings, fmt_time};
use crate::briefing::rules::{UnlockRules, in_quiet_hours, weekday_index};
use crate::briefing::{BriefingSettings, build, day_bounds, is_shown};
use crate::model::{Event, When};
use crate::tasks::TaskLists;
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, Timelike, Utc, Weekday};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Meetings closer together than this count as one run (no real break between them).
const BREAK_GAP_MINUTES: i64 = 10;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DaySettings {
    /// Notify when a task with a time comes due.
    pub task_reminders: bool,
    /// A note at the end of the day: tasks done and left, and how tomorrow starts.
    pub wrap_up: bool,
    pub wrap_up_hour: u32,
    pub wrap_up_minute: u32,
    /// After this long in meetings with no real break, suggest one when the run ends.
    pub break_nudge: bool,
    pub break_after_minutes: i64,
    /// Warn when meetings leave less than `lunch_minutes` free between the two hours (weekdays).
    pub lunch_check: bool,
    pub lunch_from_hour: u32,
    pub lunch_to_hour: u32,
    pub lunch_minutes: i64,
    /// A reminder at the end of working hours (the briefing's work end hour).
    pub stop_work: bool,
    /// After a meeting you wrote notes in: a nudge to turn them into tasks or send a recap.
    pub follow_ups: bool,
    /// Minutes to get to a meeting held somewhere (not online). 0 = off. Nothing is looked up: it is your estimate.
    pub travel_minutes: i64,
    /// On Friday afternoon, a look back at the week: meeting time, focus time, tasks done.
    pub weekly_review: bool,
    /// Ask Apple Maps how long the trip takes (Mac only). Sends the two addresses to Apple. Off by default.
    pub travel_maps: bool,
    /// Where the first trip of the day starts (your office, say), for Apple Maps.
    pub travel_from: String,
    /// Remind you to nudge someone a task waits on every this many days. 0 = never.
    pub waiting_nudge_days: u32,
    /// Turn on the computer's Do Not Disturb during meetings and focus time (Mac: through two Shortcuts the person
    /// makes). Off by default.
    pub dnd_meetings: bool,
    /// Days off. Summaries, wrap-up and nudges stay quiet on them; meetings on them are pointed out.
    pub time_off: Vec<TimeOff>,
}

/// A stretch of days off (both days included).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeOff {
    pub from: NaiveDate,
    pub to: NaiveDate,
    /// "Holiday", "Conference"... at most 60 characters.
    #[serde(default)]
    pub label: String,
}

/// Most stretches of time off kept.
pub const MAX_TIME_OFF: usize = 30;

impl DaySettings {
    /// Forgets stretches of time off that ended before `today` (done on every save).
    pub fn drop_past_time_off(&mut self, today: NaiveDate) {
        self.time_off.retain(|t| t.to >= today);
    }

    /// Whether task reminders may show now: turned on, no focus time or timer running, not a day off.
    pub fn task_reminders_now(&self, focusing: bool, today: NaiveDate) -> bool {
        self.task_reminders && !focusing && self.off_on(today).is_none()
    }

    /// The stretch of time off that covers `day`, if any.
    pub fn off_on(&self, day: NaiveDate) -> Option<&TimeOff> {
        self.time_off.iter().find(|t| t.from <= day && day <= t.to)
    }
}

impl Default for DaySettings {
    fn default() -> Self {
        Self {
            task_reminders: true,
            wrap_up: true,
            wrap_up_hour: 17,
            wrap_up_minute: 30,
            break_nudge: false,
            break_after_minutes: 150,
            lunch_check: true,
            lunch_from_hour: 12,
            lunch_to_hour: 14,
            lunch_minutes: 30,
            stop_work: false,
            follow_ups: true,
            travel_minutes: 0,
            weekly_review: true,
            travel_maps: false,
            travel_from: String::new(),
            waiting_nudge_days: 3,
            dnd_meetings: false,
            time_off: vec![],
        }
    }
}

impl DaySettings {
    /// Pulls every value into its allowed range.
    pub fn sanitize(&mut self) {
        let d = Self::default();
        if self.wrap_up_hour > 23 || self.wrap_up_minute > 59 {
            self.wrap_up_hour = d.wrap_up_hour;
            self.wrap_up_minute = d.wrap_up_minute;
        }
        self.break_after_minutes = self.break_after_minutes.clamp(60, 480);
        if self.lunch_from_hour >= self.lunch_to_hour || self.lunch_to_hour > 23 {
            self.lunch_from_hour = d.lunch_from_hour;
            self.lunch_to_hour = d.lunch_to_hour;
        }
        self.lunch_minutes = self.lunch_minutes.clamp(10, 120);
        self.travel_minutes = self.travel_minutes.clamp(0, 180);
        let from: String = self
            .travel_from
            .chars()
            .filter(|c| !c.is_control())
            .take(200)
            .collect();
        self.travel_from = from.trim().to_string();
        self.waiting_nudge_days = self.waiting_nudge_days.min(30);
        // Time off: real stretches (to on or after from, at most a year long), tidy labels, no repeats, by date.
        for t in &mut self.time_off {
            let label: String = t
                .label
                .chars()
                .filter(|c| !c.is_control())
                .take(60)
                .collect();
            t.label = label.split_whitespace().collect::<Vec<_>>().join(" ");
        }
        self.time_off
            .retain(|t| t.to >= t.from && (t.to - t.from).num_days() <= 366);
        self.time_off.sort_by_key(|t| (t.from, t.to));
        self.time_off
            .dedup_by(|a, b| a.from == b.from && a.to == b.to);
        // Over the limit: keep the latest stretches (the ones still to come), never drop future days off.
        if self.time_off.len() > MAX_TIME_OFF {
            let extra = self.time_off.len() - MAX_TIME_OFF;
            self.time_off.drain(..extra);
        }
    }
}

/// Whether a meeting you are in (timed, shown: not cancelled or declined) is running at `now`.
pub fn in_meeting_now(events: &[Event], now: DateTime<Utc>, bs: &BriefingSettings) -> bool {
    events
        .iter()
        .filter(|e| is_shown(e, bs))
        .any(|e| matches!(e.when, When::Timed { start, end, .. } if start <= now && now < end))
}

/// Nudges for tasks waiting on someone, due on weekdays inside working hours (so they never arrive at night), one per
/// round of `waiting_nudge_days`.
pub fn waiting_due(
    tasks: &[crate::tasks::Task],
    now: DateTime<Utc>,
    tz: Tz,
    ds: &DaySettings,
    bs: &BriefingSettings,
    hide_titles: bool,
    sent: &HashSet<String>,
) -> Vec<Notification> {
    let local = now.with_timezone(&tz);
    if matches!(local.weekday(), Weekday::Sat | Weekday::Sun)
        || local.hour() < bs.work_start_hour
        || local.hour() >= bs.work_end_hour
    {
        return vec![];
    }
    crate::tasks::waiting_nudges(tasks, now, ds.waiting_nudge_days)
        .into_iter()
        .filter(|n| !sent.contains(&n.id))
        .map(|n| Notification {
            id: n.id,
            kind: Kind::Waiting,
            title: format!("Still waiting on {}", n.who),
            body: if hide_titles {
                format!("A task has waited {} days. A quick nudge?", n.days)
            } else {
                format!("“{}” has waited {} days. A quick nudge?", n.title, n.days)
            },
        })
        .collect()
}

/// Timed, shown events (not all-day, not cancelled or declined) touching `[from, to)`, as start-sorted spans.
fn busy(
    events: &[Event],
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    bs: &BriefingSettings,
) -> Vec<(DateTime<Utc>, DateTime<Utc>, String)> {
    let mut out: Vec<_> = events
        .iter()
        .filter(|e| is_shown(e, bs))
        .filter_map(|e| match e.when {
            When::Timed { start, end, .. } if start < to && end > from => {
                Some((start, end, e.title.clone()))
            }
            _ => None,
        })
        .collect();
    out.sort_by_key(|(s, _, _)| *s);
    out
}

/// "Tomorrow starts at 9:00 with Standup." (or without the title, or "Nothing in your calendar tomorrow.").
fn tomorrow_line(
    events: &[Event],
    now: DateTime<Utc>,
    tz: Tz,
    bs: &BriefingSettings,
    ns: &NotifySettings,
) -> String {
    let b = build(events, now, tz, bs);
    match b.tomorrow.timed.first() {
        None => "Nothing in your calendar tomorrow.".into(),
        Some(i) if ns.hide_titles => {
            format!("Tomorrow starts at {}.", fmt_time(i.start, tz, ns.time_24h))
        }
        Some(i) => format!(
            "Tomorrow starts at {} with {}.",
            fmt_time(i.start, tz, ns.time_24h),
            i.event.title
        ),
    }
}

/// Whether the wrap-up moment has come today (the briefing shows its wrap-up card from then until midnight).
pub fn wrap_up_time(now: DateTime<Utc>, tz: Tz, ds: &DaySettings) -> bool {
    let local = now.with_timezone(&tz);
    ds.wrap_up && (local.hour(), local.minute()) >= (ds.wrap_up_hour, ds.wrap_up_minute)
}

/// The day's notifications that are due now and were not shown yet: wrap-up, break and stop-work. They follow the
/// person's days and quiet hours like the morning and evening summaries.
#[allow(clippy::too_many_arguments)] // the inputs are all distinct facts about the day
pub fn due(
    events: &[Event],
    tasks: &TaskLists,
    now: DateTime<Utc>,
    tz: Tz,
    ds: &DaySettings,
    bs: &BriefingSettings,
    rules: &UnlockRules,
    ns: &NotifySettings,
    sent: &HashSet<String>,
) -> Vec<Notification> {
    due_with_trips(
        events,
        tasks,
        now,
        tz,
        ds,
        bs,
        rules,
        ns,
        sent,
        &HashMap::new(),
    )
}

/// [`due`], with trip times looked up for some meetings (meeting id to minutes), which replace your own estimate.
#[allow(clippy::too_many_arguments)] // the inputs are all distinct facts about the day
pub fn due_with_trips(
    events: &[Event],
    tasks: &TaskLists,
    now: DateTime<Utc>,
    tz: Tz,
    ds: &DaySettings,
    bs: &BriefingSettings,
    rules: &UnlockRules,
    ns: &NotifySettings,
    sent: &HashSet<String>,
    looked_up: &HashMap<String, i64>,
) -> Vec<Notification> {
    let local = now.with_timezone(&tz);
    let today = local.date_naive();
    if !rules.days[weekday_index(local.weekday())]
        || in_quiet_hours(local.hour(), rules.quiet_hours)
    {
        return vec![];
    }
    let mut out = Vec::new();

    if wrap_up_time(now, tz, ds) && local.hour() < 23 {
        let id = format!("wrapup:{today}");
        if !sent.contains(&id) {
            let left = tasks.today.len();
            let tasks_part = match (tasks.done_today, left) {
                (0, 0) => String::new(),
                (done, left) => format!("{done} done, {left} left. "),
            };
            out.push(Notification {
                id,
                kind: Kind::WrapUp,
                title: "Wrap up your day".into(),
                body: format!("{tasks_part}{}", tomorrow_line(events, now, tz, bs, ns)),
            });
        }
    }

    if ds.stop_work
        && local.hour() >= bs.work_end_hour
        && local.hour() < (bs.work_end_hour + 2).min(23)
    {
        let id = format!("stop:{today}");
        if !sent.contains(&id) {
            out.push(Notification {
                id,
                kind: Kind::StopWork,
                title: "Time to stop for today".into(),
                body: tomorrow_line(events, now, tz, bs, ns),
            });
        }
    }

    if ds.break_nudge
        && let Some(n) = break_nudge(events, now, tz, ds, bs, ns)
        && !sent.contains(&n.id)
    {
        out.push(n);
    }
    out.extend(
        leave_now(events, now, tz, ds, bs, ns, looked_up)
            .into_iter()
            .filter(|n| !sent.contains(&n.id)),
    );
    out
}

/// Margin added to a trip time from Apple Maps (parking, finding the room).
pub const MAPS_MARGIN_MINUTES: i64 = 5;

/// The trip time for one meeting: the one looked up (plus a margin), else your own estimate; 0 = nothing to plan.
fn trip_minutes(e: &Event, ds: &DaySettings, looked_up: &HashMap<String, i64>) -> i64 {
    looked_up
        .get(&e.id)
        .map(|m| m + MAPS_MARGIN_MINUTES)
        .unwrap_or(ds.travel_minutes)
}

/// Trips worth looking up today: (meeting id, where from, where to). From the in-person meeting before it (if it ends
/// before this one starts and is somewhere else), otherwise from `travel_from`. Nothing when looking up is off.
pub fn trips(
    events: &[Event],
    now: DateTime<Utc>,
    tz: Tz,
    ds: &DaySettings,
    bs: &BriefingSettings,
) -> Vec<(String, String, String)> {
    if !ds.travel_maps {
        return vec![];
    }
    let (from, to) = day_bounds(now.with_timezone(&tz).date_naive(), tz);
    let mut todays: Vec<(&Event, DateTime<Utc>, DateTime<Utc>)> = events
        .iter()
        .filter(|e| is_shown(e, bs) && in_person(e))
        .filter_map(|e| match e.when {
            When::Timed { start, end, .. } if start < to && end > from => Some((e, start, end)),
            _ => None,
        })
        .collect();
    todays.sort_by_key(|(_, s, _)| *s);
    let mut out = Vec::new();
    for (i, (e, start, _)) in todays.iter().enumerate() {
        if *start <= now {
            continue;
        }
        let place = e.location.as_deref().unwrap_or_default().trim().to_string();
        let before = todays[..i]
            .iter()
            .filter(|(_, _, end)| *end <= *start)
            .max_by_key(|(_, _, end)| *end);
        let origin = match before {
            Some((p, _, _)) => p.location.as_deref().unwrap_or_default().trim().to_string(),
            None => ds.travel_from.clone(),
        };
        if !origin.is_empty() && origin.to_lowercase() != place.to_lowercase() {
            out.push((e.id.clone(), origin, place));
        }
    }
    out
}

/// Words in a place that mean the meeting is not somewhere you travel to.
const ONLINE_WORDS: [&str; 10] = [
    "zoom",
    "teams",
    "meet.google",
    "webex",
    "online",
    "virtual",
    "call",
    "phone",
    "http",
    "remote",
];

/// A meeting you go to: it has a place, no video link, and the place does not read as online.
pub fn in_person(e: &Event) -> bool {
    let Some(place) = e
        .location
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    else {
        return false;
    };
    let lower = place.to_lowercase();
    e.join_url.is_none() && !ONLINE_WORDS.iter().any(|w| lower.contains(w))
}

/// "Leave now": when the travel time before an in-person meeting begins (for ten minutes, if the app was asleep).
/// Comes regardless of quiet hours only if the summaries could (it is part of `due`, which checks them).
pub fn leave_now(
    events: &[Event],
    now: DateTime<Utc>,
    tz: Tz,
    ds: &DaySettings,
    bs: &BriefingSettings,
    ns: &NotifySettings,
    looked_up: &HashMap<String, i64>,
) -> Vec<Notification> {
    events
        .iter()
        .filter(|e| is_shown(e, bs) && in_person(e))
        .filter_map(|e| {
            let minutes = trip_minutes(e, ds, looked_up);
            let travel = Duration::minutes(minutes);
            match e.when {
                When::Timed { start, .. }
                    if minutes > 0
                        && start - travel <= now
                        && now < start - travel + Duration::minutes(10)
                        && now < start =>
                {
                    Some((e, start, minutes))
                }
                _ => None,
            }
        })
        .map(|(e, start, minutes)| Notification {
            id: format!("leave:{}:{}", e.id, start.timestamp()),
            kind: Kind::Leave,
            title: if ns.hide_titles {
                "Time to leave".into()
            } else {
                format!("Leave now for {}", e.title)
            },
            body: if ns.hide_titles {
                format!(
                    "Your next meeting starts at {}.",
                    fmt_time(start, tz, ns.time_24h)
                )
            } else {
                format!(
                    "Starts at {} at {}. {}",
                    fmt_time(start, tz, ns.time_24h),
                    e.location.as_deref().unwrap_or_default().trim(),
                    if looked_up.contains_key(&e.id) {
                        format!("About {minutes} minutes to get there.")
                    } else {
                        format!("You allow {minutes} minutes to get there.")
                    }
                )
            },
        })
        .collect()
}

/// An in-person meeting today that the meeting before it leaves too little time to get to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TravelWarning {
    pub event_id: String,
    pub title: String,
    pub start: DateTime<Utc>,
    pub previous_title: String,
    /// Minutes between the end of the meeting before and the start of this one.
    pub gap_minutes: i64,
}

/// Today's upcoming in-person meetings that start less than the travel time after the meeting before them ends (when
/// that one is somewhere else). Nothing when travel time is off.
pub fn travel_warnings(
    events: &[Event],
    now: DateTime<Utc>,
    tz: Tz,
    ds: &DaySettings,
    bs: &BriefingSettings,
    looked_up: &HashMap<String, i64>,
) -> Vec<TravelWarning> {
    let (from, to) = day_bounds(now.with_timezone(&tz).date_naive(), tz);
    let mut timed: Vec<(&Event, DateTime<Utc>, DateTime<Utc>)> = events
        .iter()
        .filter(|e| is_shown(e, bs))
        .filter_map(|e| match e.when {
            When::Timed { start, end, .. } if start < to && end > from => Some((e, start, end)),
            _ => None,
        })
        .collect();
    timed.sort_by_key(|(_, s, _)| *s);
    let mut out = Vec::new();
    for (i, (e, start, _)) in timed.iter().enumerate() {
        if *start <= now || !in_person(e) {
            continue;
        }
        let Some((prev, _, prev_end)) = timed[..i]
            .iter()
            .filter(|(_, _, end)| *end <= *start)
            .max_by_key(|(_, _, end)| *end)
        else {
            continue;
        };
        let same_place = prev.location.as_deref().map(|l| l.trim().to_lowercase())
            == e.location.as_deref().map(|l| l.trim().to_lowercase());
        let gap = (*start - *prev_end).num_minutes();
        let need = trip_minutes(e, ds, looked_up);
        if !same_place && need > 0 && gap < need {
            out.push(TravelWarning {
                event_id: e.id.clone(),
                title: e.title.clone(),
                start: *start,
                previous_title: prev.title.clone(),
                gap_minutes: gap,
            });
        }
    }
    out
}

/// A break suggestion when a run of meetings of at least `break_after_minutes` (gaps under 10 minutes do not count as
/// a break) has just ended, for the first ten minutes of the gap after it.
fn break_nudge(
    events: &[Event],
    now: DateTime<Utc>,
    tz: Tz,
    ds: &DaySettings,
    bs: &BriefingSettings,
    ns: &NotifySettings,
) -> Option<Notification> {
    let (from, to) = day_bounds(now.with_timezone(&tz).date_naive(), tz);
    let spans = busy(events, from, to, bs);
    let gap = Duration::minutes(BREAK_GAP_MINUTES);
    let mut runs: Vec<(DateTime<Utc>, DateTime<Utc>)> = Vec::new();
    for (s, e, _) in &spans {
        match runs.last_mut() {
            Some(last) if *s < last.1 + gap => last.1 = last.1.max(*e),
            _ => runs.push((*s, *e)),
        }
    }
    let (start, end) = runs.into_iter().find(|(s, e)| {
        *e <= now && now < *e + gap && *e - *s >= Duration::minutes(ds.break_after_minutes)
    })?;
    let next = spans.iter().map(|(s, _, _)| *s).find(|s| *s >= end);
    let free = match next {
        Some(n) => format!("Free until {}.", fmt_time(n, tz, ns.time_24h)),
        None => "Nothing else in your calendar today.".into(),
    };
    Some(Notification {
        id: format!("break:{}", end.timestamp()),
        kind: Kind::Break,
        title: "Time for a break".into(),
        body: format!(
            "In meetings since {}. {free}",
            fmt_time(start, tz, ns.time_24h)
        ),
    })
}

/// Meetings leave too little room for lunch today.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LunchWarning {
    /// The lunch window, so the page can say "between 12:00 and 14:00".
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    /// All free minutes left in the window.
    pub free_minutes: i64,
    /// The longest free stretch left, if it is at least 15 minutes (it can be blocked for lunch).
    pub longest: Option<(DateTime<Utc>, DateTime<Utc>)>,
}

/// On a weekday, before the lunch window ends: a warning when no free stretch in it is as long as `lunch_minutes`.
/// An event with "lunch" in its title inside the window means lunch is taken care of.
pub fn lunch_check(
    events: &[Event],
    now: DateTime<Utc>,
    tz: Tz,
    ds: &DaySettings,
    bs: &BriefingSettings,
) -> Option<LunchWarning> {
    let local = now.with_timezone(&tz);
    if !ds.lunch_check || matches!(local.weekday(), Weekday::Sat | Weekday::Sun) {
        return None;
    }
    let date: NaiveDate = local.date_naive();
    let at = |h: u32| {
        crate::tz::local_to_utc(
            tz,
            date.and_time(NaiveTime::from_hms_opt(h, 0, 0).unwrap_or_default()),
        )
    };
    let (from, to) = (at(ds.lunch_from_hour), at(ds.lunch_to_hour));
    if now >= to {
        return None;
    }
    let spans = busy(events, from, to, bs);
    if spans
        .iter()
        .any(|(_, _, t)| t.to_lowercase().contains("lunch"))
    {
        return None;
    }
    let mut free: Vec<(DateTime<Utc>, DateTime<Utc>)> = Vec::new();
    let mut cursor = from;
    for (s, e, _) in &spans {
        if *s > cursor {
            free.push((cursor, (*s).min(to)));
        }
        cursor = cursor.max(*e);
    }
    if cursor < to {
        free.push((cursor, to));
    }
    let need = Duration::minutes(ds.lunch_minutes);
    if free.iter().any(|(s, e)| *e - *s >= need) {
        return None;
    }
    let free_minutes = free.iter().map(|(s, e)| (*e - *s).num_minutes()).sum();
    let longest = free
        .iter()
        .copied()
        .max_by_key(|(s, e)| *e - *s)
        .filter(|(s, e)| *e - *s >= Duration::minutes(15) && *e > now);
    Some(LunchWarning {
        from,
        to,
        free_minutes,
        longest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use crate::tasks::{Task, TaskView};

    const NY: Tz = chrono_tz::America::New_York;

    fn t(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn ev(id: &str, title: &str, start: &str, end: &str) -> Event {
        Event {
            id: id.into(),
            calendar_id: "cal".into(),
            remote_id: id.into(),
            ical_uid: id.into(),
            title: title.into(),
            when: When::Timed {
                start: t(start),
                end: t(end),
                tzid: None,
            },
            location: None,
            description: None,
            join_url: None,
            status: EventStatus::Confirmed,
            my_response: Response::Accepted,
            organizer: None,
            recurrence_master_id: None,
            updated_at: t(start),
            also_in: vec![],
            attendees: vec![],
        }
    }

    fn view(id: &str) -> TaskView {
        TaskView {
            task: Task {
                id: id.into(),
                title: id.into(),
                notes: None,
                due: None,
                minutes: None,
                done_at: None,
                created_at: t("2026-11-01T00:00:00Z"),
                updated_at: t("2026-11-01T00:00:00Z"),
                from_meeting: None,
                planned_event: None,
                link: None,
                deleted_at: None,
                ..Default::default()
            },
            days_late: 0,
        }
    }

    fn all_on() -> DaySettings {
        DaySettings {
            break_nudge: true,
            stop_work: true,
            ..Default::default()
        }
    }

    fn run(
        events: &[Event],
        tasks: &TaskLists,
        now: &str,
        sent: &HashSet<String>,
    ) -> Vec<Notification> {
        due(
            events,
            tasks,
            t(now),
            NY,
            &all_on(),
            &BriefingSettings::default(),
            &UnlockRules::default(),
            &NotifySettings::default(),
            sent,
        )
    }

    // Wednesday 18 November 2026; New York is UTC-5.
    #[test]
    fn the_wrap_up_counts_tasks_and_says_how_tomorrow_starts() {
        let events = [ev(
            "s",
            "Standup",
            "2026-11-19T14:00:00Z",
            "2026-11-19T14:15:00Z",
        )];
        let tasks = TaskLists {
            today: vec![view("a"), view("b")],
            done_today: 3,
            ..Default::default()
        };
        assert!(run(&events, &tasks, "2026-11-18T22:29:00Z", &HashSet::new()).is_empty()); // 17:29
        let due = run(&events, &tasks, "2026-11-18T22:30:00Z", &HashSet::new());
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, "wrapup:2026-11-18");
        assert_eq!(
            due[0].body,
            "3 done, 2 left. Tomorrow starts at 9:00 AM with Standup."
        );
        let sent: HashSet<String> = [due[0].id.clone()].into();
        assert!(run(&events, &tasks, "2026-11-18T22:45:00Z", &sent).is_empty());
        // No tasks and nothing tomorrow: still a short note.
        let quiet = run(
            &[],
            &TaskLists::default(),
            "2026-11-18T22:30:00Z",
            &HashSet::new(),
        );
        assert_eq!(quiet[0].body, "Nothing in your calendar tomorrow.");
    }

    #[test]
    fn titles_stay_out_when_asked() {
        let events = [ev(
            "s",
            "Secret project",
            "2026-11-19T14:00:00Z",
            "2026-11-19T14:15:00Z",
        )];
        let ns = NotifySettings {
            hide_titles: true,
            ..Default::default()
        };
        let due = due(
            &events,
            &TaskLists::default(),
            t("2026-11-18T22:30:00Z"),
            NY,
            &DaySettings::default(),
            &BriefingSettings::default(),
            &UnlockRules::default(),
            &ns,
            &HashSet::new(),
        );
        assert_eq!(due[0].body, "Tomorrow starts at 9:00 AM.");
    }

    #[test]
    fn stop_work_comes_at_the_end_of_working_hours_only_when_turned_on() {
        let stop = run(
            &[],
            &TaskLists::default(),
            "2026-11-18T23:05:00Z",
            &["wrapup:2026-11-18".to_string()].into(),
        );
        assert_eq!(
            stop.iter().map(|n| n.kind).collect::<Vec<_>>(),
            [Kind::StopWork]
        );
        let off = due(
            &[],
            &TaskLists::default(),
            t("2026-11-18T23:05:00Z"),
            NY,
            &DaySettings::default(),
            &BriefingSettings::default(),
            &UnlockRules::default(),
            &NotifySettings::default(),
            &["wrapup:2026-11-18".to_string()].into(),
        );
        assert!(off.is_empty());
    }

    #[test]
    fn days_off_and_quiet_hours_are_respected() {
        let mut rules = UnlockRules::default();
        rules.days[2] = false; // Wednesday
        let none = due(
            &[],
            &TaskLists::default(),
            t("2026-11-18T22:30:00Z"),
            NY,
            &all_on(),
            &BriefingSettings::default(),
            &rules,
            &NotifySettings::default(),
            &HashSet::new(),
        );
        assert!(none.is_empty());
    }

    #[test]
    fn a_break_is_suggested_when_a_long_run_of_meetings_ends() {
        // 9:00-10:00, 10:05-11:00, 11:00-11:40 (gaps under ten minutes), then free until 12:30.
        let events = [
            ev("a", "A", "2026-11-18T14:00:00Z", "2026-11-18T15:00:00Z"),
            ev("b", "B", "2026-11-18T15:05:00Z", "2026-11-18T16:00:00Z"),
            ev("c", "C", "2026-11-18T16:00:00Z", "2026-11-18T16:40:00Z"),
            ev("d", "D", "2026-11-18T17:30:00Z", "2026-11-18T18:00:00Z"),
        ];
        let at = |now: &str| run(&events, &TaskLists::default(), now, &HashSet::new());
        assert!(at("2026-11-18T16:30:00Z").is_empty()); // still in the run
        let n = at("2026-11-18T16:42:00Z");
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].body, "In meetings since 9:00 AM. Free until 12:30 PM.");
        assert!(at("2026-11-18T16:55:00Z").is_empty()); // more than ten minutes into the gap
        // A short run (only D) never asks for a break.
        assert!(at("2026-11-18T18:02:00Z").is_empty());
    }

    #[test]
    fn lunch_is_flagged_only_when_no_stretch_is_long_enough() {
        let ds = DaySettings::default();
        let bs = BriefingSettings::default();
        let now = t("2026-11-18T15:00:00Z"); // 10:00
        // 12:00-12:45 and 13:05-14:00 leave 20 minutes.
        let squeezed = [
            ev(
                "a",
                "Planning",
                "2026-11-18T17:00:00Z",
                "2026-11-18T17:45:00Z",
            ),
            ev(
                "b",
                "Review",
                "2026-11-18T18:05:00Z",
                "2026-11-18T19:00:00Z",
            ),
        ];
        let w = lunch_check(&squeezed, now, NY, &ds, &bs).unwrap();
        assert_eq!(w.free_minutes, 20);
        assert_eq!(
            w.longest,
            Some((t("2026-11-18T17:45:00Z"), t("2026-11-18T18:05:00Z")))
        );
        // 45 free minutes in a row: fine.
        let fine = [ev(
            "a",
            "Planning",
            "2026-11-18T17:00:00Z",
            "2026-11-18T18:15:00Z",
        )];
        assert_eq!(lunch_check(&fine, now, NY, &ds, &bs), None);
        // A lunch already in the calendar: fine.
        let booked = [
            squeezed[0].clone(),
            ev(
                "l",
                "Team lunch",
                "2026-11-18T18:05:00Z",
                "2026-11-18T19:00:00Z",
            ),
        ];
        assert_eq!(lunch_check(&booked, now, NY, &ds, &bs), None);
        // After the window, or on Saturday, nothing.
        assert_eq!(
            lunch_check(&squeezed, t("2026-11-18T19:00:00Z"), NY, &ds, &bs),
            None
        );
        assert_eq!(
            lunch_check(&squeezed, t("2026-11-21T15:00:00Z"), NY, &ds, &bs),
            None
        );
    }

    #[test]
    fn travel_time_warns_about_tight_gaps_and_says_when_to_leave() {
        let ds = DaySettings {
            travel_minutes: 30,
            ..Default::default()
        };
        let bs = BriefingSettings::default();
        let mut online = ev(
            "a",
            "Standup",
            "2026-11-18T14:00:00Z",
            "2026-11-18T14:45:00Z",
        );
        online.join_url = Some("https://zoom.us/j/1".into());
        let mut offsite = ev(
            "b",
            "Offsite",
            "2026-11-18T15:00:00Z",
            "2026-11-18T16:00:00Z",
        );
        offsite.location = Some("Granary Square".into());
        let mut lunch = ev("c", "Lunch", "2026-11-18T16:10:00Z", "2026-11-18T17:00:00Z");
        lunch.location = Some("granary square ".into()); // same place: no travel
        let mut phone = ev(
            "d",
            "Vendor",
            "2026-11-18T17:05:00Z",
            "2026-11-18T17:30:00Z",
        );
        phone.location = Some("Phone call".into());
        let events = [online, offsite, lunch, phone];
        let none = HashMap::new();
        let w = travel_warnings(&events, t("2026-11-18T13:00:00Z"), NY, &ds, &bs, &none);
        assert_eq!(w.len(), 1);
        assert_eq!(
            (
                w[0].title.as_str(),
                w[0].previous_title.as_str(),
                w[0].gap_minutes
            ),
            ("Offsite", "Standup", 15)
        );
        assert!(
            travel_warnings(
                &events,
                t("2026-11-18T13:00:00Z"),
                NY,
                &DaySettings::default(),
                &bs,
                &none
            )
            .is_empty()
        );

        let at = |now: &str| {
            leave_now(
                &events,
                t(now),
                NY,
                &ds,
                &bs,
                &NotifySettings::default(),
                &none,
            )
        };
        assert!(at("2026-11-18T14:29:00Z").is_empty());
        let n = at("2026-11-18T14:30:00Z");
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].title, "Leave now for Offsite");
        assert_eq!(
            n[0].body,
            "Starts at 10:00 AM at Granary Square. You allow 30 minutes to get there."
        );
        assert!(at("2026-11-18T14:41:00Z").is_empty());
    }

    #[test]
    fn looked_up_trip_times_replace_the_estimate_and_trips_start_from_the_last_place() {
        let ds = DaySettings {
            travel_maps: true,
            travel_from: "1 Main St".into(),
            ..Default::default()
        };
        let bs = BriefingSettings::default();
        let mut a = ev(
            "a",
            "Client",
            "2026-11-18T15:00:00Z",
            "2026-11-18T16:00:00Z",
        );
        a.location = Some("Granary Square".into());
        let mut b = ev("b", "Lunch", "2026-11-18T16:30:00Z", "2026-11-18T17:30:00Z");
        b.location = Some("Borough Market".into());
        let events = [a, b];
        let trips = trips(&events, t("2026-11-18T13:00:00Z"), NY, &ds, &bs);
        assert_eq!(
            trips,
            [
                (
                    "a".to_string(),
                    "1 Main St".to_string(),
                    "Granary Square".to_string()
                ),
                (
                    "b".to_string(),
                    "Granary Square".to_string(),
                    "Borough Market".to_string()
                ),
            ]
        );
        // 40 minutes by Apple Maps (+5) for a 30-minute gap: tight, even with no estimate of your own.
        let looked_up: HashMap<String, i64> = [("b".to_string(), 40)].into();
        let w = travel_warnings(&events, t("2026-11-18T13:00:00Z"), NY, &ds, &bs, &looked_up);
        assert_eq!(
            w.iter().map(|w| w.title.as_str()).collect::<Vec<_>>(),
            ["Lunch"]
        );
        let n = leave_now(
            &events,
            t("2026-11-18T15:45:00Z"),
            NY,
            &ds,
            &bs,
            &NotifySettings::default(),
            &looked_up,
        );
        assert_eq!(
            n[0].body,
            "Starts at 11:30 AM at Borough Market. About 45 minutes to get there."
        );
        assert!(
            super::trips(
                &events,
                t("2026-11-18T13:00:00Z"),
                NY,
                &DaySettings::default(),
                &bs
            )
            .is_empty()
        );
    }

    #[test]
    fn bad_settings_are_pulled_back() {
        let mut ds = DaySettings {
            wrap_up_hour: 30,
            break_after_minutes: 5,
            lunch_from_hour: 15,
            lunch_to_hour: 12,
            lunch_minutes: 500,
            ..Default::default()
        };
        ds.sanitize();
        assert_eq!(
            (
                ds.wrap_up_hour,
                ds.break_after_minutes,
                ds.lunch_from_hour,
                ds.lunch_minutes
            ),
            (17, 60, 12, 120)
        );
    }

    #[test]
    fn waiting_nudges_come_only_on_weekdays_in_working_hours_and_once_per_round() {
        let mut t = crate::tasks::Task {
            id: "w".into(),
            title: "Budget numbers".into(),
            waiting_on: Some("Priya".into()),
            waiting_since: Some("2026-11-13T15:00:00Z".parse().unwrap()),
            ..Default::default()
        };
        let ds = DaySettings::default();
        let bs = BriefingSettings::default();
        let thu_11am: DateTime<Utc> = "2026-11-19T16:00:00Z".parse().unwrap();
        let n = waiting_due(
            std::slice::from_ref(&t),
            thu_11am,
            NY,
            &ds,
            &bs,
            false,
            &HashSet::new(),
        );
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].kind, Kind::Waiting);
        assert!(n[0].body.contains("Budget numbers") && n[0].title.contains("Priya"));
        let sent: HashSet<String> = [n[0].id.clone()].into();
        assert!(
            waiting_due(
                std::slice::from_ref(&t),
                thu_11am,
                NY,
                &ds,
                &bs,
                false,
                &sent
            )
            .is_empty()
        );
        // Not at night, not at the weekend; titles hidden when asked.
        let thu_11pm: DateTime<Utc> = "2026-11-20T04:00:00Z".parse().unwrap();
        assert!(
            waiting_due(
                std::slice::from_ref(&t),
                thu_11pm,
                NY,
                &ds,
                &bs,
                false,
                &HashSet::new()
            )
            .is_empty()
        );
        let sat: DateTime<Utc> = "2026-11-21T16:00:00Z".parse().unwrap();
        assert!(
            waiting_due(
                std::slice::from_ref(&t),
                sat,
                NY,
                &ds,
                &bs,
                false,
                &HashSet::new()
            )
            .is_empty()
        );
        let hidden = waiting_due(
            std::slice::from_ref(&t),
            thu_11am,
            NY,
            &ds,
            &bs,
            true,
            &HashSet::new(),
        );
        assert!(!hidden[0].body.contains("Budget"));
        t.done_at = Some(thu_11am);
        assert!(waiting_due(&[t], thu_11am, NY, &ds, &bs, false, &HashSet::new()).is_empty());
    }

    #[test]
    fn time_off_is_tidied_and_found_by_day() {
        let d = |s: &str| -> NaiveDate { s.parse().unwrap() };
        let mut ds = DaySettings {
            time_off: vec![
                TimeOff {
                    from: d("2026-12-24"),
                    to: d("2026-12-31"),
                    label: "  Winter\n break ".into(),
                },
                TimeOff {
                    from: d("2026-12-10"),
                    to: d("2026-12-01"),
                    label: "backwards".into(),
                },
                TimeOff {
                    from: d("2026-11-27"),
                    to: d("2026-11-27"),
                    label: String::new(),
                },
                TimeOff {
                    from: d("2026-12-24"),
                    to: d("2026-12-31"),
                    label: "again".into(),
                },
            ],
            waiting_nudge_days: 99,
            ..Default::default()
        };
        ds.sanitize();
        assert_eq!(ds.time_off.len(), 2);
        assert_eq!(ds.time_off[0].from, d("2026-11-27"));
        assert_eq!(ds.time_off[1].label, "Winter break");
        assert!(ds.task_reminders_now(false, d("2026-12-01")));
        assert!(
            !ds.task_reminders_now(true, d("2026-12-01")),
            "they wait during focus time"
        );
        assert!(
            !ds.task_reminders_now(false, d("2026-12-26")),
            "they wait on a day off"
        );
        let mut later = ds.clone();
        later.drop_past_time_off(d("2026-11-28"));
        assert_eq!(later.time_off.len(), 1);
        later.time_off = (0..40)
            .map(|i| TimeOff {
                from: d("2027-01-01") + Duration::days(i * 2),
                to: d("2027-01-01") + Duration::days(i * 2),
                label: String::new(),
            })
            .collect();
        later.sanitize();
        assert_eq!(later.time_off.len(), MAX_TIME_OFF);
        assert_eq!(
            later.time_off.last().unwrap().from,
            d("2027-01-01") + Duration::days(78)
        );
        assert_eq!(ds.waiting_nudge_days, 30);
        assert!(ds.off_on(d("2026-12-28")).is_some());
        assert!(ds.off_on(d("2026-12-23")).is_none());
        assert!(
            Kind::Before.on_time_off()
                && !Kind::Morning.on_time_off()
                && !Kind::Waiting.on_time_off()
        );
    }

    #[test]
    fn a_running_meeting_counts_but_a_declined_or_all_day_one_does_not() {
        let now: DateTime<Utc> = "2026-11-19T15:30:00Z".parse().unwrap();
        let bs = BriefingSettings::default();
        let mut e = ev("a", "Sync", "2026-11-19T15:00:00Z", "2026-11-19T16:00:00Z");
        assert!(in_meeting_now(std::slice::from_ref(&e), now, &bs));
        assert!(!in_meeting_now(
            std::slice::from_ref(&e),
            "2026-11-19T16:00:00Z".parse().unwrap(),
            &bs
        ));
        e.my_response = crate::model::Response::Declined;
        assert!(!in_meeting_now(&[e], now, &bs));
        let all_day = Event {
            when: When::AllDay {
                start: "2026-11-19".parse().unwrap(),
                end: "2026-11-20".parse().unwrap(),
            },
            ..ev(
                "b",
                "Offsite",
                "2026-11-19T15:00:00Z",
                "2026-11-19T16:00:00Z",
            )
        };
        assert!(!in_meeting_now(&[all_day], now, &bs));
    }
}
