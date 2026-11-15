use super::*;
use chrono::TimeZone;
use std::sync::Mutex;

struct Mock {
    access: Mutex<Access>,
    calendars: Vec<SysCalendar>,
    events: Vec<SysEvent>,
    asked: Mutex<Vec<String>>,
}

impl Mock {
    fn new(access: Access) -> Self {
        Self {
            access: Mutex::new(access),
            calendars: vec![],
            events: vec![],
            asked: Mutex::default(),
        }
    }
}

impl SystemSource for Mock {
    fn access(&self) -> Access {
        *self.access.lock().unwrap()
    }
    fn request_access(&self) -> Result<Access> {
        Ok(self.access())
    }
    fn calendars(&self) -> Result<Vec<SysCalendar>> {
        Ok(self.calendars.clone())
    }
    fn events(
        &self,
        calendar_id: &str,
        _: DateTime<Utc>,
        _: DateTime<Utc>,
    ) -> Result<Vec<SysEvent>> {
        self.asked.lock().unwrap().push(calendar_id.to_string());
        Ok(self.events.clone())
    }
}

fn cal(id: &str, title: &str, source: &str, kind: SysCalendarKind) -> SysCalendar {
    SysCalendar {
        id: id.into(),
        title: title.into(),
        source: source.into(),
        kind,
        read_only: false,
    }
}

fn utc(d: u32, h: u32, m: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, d, h, m, 0).unwrap()
}

fn ev(id: &str, title: &str, start: DateTime<Utc>, end: DateTime<Utc>) -> SysEvent {
    SysEvent {
        id: id.into(),
        uid: Some(format!("{id}@uid")),
        title: title.into(),
        start,
        end,
        all_day: false,
        location: None,
        notes: None,
        url: None,
        status: SysStatus::Confirmed,
        response: SysResponse::NotInvited,
        organizer: None,
        is_recurring: false,
        tzid: Some("Europe/London".into()),
        modified: Some(1_700_000_000),
        attendees: vec![],
    }
}

fn window() -> Window {
    Window {
        from: utc(1, 0, 0),
        to: Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap(),
    }
}

fn provider(m: Mock, tz: Tz) -> SystemProvider {
    SystemProvider::new(Arc::new(m), tz)
}

fn account() -> Account {
    super::account(Utc::now())
}

#[test]
fn permission_states_map_to_reconnect_or_unsupported() {
    for (access, want_auth) in [
        (Access::Denied, true),
        (Access::Restricted, true),
        (Access::NotDetermined, true),
    ] {
        let p = provider(Mock::new(access), chrono_tz::UTC);
        let err = p.list_calendars(&account()).unwrap_err();
        assert_eq!(matches!(err, Error::AuthRevoked), want_auth, "{access:?}");
    }
    let p = provider(Mock::new(Access::Unsupported), chrono_tz::UTC);
    assert!(matches!(
        p.list_calendars(&account()),
        Err(Error::Provider(_))
    ));
}

#[test]
fn losing_permission_later_marks_events_as_needing_attention_not_empty() {
    let mut m = Mock::new(Access::Granted);
    m.calendars = vec![cal("c1", "Work", "iCloud", SysCalendarKind::CalDav)];
    let m = Arc::new(m);
    let p = SystemProvider::new(m.clone(), chrono_tz::UTC);
    let c = p.list_calendars(&account()).unwrap().remove(0);
    *m.access.lock().unwrap() = Access::Denied;
    assert!(
        matches!(p.sync_events(&c, None, &window()), Err(Error::AuthRevoked)),
        "must not return an empty calendar that would wipe the cache"
    );
}

