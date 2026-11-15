use super::*;
use chrono::TimeZone;

const LONDON: Tz = chrono_tz::Europe::London;

// 2026-03-10 (a Tuesday). London is on GMT then, so local time equals UTC.
fn at(h: u32, m: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, 10, h, m, 0).unwrap()
}
fn next_day(h: u32, m: u32) -> DateTime<Utc> {
    at(h, m) + Duration::days(1)
}
fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 3, d).unwrap()
}

fn event(id: &str, start: DateTime<Utc>, end: DateTime<Utc>) -> Event {
    Event {
        id: id.into(),
        calendar_id: "c".into(),
        remote_id: id.into(),
        ical_uid: id.into(),
        title: id.into(),
        when: When::Timed {
            start,
            end,
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
    }
}

fn all_day(id: &str, start: u32, end: u32) -> Event {
    Event {
        when: When::AllDay {
            start: day(start),
            end: day(end),
        },
        ..event(id, at(0, 0), at(0, 0))
    }
}

fn run(events: &[Event], now: DateTime<Utc>) -> Briefing {
    build(events, now, LONDON, &BriefingSettings::default())
}

fn ids(items: &[Item]) -> Vec<&str> {
    items.iter().map(|i| i.event.id.as_str()).collect()
}

#[test]
fn an_empty_day_is_clear() {
    let b = run(&[], at(12, 0));
    assert_eq!(b.state, DayState::Clear);
    assert!(b.next.is_none() && b.remaining.is_empty() && b.past.is_empty());
    assert!(b.tomorrow.timed.is_empty() && !b.tomorrow.early_start);
    assert_eq!(b.date, day(10));
}

#[test]
fn a_day_that_is_over_is_done() {
    let b = run(&[event("a", at(9, 0), at(10, 0))], at(12, 0));
    assert_eq!(b.state, DayState::Done);
    assert_eq!(ids(&b.past), ["a"]);
    assert!(b.remaining.is_empty() && b.next.is_none());
}

#[test]
fn past_and_remaining_split_at_now_and_next_is_the_first_remaining() {
    let b = run(
        &[
            event("a", at(9, 0), at(10, 0)),
            event("c", at(16, 0), at(17, 0)),
            event("b", at(14, 0), at(15, 0)),
        ],
        at(12, 0),
    );
    assert_eq!(ids(&b.past), ["a"]);
    assert_eq!(
        ids(&b.remaining),
        ["b", "c"],
        "sorted by start regardless of input order"
    );
    let next = b.next.unwrap();
    assert_eq!(next.event.id, "b");
    assert!(!next.in_progress);
    assert_eq!(b.state, DayState::Upcoming);
}

#[test]
fn an_event_in_progress_is_next_and_flagged() {
    let b = run(&[event("now", at(11, 30), at(12, 30))], at(12, 0));
    assert!(b.next.as_ref().unwrap().in_progress);
    assert_eq!(b.state, DayState::InProgress);
}

#[test]
fn an_event_ending_exactly_now_is_past() {
    let b = run(&[event("a", at(11, 0), at(12, 0))], at(12, 0));
    assert_eq!(ids(&b.past), ["a"]);
}

#[test]
fn overlaps_are_conflicts_with_the_overlap_length_but_back_to_back_is_not() {
    let b = run(
        &[
            event("a", at(13, 0), at(14, 0)),
            event("b", at(13, 30), at(14, 30)),
        ],
        at(12, 0),
    );
    assert_eq!(
        b.conflicts,
        vec![Conflict {
            a: "a".into(),
            b: "b".into(),
            overlap_minutes: 30
        }]
    );
    let b2b = run(
        &[
            event("a", at(13, 0), at(14, 0)),
            event("b", at(14, 0), at(15, 0)),
        ],
        at(12, 0),
    );
    assert!(b2b.conflicts.is_empty());
}

#[test]
fn a_long_event_conflicts_with_each_event_inside_it() {
    let b = run(
        &[
            event("long", at(13, 0), at(17, 0)),
            event("x", at(14, 0), at(14, 30)),
            event("y", at(16, 0), at(16, 30)),
        ],
        at(12, 0),
    );
    assert_eq!(b.conflicts.len(), 2);
}

#[test]
fn conflicts_that_are_already_over_are_ignored() {
    let b = run(
        &[
            event("a", at(9, 0), at(10, 0)),
            event("b", at(9, 30), at(10, 30)),
        ],
        at(12, 0),
    );
    assert!(b.conflicts.is_empty());
}

#[test]
fn runs_of_three_or_more_back_to_back_meetings_are_reported() {
    let b = run(
        &[
            event("a", at(13, 0), at(14, 0)),
            event("b", at(14, 0), at(14, 30)),
            event("c", at(14, 33), at(15, 0)),
            event("d", at(16, 0), at(17, 0)),
        ],
        at(12, 0),
    );
    assert_eq!(
        b.back_to_back_runs,
        vec![vec!["a".to_string(), "b".into(), "c".into()]],
        "a 3-minute gap still counts; d is separate"
    );
    let two = run(
        &[
            event("a", at(13, 0), at(14, 0)),
            event("b", at(14, 0), at(15, 0)),
        ],
        at(12, 0),
    );
    assert!(
        two.back_to_back_runs.is_empty(),
        "two in a row is not worth flagging"
    );
}

#[test]
fn free_gaps_are_long_enough_inside_working_hours_and_before_the_last_meeting() {
    let b = run(
        &[
            event("a", at(10, 0), at(11, 0)),
            event("b", at(11, 10), at(12, 0)),
            event("c", at(14, 0), at(15, 0)),
        ],
        at(9, 0),
    );
    let gaps: Vec<_> = b
        .free_gaps
        .iter()
        .map(|g| (g.start.hour(), g.end.hour(), g.minutes))
        .collect();
    // 09:00-10:00 before the first meeting, then 12:00-14:00; the 10-minute break is too short.
    assert_eq!(gaps, [(9, 10, 60), (12, 14, 120)]);
}

#[test]
fn gaps_start_from_now_and_never_before_work_starts() {
    let early = run(&[event("a", at(10, 0), at(11, 0))], at(6, 0));
    assert_eq!(
        (early.free_gaps[0].start.hour(), early.free_gaps[0].minutes),
        (9, 60),
        "clipped to the 09:00 work start"
    );
    let late = run(&[event("a", at(15, 0), at(16, 0))], at(14, 20));
    assert_eq!(late.free_gaps[0].minutes, 40, "counted from now");
}

#[test]
fn overlapping_meetings_do_not_create_a_phantom_gap() {
    let b = run(
        &[
            event("a", at(13, 0), at(15, 0)),
            event("b", at(13, 30), at(14, 0)),
            event("c", at(15, 5), at(16, 0)),
        ],
        at(12, 0),
    );
    assert_eq!(
        b.free_gaps.len(),
        1,
        "only the lead-in gap: b sits inside a and the 5 minute break after is too short"
    );
}

#[test]
fn cancelled_and_declined_are_hidden_tentative_is_kept_and_declined_can_be_shown() {
    let mut cancelled = event("x", at(13, 0), at(14, 0));
    cancelled.status = EventStatus::Cancelled;
    let mut declined = event("y", at(15, 0), at(16, 0));
    declined.my_response = Response::Declined;
    let mut tentative = event("z", at(17, 0), at(18, 0));
    tentative.status = EventStatus::Tentative;
    let events = [cancelled, declined, tentative];
    let b = run(&events, at(12, 0));
    assert_eq!(ids(&b.remaining), ["z"]);
    let all = build(
        &events,
        at(12, 0),
        LONDON,
        &BriefingSettings {
            show_declined: true,
            ..Default::default()
        },
    );
    assert_eq!(
        ids(&all.remaining),
        ["y", "z"],
        "declined shown on request; cancelled never"
    );
}

#[test]
fn an_event_crossing_midnight_belongs_to_both_days() {
    let night = event("night", at(22, 0), next_day(2, 0));
    let b = run(&[night], at(12, 0));
    assert_eq!(ids(&b.remaining), ["night"]);
    assert_eq!(ids(&b.tomorrow.timed), ["night"]);
}

#[test]
fn all_day_only_days_and_multi_day_boundaries() {
    let b = run(
        &[
            all_day("bday", 10, 11),
            all_day("trip", 8, 10),
            all_day("conf", 9, 13),
        ],
        at(12, 0),
    );
    let titles: Vec<_> = b.all_day_today.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(
        titles,
        ["bday", "conf"],
        "a trip ending before today is gone; end date is exclusive"
    );
    assert_eq!(b.state, DayState::AllDayOnly);
    assert_eq!(
        b.tomorrow.all_day.len(),
        1,
        "the conference continues tomorrow"
    );
}

#[test]
fn tomorrow_early_starts_are_flagged_using_local_time() {
    assert!(
        run(&[event("e", next_day(6, 45), next_day(9, 30))], at(12, 0))
            .tomorrow
            .early_start
    );
    assert!(
        !run(&[event("l", next_day(10, 0), next_day(11, 0))], at(12, 0))
            .tomorrow
            .early_start
    );
    assert!(run(&[], at(12, 0)).tomorrow.timed.is_empty());
    // 07:30 in New York is 12:30 UTC: early locally even though the UTC hour is 12.
    let ny = chrono_tz::America::New_York;
    let now = Utc.with_ymd_and_hms(2026, 3, 10, 16, 0, 0).unwrap();
    let start = Utc.with_ymd_and_hms(2026, 3, 11, 11, 30, 0).unwrap(); // 07:30 EDT
    assert!(
        build(
            &[event("e", start, start + Duration::hours(1))],
            now,
            ny,
            &BriefingSettings::default()
        )
        .tomorrow
        .early_start
    );
}

#[test]
fn unanswered_invitations_are_collected_for_today_and_tomorrow() {
    let mut a = event("a", at(14, 0), at(15, 0));
    a.my_response = Response::NeedsAction;
    let mut b = event("b", next_day(10, 0), next_day(11, 0));
    b.my_response = Response::NeedsAction;
    let mut past = event("p", at(8, 0), at(9, 0));
    past.my_response = Response::NeedsAction;
    let r = run(&[a, b, past], at(12, 0));
    let got: Vec<_> = r.needs_response.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(
        got,
        ["a", "b"],
        "an invitation for a meeting that already happened is not actionable"
    );
}

#[test]
fn the_day_follows_the_users_timezone_not_utc() {
    let tokyo = chrono_tz::Asia::Tokyo;
    // 23:30 local on 10 March in Tokyo is 14:30 UTC the same day; 00:30 on the 11th is 15:30 UTC.
    let now = Utc.with_ymd_and_hms(2026, 3, 10, 12, 0, 0).unwrap(); // 21:00 in Tokyo
    let late = Utc.with_ymd_and_hms(2026, 3, 10, 14, 30, 0).unwrap();
    let after_midnight = Utc.with_ymd_and_hms(2026, 3, 10, 15, 30, 0).unwrap();
    let b = build(
        &[
            event("late", late, late + Duration::minutes(30)),
            event(
                "after",
                after_midnight,
                after_midnight + Duration::minutes(30),
            ),
        ],
        now,
        tokyo,
        &BriefingSettings::default(),
    );
    assert_eq!(ids(&b.remaining), ["late"]);
    assert_eq!(ids(&b.tomorrow.timed), ["after"]);
}

#[test]
fn day_bounds_handle_daylight_saving_days() {
    let ny = chrono_tz::America::New_York;
    let (s, e) = day_bounds(NaiveDate::from_ymd_opt(2026, 3, 8).unwrap(), ny); // clocks go forward
    assert_eq!((e - s).num_hours(), 23);
    let (s, e) = day_bounds(NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(), ny); // clocks go back
    assert_eq!((e - s).num_hours(), 25);
    let (s, e) = day_bounds(day(10), LONDON);
    assert_eq!((e - s).num_hours(), 24);
}

#[test]
fn an_event_during_the_skipped_hour_still_lands_on_the_right_day() {
    let ny = chrono_tz::America::New_York;
    let now = Utc.with_ymd_and_hms(2026, 3, 8, 6, 0, 0).unwrap(); // 01:00 EST on the spring-forward day
    let start = Utc.with_ymd_and_hms(2026, 3, 8, 14, 0, 0).unwrap(); // 10:00 EDT
    let b = build(
        &[event("e", start, start + Duration::hours(1))],
        now,
        ny,
        &BriefingSettings::default(),
    );
    assert_eq!(ids(&b.remaining), ["e"]);
    assert_eq!(b.date, NaiveDate::from_ymd_opt(2026, 3, 8).unwrap());
}

#[test]
fn output_is_deterministic_for_identical_input() {
    let events = [
        event("a", at(13, 0), at(14, 0)),
        event("b", at(13, 0), at(14, 0)),
    ];
    assert_eq!(run(&events, at(12, 0)), run(&events, at(12, 0)));
    assert_eq!(
        ids(&run(&events, at(12, 0)).remaining),
        ["a", "b"],
        "ties break by title then id"
    );
}
