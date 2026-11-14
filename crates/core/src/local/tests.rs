use super::*;
use crate::secrets::{MemorySecretStore, load_or_create_db_key};

const NY: Tz = chrono_tz::America::New_York;

fn store() -> Store {
    let key = load_or_create_db_key(&MemorySecretStore::default()).unwrap();
    Store::open_in_memory(&key).unwrap()
}

fn t(s: &str) -> DateTime<Utc> {
    s.parse().unwrap()
}

fn timed(title: &str) -> EventInput {
    EventInput {
        title: title.into(),
        all_day: false,
        start: "2026-11-20T15:00:00Z".into(),
        end: "2026-11-20T16:00:00Z".into(),
        location: Some(" Room 4 ".into()),
        join_url: Some("https://meet.google.com/abc-defg-hij".into()),
        notes: None,
        repeat: None,
        reminder_minutes: None,
        invitees: vec![],
    }
}

fn weekly(until: Option<&str>) -> RepeatInput {
    RepeatInput {
        every: "weekly".into(),
        interval: 1,
        until: until.map(String::from),
    }
}

fn all(s: &Store) -> Vec<Event> {
    let mut v = s
        .events_between(t("2000-01-01T00:00:00Z"), t("2100-01-01T00:00:00Z"))
        .unwrap();
    v.retain(|e| e.calendar_id == CALENDAR_ID);
    v
}

fn starts(v: &[Event]) -> Vec<DateTime<Utc>> {
    v.iter()
        .filter_map(|e| match &e.when {
            When::Timed { start, .. } => Some(*start),
            _ => None,
        })
        .collect()
}

#[test]
fn adds_edits_and_removes_an_event_that_the_briefing_sees() {
    let mut s = store();
    let now = t("2026-11-14T12:00:00Z");
    let e = save(&mut s, None, &timed("Dentist"), NY, now).unwrap();
    assert_eq!(e.location.as_deref(), Some("Room 4"));
    assert_eq!(s.list_accounts().unwrap().len(), 1);
    assert_eq!(all(&s).len(), 1);

    let mut edit = timed("Dentist (moved)");
    edit.start = "2026-11-21T15:00:00Z".into();
    edit.end = "2026-11-21T16:00:00Z".into();
    let moved = save(&mut s, Some(&e.id), &edit, NY, now).unwrap();
    assert_eq!(moved.id, e.id, "same event, not a copy");
    assert_eq!(all(&s).len(), 1);
    assert_eq!(all(&s)[0].title, "Dentist (moved)");

    delete(&mut s, &e.id, now).unwrap();
    assert!(all(&s).is_empty());
}

#[test]
fn all_day_end_is_inclusive_in_the_form_and_exclusive_inside() {
    let input = EventInput {
        all_day: true,
        start: "2026-12-24".into(),
        end: "2026-12-26".into(),
        ..timed("Holiday")
    };
    let e = build_event(&input, None, NY, Utc::now()).unwrap();
    assert_eq!(
        e.when,
        When::AllDay {
            start: "2026-12-24".parse().unwrap(),
            end: "2026-12-27".parse().unwrap()
        }
    );
}

#[test]
fn a_weekly_event_keeps_its_wall_clock_time_across_daylight_saving() {
    let mut s = store();
    let now = t("2026-10-28T12:00:00Z");
    // Saturday 31 Oct 10:00 New York time (EDT, 14:00 UTC); clocks go back on Sunday 1 Nov.
    let mut input = timed("Pilates");
    input.start = "2026-10-31T14:00:00Z".into();
    input.end = "2026-10-31T15:00:00Z".into();
    input.repeat = Some(weekly(Some("2026-11-21")));
    let base = save(&mut s, None, &input, NY, now).unwrap();

    let v = all(&s);
    assert_eq!(
        starts(&v),
        [
            t("2026-10-31T14:00:00Z"), // 10:00 EDT
            t("2026-11-07T15:00:00Z"), // 10:00 EST: the UTC hour moved, the wall clock did not
            t("2026-11-14T15:00:00Z"),
            t("2026-11-21T15:00:00Z"),
        ]
    );
    assert!(
        v.iter()
            .all(|e| e.recurrence_master_id.as_deref() == Some(base.remote_id.as_str()))
    );
    let ids: std::collections::HashSet<_> = v.iter().map(|e| e.id.clone()).collect();
    assert_eq!(ids.len(), 4, "every occurrence has its own id");
}

#[test]
fn an_open_ended_repeat_reaches_further_as_time_passes() {
    let mut s = store();
    let mut input = timed("Standup");
    input.repeat = Some(RepeatInput {
        every: "daily".into(),
        interval: 1,
        until: None,
    });
    save(&mut s, None, &input, NY, t("2026-11-14T12:00:00Z")).unwrap();
    let before = all(&s).len();
    assert!(before > 60 && before <= MAX_INSTANCES, "{before}");
    refresh(&mut s, t("2027-03-01T12:00:00Z")).unwrap();
    let later = all(&s);
    assert!(
        starts(&later)
            .iter()
            .any(|d| *d > t("2027-04-15T00:00:00Z")),
        "the window moved forward"
    );
    assert!(
        starts(&later)
            .iter()
            .all(|d| *d > t("2027-01-20T00:00:00Z")),
        "and the old past dropped away"
    );
}

