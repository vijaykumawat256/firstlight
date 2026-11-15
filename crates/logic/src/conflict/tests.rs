use super::*;
use chrono::TimeZone;

const LONDON: Tz = chrono_tz::Europe::London;

// 2026-03-10 is a Tuesday; London is on GMT, so local time equals UTC.
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

fn run(
    a: &Event,
    b: &Event,
    all: &[Event],
    mine: &dyn Fn(&Event) -> bool,
    force: Option<&str>,
) -> Option<Help> {
    help(
        a,
        b,
        all,
        mine,
        force,
        t(10, 8, 0),
        LONDON,
        &BriefingSettings::default(),
    )
}

fn nobody(_: &Event) -> bool {
    false
}

fn moved_id(h: &Help) -> &str {
    &h.moved.id
}

#[test]
fn the_one_you_have_not_answered_moves_and_the_reasons_say_so() {
    let a = ev("review", t(10, 10, 0), 60);
    let mut b = ev("sync", t(10, 10, 30), 30);
    b.my_response = Response::NeedsAction;
    let h = run(&a, &b, &[a.clone(), b.clone()], &nobody, None).unwrap();
    assert_eq!(moved_id(&h), "sync");
    assert_eq!(h.overlap_minutes, 30);
    assert!(h.reasons.contains(&Reason::NotAnswered) && h.reasons.contains(&Reason::Shorter));
}

#[test]
fn one_you_created_beats_one_you_accepted() {
    let a = ev("theirs", t(10, 10, 0), 30);
    let b = ev("mine", t(10, 10, 15), 60);
    let h = run(&a, &b, &[a.clone(), b.clone()], &|e| e.id == "mine", None).unwrap();
    assert_eq!(moved_id(&h), "mine");
    assert!(h.yours && h.reasons.contains(&Reason::Yours));
}

#[test]
fn a_tie_moves_the_later_one_and_you_can_overrule() {
    let a = ev("first", t(10, 10, 0), 60);
    let b = ev("second", t(10, 10, 30), 60);
    let all = [a.clone(), b.clone()];
    let h = run(&a, &b, &all, &nobody, None).unwrap();
    assert_eq!(
        (moved_id(&h), h.reasons.as_slice()),
        ("second", [Reason::Later].as_slice())
    );
    let h = run(&a, &b, &all, &nobody, Some("first")).unwrap();
    assert_eq!(
        (moved_id(&h), h.reasons.as_slice()),
        ("first", [Reason::YourChoice].as_slice())
    );
}

#[test]
fn options_start_on_a_quarter_hour_and_prefer_the_same_time_of_day() {
    let a = ev("review", t(10, 10, 0), 60);
    let b = ev("moveme", t(10, 10, 30), 60); // 10:30-11:30, 60 min
    let h = run(&a, &b, &[a.clone(), b.clone()], &nobody, None).unwrap();
    assert_eq!(moved_id(&h), "moveme");
    assert_eq!(h.options.len(), 3, "{:?}", h.options);
    // Today the kept meeting runs to 11:00, so the closest slot to 10:30 starting after it is 11:00.
    assert_eq!(h.options[0].start, t(10, 11, 0));
    assert_eq!(h.options[0].end, t(10, 12, 0));
    // The next working days offer the same time of day.
    assert_eq!(h.options[1].start, t(11, 10, 30));
    assert_eq!(h.options[2].start, t(12, 10, 30));
    assert!(
        h.options
            .iter()
            .all(|o| o.start.minute() % 15 == 0 && (o.end - o.start).num_minutes() == 60)
    );
}

#[test]
fn nothing_is_suggested_on_top_of_other_meetings_or_in_the_past_or_on_weekends() {
    let a = ev("review", t(10, 10, 0), 60);
    let b = ev("moveme", t(10, 10, 30), 60);
    let mut all = vec![a.clone(), b.clone()];
    // Block all of Wednesday and Thursday; Friday the 13th is free, then the weekend.
    all.push(ev("w", t(11, 9, 0), 9 * 60));
    all.push(ev("th", t(12, 9, 0), 9 * 60));
    let h = run(&a, &b, &all, &nobody, None).unwrap();
    let days: Vec<u32> = h
        .options
        .iter()
        .map(|o| chrono::Datelike::day(&o.start))
        .collect();
    assert_eq!(
        days,
        [10, 13, 16],
        "today, Friday, then Monday (no weekend)"
    );
    for o in &h.options {
        assert!(o.start >= t(10, 8, 0));
        assert!(!all.iter().filter(|e| e.id != "moveme").any(|e| matches!(&e.when, When::Timed { start, end, .. } if *start < o.end && *end > o.start)));
    }
}

