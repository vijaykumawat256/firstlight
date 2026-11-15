use super::*;
use crate::http::ReqwestClient;
use crate::http::testserver::{Req, Resp, TestServer};
use crate::ics::{ParseOptions, events_from_ics};
use crate::model::When;
use crate::providers::Window;
use crate::providers::caldav::CaldavSecret;
use crate::secrets::MemorySecretStore;
use chrono::TimeZone;
use std::sync::Mutex;

const USER: &str = "me@example.com";
const PASS: &str = "app-pass";
const NY: Tz = chrono_tz::America::New_York;

fn t(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

fn timed() -> SysNewEvent {
    SysNewEvent {
        title: "Dentist, 2nd; floor".into(),
        when: SysWhen::Timed {
            start: t("2026-11-20T15:00:00Z"),
            end: t("2026-11-20T16:00:00Z"),
        },
        location: Some("Room 4".into()),
        url: Some("https://meet.google.com/abc-defg-hij".into()),
        notes: Some("Bring forms\nand ID".into()),
        repeat: None,
        alarm_minutes: Some(15),
    }
}

#[test]
fn the_uid_is_the_name_of_the_file() {
    assert_eq!(
        uid_for("https://x.test/cal/firstlight-ab12.ics"),
        "firstlight-ab12"
    );
    assert_eq!(
        uid_for("https://x.test/cal/firstlight-ab12.ics?x=1"),
        "firstlight-ab12"
    );
}

#[test]
fn a_one_off_timed_event_is_written_in_utc_with_no_invitation_fields() {
    let f = event_file("firstlight-1", &timed(), NY, t("2026-11-14T12:00:00Z"));
    assert!(f.contains("DTSTART:20261120T150000Z\r\n"));
    assert!(f.contains("DTEND:20261120T160000Z\r\n"));
    assert!(f.contains("SUMMARY:Dentist\\, 2nd\\; floor\r\n"));
    assert!(f.contains("DESCRIPTION:Bring forms\\nand ID\r\n"));
    assert!(f.contains("TRIGGER:-PT15M\r\n"));
    for banned in ["METHOD", "ORGANIZER", "ATTENDEE"] {
        assert!(
            !f.contains(banned),
            "{banned} would make the server mail people"
        );
    }
    assert!(f.lines().all(|l| l.len() <= 75), "lines are folded");
}

#[test]
fn a_repeating_event_keeps_its_wall_clock_time_and_its_last_day() {
    let mut e = timed();
    e.repeat = Some(SysRepeat {
        freq: SysFreq::Weekly,
        interval: 2,
        until: NaiveDate::from_ymd_opt(2027, 3, 31),
    });
    let f = event_file("firstlight-1", &e, NY, t("2026-11-14T12:00:00Z"));
    assert!(f.contains("DTSTART;TZID=America/New_York:20261120T100000\r\n"));
    // The last day counts: the series runs to the end of 31 March (EDT, UTC-4) = 1 April 03:59:59Z.
    assert!(
        f.contains("RRULE:FREQ=WEEKLY;INTERVAL=2;UNTIL=20270401T035959Z\r\n"),
        "{f}"
    );
}

#[test]
fn an_all_day_event_ends_the_day_after_its_last_day() {
    let e = SysNewEvent {
        when: SysWhen::AllDay {
            first: NaiveDate::from_ymd_opt(2026, 12, 24).unwrap(),
            last: NaiveDate::from_ymd_opt(2026, 12, 26).unwrap(),
        },
        alarm_minutes: Some(30),
        repeat: Some(SysRepeat {
            freq: SysFreq::Yearly,
            interval: 1,
            until: NaiveDate::from_ymd_opt(2030, 12, 26),
        }),
        ..timed()
    };
    let f = event_file("firstlight-1", &e, NY, t("2026-11-14T12:00:00Z"));
    assert!(f.contains("DTSTART;VALUE=DATE:20261224\r\n"));
    assert!(f.contains("DTEND;VALUE=DATE:20261227\r\n"));
    assert!(f.contains("RRULE:FREQ=YEARLY;INTERVAL=1;UNTIL=20301226\r\n"));
    assert!(!f.contains("VALARM"), "alerts are for timed events only");
}

#[test]
fn a_repeating_timed_event_carries_its_zone_rules_and_others_do_not() {
    let mut e = timed();
    e.repeat = Some(SysRepeat {
        freq: SysFreq::Weekly,
        interval: 1,
        until: None,
    });
    let f = event_file("firstlight-1", &e, NY, t("2026-11-14T12:00:00Z"));
    let (vtz, vev) = (f.find("BEGIN:VTIMEZONE"), f.find("BEGIN:VEVENT"));
    assert!(
        vtz.is_some() && vtz < vev,
        "zone rules come before the event: {f}"
    );
    assert!(
        f.contains("TZID:America/New_York\r\n")
            && f.contains("RRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=2SU"),
        "{f}"
    );
    assert!(
        !event_file("firstlight-1", &timed(), NY, t("2026-11-14T12:00:00Z")).contains("VTIMEZONE")
    );
}

/// What we write must read back as the same event through the same reader the sync uses.
#[test]
fn what_we_write_reads_back_the_same_through_the_sync_reader() {
    let mut e = timed();
    e.repeat = Some(SysRepeat {
        freq: SysFreq::Weekly,
        interval: 1,
        until: None,
    });
    let f = event_file("firstlight-1", &e, NY, t("2026-11-14T12:00:00Z"));
    let window = Window {
        from: t("2026-11-01T00:00:00Z"),
        to: t("2026-12-15T00:00:00Z"),
    };
    let evs = events_from_ics(
        &f,
        &ParseOptions {
            calendar_id: "c",
            default_tz: chrono_tz::UTC,
            window,
            my_email: None,
        },
    )
    .unwrap();
    assert_eq!(evs.len(), 4, "20 Nov, 27 Nov, 4 Dec, 11 Dec");
    assert!(
        evs.iter()
            .all(|x| x.title == "Dentist, 2nd; floor" && x.ical_uid == "firstlight-1")
    );
    // 10:00 New York time on 4 December (EST, after the clocks changed) is 15:00Z.
    let dec4 = evs.iter().find(
        |x| matches!(&x.when, When::Timed { start, .. } if *start == t("2026-12-04T15:00:00Z")),
    );
    assert!(dec4.is_some(), "wall-clock time held across the change");
    let _ = Utc.timestamp_opt(0, 0);
}

// ---- against a mock server ---------------------------------------------------------------

#[derive(Default)]
struct Seen(Mutex<Vec<Req>>);

fn server(seen: Arc<Seen>, put: u16, delete: u16) -> TestServer {
    TestServer::start(move |r| {
        seen.0.lock().unwrap().push(r.clone());
        match r.method.as_str() {
            "PUT" => Resp::status(put),
            "DELETE" => Resp::status(delete),
            _ => Resp::status(404),
        }
    })
}

fn writer(server: &TestServer) -> CaldavWriter {
    let secrets = Arc::new(MemorySecretStore::default());
    secrets
        .set(
            &CaldavProvider::secret_key("acct"),
            &serde_json::to_string(&CaldavSecret {
                server: server.url("/"),
                username: USER.into(),
                password: PASS.into(),
            })
            .unwrap(),
        )
        .unwrap();
    let mut w = CaldavWriter::new(
        Arc::new(ReqwestClient::new_allow_http_for_tests().unwrap()),
        secrets,
        "acct",
        &server.url("/cals/me/work/"),
    );
    w.now = || DateTime::from_timestamp(1_800_000_000, 0).unwrap();
    w
}

#[test]
fn creating_puts_a_new_file_that_can_never_overwrite_another() {
    let seen = Arc::new(Seen::default());
    let srv = server(seen.clone(), 201, 204);
    let w = writer(&srv);
    let url = w.save_event("ignored", None, &timed(), NY).unwrap();
    assert!(
        url.starts_with(&srv.url("/cals/me/work/firstlight-")) && url.ends_with(".ics"),
        "{url}"
    );
    let reqs = seen.0.lock().unwrap();
    assert_eq!(reqs.len(), 1);
    let r = &reqs[0];
    assert_eq!(r.method, "PUT");
    assert_eq!(r.header("if-none-match"), Some("*"));
    assert!(
        r.header("content-type")
            .unwrap()
            .starts_with("text/calendar")
    );
    assert!(r.header("authorization").unwrap().starts_with("Basic "));
    assert!(r.body.contains(&format!("UID:{}", uid_for(&url))));
}

#[test]
fn changing_puts_to_the_same_address_and_removing_deletes_it() {
    let seen = Arc::new(Seen::default());
    let srv = server(seen.clone(), 204, 204);
    let w = writer(&srv);
    let url = srv.url("/cals/me/work/firstlight-abc.ics");
    assert_eq!(w.save_event("x", Some(&url), &timed(), NY).unwrap(), url);
    w.delete_event(&url).unwrap();
    let reqs = seen.0.lock().unwrap();
    assert_eq!(reqs[0].method, "PUT");
    assert_eq!(
        reqs[0].header("if-none-match"),
        None,
        "an edit replaces the file"
    );
    assert_eq!(reqs[0].path, "/cals/me/work/firstlight-abc.ics");
    assert!(
        reqs[0].body.contains("UID:firstlight-abc\r\n"),
        "an edit keeps the event's UID"
    );
    assert_eq!(reqs[1].method, "DELETE");
}

#[test]
fn removing_something_already_gone_counts_as_done() {
    let srv = server(Arc::new(Seen::default()), 201, 404);
    let w = writer(&srv);
    assert!(
        w.delete_event(&srv.url("/cals/me/work/firstlight-abc.ics"))
            .is_ok()
    );
}

#[test]
fn it_refuses_any_address_outside_this_calendar_before_sending_anything() {
    let seen = Arc::new(Seen::default());
    let srv = server(seen.clone(), 201, 204);
    let w = writer(&srv);
    for bad in [
        "http://evil.example/cals/me/work/firstlight-a.ics", // other host
        "/cals/me/other/firstlight-a.ics",                   // other calendar
        "/cals/me/work/sub/firstlight-a.ics",                // nested
        "/cals/me/work/",                                    // the calendar itself
        "/cals/me/work/a.txt",                               // not an event file
        "/cals/me/work/..%2Fx.ics",                          // odd characters
        "/cals/me/work/../other/a.ics",                      // escape
    ] {
        assert!(w.save_event("x", Some(bad), &timed(), NY).is_err(), "{bad}");
        assert!(w.delete_event(bad).is_err(), "{bad}");
    }
    assert!(
        seen.0.lock().unwrap().is_empty(),
        "nothing reached the server"
    );
}

#[test]
fn a_calendar_address_on_another_host_is_refused() {
    let srv = server(Arc::new(Seen::default()), 201, 204);
    let mut w = writer(&srv);
    w.calendar_url = "http://evil.example/cals/".into();
    assert!(w.save_event("x", None, &timed(), NY).is_err());
}

#[test]
fn a_refusal_is_not_taken_for_a_bad_password() {
    // 403 on a PUT means "not allowed here", not "sign in again": the account must not be marked broken.
    let srv = server(Arc::new(Seen::default()), 403, 403);
    let w = writer(&srv);
    assert!(matches!(
        w.save_event("x", None, &timed(), NY),
        Err(Error::Provider(_))
    ));
    assert!(matches!(
        w.delete_event(&srv.url("/cals/me/work/firstlight-a.ics")),
        Err(Error::Provider(_))
    ));
    // 401 is a wrong password.
    let srv = server(Arc::new(Seen::default()), 401, 401);
    assert!(matches!(
        writer(&srv).save_event("x", None, &timed(), NY),
        Err(Error::AuthRevoked)
    ));
    // An edit of something that vanished from the server says so; it is not recreated silently.
    let srv = server(Arc::new(Seen::default()), 404, 204);
    let w = writer(&srv);
    assert!(
        w.save_event(
            "x",
            Some(&srv.url("/cals/me/work/firstlight-a.ics")),
            &timed(),
            NY
        )
        .is_err()
    );
}