#[test]
fn a_series_is_edited_and_deleted_as_a_whole_from_any_occurrence() {
    let mut s = store();
    let now = t("2026-11-14T12:00:00Z");
    let mut input = timed("Book club");
    input.repeat = Some(weekly(Some("2026-12-18")));
    input.reminder_minutes = Some(30);
    input.invitees = vec![
        "Sam@Example.com".into(),
        "sam@example.com".into(),
        "kim@example.org".into(),
    ];
    let first = save(&mut s, None, &input, NY, now).unwrap();
    let occurrence = all(&s)[2].id.clone();
    assert!(occurrence.starts_with(&format!("{CALENDAR_ID}:{}:", first.remote_id)));

    // The form loads the record, not the occurrence.
    let own = get(&s, &occurrence).unwrap().unwrap();
    assert_eq!(own.id, first.id);
    assert_eq!(
        own.input.invitees,
        ["sam@example.com", "kim@example.org"],
        "tidied and de-duplicated"
    );
    assert_eq!(own.input.reminder_minutes, Some(30));
    assert_eq!(own.input.repeat, input.repeat);

    let mut edit = own.input.clone();
    edit.title = "Book club (new venue)".into();
    save(&mut s, Some(&occurrence), &edit, NY, now).unwrap();
    assert!(all(&s).iter().all(|e| e.title == "Book club (new venue)"));
    assert_eq!(all(&s).len(), 5);

    delete(&mut s, &occurrence, now).unwrap();
    assert!(all(&s).is_empty());
    assert!(s.all_own_events().unwrap().is_empty());
}