#[test]
fn the_moved_meeting_does_not_block_its_own_replacement() {
    // Moving a 60 min meeting that sits 10:30-11:30 may land at 11:00 even though 11:00-11:30 is where it is now.
    let a = ev("review", t(10, 9, 30), 90); // 09:30-11:00
    let b = ev("moveme", t(10, 10, 30), 60);
    let h = run(&a, &b, &[a.clone(), b.clone()], &nobody, None).unwrap();
    assert_eq!(h.options[0].start, t(10, 11, 0));
}

#[test]
fn no_overlap_or_an_all_day_event_gives_no_help() {
    let a = ev("a", t(10, 10, 0), 30);
    let b = ev("b", t(10, 11, 0), 30);
    assert!(run(&a, &b, &[a.clone(), b.clone()], &nobody, None).is_none());
    let mut c = ev("c", t(10, 10, 0), 30);
    c.when = When::AllDay {
        start: chrono::NaiveDate::from_ymd_opt(2026, 3, 10).unwrap(),
        end: chrono::NaiveDate::from_ymd_opt(2026, 3, 11).unwrap(),
    };
    assert!(run(&a, &c, &[a.clone(), c.clone()], &nobody, None).is_none());
}

// ---- pile-ups of three or more ------------------------------------------------------------

#[test]
fn two_meetings_are_not_a_pile_up() {
    let a = ev("a", t(10, 10, 0), 60);
    let b = ev("b", t(10, 10, 30), 60);
    let h = run(&a, &b, &[a.clone(), b.clone()], &nobody, None).unwrap();
    assert!(h.pile_up.is_none());
}

#[test]
fn a_chain_of_overlaps_is_found_and_the_fewest_moves_clear_it() {
    // a 10:00-11:00, b 10:30-11:30, c 11:15-12:00: a-b and b-c overlap; a and c do not. Moving just b clears it.
    let a = ev("a", t(10, 10, 0), 60);
    let b = ev("b", t(10, 10, 30), 60);
    let c = ev("c", t(10, 11, 15), 45);
    let all = [a.clone(), b.clone(), c.clone()];
    let h = run(&a, &b, &all, &nobody, None).unwrap();
    let p = h.pile_up.expect("three linked meetings");
    assert_eq!(p.events.len(), 3);
    assert_eq!(p.keep, ["a", "c"]);
    assert_eq!(p.moves.len(), 1);
    assert_eq!(p.moves[0].event.id, "b");
    assert!(!p.moves[0].options.is_empty());
}

#[test]
fn what_is_easy_to_move_is_what_moves() {
    // Three that all overlap each other: only one can stay. The unanswered and the one you made go.
    let keep_me = ev("keep", t(10, 10, 0), 60);
    let mut open = ev("open", t(10, 10, 15), 60);
    open.my_response = Response::NeedsAction;
    let mine = ev("mine", t(10, 10, 30), 60);
    let all = [keep_me.clone(), open.clone(), mine.clone()];
    let h = run(&keep_me, &open, &all, &|e| e.id == "mine", None).unwrap();
    let p = h.pile_up.unwrap();
    assert_eq!(p.keep, ["keep"]);
    let mut moved: Vec<&str> = p.moves.iter().map(|m| m.event.id.as_str()).collect();
    moved.sort();
    assert_eq!(moved, ["mine", "open"]);
    assert!(p.moves.iter().find(|m| m.event.id == "mine").unwrap().yours);
}

#[test]
fn the_places_suggested_for_the_moves_never_collide() {
    let keep_me = ev("keep", t(10, 10, 0), 60);
    let x = ev("x", t(10, 10, 10), 60);
    let y = ev("y", t(10, 10, 20), 60);
    let all = [keep_me.clone(), x.clone(), y.clone()];
    let h = run(&x, &y, &all, &nobody, None).unwrap();
    let p = h.pile_up.unwrap();
    assert_eq!(p.moves.len(), 2);
    let (f0, f1) = (&p.moves[0].options[0], &p.moves[1].options[0]);
    assert!(f0.end <= f1.start || f1.end <= f0.start, "{f0:?} vs {f1:?}");
    // And neither lands on the meeting that stays.
    for f in [f0, f1] {
        assert!(f.end <= t(10, 10, 0) || f.start >= t(10, 11, 0));
    }
}

#[test]
fn a_huge_pile_up_is_left_to_the_pair_view() {
    let mut all = vec![];
    for i in 0..12 {
        all.push(ev(
            &format!("e{i}"),
            t(10, 10, 0) + Duration::minutes(i * 5),
            120,
        ));
    }
    let h = run(&all[0], &all[1], &all, &nobody, None).unwrap();
    assert!(h.pile_up.is_none(), "more than ten is not worked out");
}
