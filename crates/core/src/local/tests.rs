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

#[test]
fn one_day_can_be_removed_from_a_series_and_stays_removed() {
    let mut s = store();
    let now = t("2026-11-14T12:00:00Z");
    let mut input = timed("Standup");
    input.repeat = Some(RepeatInput {
        every: "daily".into(),
        interval: 1,
        until: Some("2026-11-24".into()),
    });
    save(&mut s, None, &input, NY, now).unwrap();
    let before = all(&s);
    assert_eq!(before.len(), 5);
    let victim = before[2].clone();

    skip_occurrence(&mut s, &victim.id, now).unwrap();
    let after = all(&s);
    assert_eq!(after.len(), 4);
    assert!(after.iter().all(|e| e.id != victim.id));
    // Doing it twice is harmless; editing the series keeps the day removed; a later refresh keeps it too.
    skip_occurrence(&mut s, &victim.id, now).unwrap();
    let mut edit = get(&s, &victim.id).unwrap().unwrap().input;
    edit.title = "Standup (new)".into();
    save(&mut s, Some(&victim.id), &edit, NY, now).unwrap();
    refresh(&mut s, now).unwrap();
    let end = all(&s);
    assert_eq!(end.len(), 4);
    assert!(
        end.iter()
            .all(|e| e.id != victim.id && e.title == "Standup (new)")
    );
}

#[test]
fn only_a_day_of_a_real_series_can_be_removed() {
    let mut s = store();
    let now = t("2026-11-14T12:00:00Z");
    let single = save(&mut s, None, &timed("One-off"), NY, now).unwrap();
    assert!(
        skip_occurrence(&mut s, &single.id, now).is_err(),
        "no occurrence part"
    );
    assert!(
        skip_occurrence(&mut s, &format!("{}:123", single.id), now).is_err(),
        "not a series"
    );
    for bad in [
        "fixture-work:x:1",
        "local-events:",
        "local-events:zzz:5",
        "",
    ] {
        assert!(skip_occurrence(&mut s, bad, now).is_err(), "{bad}");
    }
    assert_eq!(all(&s).len(), 1);
}

#[test]
fn the_export_is_a_calendar_file_that_reads_back_as_the_same_events() {
    let mut s = store();
    let now = t("2026-11-14T12:00:00Z");
    assert!(
        export_all(&s, now).unwrap().is_none(),
        "nothing to export yet"
    );

    save(&mut s, None, &timed("Dentist"), NY, now).unwrap();
    let mut rep = timed("Standup");
    rep.start = "2026-11-16T15:00:00Z".into();
    rep.end = "2026-11-16T15:30:00Z".into();
    rep.repeat = Some(RepeatInput {
        every: "daily".into(),
        interval: 1,
        until: Some("2026-11-22".into()),
    });
    rep.reminder_minutes = Some(15);
    rep.notes = Some("Bring the notes, please; line two".into());
    let series = save(&mut s, None, &rep, NY, now).unwrap();
    let victim = all(&s)
        .into_iter()
        .find(|e| {
            e.id.starts_with(&format!("{CALENDAR_ID}:{}:", series.remote_id))
        })
        .unwrap();
    skip_occurrence(&mut s, &victim.id, now).unwrap();
    let allday = EventInput {
        all_day: true,
        start: "2026-12-24".into(),
        end: "2026-12-26".into(),
        repeat: None,
        reminder_minutes: None,
        ..timed("Holiday")
    };
    save(&mut s, None, &allday, NY, now).unwrap();

    let (text, count) = export_all(&s, now).unwrap().unwrap();
    assert_eq!(count, 3, "a series counts once");
    assert_eq!(text.matches("BEGIN:VEVENT").count(), 3);
    assert!(text.contains("BEGIN:VALARM") && text.contains("TRIGGER:-PT15M"));
    assert!(
        text.contains("EXDATE;TZID=America/New_York:"),
        "the removed day is written as an exception"
    );
    assert!(text.lines().all(|l| l.len() <= 75), "folded");

    // Reading it back gives exactly the events the app shows, removed day included.
    let w = crate::sync::default_window(now);
    let back = crate::ics::events_from_ics(
        &text,
        &crate::ics::ParseOptions {
            calendar_id: "x",
            default_tz: NY,
            window: w,
            my_email: None,
        },
    )
    .unwrap();
    let key = |e: &Event| match &e.when {
        When::Timed { start, .. } => (e.title.clone(), start.timestamp().to_string()),
        When::AllDay { start, .. } => (e.title.clone(), start.to_string()),
    };
    let mut here: Vec<_> = all(&s).iter().map(key).collect();
    let mut there: Vec<_> = back.iter().map(key).collect();
    here.sort();
    there.sort();
    assert_eq!(here, there);
    assert_eq!(
        back.iter().filter(|e| e.title == "Standup").count(),
        6,
        "7 days minus the one removed"
    );
}