#[test]
fn all_day_repeats_work() {
    let mut s = store();
    let input = EventInput {
        all_day: true,
        start: "2026-11-16".into(),
        end: "2026-11-16".into(),
        repeat: Some(RepeatInput {
            every: "monthly".into(),
            interval: 1,
            until: Some("2027-01-31".into()),
        }),
        ..timed("Rent day")
    };
    save(&mut s, None, &input, NY, t("2026-11-14T12:00:00Z")).unwrap();
    let days: Vec<_> = all(&s)
        .iter()
        .filter_map(|e| match &e.when {
            When::AllDay { start, .. } => Some(start.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(days, ["2026-11-16", "2026-12-16", "2027-01-16"]);
}

#[test]
fn bad_input_is_refused_with_a_reason() {
    let now = Utc::now();
    let bad = |f: &dyn Fn(&mut EventInput)| {
        let mut i = timed("x");
        f(&mut i);
        let mut s = store();
        save(&mut s, None, &i, NY, now).unwrap_err().to_string()
    };
    assert!(bad(&|i| i.title = "  ".into()).contains("title"));
    assert!(bad(&|i| i.title = "a".repeat(201)).contains("too long"));
    assert!(bad(&|i| i.title = "a\nb".into()).contains("control"));
    assert!(bad(&|i| i.end = i.start.clone()).contains("end after"));
    assert!(bad(&|i| i.start = "tomorrow".into()).contains("valid"));
    assert!(bad(&|i| i.end = "2030-01-01T00:00:00Z".into()).contains("a year"));
    assert!(bad(&|i| i.join_url = Some("http://zoom.us/j/1".into())).contains("https"));
    assert!(bad(&|i| i.join_url = Some("javascript:alert(1)".into())).contains("https"));
    assert!(bad(&|i| i.join_url = Some("https://a.com/x y".into())).contains("single link"));
    assert!(bad(&|i| i.notes = Some("n".repeat(4001))).contains("too long"));
    assert!(
        bad(&|i| {
            i.all_day = true;
            i.start = "2026-12-05".into();
            i.end = "2026-12-01".into()
        })
        .contains("before")
    );
    // repeats, reminders, invitees
    let rep = |every: &str, interval: u32, until: Option<&str>| RepeatInput {
        every: every.into(),
        interval,
        until: until.map(String::from),
    };
    assert!(bad(&|i| i.repeat = Some(rep("hourly", 1, None))).contains("how often"));
    assert!(
        bad(&|i| i.repeat = Some(rep("FREQ=DAILY;COUNT=999999", 1, None))).contains("how often")
    );
    assert!(bad(&|i| i.repeat = Some(rep("daily", 0, None))).contains("1 to 30"));
    assert!(bad(&|i| i.repeat = Some(rep("daily", 31, None))).contains("1 to 30"));
    assert!(
        bad(&|i| i.repeat = Some(rep("daily", 1, Some("2020-01-01")))).contains("before it starts")
    );
    assert!(bad(&|i| i.repeat = Some(rep("daily", 1, Some("2099-01-01")))).contains("20 years"));
    assert!(bad(&|i| i.repeat = Some(rep("daily", 1, Some("soon")))).contains("valid dates"));
    assert!(bad(&|i| i.reminder_minutes = Some(0)).contains("reminder"));
    assert!(bad(&|i| i.reminder_minutes = Some(99_999)).contains("reminder"));
    assert!(
        bad(&|i| {
            i.all_day = true;
            i.start = "2026-12-01".into();
            i.end = "2026-12-01".into();
            i.reminder_minutes = Some(10)
        })
        .contains("start time")
    );
    assert!(bad(&|i| i.invitees = vec!["not an email".into()]).contains("email"));
    assert!(
        bad(&|i| i.invitees = (0..21).map(|n| format!("u{n}@x.org")).collect())
            .contains("too many")
    );
}

#[test]
fn only_our_own_events_can_be_touched() {
    let mut s = store();
    let now = Utc::now();
    let real = save(&mut s, None, &timed("Mine"), NY, now).unwrap();
    for id in [
        "fixture-work:standup",
        "local-events:",
        "local-events:a/b",
        "local-events:x:12a",
        "../local-events:x",
        "",
        "local-events:nope",
    ] {
        assert!(
            save(&mut s, Some(id), &timed("x"), NY, now).is_err(),
            "{id}"
        );
        assert!(get(&s, id).map(|o| o.is_none()).unwrap_or(true), "{id}");
    }
    for id in [
        "fixture-work:standup",
        "local-events:",
        "local-events:a/b",
        "",
        "local-events:x:12a",
    ] {
        assert!(delete(&mut s, id, now).is_err(), "{id}");
    }
    assert_eq!(all(&s).len(), 1);
    assert_eq!(all(&s)[0].id, real.id);
}

#[test]
fn removing_the_account_removes_the_events_for_good() {
    let mut s = store();
    let now = Utc::now();
    let mut input = timed("Private");
    input.repeat = Some(weekly(None));
    save(&mut s, None, &input, NY, now).unwrap();
    assert!(!all(&s).is_empty());
    s.remove_account(ACCOUNT_ID).unwrap();
    assert!(s.all_own_events().unwrap().is_empty());
    refresh(&mut s, now).unwrap(); // must not bring anything back
    assert!(all(&s).is_empty());
    assert!(s.get_account(ACCOUNT_ID).unwrap().is_none());
}

#[test]
fn erase_everything_removes_them_too() {
    let mut s = store();
    save(&mut s, None, &timed("Private"), NY, Utc::now()).unwrap();
    s.erase_all().unwrap();
    assert!(s.all_own_events().unwrap().is_empty());
}

#[test]
fn reminders_fire_inside_their_window_once_per_occurrence() {
    let mut s = store();
    let mut input = timed("Call mum");
    input.start = "2026-11-20T15:00:00Z".into();
    input.end = "2026-11-20T15:30:00Z".into();
    input.reminder_minutes = Some(15);
    input.repeat = Some(RepeatInput {
        every: "daily".into(),
        interval: 1,
        until: Some("2026-11-22".into()),
    });
    save(&mut s, None, &input, NY, t("2026-11-14T12:00:00Z")).unwrap();

    assert!(
        due_reminders(&s, t("2026-11-20T14:44:00Z"))
            .unwrap()
            .is_empty(),
        "too early"
    );
    let due = due_reminders(&s, t("2026-11-20T14:45:00Z")).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].title, "Call mum");
    assert!(due[0].id.ends_with(":15"));
    assert!(
        due_reminders(&s, t("2026-11-20T15:00:01Z"))
            .unwrap()
            .is_empty(),
        "already started"
    );
    // The next day is a different occurrence with a different id.
    let next = due_reminders(&s, t("2026-11-21T14:50:00Z")).unwrap();
    assert_eq!(next.len(), 1);
    assert_ne!(next[0].id, due[0].id);

    // The general heads-up leaves events that have their own reminder alone.
    let map = reminder_map(&s).unwrap();
    let ev = &all(&s)[0];
    assert!(has_own_reminder(&map, ev));
    let mut other = ev.clone();
    other.calendar_id = "google:x".into();
    assert!(!has_own_reminder(&map, &other));
}

#[test]
fn an_event_without_a_reminder_costs_nothing() {
    let mut s = store();
    save(&mut s, None, &timed("Plain"), NY, t("2026-11-14T12:00:00Z")).unwrap();
    assert!(
        due_reminders(&s, t("2026-11-20T14:50:00Z"))
            .unwrap()
            .is_empty()
    );
    assert!(reminder_map(&s).unwrap().is_empty());
}

#[test]
fn an_unreadable_record_does_not_hide_the_others() {
    let mut s = store();
    let now = t("2026-11-14T12:00:00Z");
    save(&mut s, None, &timed("Good"), NY, now).unwrap();
    s.put_own_event("broken", "{not json").unwrap();
    refresh(&mut s, now).unwrap();
    assert_eq!(all(&s).len(), 1);
}
