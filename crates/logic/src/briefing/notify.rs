//! Which notifications are due right now? Pure: the caller supplies the clock and remembers what was sent.
//!
//! Three kinds: a heads-up shortly before an event, a short morning briefing, and an evening look-ahead
//! at tomorrow. Every notification has a stable id that includes its date or start time, so a restart
//! cannot repeat one and a rescheduled meeting gets a fresh heads-up.

use super::rules::{UnlockRules, in_quiet_hours, weekday_index};
use super::{BriefingSettings, build};
use crate::model::Event;
use crate::weekahead::{self, WeekAhead};
use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct NotifySettings {
    pub before_event: bool,
    pub before_minutes: i64,
    pub morning: bool,
    /// Local hour at which the morning briefing becomes due (it stays due until noon).
    pub morning_hour: u32,
    pub evening: bool,
    /// Local hour at which the evening look-ahead becomes due (it stays due until 23:00).
    pub evening_hour: u32,
    /// On Sunday evening, show a look at the whole week instead of the usual "tomorrow" note (needs the evening look-ahead).
    pub week_preview: bool,
    /// Show times as 13:00 instead of 1:00 PM.
    pub time_24h: bool,
    /// Leave event names (and places) out of notifications. They appear on the lock screen and in screen shares.
    pub hide_titles: bool,
}

