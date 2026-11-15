use super::*;
use crate::providers::{Capabilities, SyncResult};
use crate::secrets::{MemorySecretStore, load_or_create_db_key};
use chrono::TimeZone;
use std::sync::Mutex;

fn store() -> Store {
    Store::open_in_memory(&load_or_create_db_key(&MemorySecretStore::default()).unwrap()).unwrap()
}

fn account(status: AccountStatus) -> Account {
    Account {
        id: "acct".into(),
        provider: ProviderKind::Fixture,
        display_name: "Test".into(),
        email: None,
        status,
        secret_ref: None,
        created_at: Utc::now(),
    }
}

fn cal(id: &str) -> Calendar {
    Calendar {
        id: id.into(),
        account_id: "acct".into(),
        remote_id: id.into(),
        name: id.into(),
        colour: "blue".into(),
        visible: true,
        read_only: true,
    }
}

fn ev(cal: &str, remote: &str, hour: u32) -> Event {
    Event {
        id: format!("{cal}:{remote}"),
        calendar_id: cal.into(),
        remote_id: remote.into(),
        ical_uid: remote.into(),
        title: remote.into(),
        when: When::Timed {
            start: Utc.with_ymd_and_hms(2026, 3, 10, hour, 0, 0).unwrap(),
            end: Utc.with_ymd_and_hms(2026, 3, 10, hour + 1, 0, 0).unwrap(),
            tzid: None,
        },
        location: None,
        description: None,
        join_url: None,
        status: EventStatus::Confirmed,
        my_response: Response::None,
        organizer: None,
        recurrence_master_id: None,
        updated_at: Utc::now(),
        also_in: vec![],
        attendees: vec![],
    }
}

fn window() -> Window {
    Window {
        from: Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap(),
        to: Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap(),
    }
}

fn day_events(s: &Store) -> Vec<String> {
    let from = Utc.with_ymd_and_hms(2026, 3, 10, 0, 0, 0).unwrap();
    let mut v: Vec<_> = s
        .events_between(from, from + Duration::days(1))
        .unwrap()
        .into_iter()
        .map(|e| e.remote_id)
        .collect();
    v.sort();
    v
}

/// A provider that plays back scripted results and records the tokens it was given.
struct Script {
    calendars: Mutex<Result<Vec<Calendar>>>,
    results: Mutex<Vec<Result<SyncResult>>>,
    tokens_seen: Mutex<Vec<Option<String>>>,
}

impl Script {
    fn new(calendars: Vec<Calendar>, results: Vec<Result<SyncResult>>) -> Self {
        Self {
            calendars: Mutex::new(Ok(calendars)),
            results: Mutex::new(results.into_iter().rev().collect()),
            tokens_seen: Mutex::default(),
        }
    }
}

impl Provider for Script {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Fixture
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            incremental_sync: true,
            email: false,
            needs_polling: true,
        }
    }
    fn list_calendars(&self, _: &Account) -> Result<Vec<Calendar>> {
        match &*self.calendars.lock().unwrap() {
            Ok(c) => Ok(c.clone()),
            Err(Error::AuthRevoked) => Err(Error::AuthRevoked),
            Err(Error::Offline) => Err(Error::Offline),
            Err(_) => Err(Error::Provider("x".into())),
        }
    }
    fn sync_events(&self, _: &Calendar, state: Option<&str>, _: &Window) -> Result<SyncResult> {
        self.tokens_seen
            .lock()
            .unwrap()
            .push(state.map(String::from));
        self.results
            .lock()
            .unwrap()
            .pop()
            .expect("script exhausted")
    }
}

fn full(events: Vec<Event>, token: &str) -> Result<SyncResult> {
    Ok(SyncResult {
        changed: events,
        deleted: vec![],
        next_state: Some(token.into()),
        full: true,
    })
}

fn seeded(calendars: &[&str]) -> Store {
    let s = store();
    s.upsert_account(&account(AccountStatus::Ok)).unwrap();
    for c in calendars {
        s.upsert_calendar(&cal(c)).unwrap();
    }
    s
}