// ---- saving into the Calendar app ------------------------------------------------------------

use crate::providers::system::{
    Access, SysCalendar, SysEvent, SysFreq, SysNewEvent, SysWhen, SystemSource,
};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A stand-in Calendar app that records what it is asked to do.
#[derive(Default)]
struct FakeCalendarApp {
    saved: Mutex<Vec<(String, Option<String>, SysNewEvent)>>,
    deleted: Mutex<Vec<String>>,
    fail_save: bool,
    fail_delete: bool,
    next: AtomicUsize,
}

impl SystemSource for FakeCalendarApp {
    fn access(&self) -> Access {
        Access::Granted
    }
    fn request_access(&self) -> Result<Access> {
        Ok(Access::Granted)
    }
    fn calendars(&self) -> Result<Vec<SysCalendar>> {
        Ok(vec![])
    }
    fn events(&self, _: &str, _: DateTime<Utc>, _: DateTime<Utc>) -> Result<Vec<SysEvent>> {
        Ok(vec![])
    }
    fn save_event(
        &self,
        calendar_id: &str,
        existing: Option<&str>,
        e: &SysNewEvent,
        _tz: Tz,
    ) -> Result<String> {
        if self.fail_save {
            return Err(Error::Provider("calendar refused".into()));
        }
        let id = existing.map_or_else(
            || format!("EK-{}", self.next.fetch_add(1, Ordering::SeqCst) + 1),
            String::from,
        );
        self.saved.lock().unwrap().push((
            calendar_id.into(),
            existing.map(String::from),
            e.clone(),
        ));
        Ok(id)
    }
    fn delete_event(&self, event_id: &str) -> Result<()> {
        if self.fail_delete {
            return Err(Error::Provider("calendar refused".into()));
        }
        self.deleted.lock().unwrap().push(event_id.into());
        Ok(())
    }
}

/// What the sync would later bring back for an event saved in the Calendar app: a row in a system calendar.
fn as_synced(s: &mut Store, system_event_id: &str, title: &str) -> Event {
    let account = Account {
        id: crate::providers::system::SYSTEM_ACCOUNT_ID.into(),
        provider: ProviderKind::System,
        display_name: "Calendar app".into(),
        email: None,
        status: AccountStatus::Ok,
        secret_ref: None,
        created_at: Utc::now(),
    };
    s.upsert_account(&account).unwrap();
    let cal = Calendar {
        id: format!("{}:abc", account.id),
        account_id: account.id.clone(),
        remote_id: "EK-CAL".into(),
        name: "Work".into(),
        colour: "blue".into(),
        visible: true,
        read_only: false,
    };
    s.upsert_calendar(&cal).unwrap();
    let mut e = build_event(&timed(title), Some("zzz"), NY, Utc::now()).unwrap();
    e.id = format!("{}:{system_event_id}:1", cal.id);
    e.calendar_id = cal.id.clone();
    e.remote_id = system_event_id.into();
    s.upsert_events(&cal.id, std::slice::from_ref(&e)).unwrap();
    e
}

#[test]
fn an_event_saved_to_the_calendar_app_is_sent_there_and_not_shown_twice_here() {
    let mut s = store();
    let app = FakeCalendarApp::default();
    let now = t("2026-11-14T12:00:00Z");
    let mut input = timed("Dentist");
    input.reminder_minutes = Some(30);
    input.invitees = vec!["sam@example.com".into()];
    let e = save_to_calendar(&mut s, &app, None, None, "EK-CAL", &input, NY, now).unwrap();

    // The Calendar app got exactly what the form held (no attendees: it cannot add them).
    let saved = app.saved.lock().unwrap();
    assert_eq!(saved.len(), 1);
    let (cal, existing, sys) = &saved[0];
    assert_eq!((cal.as_str(), existing.as_deref()), ("EK-CAL", None));
    assert_eq!(sys.title, "Dentist");
    assert_eq!(
        sys.when,
        SysWhen::Timed {
            start: t("2026-11-20T15:00:00Z"),
            end: t("2026-11-20T16:00:00Z")
        }
    );
    assert_eq!(sys.location.as_deref(), Some("Room 4"));
    assert_eq!(
        sys.url.as_deref(),
        Some("https://meet.google.com/abc-defg-hij")
    );
    assert_eq!(sys.alarm_minutes, Some(30));
    assert!(sys.repeat.is_none());

    // Our record points at it; no local copy is generated (the sync brings the real one back).
    assert!(
        all(&s).is_empty(),
        "nothing is shown from the local calendar"
    );
    assert!(s.get_account(ACCOUNT_ID).unwrap().is_some() || s.all_own_events().unwrap().len() == 1);
    let own = get(&s, &e.id).unwrap().unwrap();
    assert_eq!(own.saved_to.as_deref(), Some("EK-CAL"));
    assert_eq!(
        own.input.invitees,
        ["sam@example.com"],
        "the invitees are kept for Send invitation"
    );
}