impl Default for NotifySettings {
    fn default() -> Self {
        Self {
            before_event: true,
            before_minutes: 10,
            morning: true,
            morning_hour: 8,
            evening: true,
            evening_hour: 18,
            week_preview: true,
            time_24h: false,
            hide_titles: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Before,
    Morning,
    Evening,
    /// Sunday evening: the week ahead.
    Week,
    /// The end-of-day wrap-up (`day::due`).
    WrapUp,
    /// A break after a long run of meetings.
    Break,
    /// The end of working hours.
    StopWork,
    /// After a meeting with notes (`followup`).
    FollowUp,
    /// Time to leave for an in-person meeting (`day`, travel time).
    Leave,
    /// Friday's look back at the week (`review`).
    WeekReview,
    /// A task has waited on someone long enough to nudge them (`day::waiting_due`).
    Waiting,
}

impl Kind {
    /// Whether it still comes on a day off: only what is about a meeting you have that day.
    pub fn on_time_off(self) -> bool {
        matches!(self, Kind::Before | Kind::Leave)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Notification {
    /// Stable and unique per occasion; store it once shown.
    pub id: String,
    pub kind: Kind,
    pub title: String,
    pub body: String,
}

pub fn fmt_time(t: DateTime<Utc>, tz: Tz, use_24h: bool) -> String {
    t.with_timezone(&tz)
        .format(if use_24h { "%H:%M" } else { "%-I:%M %p" })
        .to_string()
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn hours_phrase(minutes: i64) -> String {
    match minutes {
        m if m < 60 => format!("{m} min"),
        m if m < 150 => format!("{} min", (m + 2) / 5 * 5),
        m => format!("about {} hours", (m + 30) / 60),
    }
}

/// "Wednesday", or "Tuesday and Thursday".
fn day_names(dates: &[chrono::NaiveDate]) -> String {
    let names: Vec<String> = dates
        .iter()
        .take(2)
        .map(|d| d.format("%A").to_string())
        .collect();
    names.join(" and ")
}

/// The Sunday-evening note: how big the week is, the heaviest day, early starts, invitations waiting.
fn week_body(w: &WeekAhead) -> String {
    let mut body = format!(
        "{}, {}.",
        plural(w.meetings, "meeting", "meetings"),
        hours_phrase(w.minutes)
    );
    if let Some(d) = w.busiest.filter(|_| w.meetings > 1) {
        body += &format!(" Busiest: {}.", d.format("%A"));
    }
    if !w.early_starts.is_empty() {
        body += &format!(" Early start: {}.", day_names(&w.early_starts));
    }
    if w.conflicts > 0 {
        body += &format!(
            " {} to sort out.",
            plural(w.conflicts, "overlap", "overlaps")
        );
    }
    if w.needs_response > 0 {
        body += &format!(
            " {} waiting.",
            plural(w.needs_response, "invitation", "invitations")
        );
    }
    body
}

/// Notifications that should be shown now and have not been shown yet.
pub fn due(
    events: &[Event],
    now: DateTime<Utc>,
    tz: Tz,
    ns: &NotifySettings,
    bs: &BriefingSettings,
    rules: &UnlockRules,
    sent: &HashSet<String>,
) -> Vec<Notification> {
    let b = build(events, now, tz, bs);
    let local = now.with_timezone(&tz);
    let mut out = Vec::new();

    // Heads-up before an event. Always on its own schedule: if you asked for it, a quiet-hours
    // setting should not hide a 06:30 meeting.
    if ns.before_event {
        let lead = Duration::minutes(ns.before_minutes.clamp(1, 120));
        for item in b
            .remaining
            .iter()
            .filter(|i| i.start > now && i.start - lead <= now)
        {
            let id = format!("before:{}:{}", item.event.id, item.start.timestamp());
            if sent.contains(&id) {
                continue;
            }
            let mins = ((item.start - now).num_seconds() + 59) / 60;
            let when = if mins <= 1 {
                "Starting now".to_string()
            } else {
                format!("In {mins} min")
            };
            let tail = if ns.hide_titles {
                None // not even the place: it can give the meeting away
            } else {
                item.event.location.clone().or_else(|| {
                    item.event
                        .join_url
                        .as_ref()
                        .map(|_| "Join link ready".to_string())
                })
            };
            let body = match tail {
                Some(t) => format!("{when} · {} · {t}", fmt_time(item.start, tz, ns.time_24h)),
                None => format!("{when} · {}", fmt_time(item.start, tz, ns.time_24h)),
            };
            let title = if ns.hide_titles {
                "Meeting starting soon".to_string()
            } else {
                item.event.title.clone()
            };
            out.push(Notification {
                id,
                kind: Kind::Before,
                title,
                body,
            });
        }
    }

    // The two summaries respect the user's days and quiet hours.
    let allowed_today = rules.days[weekday_index(local.weekday())]
        && !in_quiet_hours(local.hour(), rules.quiet_hours);

    if ns.morning
        && allowed_today
        && local.hour() >= ns.morning_hour
        && local.hour() < 12
        && !b.remaining.is_empty()
    {
        let id = format!("morning:{}", b.date);
        if !sent.contains(&id) {
            let first = &b.remaining[0];
            let first_part = if ns.hide_titles {
                format!("First at {}.", fmt_time(first.start, tz, ns.time_24h))
            } else {
                format!(
                    "First: {} at {}.",
                    first.event.title,
                    fmt_time(first.start, tz, ns.time_24h)
                )
            };
            let mut body = format!(
                "{} left today. {first_part}",
                plural(b.remaining.len(), "event", "events")
            );
            if !b.conflicts.is_empty() {
                body += &format!(
                    " {} to sort out.",
                    plural(b.conflicts.len(), "overlap", "overlaps")
                );
            }
            if !b.needs_response.is_empty() {
                body += &format!(
                    " {} waiting.",
                    plural(b.needs_response.len(), "invitation", "invitations")
                );
            }
            out.push(Notification {
                id,
                kind: Kind::Morning,
                title: "Your day".into(),
                body,
            });
        }
    }

    // Sunday evening shows the whole week in place of the usual note about tomorrow.
    let evening_ok =
        ns.evening && allowed_today && local.hour() >= ns.evening_hour && local.hour() < 23;
    let mut week_shown = false;
    if ns.week_preview && evening_ok && local.weekday() == chrono::Weekday::Sun {
        let tomorrow = local.date_naive() + Duration::days(1);
        let week = weekahead::build(events, tomorrow, 7, tz, bs);
        if week.meetings > 0 {
            week_shown = true;
            let id = format!("week:{}", local.date_naive());
            if !sent.contains(&id) {
                out.push(Notification {
                    id,
                    kind: Kind::Week,
                    title: "Your week".into(),
                    body: week_body(&week),
                });
            }
        }
    }

    let tomorrow_has_something = !b.tomorrow.timed.is_empty() || !b.tomorrow.all_day.is_empty();
    if ns.evening
        && !week_shown
        && allowed_today
        && local.hour() >= ns.evening_hour
        && local.hour() < 23
        && tomorrow_has_something
    {
        let id = format!("evening:{}", b.date);
        if !sent.contains(&id) {
            let n = b.tomorrow.timed.len() + b.tomorrow.all_day.len();
            let mut body = plural(n, "event", "events");
            if let Some(first) = b.tomorrow.timed.first() {
                body += &format!(", starting at {}", fmt_time(first.start, tz, ns.time_24h));
            }
            body.push('.');
            if b.tomorrow.early_start {
                body += " Early start.";
            }
            if !b.needs_response.is_empty() {
                body += &format!(
                    " {} waiting.",
                    plural(b.needs_response.len(), "invitation", "invitations")
                );
            }
            out.push(Notification {
                id,
                kind: Kind::Evening,
                title: "Tomorrow".into(),
                body,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use chrono::TimeZone;

    const LONDON: Tz = chrono_tz::Europe::London;

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 3, 10, h, m, 0).unwrap() // a Tuesday, GMT
    }

    fn ev(id: &str, start: DateTime<Utc>, mins: i64) -> Event {
        Event {
            id: id.into(),
            calendar_id: "c".into(),
            remote_id: id.into(),
            ical_uid: id.into(),
            title: id.into(),
            when: When::Timed {
                start,
                end: start + Duration::minutes(mins),
                tzid: None,
            },
            location: None,
            description: None,
            join_url: None,
            status: EventStatus::Confirmed,
            my_response: Response::Accepted,
            organizer: None,
            recurrence_master_id: None,
            updated_at: Utc::now(),
            also_in: vec![],
            attendees: vec![],
        }
    }

    fn go(events: &[Event], now: DateTime<Utc>, sent: &HashSet<String>) -> Vec<Notification> {
        due(
            events,
            now,
            LONDON,
            &NotifySettings::default(),
            &BriefingSettings::default(),
            &UnlockRules::default(),
            sent,
        )
    }

    fn kinds(n: &[Notification]) -> Vec<Kind> {
        n.iter().map(|x| x.kind).collect()
    }

    #[test]
    fn a_heads_up_appears_inside_the_lead_window_and_not_before_or_after() {
        let events = [ev("standup", at(14, 0), 30)];
        assert!(
            go(&events, at(13, 49), &HashSet::new()).is_empty(),
            "11 minutes ahead is too early"
        );
        let n = go(&events, at(13, 50), &HashSet::new());
        assert_eq!(kinds(&n), [Kind::Before]);
        assert_eq!(n[0].title, "standup");
        assert_eq!(n[0].body, "In 10 min · 2:00 PM");
        assert!(
            go(&events, at(14, 0), &HashSet::new()).is_empty(),
            "once it has started, the moment has passed"
        );
        assert!(go(&events, at(14, 20), &HashSet::new()).is_empty());
    }

    #[test]
    fn it_never_repeats_after_being_sent_even_across_restarts() {
        let events = [ev("standup", at(14, 0), 30)];
        let n = go(&events, at(13, 52), &HashSet::new());
        let sent: HashSet<String> = n.iter().map(|x| x.id.clone()).collect();
        assert!(go(&events, at(13, 55), &sent).is_empty());
        assert!(go(&events, at(13, 59), &sent).is_empty());
    }

    #[test]
    fn a_rescheduled_meeting_gets_a_fresh_heads_up() {
        let first = go(&[ev("m", at(14, 0), 30)], at(13, 55), &HashSet::new());
        let sent: HashSet<String> = first.iter().map(|x| x.id.clone()).collect();
        let moved = go(&[ev("m", at(15, 0), 30)], at(14, 55), &sent);
        assert_eq!(moved.len(), 1, "same event, new start time: a new id");
    }

    #[test]
    fn opening_the_app_late_still_warns_about_what_is_about_to_start() {
        let n = go(&[ev("m", at(14, 0), 30)], at(13, 56), &HashSet::new());
        assert_eq!(n[0].body, "In 4 min · 2:00 PM");
        let now = go(
            &[ev("m", at(14, 0), 30)],
            at(13, 59) + Duration::seconds(30),
            &HashSet::new(),
        );
        assert_eq!(now[0].body, "Starting now · 2:00 PM");
    }

    #[test]
    fn heads_ups_skip_all_day_declined_cancelled_and_in_progress_events() {
        let mut declined = ev("d", at(14, 0), 30);
        declined.my_response = Response::Declined;
        let mut cancelled = ev("x", at(14, 0), 30);
        cancelled.status = EventStatus::Cancelled;
        let mut allday = ev("a", at(0, 0), 0);
        allday.when = When::AllDay {
            start: at(0, 0).date_naive(),
            end: at(0, 0).date_naive() + Duration::days(1),
        };
        let running = ev("r", at(13, 0), 120);
        assert!(
            go(
                &[declined, cancelled, allday, running],
                at(13, 55),
                &HashSet::new()
            )
            .is_empty()
        );
    }

    #[test]
    fn heads_up_text_includes_the_place_or_that_a_join_link_is_ready() {
        let mut room = ev("room", at(14, 0), 30);
        room.location = Some("Room 4B".into());
        assert_eq!(
            go(&[room], at(13, 50), &HashSet::new())[0].body,
            "In 10 min · 2:00 PM · Room 4B"
        );
        let mut call = ev("call", at(14, 0), 30);
        call.join_url = Some("https://zoom.us/j/1".into());
        assert_eq!(
            go(&[call], at(13, 50), &HashSet::new())[0].body,
            "In 10 min · 2:00 PM · Join link ready"
        );
    }

    #[test]
    fn the_lead_time_and_24_hour_format_are_configurable() {
        let ns = NotifySettings {
            before_minutes: 5,
            time_24h: true,
            morning: false,
            evening: false,
            ..Default::default()
        };
        let run = |now| {
            due(
                &[ev("m", at(14, 0), 30)],
                now,
                LONDON,
                &ns,
                &BriefingSettings::default(),
                &UnlockRules::default(),
                &HashSet::new(),
            )
        };
        assert!(run(at(13, 50)).is_empty());
        assert_eq!(run(at(13, 56))[0].body, "In 4 min · 14:00");
        let off = NotifySettings {
            before_event: false,
            morning: false,
            evening: false,
            ..Default::default()
        };
        assert!(
            due(
                &[ev("m", at(14, 0), 30)],
                at(13, 56),
                LONDON,
                &off,
                &BriefingSettings::default(),
                &UnlockRules::default(),
                &HashSet::new()
            )
            .is_empty()
        );
    }

    #[test]
    fn the_morning_briefing_is_due_once_after_the_set_hour_until_noon() {
        let events = [
            ev("standup", at(9, 0), 15),
            ev("review", at(10, 30), 60),
            ev("clash", at(11, 0), 30),
        ];
        assert!(!kinds(&go(&events, at(7, 59), &HashSet::new())).contains(&Kind::Morning));
        let n = go(&events, at(8, 5), &HashSet::new());
        let morning = n.iter().find(|x| x.kind == Kind::Morning).unwrap();
        assert_eq!(morning.id, "morning:2026-03-10");
        assert_eq!(morning.title, "Your day");
        assert_eq!(
            morning.body,
            "3 events left today. First: standup at 9:00 AM. 1 overlap to sort out."
        );
        let sent: HashSet<String> = [morning.id.clone()].into();
        assert!(
            !kinds(&go(&events, at(8, 30), &sent)).contains(&Kind::Morning),
            "once per day"
        );
        assert!(
            !kinds(&go(&events, at(12, 0), &HashSet::new())).contains(&Kind::Morning),
            "a morning briefing at noon would be silly"
        );
    }

    #[test]
    fn the_morning_briefing_stays_quiet_on_an_empty_or_finished_day() {
        assert!(go(&[], at(8, 30), &HashSet::new()).is_empty());
        // Everything already happened (late opener).
        assert!(
            !kinds(&go(
                &[ev("early", at(6, 0), 30)],
                at(8, 30),
                &HashSet::new()
            ))
            .contains(&Kind::Morning)
        );
    }

    #[test]
    fn the_morning_briefing_mentions_waiting_invitations() {
        let mut inv = ev("inv", at(10, 0), 30);
        inv.my_response = Response::NeedsAction;
        let body = go(&[inv], at(8, 30), &HashSet::new())
            .into_iter()
            .find(|x| x.kind == Kind::Morning)
            .unwrap()
            .body;
        assert!(body.ends_with("1 invitation waiting."), "{body}");
    }

    #[test]
    fn the_evening_look_ahead_needs_something_tomorrow() {
        assert!(go(&[], at(18, 30), &HashSet::new()).is_empty());
        let events = [
            ev("flight", at(6, 45) + Duration::days(1), 120),
            ev("sync", at(11, 0) + Duration::days(1), 45),
        ];
        assert!(go(&events, at(17, 59), &HashSet::new()).is_empty());
        let n = go(&events, at(18, 0), &HashSet::new());
        assert_eq!(n.len(), 1);
        assert_eq!(
            (n[0].id.as_str(), n[0].title.as_str()),
            ("evening:2026-03-10", "Tomorrow")
        );
        assert_eq!(n[0].body, "2 events, starting at 6:45 AM. Early start.");
        let sent: HashSet<String> = [n[0].id.clone()].into();
        assert!(go(&events, at(19, 0), &sent).is_empty());
        assert!(
            go(&events, at(23, 30), &HashSet::new()).is_empty(),
            "too late to be useful"
        );
    }

    #[test]
    fn summaries_obey_quiet_hours_and_disabled_days_but_heads_ups_do_not() {
        let events = [ev("early", at(9, 0), 30)];
        // Quiet 07:00-10:00 swallows the morning briefing but not the 08:50 heads-up.
        let rules = UnlockRules {
            quiet_hours: Some((7, 10)),
            ..Default::default()
        };
        let run = |now| {
            due(
                &events,
                now,
                LONDON,
                &NotifySettings::default(),
                &BriefingSettings::default(),
                &rules,
                &HashSet::new(),
            )
        };
        assert!(run(at(8, 5)).is_empty());
        assert_eq!(kinds(&run(at(8, 52))), [Kind::Before]);
        // A disabled Tuesday: no morning briefing.
        let mut days = [true; 7];
        days[1] = false;
        let off = UnlockRules {
            days,
            ..Default::default()
        };
        assert!(
            due(
                &events,
                at(8, 5),
                LONDON,
                &NotifySettings::default(),
                &BriefingSettings::default(),
                &off,
                &HashSet::new()
            )
            .is_empty()
        );
    }

    #[test]
    fn hiding_titles_removes_every_event_name_and_place() {
        let mut room = ev("SECRET-PROJECT-LAUNCH", at(14, 0), 30);
        room.location = Some("SECRET-ROOM".into());
        room.join_url = Some("https://zoom.us/j/1".into());
        let hide = NotifySettings {
            hide_titles: true,
            ..Default::default()
        };
        let run = |now| {
            due(
                &[room.clone(), ev("SECRET-OTHER", at(15, 0), 30)],
                now,
                LONDON,
                &hide,
                &BriefingSettings::default(),
                &UnlockRules::default(),
                &HashSet::new(),
            )
        };

        let before = run(at(13, 52));
        assert_eq!(before.len(), 1);
        assert_eq!(before[0].title, "Meeting starting soon");
        assert_eq!(before[0].body, "In 8 min · 2:00 PM");

        let morning = run(at(8, 30))
            .into_iter()
            .find(|n| n.kind == Kind::Morning)
            .unwrap();
        assert_eq!(morning.body, "2 events left today. First at 2:00 PM.");

        for n in before.iter().chain(&[morning]) {
            let text = format!("{} {}", n.title, n.body);
            assert!(
                !text.contains("SECRET") && !text.contains("zoom"),
                "leaked: {text}"
            );
        }
        // Ids are unchanged, so turning the option on or off never repeats or drops a notification.
        let shown = due(
            &[room],
            at(13, 52),
            LONDON,
            &NotifySettings::default(),
            &BriefingSettings::default(),
            &UnlockRules::default(),
            &HashSet::new(),
        );
        assert_eq!(shown[0].id, before[0].id);
        assert_eq!(
            shown[0].title, "SECRET-PROJECT-LAUNCH",
            "titles are shown by default"
        );
    }

    #[test]
    fn ids_follow_the_users_local_date() {
        // 23:30 UTC on the 9th is already the 10th, 08:30, in Tokyo.
        let tokyo = chrono_tz::Asia::Tokyo;
        let now = Utc.with_ymd_and_hms(2026, 3, 9, 23, 30, 0).unwrap();
        let start = now + Duration::hours(2);
        let n = due(
            &[ev("e", start, 30)],
            now,
            tokyo,
            &NotifySettings::default(),
            &BriefingSettings::default(),
            &UnlockRules {
                quiet_hours: None,
                ..Default::default()
            },
            &HashSet::new(),
        );
        assert_eq!(n[0].id, "morning:2026-03-10");
        assert_eq!(n[0].body, "1 event left today. First: e at 10:30 AM.");
    }

    // ---- the Sunday week preview ----------------------------------------------------------

    // Sunday 2026-03-08; the week ahead is Monday 9th to Sunday 15th.
    fn sun(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 3, 8, h, m, 0).unwrap()
    }
    fn mon(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 3, 9, h, m, 0).unwrap()
    }
    fn wed(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 3, 11, h, m, 0).unwrap()
    }

    fn the_week() -> Vec<Event> {
        let mut invite = ev("invite", wed(15, 0), 60);
        invite.my_response = Response::NeedsAction;
        vec![
            ev("early", mon(6, 45), 30),
            ev("big", wed(9, 0), 5 * 60),
            invite,
            ev("other", wed(14, 0), 30),
        ]
    }

    #[test]
    fn sunday_evening_shows_the_week_instead_of_the_tomorrow_note() {
        let n = go(&the_week(), sun(18, 0), &HashSet::new());
        assert_eq!(kinds(&n), [Kind::Week]);
        assert_eq!(n[0].id, "week:2026-03-08");
        assert_eq!(n[0].title, "Your week");
        assert_eq!(
            n[0].body,
            "4 meetings, about 7 hours. Busiest: Wednesday. Early start: Monday. 1 invitation waiting."
        );
    }

    #[test]
    fn it_is_not_shown_before_the_evening_hour_twice_or_on_other_days() {
        assert!(go(&the_week(), sun(17, 59), &HashSet::new()).is_empty());
        let n = go(&the_week(), sun(18, 5), &HashSet::new());
        let sent: HashSet<String> = n.iter().map(|x| x.id.clone()).collect();
        assert!(
            go(&the_week(), sun(20, 0), &sent).is_empty(),
            "never repeats"
        );
        // On a Saturday evening, with Monday not tomorrow, it is the ordinary look-ahead (or nothing).
        let sat = Utc.with_ymd_and_hms(2026, 3, 7, 18, 30, 0).unwrap();
        assert!(!kinds(&go(&the_week(), sat, &HashSet::new())).contains(&Kind::Week));
    }

    #[test]
    fn it_can_be_turned_off_and_then_the_usual_note_returns() {
        let ns = NotifySettings {
            week_preview: false,
            ..Default::default()
        };
        let events = [ev("m", mon(9, 0), 30)];
        let n = due(
            &events,
            sun(18, 0),
            LONDON,
            &ns,
            &BriefingSettings::default(),
            &UnlockRules::default(),
            &HashSet::new(),
        );
        assert_eq!(kinds(&n), [Kind::Evening]);
    }

    #[test]
    fn an_empty_week_says_nothing_and_quiet_hours_and_excluded_days_are_respected() {
        assert!(go(&[], sun(18, 0), &HashSet::new()).is_empty());
        let rules = UnlockRules {
            days: [true, true, true, true, true, true, false],
            ..Default::default()
        };
        let n = due(
            &the_week(),
            sun(18, 0),
            LONDON,
            &NotifySettings::default(),
            &BriefingSettings::default(),
            &rules,
            &HashSet::new(),
        );
        assert!(n.is_empty(), "Sunday is switched off in the rules");
    }

    #[test]
    fn the_week_note_contains_no_event_titles() {
        let mut e = ev("Secret merger talks", mon(9, 0), 60);
        e.title = "Secret merger talks".into();
        let n = go(&[e], sun(18, 0), &HashSet::new());
        assert!(!n[0].body.contains("Secret") && !n[0].title.contains("Secret"));
    }
}