#[test]
fn full_sync_replaces_the_cache_and_stores_the_token() {
    let mut s = seeded(&[]);
    let p = Script::new(
        vec![cal("c1")],
        vec![
            full(vec![ev("c1", "a", 9), ev("c1", "b", 10)], "t1"),
            full(vec![ev("c1", "b", 10)], "t2"),
        ],
    );
    let r = sync_account(&mut s, &p, &account(AccountStatus::Ok), &window());
    assert!(r.is_clean() && r.calendars_ok == 1);
    assert_eq!(day_events(&s), ["a", "b"]);
    assert_eq!(s.sync_token("c1").unwrap().as_deref(), Some("t1"));

    sync_account(&mut s, &p, &account(AccountStatus::Ok), &window());
    assert_eq!(
        day_events(&s),
        ["b"],
        "a deletion upstream disappears locally"
    );
    assert_eq!(
        *p.tokens_seen.lock().unwrap(),
        [None, Some("t1".to_string())],
        "second sync resumes from the stored token"
    );
}

#[test]
fn incremental_sync_upserts_and_deletes_without_touching_the_rest() {
    let mut s = seeded(&["c1"]);
    s.replace_events(
        "c1",
        &[
            ev("c1", "keep", 9),
            ev("c1", "gone", 10),
            ev("c1", "edit", 11),
        ],
    )
    .unwrap();
    let mut edited = ev("c1", "edit", 11);
    edited.title = "edited".into();
    let inc = Ok(SyncResult {
        changed: vec![edited, ev("c1", "new", 13)],
        deleted: vec!["gone".into()],
        next_state: Some("t2".into()),
        full: false,
    });
    let p = Script::new(vec![cal("c1")], vec![inc]);
    assert!(sync_account(&mut s, &p, &account(AccountStatus::Ok), &window()).is_clean());
    assert_eq!(day_events(&s), ["edit", "keep", "new"]);
    let from = Utc.with_ymd_and_hms(2026, 3, 10, 0, 0, 0).unwrap();
    let titles: Vec<_> = s
        .events_between(from, from + Duration::days(1))
        .unwrap()
        .into_iter()
        .map(|e| e.title)
        .collect();
    assert!(titles.contains(&"edited".to_string()));
}

#[test]
fn expired_token_triggers_exactly_one_clean_full_resync() {
    let mut s = seeded(&["c1"]);
    s.set_sync_state("c1", Some("stale"), None).unwrap();
    let p = Script::new(
        vec![cal("c1")],
        vec![
            Err(Error::SyncTokenExpired),
            full(vec![ev("c1", "a", 9)], "fresh"),
        ],
    );
    let r = sync_account(&mut s, &p, &account(AccountStatus::Ok), &window());
    assert!(r.is_clean());
    assert_eq!(
        *p.tokens_seen.lock().unwrap(),
        [Some("stale".to_string()), None]
    );
    assert_eq!(s.sync_token("c1").unwrap().as_deref(), Some("fresh"));
    assert_eq!(day_events(&s), ["a"]);
}

#[test]
fn a_second_consecutive_token_expiry_is_an_ordinary_failure_not_a_loop() {
    let mut s = seeded(&["c1"]);
    let p = Script::new(
        vec![cal("c1")],
        vec![Err(Error::SyncTokenExpired), Err(Error::SyncTokenExpired)],
    );
    let r = sync_account(&mut s, &p, &account(AccountStatus::Ok), &window());
    assert_eq!(r.calendars_failed, 1);
    assert_eq!(p.tokens_seen.lock().unwrap().len(), 2, "no further retries");
}

#[test]
fn revoked_access_marks_the_account_stops_and_keeps_the_cache() {
    let mut s = seeded(&["c1", "c2"]);
    s.replace_events("c1", &[ev("c1", "cached", 9)]).unwrap();
    let p = Script::new(vec![cal("c1"), cal("c2")], vec![Err(Error::AuthRevoked)]);
    let r = sync_account(&mut s, &p, &account(AccountStatus::Ok), &window());
    assert!(r.needs_attention && !r.is_clean());
    assert_eq!(
        s.get_account("acct").unwrap().unwrap().status,
        AccountStatus::NeedsAttention
    );
    assert_eq!(
        p.tokens_seen.lock().unwrap().len(),
        1,
        "c2 is not attempted with the dead credential"
    );
    assert_eq!(
        day_events(&s),
        ["cached"],
        "events stay visible while the user reconnects"
    );
    assert_eq!(
        s.sync_info("c1").unwrap().unwrap().last_error.as_deref(),
        Some("auth")
    );
}

#[test]
fn recovery_clears_needs_attention() {
    let mut s = seeded(&["c1"]);
    s.set_account_status("acct", AccountStatus::NeedsAttention)
        .unwrap();
    let p = Script::new(vec![cal("c1")], vec![full(vec![], "t")]);
    sync_account(
        &mut s,
        &p,
        &account(AccountStatus::NeedsAttention),
        &window(),
    );
    assert_eq!(
        s.get_account("acct").unwrap().unwrap().status,
        AccountStatus::Ok
    );
}