#[test]
fn calendars_are_named_by_source_and_noisy_ones_start_hidden() {
    let mut m = Mock::new(Access::Granted);
    m.calendars = vec![
        cal("a", "Home", "iCloud", SysCalendarKind::CalDav),
        cal("b", "Work", "Gmail", SysCalendarKind::CalDav),
        cal("c", "Gmail Tasks", "Gmail", SysCalendarKind::CalDav), // name already says the source
        cal("d", "Birthdays", "Other", SysCalendarKind::Birthday),
        cal(
            "e",
            "US Holidays",
            "Subscribed",
            SysCalendarKind::Subscription,
        ),
        cal("f", "Scratch", "", SysCalendarKind::Local),
    ];
    let cals = provider(m, chrono_tz::UTC)
        .list_calendars(&account())
        .unwrap();
    let get = |n: &str| {
        cals.iter().find(|c| c.name == n).unwrap_or_else(|| {
            panic!(
                "no {n}: {:?}",
                cals.iter().map(|c| &c.name).collect::<Vec<_>>()
            )
        })
    };
    assert!(get("Home (iCloud)").visible && get("Work (Gmail)").visible);
    assert!(get("Gmail Tasks").visible);
    assert!(!get("Birthdays (Other)").visible && !get("US Holidays (Subscribed)").visible);
    assert_eq!(get("Scratch").remote_id, "f");
    let ids: std::collections::HashSet<_> = cals.iter().map(|c| &c.id).collect();
    assert_eq!(ids.len(), cals.len(), "calendar ids are unique and stable");
}

#[test]
fn all_day_events_keep_their_calendar_dates_for_both_end_conventions() {
    let tz = chrono_tz::Europe::London;
    let day = |d| NaiveDate::from_ymd_opt(2026, 3, d).unwrap();
    // Single day, end reported as 23:59:59 (EventKit's usual form).
    assert_eq!(
        all_day_dates(utc(10, 0, 0), utc(10, 23, 59) + Duration::seconds(59), tz),
        (day(10), day(11))
    );
    // Single day, end reported as the next midnight.
    assert_eq!(
        all_day_dates(utc(10, 0, 0), utc(11, 0, 0), tz),
        (day(10), day(11))
    );
    // Three days.
    assert_eq!(
        all_day_dates(utc(10, 0, 0), utc(12, 23, 59), tz),
        (day(10), day(13))
    );
    // Zero-length never collapses.
    assert_eq!(
        all_day_dates(utc(10, 0, 0), utc(10, 0, 0), tz),
        (day(10), day(11))
    );
}

#[test]
fn all_day_dates_follow_the_users_timezone_not_utc() {
    // Midnight 10 March in Tokyo is 15:00 UTC on the 9th; the date must stay the 10th.
    let tokyo = chrono_tz::Asia::Tokyo;
    let start = utc(9, 15, 0);
    let end = utc(10, 14, 59) + Duration::seconds(59);
    assert_eq!(
        all_day_dates(start, end, tokyo),
        (
            NaiveDate::from_ymd_opt(2026, 3, 10).unwrap(),
            NaiveDate::from_ymd_opt(2026, 3, 11).unwrap()
        )
    );
}

