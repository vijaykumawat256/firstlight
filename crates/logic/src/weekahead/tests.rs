use super::*;
use chrono::TimeZone;

const LONDON: Tz = chrono_tz::Europe::London;

// Sunday 2026-03-08 is the evening before this week; Monday is the 9th. London is on GMT.
fn d(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 3, day).unwrap()
}
fn t(day: u32, h: u32, m: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, day, h, m, 0).unwrap()
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
fn week(events: &[Event]) -> WeekAhead {
    build(events, d(9), 7, LONDON, &BriefingSettings::default())
}

#[test]
fn it_covers_seven_days_from_the_start_and_marks_the_weekend() {
    let w = week(&[]);
    assert_eq!(w.days.len(), 7);
    assert_eq!(w.days[0].date, d(9));
    assert_eq!(w.days.iter().filter(|x| x.weekend).count(), 2);
    assert_eq!(
        (w.meetings, w.minutes, w.busiest, w.lightest),
        (0, 0, None, None)
    );
}

#[test]
fn time_in_meetings_counts_overlaps_once_and_the_busiest_day_is_named() {
    let w = week(&[
        ev("a", t(9, 10, 0), 60),
        ev("b", t(9, 10, 30), 60), // overlaps a by 30 min -> 10:00-11:30 = 90 min
        ev("c", t(10, 9, 0), 6 * 60), // Tuesday, 6 hours
    ]);
    assert_eq!(w.days[0].minutes, 90);
    assert_eq!(w.days[0].conflicts, 1);
    assert_eq!(w.days[1].minutes, 360);
    assert!(w.days[1].heavy && !w.days[0].heavy);
    assert_eq!(w.busiest, Some(d(10)));
    assert_eq!((w.meetings, w.minutes, w.conflicts), (3, 450, 1));
}

#[test]
fn the_lightest_working_day_ignores_weekends_and_differs_from_the_busiest() {
    let w = week(&[
        ev("m", t(9, 9, 0), 120),
        ev("t", t(10, 9, 0), 240),
        ev("w", t(11, 9, 0), 60),
        ev("th", t(12, 9, 0), 180),
        ev("sat", t(14, 9, 0), 30), // weekend busy time does not matter for "lightest"
    ]);
    // Friday has nothing: it is the lightest working day.
    assert_eq!(w.lightest, Some(d(13)));
    assert_eq!(w.busiest, Some(d(10)));
}

#[test]
fn an_early_first_meeting_and_a_back_to_back_run_are_noticed() {
    let w = week(&[
        ev("early", t(10, 6, 45), 30),
        ev("r1", t(11, 13, 0), 60),
        ev("r2", t(11, 14, 0), 30),
        ev("r3", t(11, 14, 33), 30), // within five minutes
        ev("later", t(11, 16, 0), 30),
    ]);
    assert_eq!(w.early_starts, vec![d(10)]);
    assert_eq!(w.days[2].back_to_back_runs, 1);
    assert_eq!(w.days[1].back_to_back_runs, 0);
}

#[test]
fn the_best_block_for_focus_is_the_longest_free_stretch_of_two_hours_or_more() {
    // Monday is full 09:00-18:00 except 14:00-16:00 would be too short; Tuesday is open until 15:00.
    let w = week(&[
        ev("m1", t(9, 9, 0), 5 * 60),
        ev("m2", t(9, 16, 0), 2 * 60),
        ev("t1", t(10, 15, 0), 3 * 60),
    ]);
    let f = w.focus.unwrap();
    assert_eq!(
        f.date,
        d(11),
        "Wednesday is wide open: 9 hours, the longest"
    );
    assert_eq!(f.minutes, 9 * 60);
}

#[test]
fn unanswered_invitations_cancelled_and_declined_events_and_all_day_titles() {
    let mut open = ev("open", t(10, 10, 0), 30);
    open.my_response = Response::NeedsAction;
    let mut cancelled = ev("x", t(10, 11, 0), 30);
    cancelled.status = EventStatus::Cancelled;
    let mut declined = ev("y", t(10, 12, 0), 30);
    declined.my_response = Response::Declined;
    let mut holiday = ev("h", t(12, 0, 0), 0);
    holiday.title = "Bank holiday".into();
    holiday.when = When::AllDay {
        start: d(12),
        end: d(13),
    };
    let w = week(&[open, cancelled, declined, holiday]);
    assert_eq!(w.needs_response, 1);
    assert_eq!(w.meetings, 1, "cancelled and declined do not count");
    assert_eq!(w.days[3].all_day, vec!["Bank holiday".to_string()]);
}

#[test]
fn a_meeting_that_runs_past_midnight_counts_only_to_midnight() {
    let w = week(&[ev("late", t(9, 23, 0), 120)]);
    assert_eq!(w.days[0].minutes, 60);
    assert_eq!(w.days[1].meetings, 0);
}
