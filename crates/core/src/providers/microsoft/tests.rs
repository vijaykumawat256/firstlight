use super::*;
use crate::http::ReqwestClient;
use crate::http::testserver::{Resp, TestServer};
use crate::oauth::{TokenSet, secret_key};
use crate::secrets::MemorySecretStore;
use chrono::{Duration, TimeZone};

const CALENDARS: &str = r##"{"value":[
 {"id":"AAMk-default","name":"Calendar","color":"auto","hexColor":"","canEdit":true,"isDefaultCalendar":true},
 {"id":"AAMk-team","name":"Team","color":"lightGreen","hexColor":"#ff8c00","canEdit":false},
 {"id":"AAMk-named","name":"Named","color":"lightBlue","hexColor":"","canEdit":false}
]}"##;

const EVENTS: &str = r##"{"value":[
 {"id":"ev1","subject":"Roadmap planning","iCalUId":"uid-1","isAllDay":false,"isCancelled":false,"bodyPreview":"Agenda attached",
  "start":{"dateTime":"2026-03-10T09:00:00.0000000","timeZone":"Pacific Standard Time"},"end":{"dateTime":"2026-03-10T10:00:00.0000000","timeZone":"Pacific Standard Time"},
  "location":{"displayName":"Room 12"},"onlineMeeting":{"joinUrl":"https://teams.microsoft.com/l/meetup-join/abc"},
  "responseStatus":{"response":"notResponded"},"organizer":{"emailAddress":{"name":"Jordan","address":"j@x.com"}},"type":"singleInstance","lastModifiedDateTime":"2026-03-01T10:00:00Z"},
 {"id":"ev2","subject":"Standup","isAllDay":false,"isCancelled":false,"type":"occurrence","seriesMasterId":"master-1",
  "start":{"dateTime":"2026-03-11T09:00:00.0000000","timeZone":"UTC"},"end":{"dateTime":"2026-03-11T09:15:00.0000000","timeZone":"UTC"},
  "responseStatus":{"response":"tentativelyAccepted"},"bodyPreview":"Dial in: https://acme.zoom.us/j/55"},
 {"id":"ev3","subject":"Company holiday","isAllDay":true,"isCancelled":false,
  "start":{"dateTime":"2026-03-12T00:00:00.0000000","timeZone":"Tokyo Standard Time"},"end":{"dateTime":"2026-03-13T00:00:00.0000000","timeZone":"Tokyo Standard Time"},
  "responseStatus":{"response":"organizer"}},
 {"id":"ev4","subject":"Cancelled thing","isAllDay":false,"isCancelled":true,
  "start":{"dateTime":"2026-03-10T12:00:00.0000000","timeZone":"UTC"},"end":{"dateTime":"2026-03-10T13:00:00.0000000","timeZone":"UTC"}},
 {"id":"ev5","subject":"","isAllDay":false,"isCancelled":false,
  "start":{"dateTime":"2026-03-10T14:00:00.0000000","timeZone":"Mystery Standard Time"},"end":{"dateTime":"2026-03-10T15:00:00.0000000","timeZone":"Mystery Standard Time"},
  "responseStatus":{"response":"declined"}},
 {"id":"broken","subject":"no times"},
 {"id":"outside","subject":"Next year","isAllDay":false,"start":{"dateTime":"2027-03-10T14:00:00.0000000","timeZone":"UTC"},"end":{"dateTime":"2027-03-10T15:00:00.0000000","timeZone":"UTC"}}
]}"##;

fn window() -> Window {
    Window {
        from: Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap(),
        to: Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap(),
    }
}

fn setup(server: &TestServer) -> (MicrosoftProvider, Account) {
    let secrets = Arc::new(MemorySecretStore::default());
    let t = TokenSet {
        access_token: "AT".into(),
        refresh_token: "RT".into(),
        expires_at: Utc::now() + Duration::hours(1),
    };
    secrets
        .set(&secret_key("acct"), &serde_json::to_string(&t).unwrap())
        .unwrap();
    let mut cfg = oauth_config("cid");
    cfg.token_url = server.url("/token");
    let p = MicrosoftProvider::with_base(
        Arc::new(ReqwestClient::new_allow_http_for_tests().unwrap()),
        secrets,
        cfg,
        &server.url("/v1.0"),
    );
    let account = Account {
        id: "acct".into(),
        provider: ProviderKind::Microsoft,
        display_name: "me@outlook.com".into(),
        email: Some("me@outlook.com".into()),
        status: AccountStatus::Ok,
        secret_ref: Some(secret_key("acct")),
        created_at: Utc::now(),
    };
    (p, account)
}

