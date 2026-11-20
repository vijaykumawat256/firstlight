//! Time off: meetings that fall on your days off (so you can decline or move them before you go), and the public
//! holidays coming up (all-day events in holiday calendars). The days off themselves are `day::DaySettings::time_off`.

use crate::briefing::{BriefingSettings, is_shown};
use crate::day::TimeOff;
use crate::model::{Calendar, Event, When};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use chrono_tz::Tz;
use serde::Serialize;
use std::collections::HashSet;

/// How far ahead clashes and holidays are looked for.
pub const LOOK_AHEAD_DAYS: i64 = 90;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Clash {
    pub event_id: String,
    pub title: String,
    pub start: DateTime<Utc>,
    /// The organiser's address, to write to (none for your own events).
    pub organizer: Option<String>,
    /// The stretch of time off it falls in.
    pub off: TimeOff,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Holiday {
    pub date: NaiveDate,
    pub name: String,
}

/// A calendar of public holidays: Google's holiday calendars (connected or added as a link) or one with "holiday"
/// in its name (the Mac's Holidays calendar).
pub fn is_holiday_calendar(c: &Calendar) -> bool {
    let id = c.remote_id.to_lowercase();
    id.contains("holiday@group.v.calendar.google.com")
        || id.contains("%23holiday%40group.v.calendar.google.com")
        || c.name.to_lowercase().contains("holiday")
}

/// Upcoming timed meetings (not cancelled or declined) that start on a day off, soonest first.
pub fn clashes(
    events: &[Event],
    time_off: &[TimeOff],
    now: DateTime<Utc>,
    tz: Tz,
    bs: &BriefingSettings,
) -> Vec<Clash> {
    if time_off.is_empty() {
        return vec![];
    }
    let until = now + Duration::days(LOOK_AHEAD_DAYS);
    let mut seen = HashSet::new();
    let mut out: Vec<Clash> = events
        .iter()
        .filter(|e| is_shown(e, bs))
        .filter_map(|e| match e.when {
            When::Timed { start, .. } if start >= now && start < until => {
                let day = start.with_timezone(&tz).date_naive();
                let off = time_off.iter().find(|t| t.from <= day && day <= t.to)?;
                seen.insert(e.id.clone()).then(|| Clash {
                    event_id: e.id.clone(),
                    title: e.title.clone(),
                    start,
                    organizer: e
                        .attendees
                        .iter()
                        .find(|a| a.organizer)
                        .map(|a| a.email.clone()),
                    off: off.clone(),
                })
            }
            _ => None,
        })
        .collect();
    out.sort_by_key(|c| c.start);
    out
}

/// Public holidays from today on (all-day events in holiday calendars), one per day and name, soonest first.
pub fn holidays(
    events: &[Event],
    holiday_calendars: &HashSet<String>,
    today: NaiveDate,
    limit: usize,
) -> Vec<Holiday> {
    let until = today + Duration::days(LOOK_AHEAD_DAYS);
    let mut seen = HashSet::new();
    let mut out: Vec<Holiday> = events
        .iter()
        .filter(|e| holiday_calendars.contains(&e.calendar_id))
        .filter_map(|e| match e.when {
            When::AllDay { start, .. } if start >= today && start < until => Some(Holiday {
                date: start,
                name: e.title.trim().to_string(),
            }),
            _ => None,
        })
        .filter(|h| !h.name.is_empty() && seen.insert((h.date, h.name.to_lowercase())))
        .collect();
    out.sort_by(|a, b| (a.date, &a.name).cmp(&(b.date, &b.name)));
    out.truncate(limit);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Attendee, EventStatus, Response};

    const NY: Tz = chrono_tz::America::New_York;

    fn t(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }
    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    fn ev(id: &str, cal: &str, when: When) -> Event {
        Event {
            id: id.into(),
            calendar_id: cal.into(),
            remote_id: id.into(),
            ical_uid: id.into(),
            title: format!("Event {id}"),
            when,
            location: None,
            description: None,
            join_url: None,
            status: EventStatus::Confirmed,
            my_response: Response::Accepted,
            organizer: None,
            recurrence_master_id: None,
            updated_at: t("2026-11-01T00:00:00Z"),
            also_in: vec![],
            attendees: vec![],
        }
    }

    fn timed(start: &str) -> When {
        let s = t(start);
        When::Timed {
            start: s,
            end: s + Duration::hours(1),
            tzid: None,
        }
    }

    #[test]
    fn meetings_on_days_off_are_found_with_the_organiser() {
        let off = vec![TimeOff {
            from: d("2026-12-24"),
            to: d("2026-12-31"),
            label: "Winter break".into(),
        }];
        let mut m = ev("a", "w", timed("2026-12-28T15:00:00Z"));
        m.attendees = vec![Attendee {
            email: "boss@example.com".into(),
            name: None,
            response: Response::Accepted,
            organizer: true,
        }];
        let mut declined = ev("b", "w", timed("2026-12-29T15:00:00Z"));
        declined.my_response = Response::Declined;
        let before = ev("c", "w", timed("2026-12-23T15:00:00Z"));
        let past = ev("p", "w", timed("2026-11-01T15:00:00Z"));
        let c = clashes(
            &[before, m, declined, past],
            &off,
            t("2026-11-19T12:00:00Z"),
            NY,
            &BriefingSettings::default(),
        );
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].event_id, "a");
        assert_eq!(c[0].organizer.as_deref(), Some("boss@example.com"));
        assert_eq!(c[0].off.label, "Winter break");
        assert!(
            clashes(
                &[ev("x", "w", timed("2026-12-28T15:00:00Z"))],
                &[],
                t("2026-11-19T12:00:00Z"),
                NY,
                &BriefingSettings::default()
            )
            .is_empty()
        );
    }

    #[test]
    fn holidays_come_from_holiday_calendars_only_once_each() {
        let cal = |id: &str, remote: &str, name: &str| Calendar {
            id: id.into(),
            account_id: "x".into(),
            remote_id: remote.into(),
            name: name.into(),
            colour: "blue".into(),
            visible: true,
            read_only: true,
        };
        assert!(is_holiday_calendar(&cal(
            "1",
            "en.usa#holiday@group.v.calendar.google.com",
            "Holidays in United States"
        )));
        assert!(is_holiday_calendar(&cal(
            "2",
            "https://calendar.google.com/calendar/ical/en.uk%23holiday%40group.v.calendar.google.com/public/basic.ics",
            "UK"
        )));
        assert!(is_holiday_calendar(&cal("3", "abc", "US Holidays")));
        assert!(!is_holiday_calendar(&cal("4", "work", "Work")));

        let ids: HashSet<String> = ["h".to_string()].into();
        let mut xmas = ev(
            "x",
            "h",
            When::AllDay {
                start: d("2026-12-25"),
                end: d("2026-12-26"),
            },
        );
        xmas.title = "Christmas Day".into();
        let mut dup = xmas.clone();
        dup.id = "x2".into();
        let mut past = ev(
            "p",
            "h",
            When::AllDay {
                start: d("2026-11-11"),
                end: d("2026-11-12"),
            },
        );
        past.title = "Veterans Day".into();
        let mut other = ev(
            "o",
            "w",
            When::AllDay {
                start: d("2026-12-01"),
                end: d("2026-12-02"),
            },
        );
        other.title = "Offsite".into();
        let h = holidays(&[xmas, dup, past, other], &ids, d("2026-11-19"), 10);
        assert_eq!(
            h,
            vec![Holiday {
                date: d("2026-12-25"),
                name: "Christmas Day".into()
            }]
        );
    }
}
