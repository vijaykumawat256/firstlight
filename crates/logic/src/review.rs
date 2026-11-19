//! The weekly review: a look back at the week so far (Monday to now) next to the same days last week.
//! Numbers only: meeting time, focus time, tasks done, the busiest day.

use crate::briefing::notify::{Kind, Notification};
use crate::briefing::rules::{UnlockRules, in_quiet_hours, weekday_index};
use crate::briefing::{BriefingSettings, day_bounds, is_shown};
use crate::day::DaySettings;
use crate::model::{Event, When};
use crate::tasks::Task;
use chrono::{DateTime, Datelike, Duration, NaiveDate, Timelike, Utc, Weekday};
use chrono_tz::Tz;
use serde::Serialize;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WeekReview {
    /// Monday of this week and today.
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub meetings: usize,
    pub meeting_minutes: i64,
    /// The same days of last week, to compare.
    pub previous_meeting_minutes: i64,
    pub focus_minutes: i64,
    pub tasks_done: usize,
    /// The day with the most meeting time, and how much.
    pub busiest: Option<(NaiveDate, i64)>,
}

/// Minutes covered by these spans, overlaps counted once.
fn covered(mut spans: Vec<(DateTime<Utc>, DateTime<Utc>)>) -> i64 {
    spans.sort();
    let mut total = 0;
    let mut cur: Option<(DateTime<Utc>, DateTime<Utc>)> = None;
    for (s, e) in spans {
        match cur {
            Some((cs, ce)) if s <= ce => cur = Some((cs, ce.max(e))),
            Some((cs, ce)) => {
                total += (ce - cs).num_minutes();
                cur = Some((s, e));
            }
            None => cur = Some((s, e)),
        }
    }
    total + cur.map_or(0, |(s, e)| (e - s).num_minutes())
}

/// Timed meeting spans (shown, not focus time) clipped to `[from, to)`.
fn meeting_spans(
    events: &[Event],
    focus: &HashSet<String>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    bs: &BriefingSettings,
) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
    events
        .iter()
        .filter(|e| is_shown(e, bs) && !focus.contains(&e.id))
        .filter_map(|e| match e.when {
            When::Timed { start, end, .. } if start < to && end > from => {
                Some((start.max(from), end.min(to)))
            }
            _ => None,
        })
        .collect()
}

/// `focus` holds the ids of focus-time events (own events marked as focus).
pub fn review(
    events: &[Event],
    focus: &HashSet<String>,
    tasks: &[Task],
    now: DateTime<Utc>,
    tz: Tz,
    bs: &BriefingSettings,
) -> WeekReview {
    let today = now.with_timezone(&tz).date_naive();
    let monday = today - Duration::days(i64::from(today.weekday().num_days_from_monday()));
    let (from, _) = day_bounds(monday, tz);
    let mut per_day = Vec::new();
    let mut meetings = 0;
    for d in 0..=(today - monday).num_days() {
        let day = monday + Duration::days(d);
        let (s, e) = day_bounds(day, tz);
        let spans = meeting_spans(events, focus, s, e.min(now), bs);
        meetings += events
            .iter()
            .filter(|ev| is_shown(ev, bs) && !focus.contains(&ev.id))
            .filter(|ev| matches!(ev.when, When::Timed { start, .. } if start >= s && start < e.min(now)))
            .count();
        per_day.push((day, covered(spans)));
    }
    let (prev_from, _) = day_bounds(monday - Duration::days(7), tz);
    let previous = covered(meeting_spans(
        events,
        focus,
        prev_from,
        now - Duration::days(7),
        bs,
    ));
    let focus_minutes = covered(
        events
            .iter()
            .filter(|e| focus.contains(&e.id))
            .filter_map(|e| match e.when {
                When::Timed { start, end, .. } if start < now && end > from => {
                    Some((start.max(from), end.min(now)))
                }
                _ => None,
            })
            .collect(),
    );
    let tasks_done = tasks
        .iter()
        .filter(|t| t.done_at.is_some_and(|d| d >= from && d <= now))
        .count();
    WeekReview {
        from: monday,
        to: today,
        meetings,
        meeting_minutes: per_day.iter().map(|(_, m)| m).sum(),
        previous_meeting_minutes: previous,
        focus_minutes,
        tasks_done,
        busiest: per_day
            .into_iter()
            .filter(|(_, m)| *m > 0)
            .max_by_key(|(_, m)| *m),
    }
}