fn graph() -> TestServer {
    TestServer::start(|r| {
        if r.header("authorization") != Some("Bearer AT") {
            return Resp::status(401);
        }
        if r.path.starts_with("/v1.0/me/calendars?") {
            Resp::json(CALENDARS)
        } else if r.path.contains("/calendarView") {
            Resp::json(EVENTS)
        } else {
            Resp::status(404)
        }
    })
}

#[test]
fn lists_calendars_with_hex_then_named_then_default_colours() {
    let server = graph();
    let (p, acct) = setup(&server);
    let cals = p.list_calendars(&acct).unwrap();
    assert_eq!(
        cals.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        ["Calendar", "Team", "Named"]
    );
    assert_eq!(cals[1].colour, "orange", "hexColor beats the named colour");
    assert_eq!(
        cals[2].colour, "blue",
        "named colour used when there is no hex"
    );
    assert!(cals[0].colour.len() > 2, "falls back to a stable default");
    assert!(!cals[0].read_only && cals[1].read_only);
}

#[test]
fn maps_timezones_all_day_events_and_responses() {
    let server = graph();
    let (p, acct) = setup(&server);
    let cal = p.list_calendars(&acct).unwrap().remove(0);
    let r = p.sync_events(&cal, None, &window()).unwrap();
    let by = |id: &str| {
        r.changed
            .iter()
            .find(|e| e.remote_id == id)
            .unwrap_or_else(|| panic!("missing {id}"))
    };

    // 09:00 Pacific on 10 March 2026 (PDT, UTC-7) is 16:00 UTC.
    let e1 = by("ev1");
    assert!(
        matches!(&e1.when, When::Timed { start, tzid: Some(t), .. } if start.format("%Y-%m-%d %H:%M").to_string() == "2026-03-10 16:00" && t == "America/Los_Angeles")
    );
    assert_eq!(
        e1.join_url.as_deref(),
        Some("https://teams.microsoft.com/l/meetup-join/abc")
    );
    assert_eq!(e1.my_response, Response::NeedsAction);
    assert_eq!(e1.organizer.as_deref(), Some("Jordan"));
    assert_eq!(e1.location.as_deref(), Some("Room 12"));
    assert_eq!(e1.ical_uid, "uid-1");

    let e2 = by("ev2");
    assert_eq!(
        (e2.status, e2.my_response),
        (EventStatus::Tentative, Response::Tentative)
    );
    assert_eq!(e2.recurrence_master_id.as_deref(), Some("master-1"));
    assert_eq!(e2.join_url.as_deref(), Some("https://acme.zoom.us/j/55"));

    // All-day keeps its calendar date even though Tokyo midnight is the previous day in UTC.
    assert_eq!(
        by("ev3").when,
        When::AllDay {
            start: NaiveDate::from_ymd_opt(2026, 3, 12).unwrap(),
            end: NaiveDate::from_ymd_opt(2026, 3, 13).unwrap()
        }
    );
    assert_eq!(
        by("ev3").my_response,
        Response::None,
        "the organiser has no RSVP"
    );

    assert_eq!(by("ev4").status, EventStatus::Cancelled);
    let e5 = by("ev5");
    assert_eq!(
        (e5.title.as_str(), e5.my_response),
        ("(No title)", Response::Declined)
    );
    assert!(
        matches!(&e5.when, When::Timed { start, tzid: None, .. } if start.format("%H:%M").to_string() == "14:00"),
        "unknown zone falls back to UTC"
    );
    assert!(
        r.changed
            .iter()
            .all(|e| e.remote_id != "broken" && e.remote_id != "outside")
    );
}

#[test]
fn calendar_view_request_is_windowed_and_the_id_is_encoded() {
    let server = graph();
    let (p, _) = setup(&server);
    let cal = Calendar {
        id: "acct:x".into(),
        account_id: "acct".into(),
        remote_id: "AAMk/with+odd=chars".into(),
        name: "x".into(),
        colour: "blue".into(),
        visible: true,
        read_only: true,
    };
    p.sync_events(&cal, None, &window()).unwrap();
    let seen = server.requests.lock().unwrap();
    let path = &seen[0].path;
    assert!(
        path.contains("/me/calendars/AAMk%2Fwith+odd=chars/calendarView")
            || path.contains("/me/calendars/AAMk%2Fwith%2Bodd%3Dchars/calendarView"),
        "{path}"
    );
    assert!(
        path.contains("startDateTime=2026-03-01T00%3A00%3A00Z")
            && path.contains("endDateTime=2026-04-01T00%3A00%3A00Z"),
        "{path}"
    );
}