#[test]
fn the_synced_copy_is_recognised_as_ours_and_can_be_edited_and_deleted() {
    let mut s = store();
    let app = FakeCalendarApp::default();
    let now = t("2026-11-14T12:00:00Z");
    save_to_calendar(
        &mut s,
        &app,
        None,
        None,
        "EK-CAL",
        &timed("Dentist"),
        NY,
        now,
    )
    .unwrap();
    let synced = as_synced(&mut s, "EK-1", "Dentist");
    // A different event in the same calendar that Firstlight did not create is not ours.
    let other = as_synced(&mut s, "EK-OTHER", "Someone else's");
    assert!(get(&s, &other.id).unwrap().is_none());
    assert!(resolve(&s, &other.id).unwrap().is_none());

    // Opening the synced row finds our record.
    let own = get(&s, &synced.id).unwrap().expect("ours");
    assert_eq!(own.saved_to.as_deref(), Some("EK-CAL"));

    // Editing sends the change to the SAME Calendar-app event, whichever calendar the form names.
    let mut edit = own.input.clone();
    edit.title = "Dentist (moved)".into();
    save_to_calendar(
        &mut s,
        &app,
        None,
        Some(&synced.id),
        "SOMEWHERE-ELSE",
        &edit,
        NY,
        now,
    )
    .unwrap();
    {
        let saved = app.saved.lock().unwrap();
        assert_eq!(saved.len(), 2);
        assert_eq!(
            (saved[1].0.as_str(), saved[1].1.as_deref()),
            ("EK-CAL", Some("EK-1"))
        );
        assert_eq!(saved[1].2.title, "Dentist (moved)");
    }
    assert_eq!(
        s.all_own_events().unwrap().len(),
        1,
        "same record, not a copy"
    );

    // Deleting removes it from the Calendar app first, then our record.
    delete_with(&mut s, Some(&app), &synced.id, now).unwrap();
    assert_eq!(app.deleted.lock().unwrap().as_slice(), ["EK-1"]);
    assert!(s.all_own_events().unwrap().is_empty());
}

#[test]
fn a_failure_in_the_calendar_app_leaves_nothing_half_saved() {
    let mut s = store();
    let now = t("2026-11-14T12:00:00Z");
    // Saving fails: no record at all.
    let broken = FakeCalendarApp {
        fail_save: true,
        ..Default::default()
    };
    assert!(save_to_calendar(&mut s, &broken, None, None, "EK-CAL", &timed("x"), NY, now).is_err());
    assert!(s.all_own_events().unwrap().is_empty());

    // Deleting fails: the record stays, so it can be tried again.
    let app = FakeCalendarApp::default();
    save_to_calendar(&mut s, &app, None, None, "EK-CAL", &timed("keep"), NY, now).unwrap();
    let synced = as_synced(&mut s, "EK-1", "keep");
    let no_delete = FakeCalendarApp {
        fail_delete: true,
        ..Default::default()
    };
    assert!(delete_with(&mut s, Some(&no_delete), &synced.id, now).is_err());
    assert_eq!(s.all_own_events().unwrap().len(), 1);

    // Without a Calendar app handed in (the setting is off) an event that lives there cannot be deleted.
    assert!(delete_with(&mut s, None, &synced.id, now).is_err());
    assert_eq!(s.all_own_events().unwrap().len(), 1);
}