#[test]
fn events_map_status_response_join_link_and_recurrence() {
    let mut m = Mock::new(Access::Granted);
    m.calendars = vec![cal("c1", "Work", "iCloud", SysCalendarKind::CalDav)];
    let mut review = ev("e1", "Design review", utc(10, 9, 0), utc(10, 10, 0));
    review.location = Some("Room 4B".into());
    review.url = Some("https://zoom.us/j/123".into());
    review.response = SysResponse::Pending;
    review.organizer = Some("Priya".into());
    let mut standup = ev("e2", "Standup", utc(11, 9, 0), utc(11, 9, 15));
    standup.is_recurring = true;
    standup.status = SysStatus::Tentative;
    standup.response = SysResponse::Tentative;
    let mut standup2 = standup.clone();
    standup2.start = utc(12, 9, 0);
    standup2.end = utc(12, 9, 15);
    let mut gone = ev("e3", "Cancelled", utc(10, 12, 0), utc(10, 13, 0));
    gone.status = SysStatus::Canceled;
    let mut blank = ev("e4", "  ", utc(10, 14, 0), utc(10, 15, 0));
    blank.notes = Some("Dial https://acme.zoom.us/j/9 now".into());
    blank.uid = None;
    let mut declined = ev("e5", "Declined", utc(10, 16, 0), utc(10, 17, 0));
    declined.response = SysResponse::Declined;
    m.events = vec![review, standup, standup2, gone, blank, declined];

    let p = provider(m, chrono_tz::UTC);
    let c = p.list_calendars(&account()).unwrap().remove(0);
    let r = p.sync_events(&c, None, &window()).unwrap();
    assert!(r.full);
    let by = |remote: &str| {
        r.changed
            .iter()
            .find(|e| e.remote_id == remote)
            .unwrap_or_else(|| panic!("missing {remote}"))
    };

    let e1 = by("e1");
    assert_eq!(
        (e1.my_response, e1.status),
        (Response::NeedsAction, EventStatus::Confirmed)
    );
    assert_eq!(e1.join_url.as_deref(), Some("https://zoom.us/j/123"));
    assert_eq!(e1.organizer.as_deref(), Some("Priya"));
    assert_eq!(e1.ical_uid, "e1@uid");

    // Both occurrences of the recurring event survive with distinct ids.
    let occurrences: Vec<_> = r
        .changed
        .iter()
        .filter(|e| e.recurrence_master_id.is_some())
        .collect();
    assert_eq!(occurrences.len(), 2);
    assert_ne!(occurrences[0].id, occurrences[1].id);
    assert_ne!(occurrences[0].remote_id, occurrences[1].remote_id);
    assert!(
        occurrences
            .iter()
            .all(|e| e.status == EventStatus::Tentative && e.my_response == Response::Tentative)
    );

    assert_eq!(by("e3").status, EventStatus::Cancelled);
    assert_eq!(by("e5").my_response, Response::Declined);
    let e4 = by("e4");
    assert_eq!(e4.title, "(No title)");
    assert_eq!(
        e4.ical_uid, "e4",
        "falls back to the store id when there is no UID"
    );
    assert_eq!(
        e4.join_url.as_deref(),
        Some("https://acme.zoom.us/j/9"),
        "found in the notes"
    );
}

#[test]
fn only_https_links_are_kept_and_events_outside_the_window_are_dropped() {
    let mut m = Mock::new(Access::Granted);
    m.calendars = vec![cal("c1", "Work", "iCloud", SysCalendarKind::CalDav)];
    let mut insecure = ev("a", "Insecure", utc(10, 9, 0), utc(10, 10, 0));
    insecure.url = Some("http://zoom.us/j/1".into());
    let outside = ev(
        "b",
        "Next year",
        Utc.with_ymd_and_hms(2027, 3, 10, 9, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(2027, 3, 10, 10, 0, 0).unwrap(),
    );
    m.events = vec![insecure, outside];
    let p = provider(m, chrono_tz::UTC);
    let c = p.list_calendars(&account()).unwrap().remove(0);
    let r = p.sync_events(&c, None, &window()).unwrap();
    assert_eq!(r.changed.len(), 1);
    assert_eq!(r.changed[0].join_url, None);
}

#[test]
fn events_are_requested_per_calendar_using_the_stores_own_id() {
    let mut m = Mock::new(Access::Granted);
    m.calendars = vec![
        cal("store-id-1", "A", "iCloud", SysCalendarKind::CalDav),
        cal("store-id-2", "B", "iCloud", SysCalendarKind::CalDav),
    ];
    let m = Arc::new(m);
    let p = SystemProvider::new(m.clone(), chrono_tz::UTC);
    for c in p.list_calendars(&account()).unwrap() {
        p.sync_events(&c, None, &window()).unwrap();
    }
    let mut asked = m.asked.lock().unwrap().clone();
    asked.sort();
    assert_eq!(asked, ["store-id-1", "store-id-2"]);
}

#[test]
fn the_account_is_a_single_keyless_record() {
    let a = super::account(Utc::now());
    assert_eq!(
        (a.id.as_str(), a.provider, a.secret_ref.as_deref()),
        (SYSTEM_ACCOUNT_ID, ProviderKind::System, None)
    );
}