#[test]
fn follows_next_links_within_the_graph_origin() {
    let server = TestServer::start(|r| {
        if r.path.contains("page=2") {
            Resp::json(
                r#"{"value":[{"id":"b","subject":"B","isAllDay":false,"start":{"dateTime":"2026-03-10T10:00:00","timeZone":"UTC"},"end":{"dateTime":"2026-03-10T11:00:00","timeZone":"UTC"}}]}"#,
            )
        } else {
            let base = format!("http://{}", r.header("host").unwrap());
            Resp::json(&format!(
                r#"{{"@odata.nextLink":"{base}/v1.0/me/calendars/c/calendarView?page=2","value":[{{"id":"a","subject":"A","isAllDay":false,"start":{{"dateTime":"2026-03-10T08:00:00","timeZone":"UTC"}},"end":{{"dateTime":"2026-03-10T09:00:00","timeZone":"UTC"}}}}]}}"#
            ))
        }
    });
    let (p, _) = setup(&server);
    let cal = Calendar {
        id: "acct:x".into(),
        account_id: "acct".into(),
        remote_id: "c".into(),
        name: "x".into(),
        colour: "blue".into(),
        visible: true,
        read_only: true,
    };
    assert_eq!(
        p.sync_events(&cal, None, &window()).unwrap().changed.len(),
        2
    );
}

#[test]
fn a_next_link_to_another_host_is_refused_and_no_token_is_sent_there() {
    let evil = TestServer::start(|_| Resp::json(r#"{"value":[]}"#));
    let evil_base = evil.base.clone();
    let server = TestServer::start(move |_| {
        Resp::json(&format!(
            r#"{{"@odata.nextLink":"{evil_base}/steal","value":[]}}"#
        ))
    });
    let (p, _) = setup(&server);
    let cal = Calendar {
        id: "acct:x".into(),
        account_id: "acct".into(),
        remote_id: "c".into(),
        name: "x".into(),
        colour: "blue".into(),
        visible: true,
        read_only: true,
    };
    let err = p.sync_events(&cal, None, &window()).unwrap_err();
    assert!(err.to_string().contains("different host"), "{err}");
    assert_eq!(evil.count(), 0);
}

#[test]
fn error_statuses_are_classified() {
    for (status, kind) in [
        (401u16, "auth"),
        (403, "auth"),
        (429, "offline"),
        (503, "offline"),
        (500, "other"),
        (404, "other"),
    ] {
        let server = TestServer::start(move |_| Resp::status(status));
        let (p, acct) = setup(&server);
        let got = match p.list_calendars(&acct).unwrap_err() {
            Error::AuthRevoked => "auth",
            Error::Offline => "offline",
            _ => "other",
        };
        assert_eq!(got, kind, "status {status}");
    }
}

#[test]
fn profile_email_prefers_mail_then_user_principal_name() {
    let http = ReqwestClient::new_allow_http_for_tests().unwrap();
    let a = TestServer::start(|_| {
        Resp::json(r#"{"mail":"me@outlook.com","userPrincipalName":"upn@x.com"}"#)
    });
    assert_eq!(
        MicrosoftProvider::fetch_email(&http, &a.url("/v1.0"), "AT").unwrap(),
        "me@outlook.com"
    );
    let b = TestServer::start(|_| Resp::json(r#"{"mail":null,"userPrincipalName":"upn@x.com"}"#));
    assert_eq!(
        MicrosoftProvider::fetch_email(&http, &b.url("/v1.0"), "AT").unwrap(),
        "upn@x.com"
    );
    let c = TestServer::start(|_| Resp::json("{}"));
    assert!(MicrosoftProvider::fetch_email(&http, &c.url("/v1.0"), "AT").is_err());
}

#[test]
fn config_is_a_public_client_with_read_only_scopes() {
    let c = oauth_config("cid");
    assert!(c.client_secret.is_none());
    assert!(
        c.scopes.contains(&"Calendars.Read".to_string())
            && c.scopes.contains(&"offline_access".to_string())
    );
    assert!(
        !c.scopes.iter().any(|s| s.contains("ReadWrite")),
        "no write scopes"
    );
}