#[test]
fn bad_input_never_reaches_the_calendar_app() {
    let mut s = store();
    let app = FakeCalendarApp::default();
    let now = t("2026-11-14T12:00:00Z");
    let mut bad = timed("");
    assert!(save_to_calendar(&mut s, &app, None, None, "EK-CAL", &bad, NY, now).is_err());
    bad = timed("x");
    bad.join_url = Some("http://insecure.example.com".into());
    assert!(save_to_calendar(&mut s, &app, None, None, "EK-CAL", &bad, NY, now).is_err());
    bad = timed("x");
    bad.end = bad.start.clone();
    assert!(save_to_calendar(&mut s, &app, None, None, "EK-CAL", &bad, NY, now).is_err());
    assert!(app.saved.lock().unwrap().is_empty(), "nothing was sent");
    assert!(s.all_own_events().unwrap().is_empty());
}

#[test]
fn only_events_firstlight_created_can_be_changed_in_the_calendar_app() {
    let mut s = store();
    let app = FakeCalendarApp::default();
    let now = t("2026-11-14T12:00:00Z");
    let other = as_synced(&mut s, "EK-THEIRS", "Not ours");
    // An event from a calendar is refused for edit, delete, share and save.
    assert!(
        save_to_calendar(
            &mut s,
            &app,
            None,
            Some(&other.id),
            "EK-CAL",
            &timed("hijack"),
            NY,
            now
        )
        .is_err()
    );
    assert!(delete_with(&mut s, Some(&app), &other.id, now).is_err());
    assert!(share_parts(&s, &other.id).unwrap().is_none());
    assert!(save(&mut s, Some(&other.id), &timed("hijack"), NY, now).is_err());
    assert!(app.saved.lock().unwrap().is_empty() && app.deleted.lock().unwrap().is_empty());
    // A purely local event cannot be "edited into" the Calendar app by passing its id.
    let local = save(&mut s, None, &timed("mine"), NY, now).unwrap();
    assert!(
        save_to_calendar(
            &mut s,
            &app,
            None,
            Some(&local.id),
            "EK-CAL",
            &timed("mine"),
            NY,
            now
        )
        .is_err()
    );
    assert!(app.saved.lock().unwrap().is_empty());
    // And a Calendar-app event cannot be changed through the local-only path.
    save_to_calendar(
        &mut s,
        &app,
        None,
        None,
        "EK-CAL",
        &timed("remote"),
        NY,
        now,
    )
    .unwrap();
    let synced = as_synced(&mut s, "EK-1", "remote");
    assert!(save(&mut s, Some(&synced.id), &timed("x"), NY, now).is_err());
}

#[test]
fn repeats_alerts_and_all_day_events_are_passed_on_faithfully() {
    let mut s = store();
    let app = FakeCalendarApp::default();
    let now = t("2026-11-14T12:00:00Z");
    let mut rep = timed("Standup");
    rep.repeat = Some(RepeatInput {
        every: "weekly".into(),
        interval: 2,
        until: Some("2026-12-31".into()),
    });
    save_to_calendar(&mut s, &app, None, None, "EK-CAL", &rep, NY, now).unwrap();
    let allday = EventInput {
        all_day: true,
        start: "2026-12-24".into(),
        end: "2026-12-26".into(),
        repeat: None,
        reminder_minutes: None,
        ..timed("Holiday")
    };
    save_to_calendar(&mut s, &app, None, None, "EK-CAL", &allday, NY, now).unwrap();
    let saved = app.saved.lock().unwrap();
    let r = saved[0].2.repeat.as_ref().unwrap();
    assert_eq!(
        (r.freq, r.interval, r.until),
        (SysFreq::Weekly, 2, Some("2026-12-31".parse().unwrap()))
    );
    assert_eq!(
        saved[1].2.when,
        SysWhen::AllDay {
            first: "2026-12-24".parse().unwrap(),
            last: "2026-12-26".parse().unwrap()
        },
        "the last day is inclusive"
    );
}

#[test]
fn events_in_the_calendar_app_are_not_exported_or_reminded_about_twice() {
    let mut s = store();
    let app = FakeCalendarApp::default();
    let now = t("2026-11-14T12:00:00Z");
    let mut input = timed("In the Calendar app");
    input.reminder_minutes = Some(15);
    save_to_calendar(&mut s, &app, None, None, "EK-CAL", &input, NY, now).unwrap();
    // Exporting would only make a copy of something already there; the alert lives in the Calendar app.
    assert!(export_all(&s, now).unwrap().is_none());
    assert!(reminder_map(&s).unwrap().is_empty());
    assert!(
        due_reminders(&s, t("2026-11-20T14:50:00Z"))
            .unwrap()
            .is_empty()
    );
    // One removed day of a series is not supported for them (it would need the Calendar app's own rules).
    let mut rep = timed("Series");
    rep.repeat = Some(RepeatInput {
        every: "daily".into(),
        interval: 1,
        until: Some("2026-11-24".into()),
    });
    let e = save_to_calendar(&mut s, &app, None, None, "EK-CAL", &rep, NY, now).unwrap();
    assert!(skip_occurrence(&mut s, &format!("{CALENDAR_ID}:{}:123", e.remote_id), now).is_err());
}

