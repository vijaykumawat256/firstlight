use super::*;
use chrono::TimeZone;

const LONDON: Tz = chrono_tz::Europe::London;

fn t(day: u32, h: u32, m: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, day, h, m, 0).unwrap()
}

fn ev(id: &str, title: &str, start: DateTime<Utc>, mins: i64) -> Event {
    Event {
        id: id.into(),
        calendar_id: "c".into(),
        remote_id: id.into(),
        ical_uid: id.into(),
        title: title.into(),
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

fn prep(target: &Event, all: &[Event]) -> Prep {
    prepare(target, all, LONDON, &BriefingSettings::default())
}

#[test]
fn earlier_times_of_the_same_meeting_come_newest_first_with_their_notes() {
    let target = ev("now", "Design review", t(10, 10, 0), 60);
    let mut last = ev("w1", "design  review", t(3, 10, 0), 60);
    last.description = Some(
        "Agree the nav pattern.\nSee https://x.test/doc?pw=secret for mock-ups. Priya to share."
            .into(),
    );
    let all = [
        target.clone(),
        ev("w3", "Design review", t(17, 10, 0), 60),
        last,
        ev("w2", "Design review", t(24, 10, 0) - Duration::days(30), 60),
        ev("other", "Lunch", t(3, 12, 0), 60),
    ];
    let p = prep(&target, &all);
    assert_eq!(
        p.earlier.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        ["w1", "w2"]
    );
    assert_eq!(
        p.earlier[0].notes.as_deref(),
        Some("Agree the nav pattern. See for mock-ups. Priya to share."),
        "links are dropped (they can carry passwords)"
    );
    assert_eq!(p.next_time.as_ref().map(|n| n.id.as_str()), Some("w3"));
}

#[test]
fn a_series_matches_by_its_id_even_when_the_title_was_renamed() {
    let mut target = ev("now", "Weekly sync", t(10, 10, 0), 30);
    target.ical_uid = "series-1".into();
    let mut old = ev("old", "Weekly sync (moved)", t(3, 10, 0), 30);
    old.ical_uid = "series-1".into();
    let p = prep(&target, &[target.clone(), old]);
    assert_eq!(p.earlier.len(), 1);
}

#[test]
fn short_titles_are_not_matched_by_name_alone() {
    let target = ev("now", "1:1", t(10, 10, 0), 30);
    let p = prep(&target, &[target.clone(), ev("o", "1:1", t(3, 10, 0), 30)]);
    assert!(p.earlier.is_empty(), "too short to mean the same meeting");
}

#[test]
fn at_most_three_earlier_times_and_cancelled_or_declined_ones_are_ignored() {
    let target = ev("now", "Board prep", t(20, 9, 0), 60);
    let mut all = vec![target.clone()];
    for d in 1..=6 {
        all.push(ev(&format!("e{d}"), "Board prep", t(d, 9, 0), 60));
    }
    all[5].status = EventStatus::Cancelled; // e5
    all[6].my_response = Response::Declined; // e6
    let p = prep(&target, &all);
    assert_eq!(
        p.earlier.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        ["e4", "e3", "e2"]
    );
}

#[test]
fn what_comes_right_before_and_after_is_named_with_the_gap() {
    let target = ev("now", "Review", t(10, 10, 0), 60);
    let all = [
        target.clone(),
        ev("a", "Standup", t(10, 9, 30), 25), // ends 09:55, 5 min before
        ev("b", "Earlier call", t(10, 8, 0), 30), // ends 08:30, further away
        ev("c", "Retro", t(10, 11, 0), 30),   // starts as this ends
        ev("d", "Far later", t(10, 16, 0), 30), // more than four hours after
    ];
    let p = prep(&target, &all);
    assert_eq!(
        p.before,
        Some(Neighbour {
            id: "a".into(),
            title: "Standup".into(),
            gap_minutes: 5
        })
    );
    assert_eq!(
        p.after,
        Some(Neighbour {
            id: "c".into(),
            title: "Retro".into(),
            gap_minutes: 0
        })
    );
    assert!(p.overlaps.is_empty());
}

#[test]
fn an_overlap_is_reported_with_how_long_it_lasts() {
    let target = ev("now", "Review", t(10, 10, 0), 60);
    let all = [target.clone(), ev("x", "1:1 with Sam", t(10, 10, 45), 60)];
    let p = prep(&target, &all);
    assert_eq!(
        p.overlaps,
        vec![Overlap {
            id: "x".into(),
            title: "1:1 with Sam".into(),
            minutes: 15
        }]
    );
    assert!(
        p.before.is_none() && p.after.is_none(),
        "an overlap is not a neighbour"
    );
}

#[test]
fn all_day_events_have_no_prep_and_do_not_count_as_neighbours() {
    let mut allday = ev("h", "Holiday", t(10, 0, 0), 0);
    allday.when = When::AllDay {
        start: chrono::NaiveDate::from_ymd_opt(2026, 3, 10).unwrap(),
        end: chrono::NaiveDate::from_ymd_opt(2026, 3, 11).unwrap(),
    };
    let target = ev("now", "Review", t(10, 10, 0), 60);
    let p = prep(&target, &[target.clone(), allday.clone()]);
    assert!(p.is_empty());
    assert!(prep(&allday, &[allday.clone(), target]).is_empty());
}

#[test]
fn notes_are_one_short_paragraph() {
    assert_eq!(clean_notes("  \n\t "), None);
    assert_eq!(clean_notes("https://zoom.us/j/1"), None);
    let long = "word ".repeat(200);
    let n = clean_notes(&long).unwrap();
    assert!(n.chars().count() <= 300 && n.ends_with('…'));
}