/// "14 h", "2 h 30 min", "45 min".
pub fn hours(minutes: i64) -> String {
    let (h, m) = (minutes / 60, minutes % 60);
    match (h, m) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// The Friday notification (numbers only), from the wrap-up time (or 16:00 when the wrap-up is off).
pub fn due(
    review: &WeekReview,
    now: DateTime<Utc>,
    tz: Tz,
    ds: &DaySettings,
    rules: &UnlockRules,
    sent: &HashSet<String>,
) -> Option<Notification> {
    let local = now.with_timezone(&tz);
    let (h, m) = if ds.wrap_up {
        (ds.wrap_up_hour, ds.wrap_up_minute)
    } else {
        (16, 0)
    };
    if !ds.weekly_review
        || local.weekday() != Weekday::Fri
        || (local.hour(), local.minute()) < (h, m)
        || local.hour() >= 23
        || !rules.days[weekday_index(local.weekday())]
        || in_quiet_hours(local.hour(), rules.quiet_hours)
    {
        return None;
    }
    let id = format!("review:{}", local.date_naive());
    if sent.contains(&id) {
        return None;
    }
    let diff = review.meeting_minutes - review.previous_meeting_minutes;
    let compare = match diff {
        d if d.abs() < 30 => " (about the same as last week)".to_string(),
        d if d > 0 => format!(" ({} more than last week)", hours(d)),
        d => format!(" ({} less than last week)", hours(-d)),
    };
    let mut body = format!("{} in meetings{compare}", hours(review.meeting_minutes));
    if review.focus_minutes > 0 {
        body.push_str(&format!(", {} of focus", hours(review.focus_minutes)));
    }
    body.push_str(&format!(
        ", {} done.",
        if review.tasks_done == 1 {
            "1 task".to_string()
        } else {
            format!("{} tasks", review.tasks_done)
        }
    ));
    Some(Notification {
        id,
        kind: Kind::WeekReview,
        title: "Your week".into(),
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    const NY: Tz = chrono_tz::America::New_York;

    fn t(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn ev(id: &str, start: &str, end: &str) -> Event {
        Event {
            id: id.into(),
            calendar_id: "cal".into(),
            remote_id: id.into(),
            ical_uid: id.into(),
            title: id.into(),
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

    #[test]
    fn the_week_is_counted_from_monday_with_overlaps_once_and_focus_apart() {
        // Friday 20 November 2026, 17:30 in New York.
        let now = t("2026-11-20T22:30:00Z");
        let events = [
            ev("mon1", "2026-11-16T14:00:00Z", "2026-11-16T15:00:00Z"),
            ev("mon2", "2026-11-16T14:30:00Z", "2026-11-16T15:30:00Z"), // overlaps mon1: 90 min together
            ev("wed", "2026-11-18T15:00:00Z", "2026-11-18T17:00:00Z"),
            ev("focus", "2026-11-19T14:00:00Z", "2026-11-19T16:00:00Z"),
            ev("later", "2026-11-20T23:00:00Z", "2026-11-21T00:00:00Z"), // after now
            ev("last-week", "2026-11-10T14:00:00Z", "2026-11-10T18:00:00Z"),
        ];
        let focus: HashSet<String> = ["focus".to_string()].into();
        let mut done = Task {
            id: "t".into(),
            title: "t".into(),
            notes: None,
            due: None,
            minutes: None,
            done_at: Some(t("2026-11-17T15:00:00Z")),
            created_at: t("2026-11-10T00:00:00Z"),
            updated_at: t("2026-11-17T15:00:00Z"),
            from_meeting: None,
            planned_event: None,
            link: None,
        };
        let old = Task {
            done_at: Some(t("2026-11-12T15:00:00Z")),
            ..done.clone()
        };
        done.id = "t2".into();
        let r = review(
            &events,
            &focus,
            &[done, old],
            now,
            NY,
            &BriefingSettings::default(),
        );
        assert_eq!(r.from, "2026-11-16".parse::<NaiveDate>().unwrap());
        assert_eq!(
            (r.meetings, r.meeting_minutes, r.focus_minutes, r.tasks_done),
            (3, 210, 120, 1)
        );
        assert_eq!(r.previous_meeting_minutes, 240);
        assert_eq!(r.busiest, Some(("2026-11-18".parse().unwrap(), 120)));

        let n = due(
            &r,
            now,
            NY,
            &DaySettings::default(),
            &UnlockRules::default(),
            &HashSet::new(),
        )
        .unwrap();
        assert_eq!(
            n.body,
            "3 h 30 min in meetings (30 min less than last week), 2 h of focus, 1 task done."
        );
        // Not on Thursday, and not twice.
        assert!(
            due(
                &r,
                now - Duration::days(1),
                NY,
                &DaySettings::default(),
                &UnlockRules::default(),
                &HashSet::new()
            )
            .is_none()
        );
        assert!(
            due(
                &r,
                now,
                NY,
                &DaySettings::default(),
                &UnlockRules::default(),
                &[n.id.clone()].into()
            )
            .is_none()
        );
    }
}