#[test]
fn a_record_that_cannot_be_written_takes_the_new_event_back_out_of_the_calendar_app() {
    // The database is read-only from our side here, so writing the record fails after the Calendar app saved.
    let mut s = store();
    s.put_own_event("x", "{}").unwrap();
    let app = FakeCalendarApp::default();
    // Make our write fail by dropping the table the record is written to.
    s.drop_own_events_table_for_tests();
    let r = save_to_calendar(
        &mut s,
        &app,
        None,
        None,
        "EK-CAL",
        &timed("rollback"),
        NY,
        t("2026-11-14T12:00:00Z"),
    );
    assert!(r.is_err());
    assert_eq!(
        app.deleted.lock().unwrap().as_slice(),
        ["EK-1"],
        "the event it just added was removed again"
    );
}

/// What the sync brings back for an event saved to a CalDAV calendar: a row in that account's calendar, with
/// the UID we wrote (made from the event's address).
fn as_synced_caldav(s: &mut Store, account: &str, event_url: &str, title: &str) -> Event {
    let acct = Account {
        id: account.into(),
        provider: ProviderKind::Caldav,
        display_name: "Fastmail".into(),
        email: None,
        status: AccountStatus::Ok,
        secret_ref: None,
        created_at: Utc::now(),
    };
    s.upsert_account(&acct).unwrap();
    let cal = Calendar {
        id: format!("{account}:abc"),
        account_id: account.into(),
        remote_id: "https://dav.example.com/cal/".into(),
        name: "Work".into(),
        colour: "blue".into(),
        visible: true,
        read_only: false,
    };
    s.upsert_calendar(&cal).unwrap();
    let mut e = build_event(&timed(title), Some("zzz"), NY, Utc::now()).unwrap();
    e.id = format!(
        "{}:{}",
        cal.id,
        crate::providers::caldav::uid_for(event_url)
    );
    e.calendar_id = cal.id.clone();
    e.remote_id = event_url.into();
    e.ical_uid = crate::providers::caldav::uid_for(event_url);
    s.upsert_events(&cal.id, std::slice::from_ref(&e)).unwrap();
    e
}

#[test]
fn a_caldav_event_is_found_again_by_its_uid_and_stays_in_its_own_account() {
    let mut s = store();
    let app = FakeCalendarApp::default();
    let now = t("2026-11-14T12:00:00Z");
    // The writer here stands in for a CalDAV account (it returns the address of the file it made).
    let e = save_to_calendar(
        &mut s,
        &app,
        Some("acct1"),
        None,
        "acct1:abc",
        &timed("Dentist"),
        NY,
        now,
    )
    .unwrap();
    let url = "EK-1"; // the fake's identifier; for CalDAV this is the file's address
    let remote = remote_of(&s, &e.id).unwrap().unwrap();
    assert_eq!(remote.account_id.as_deref(), Some("acct1"));
    assert_eq!(remote.calendar_id, "acct1:abc");

    // A CalDAV row only matches when the account AND the UID match.
    let synced = as_synced_caldav(&mut s, "acct1", url, "Dentist");
    assert_eq!(
        get(&s, &synced.id).unwrap().unwrap().saved_to.as_deref(),
        Some("acct1:abc")
    );
    let other_account = as_synced_caldav(&mut s, "acct2", url, "Dentist");
    assert!(
        resolve(&s, &other_account.id).unwrap().is_none(),
        "same UID in another account is not ours"
    );
    let stranger = as_synced_caldav(
        &mut s,
        "acct1",
        "https://dav.example.com/cal/theirs.ics",
        "Theirs",
    );
    assert!(
        resolve(&s, &stranger.id).unwrap().is_none(),
        "an event we did not make is never ours"
    );

    // An edit keeps the account it was saved to, whatever the caller names.
    let mut edit = timed("Dentist (moved)");
    edit.location = None;
    save_to_calendar(
        &mut s,
        &app,
        Some("acct9"),
        Some(&synced.id),
        "elsewhere",
        &edit,
        NY,
        now,
    )
    .unwrap();
    let remote = remote_of(&s, &e.id).unwrap().unwrap();
    assert_eq!(remote.account_id.as_deref(), Some("acct1"));
    assert_eq!(remote.calendar_id, "acct1:abc");
}
