use super::*;
use chrono::TimeZone;

const LONDON: Tz = chrono_tz::Europe::London;

fn d(m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, m, day).unwrap()
}
fn t(day: u32, h: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, day, h, 0, 0).unwrap()
}
fn plan(reply: &str) -> Option<Understood> {
    parse_plan(reply, d(3, 10))
}
fn event(id: &str, title: &str, place: Option<&str>, start: DateTime<Utc>, hours: i64) -> Event {
    Event {
        id: id.into(),
        calendar_id: "c".into(),
        remote_id: id.into(),
        ical_uid: id.into(),
        title: title.into(),
        when: When::Timed {
            start,
            end: start + Duration::hours(hours),
            tzid: None,
        },
        location: place.map(Into::into),
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

#[test]
fn a_clean_reply_becomes_a_plan() {
    let p = plan(
        r#"{"kind":"events","from":"2026-03-11","to":"2026-03-13","keywords":["Lunch","  Sam "],"minutes":null}"#,
    );
    assert_eq!(
        p,
        Some(Understood::Plan(Plan {
            kind: Kind::Events,
            from: d(3, 11),
            to: d(3, 13),
            keywords: vec!["lunch".into(), "sam".into()],
            minutes: 30,
        }))
    );
}

#[test]
fn prose_or_a_code_fence_around_the_json_is_tolerated() {
    let r = "Sure!\n```json\n{\"kind\":\"free\",\"from\":\"2026-03-11\",\"to\":\"2026-03-12\",\"minutes\":45}\n```";
    assert!(
        matches!(plan(r), Some(Understood::Plan(p)) if p.kind == Kind::Free && p.minutes == 45)
    );
}

#[test]
fn a_question_that_is_not_about_the_calendar_is_reported_as_such() {
    assert_eq!(
        plan(r#"{"kind":"none"}"#),
        Some(Understood::NotACalendarQuestion)
    );
}

#[test]
fn anything_unreasonable_is_rejected_and_nothing_runs() {
    for bad in [
        "",
        "no json here",
        r#"{"kind":"delete","from":"2026-03-11","to":"2026-03-11"}"#,
        r#"{"kind":"events","from":"yesterday","to":"today"}"#,
        r#"{"kind":"events","from":"2026-03-12","to":"2026-03-11"}"#,
        r#"{"kind":"events","from":"2026-03-01","to":"2026-12-31"}"#,
        r#"{"kind":"events","from":"2031-03-01","to":"2031-03-02"}"#,
        r#"[1,2,3]"#,
    ] {
        assert_eq!(plan(bad), None, "{bad}");
    }
}

#[test]
fn keywords_are_cleaned_capped_and_cannot_be_anything_but_words() {
    let r = r#"{"kind":"events","from":"2026-03-11","to":"2026-03-11",
      "keywords":["a","b","c","d","e","f","g","<script>x</script>", 7, null, "", "this one is far far too long to be a sensible search word at all"]}"#;
    let Some(Understood::Plan(p)) = plan(r) else {
        panic!()
    };
    assert_eq!(p.keywords.len(), 5);
    assert!(
        p.keywords
            .iter()
            .all(|k| k.chars().all(|c| c.is_alphanumeric() || " '-".contains(c)))
    );
    let r = r#"{"kind":"events","from":"2026-03-11","to":"2026-03-11","keywords":["<script>x</script>"]}"#;
    let Some(Understood::Plan(p)) = plan(r) else {
        panic!()
    };
    assert_eq!(p.keywords, vec!["scriptxscript".to_string()]);
}

#[test]
fn free_time_questions_are_limited_to_three_weeks_and_five_minutes_or_more() {
    let r = r#"{"kind":"free","from":"2026-03-11","to":"2026-05-30","minutes":1}"#;
    let Some(Understood::Plan(p)) = plan(r) else {
        panic!()
    };
    assert_eq!((p.to - p.from).num_days(), 20);
    assert_eq!(p.minutes, 5);
}

fn week() -> Vec<Event> {
    vec![
        event("a", "Lunch with Sam", Some("Cafe Nero"), t(11, 12), 1),
        event("b", "Dentist", Some("Smile Clinic"), t(12, 9), 1),
        event("c", "Team lunch", None, t(13, 12), 1),
        event("d", "Dentist follow-up", None, t(20, 9), 1),
    ]
}

fn ask(p: Plan, events: &[Event]) -> Answer {
    answer(&p, events, t(10, 0), LONDON, &BriefingSettings::default())
}

fn events_plan(from: NaiveDate, to: NaiveDate, words: &[&str]) -> Plan {
    Plan {
        kind: Kind::Events,
        from,
        to,
        keywords: words.iter().map(|w| w.to_string()).collect(),
        minutes: 30,
    }
}

#[test]
fn every_keyword_must_match_the_title_or_place_inside_the_dates() {
    let w = week();
    let a = ask(events_plan(d(3, 11), d(3, 15), &["lunch"]), &w);
    assert_eq!(a.count, 2, "both lunches, not the dentist");
    let a = ask(events_plan(d(3, 11), d(3, 15), &["lunch", "sam"]), &w);
    assert_eq!(
        a.events.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        ["a"]
    );
    let a = ask(events_plan(d(3, 11), d(3, 15), &["clinic"]), &w);
    assert_eq!(a.count, 1, "the place counts");
    let a = ask(events_plan(d(3, 11), d(3, 15), &["dentist"]), &w);
    assert_eq!(a.count, 1, "the follow-up on the 20th is outside the dates");
}

#[test]
fn no_keywords_lists_the_range_in_order_and_hides_cancelled_and_declined() {
    let mut w = week();
    w[1].status = EventStatus::Cancelled;
    w[2].my_response = Response::Declined;
    let a = ask(events_plan(d(3, 11), d(3, 13), &[]), &w);
    assert_eq!(
        a.events.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        ["a"]
    );
}

#[test]
fn the_list_is_capped_but_the_count_is_honest() {
    let many: Vec<Event> = (0..80)
        .map(|i| {
            event(
                &format!("e{i}"),
                "Standup",
                None,
                t(11, 8) + Duration::minutes(i),
                0,
            )
        })
        .collect();
    let a = ask(events_plan(d(3, 11), d(3, 11), &["standup"]), &many);
    assert_eq!(a.count, 80);
    assert_eq!(a.events.len(), MAX_EVENTS);
}

#[test]
fn an_all_day_event_in_range_is_found() {
    let mut e = event("h", "Holiday", None, t(11, 0), 1);
    e.when = When::AllDay {
        start: d(3, 11),
        end: d(3, 12),
    };
    let a = ask(events_plan(d(3, 11), d(3, 11), &["holiday"]), &[e.clone()]);
    assert_eq!(a.count, 1);
    let a = ask(events_plan(d(3, 12), d(3, 12), &["holiday"]), &[e]);
    assert_eq!(a.count, 0, "it ended the day before");
}

#[test]
fn free_time_uses_the_same_rules_as_find_a_time() {
    let w = vec![event("a", "Long meeting", None, t(11, 9), 8)];
    let p = Plan {
        kind: Kind::Free,
        from: d(3, 11),
        to: d(3, 12),
        keywords: vec![],
        minutes: 60,
    };
    let a = ask(p, &w);
    // The 11th is blocked 09:00-17:00 so only 17:00-18:00 is left; the 12th is wide open.
    assert_eq!(a.slots.len(), 2);
    assert_eq!(a.slots[0].minutes, 60);
    assert!(a.events.is_empty());
}