#[test]
fn offline_keeps_the_cache_and_the_token() {
    let mut s = seeded(&["c1"]);
    s.replace_events("c1", &[ev("c1", "cached", 9)]).unwrap();
    s.set_sync_state("c1", Some("keepme"), None).unwrap();
    let p = Script::new(vec![cal("c1")], vec![Err(Error::Offline)]);
    let r = sync_account(&mut s, &p, &account(AccountStatus::Ok), &window());
    assert!(r.offline && !r.needs_attention);
    assert_eq!(day_events(&s), ["cached"]);
    let info = s.sync_info("c1").unwrap().unwrap();
    assert_eq!(
        info.token.as_deref(),
        Some("keepme"),
        "token survives a failure so we can resume"
    );
    assert_eq!(info.last_error.as_deref(), Some("offline"));
    assert_eq!(
        s.get_account("acct").unwrap().unwrap().status,
        AccountStatus::Ok,
        "offline is not a credential problem"
    );
}

#[test]
fn one_failing_calendar_does_not_block_the_others() {
    let mut s = seeded(&[]);
    let p = Script::new(
        vec![cal("c1"), cal("c2")],
        vec![
            Err(Error::Provider("boom".into())),
            full(vec![ev("c2", "ok", 9)], "t"),
        ],
    );
    let r = sync_account(&mut s, &p, &account(AccountStatus::Ok), &window());
    assert_eq!((r.calendars_ok, r.calendars_failed), (1, 1));
    assert_eq!(day_events(&s), ["ok"]);
}

#[test]
fn calendars_that_disappear_remotely_are_removed_with_their_events() {
    let mut s = seeded(&["c1", "c2"]);
    s.replace_events("c2", &[ev("c2", "x", 9)]).unwrap();
    let p = Script::new(vec![cal("c1")], vec![full(vec![], "t")]);
    sync_account(&mut s, &p, &account(AccountStatus::Ok), &window());
    let ids: Vec<_> = s
        .list_calendars()
        .unwrap()
        .into_iter()
        .map(|c| c.id)
        .collect();
    assert_eq!(ids, ["c1"]);
    assert!(day_events(&s).is_empty());
}

#[test]
fn user_choices_survive_a_resync() {
    let mut s = seeded(&["c1"]);
    s.set_calendar_visible("c1", false).unwrap();
    s.set_calendar_colour("c1", "pink").unwrap();
    let p = Script::new(vec![cal("c1")], vec![full(vec![], "t")]);
    sync_account(&mut s, &p, &account(AccountStatus::Ok), &window());
    let c = &s.list_calendars().unwrap()[0];
    assert!(!c.visible);
    assert_eq!(c.colour, "pink");
}

#[test]
fn listing_failure_is_recorded_and_distinguishes_auth_from_offline() {
    for (err, auth, offline) in [
        (Error::AuthRevoked, true, false),
        (Error::Offline, false, true),
    ] {
        let mut s = seeded(&["c1"]);
        let p = Script::new(vec![], vec![]);
        *p.calendars.lock().unwrap() = Err(err);
        let r = sync_account(&mut s, &p, &account(AccountStatus::Ok), &window());
        assert_eq!((r.needs_attention, r.offline), (auth, offline));
    }
}

#[test]
fn backoff_grows_caps_resets_and_stops_for_revoked_accounts() {
    let normal = Duration::minutes(10);
    let mut b = Backoff::default();
    assert_eq!(b.next_delay(normal, false), Some(normal));
    let bad = SyncReport {
        calendars_failed: 1,
        ..Default::default()
    };
    let delays: Vec<i64> = (0..8)
        .map(|_| {
            b.on_result(&bad);
            b.next_delay(normal, false).unwrap().num_minutes()
        })
        .collect();
    assert_eq!(delays, [1, 2, 4, 8, 16, 30, 30, 30]);
    b.on_result(&SyncReport {
        calendars_ok: 1,
        ..Default::default()
    });
    assert_eq!(b.next_delay(normal, false), Some(normal));
    assert_eq!(
        b.next_delay(normal, true),
        None,
        "revoked accounts wait for the user"
    );
}

#[test]
fn default_window_spans_30_days_back_and_90_forward() {
    let now = Utc.with_ymd_and_hms(2026, 6, 15, 12, 0, 0).unwrap();
    let w = default_window(now);
    assert_eq!((now - w.from).num_days(), 30);
    assert_eq!((w.to - now).num_days(), 90);
}
