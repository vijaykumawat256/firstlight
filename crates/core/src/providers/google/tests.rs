use super::*;
use crate::http::ReqwestClient;
use crate::http::testserver::{Resp, TestServer};
use crate::oauth::{TokenManager, TokenSet, secret_key};
use crate::secrets::MemorySecretStore;
use chrono::{Duration, TimeZone};

const CAL_LIST: &str = r##"{"kind":"calendar#calendarList","items":[
  {"id":"me@gmail.com","summary":"me@gmail.com","backgroundColor":"#9fe1e7","accessRole":"owner","selected":true},
  {"id":"fam#family@group.v.calendar.google.com","summary":"Family","summaryOverride":"Home","backgroundColor":"#f83a22","accessRole":"reader"},
  {"id":"hidden@x.com","summary":"Hidden","hidden":true,"accessRole":"owner"},
  {"id":"holidays#holiday@group.v.calendar.google.com","summary":"Holidays","backgroundColor":"#16a765","accessRole":"reader"}
]}"##;

const EVENTS: &str = r##"{"kind":"calendar#events","items":[
 {"id":"e1","status":"confirmed","summary":"Design review","location":"Room 4B","iCalUID":"e1@google.com","updated":"2026-03-01T10:00:00.000Z",
  "start":{"dateTime":"2026-03-10T10:30:00+01:00","timeZone":"Europe/Berlin"},"end":{"dateTime":"2026-03-10T11:30:00+01:00","timeZone":"Europe/Berlin"},
  "organizer":{"displayName":"Priya","email":"priya@x.com"},
  "attendees":[{"email":"priya@x.com","responseStatus":"accepted"},{"email":"me@gmail.com","self":true,"responseStatus":"needsAction"}],
  "conferenceData":{"entryPoints":[{"entryPointType":"phone","uri":"tel:+1555"},{"entryPointType":"video","uri":"https://meet.google.com/abc-defg-hij"}]}},
 {"id":"e2_20260311T090000Z","status":"tentative","summary":"Standup","recurringEventId":"e2","iCalUID":"e2@google.com",
  "start":{"dateTime":"2026-03-11T09:00:00Z"},"end":{"dateTime":"2026-03-11T09:15:00Z"},
  "description":"Join https://acme.zoom.us/j/99 please"},
 {"id":"e3","summary":"Maya's birthday","start":{"date":"2026-03-12"},"end":{"date":"2026-03-13"}},
 {"id":"e4","status":"cancelled","summary":"Gone","start":{"dateTime":"2026-03-10T12:00:00Z"},"end":{"dateTime":"2026-03-10T13:00:00Z"}},
 {"id":"e5","summary":"","start":{"dateTime":"2026-03-10T14:00:00Z"},"end":{"dateTime":"2026-03-10T15:00:00Z"},"attendees":[{"email":"me@gmail.com","self":true,"responseStatus":"declined"}]},
 {"id":"bad","summary":"no times"},
 {"id":"outside","summary":"Next year","start":{"dateTime":"2027-03-10T14:00:00Z"},"end":{"dateTime":"2027-03-10T15:00:00Z"}}
]}"##;

fn window() -> Window {
    Window {
        from: Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap(),
        to: Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap(),
    }
}

fn setup(server: &TestServer, tokens: Option<TokenSet>) -> (GoogleProvider, Account) {
    let secrets = Arc::new(MemorySecretStore::default());
    let t = tokens.unwrap_or(TokenSet {
        access_token: "AT".into(),
        refresh_token: "RT".into(),
        expires_at: Utc::now() + Duration::hours(1),
    });
    secrets
        .set(&secret_key("acct"), &serde_json::to_string(&t).unwrap())
        .unwrap();
    let mut cfg = oauth_config("cid", None);
    cfg.token_url = server.url("/token");
    let p = GoogleProvider::with_base(
        Arc::new(ReqwestClient::new_allow_http_for_tests().unwrap()),
        secrets,
        cfg,
        &server.url("/calendar/v3"),
    );
    let account = Account {
        id: "acct".into(),
        provider: ProviderKind::Google,
        display_name: "me@gmail.com".into(),
        email: Some("me@gmail.com".into()),
        status: AccountStatus::Ok,
        secret_ref: Some(secret_key("acct")),
        created_at: Utc::now(),
    };
    (p, account)
}

fn api() -> TestServer {
    TestServer::start(|r| {
        if r.header("authorization") != Some("Bearer AT") && !r.path.starts_with("/token") {
            return Resp::status(401);
        }
        if r.path.starts_with("/calendar/v3/users/me/calendarList") {
            Resp::json(CAL_LIST)
        } else if r.path.contains("/events") {
            Resp::json(EVENTS)
        } else {
            Resp::status(404)
        }
    })
}

#[test]
fn lists_calendars_with_names_colours_and_roles() {
    let server = api();
    let (p, acct) = setup(&server, None);
    let cals = p.list_calendars(&acct).unwrap();
    let names: Vec<_> = cals.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        ["me@gmail.com", "Home", "Holidays"],
        "hidden calendars skipped, summaryOverride preferred"
    );
    assert_eq!(cals[0].colour, "teal");
    assert_eq!(cals[1].colour, "red");
    assert_eq!(cals[2].colour, "green");
    assert!(!cals[0].read_only && cals[1].read_only);
    assert_eq!(cals[1].remote_id, "fam#family@group.v.calendar.google.com");
    assert!(cals.iter().all(|c| c.id.starts_with("acct:")));
    // The request carried the bearer token and asked for readable calendars.
    let seen = server.requests.lock().unwrap();
    assert!(seen[0].path.contains("minAccessRole=reader"));
}

#[test]
fn maps_events_including_all_day_conference_and_rsvp() {
    let server = api();
    let (p, acct) = setup(&server, None);
    let cal = p.list_calendars(&acct).unwrap().remove(0);
    let r = p.sync_events(&cal, None, &window()).unwrap();
    assert!(r.full && r.next_state.is_none());
    let by = |id: &str| {
        r.changed
            .iter()
            .find(|e| e.remote_id == id)
            .unwrap_or_else(|| panic!("missing {id}"))
    };

    let e1 = by("e1");
    assert_eq!(e1.title, "Design review");
    assert_eq!(
        e1.join_url.as_deref(),
        Some("https://meet.google.com/abc-defg-hij"),
        "video entry point beats the phone one"
    );
    assert_eq!(e1.my_response, Response::NeedsAction);
    assert_eq!(e1.organizer.as_deref(), Some("Priya"));
    assert_eq!(e1.ical_uid, "e1@google.com");
    assert!(
        matches!(&e1.when, When::Timed { start, tzid: Some(t), .. } if start.format("%H:%M").to_string() == "09:30" && t == "Europe/Berlin")
    );

    let e2 = by("e2_20260311T090000Z");
    assert_eq!(e2.status, EventStatus::Tentative);
    assert_eq!(e2.recurrence_master_id.as_deref(), Some("e2"));
    assert_eq!(
        e2.join_url.as_deref(),
        Some("https://acme.zoom.us/j/99"),
        "falls back to links in the description"
    );

    let e3 = by("e3");
    assert_eq!(
        e3.when,
        When::AllDay {
            start: NaiveDate::from_ymd_opt(2026, 3, 12).unwrap(),
            end: NaiveDate::from_ymd_opt(2026, 3, 13).unwrap()
        }
    );

    assert_eq!(by("e5").title, "(No title)");
    assert_eq!(by("e5").my_response, Response::Declined);
    assert!(
        r.changed
            .iter()
            .all(|e| e.remote_id != "bad" && e.remote_id != "outside"),
        "malformed and out-of-window items are dropped"
    );
}

#[test]
fn events_url_percent_encodes_the_calendar_id_and_sets_the_window() {
    let server = api();
    let (p, acct) = setup(&server, None);
    let cal = p.list_calendars(&acct).unwrap().remove(1); // id contains '#' and '@'
    p.sync_events(&cal, None, &window()).unwrap();
    let seen = server.requests.lock().unwrap();
    let req = seen.iter().find(|r| r.path.contains("/events")).unwrap();
    assert!(
        req.path
            .contains("/calendars/fam%23family@group.v.calendar.google.com/events")
            || req
                .path
                .contains("/calendars/fam%23family%40group.v.calendar.google.com/events"),
        "{}",
        req.path
    );
    for needle in [
        "singleEvents=true",
        "showDeleted=false",
        "timeMin=2026-03-01T00%3A00%3A00Z",
        "timeMax=2026-04-01T00%3A00%3A00Z",
    ] {
        assert!(
            req.path.contains(needle),
            "{needle} missing in {}",
            req.path
        );
    }
}

#[test]
fn follows_pagination() {
    let server = TestServer::start(|r| {
        if r.path.contains("pageToken=p2") {
            Resp::json(
                r#"{"items":[{"id":"b","summary":"B","start":{"dateTime":"2026-03-10T10:00:00Z"},"end":{"dateTime":"2026-03-10T11:00:00Z"}}]}"#,
            )
        } else {
            Resp::json(
                r#"{"nextPageToken":"p2","items":[{"id":"a","summary":"A","start":{"dateTime":"2026-03-10T08:00:00Z"},"end":{"dateTime":"2026-03-10T09:00:00Z"}}]}"#,
            )
        }
    });
    let (p, acct) = setup(&server, None);
    let cal = Calendar {
        id: "acct:x".into(),
        account_id: acct.id,
        remote_id: "primary".into(),
        name: "x".into(),
        colour: "blue".into(),
        visible: true,
        read_only: true,
    };
    let r = p.sync_events(&cal, None, &window()).unwrap();
    assert_eq!(r.changed.len(), 2);
}

#[test]
fn an_expired_access_token_is_refreshed_transparently() {
    let server = TestServer::start(|r| {
        if r.path == "/token" {
            Resp::json(r#"{"access_token":"AT","refresh_token":"RT2","expires_in":3600}"#)
        } else if r.header("authorization") == Some("Bearer AT") {
            Resp::json(CAL_LIST)
        } else {
            Resp::status(401)
        }
    });
    let expired = TokenSet {
        access_token: "stale".into(),
        refresh_token: "RT".into(),
        expires_at: Utc::now() - Duration::minutes(5),
    };
    let (p, acct) = setup(&server, Some(expired));
    assert_eq!(p.list_calendars(&acct).unwrap().len(), 3);
    let saved = TokenManager {
        http: p.http.clone(),
        secrets: p.tokens.secrets.clone(),
        cfg: p.tokens.cfg.clone(),
    }
    .load("acct")
    .unwrap();
    assert_eq!(
        saved.refresh_token, "RT2",
        "rotated refresh token was persisted"
    );
}

#[test]
fn revoked_refresh_token_or_rejected_access_means_reconnect() {
    let server = TestServer::start(|r| {
        if r.path == "/token" {
            Resp {
                status: 400,
                headers: vec![],
                body: r#"{"error":"invalid_grant"}"#.into(),
            }
        } else {
            Resp::status(401)
        }
    });
    let expired = TokenSet {
        access_token: "x".into(),
        refresh_token: "RT".into(),
        expires_at: Utc::now() - Duration::minutes(1),
    };
    let (p, acct) = setup(&server, Some(expired));
    assert!(matches!(p.list_calendars(&acct), Err(Error::AuthRevoked)));

    let rejecting = TestServer::start(|_| Resp::status(401));
    let (p, acct) = setup(&rejecting, None);
    assert!(matches!(p.list_calendars(&acct), Err(Error::AuthRevoked)));
}

#[test]
fn rate_limits_and_server_errors_are_not_revocation() {
    let limited = TestServer::start(|_| Resp {
        status: 403,
        headers: vec![],
        body: r#"{"error":{"errors":[{"reason":"rateLimitExceeded"}]}}"#.into(),
    });
    let (p, acct) = setup(&limited, None);
    assert!(matches!(p.list_calendars(&acct), Err(Error::Offline)));
    let throttled = TestServer::start(|_| Resp::status(429));
    let (p, acct) = setup(&throttled, None);
    assert!(matches!(p.list_calendars(&acct), Err(Error::Offline)));
    let broken = TestServer::start(|_| Resp::status(503));
    let (p, acct) = setup(&broken, None);
    assert!(matches!(p.list_calendars(&acct), Err(Error::Provider(_))));
}

#[test]
fn a_plain_forbidden_means_permission_was_withdrawn() {
    let server = TestServer::start(|_| Resp {
        status: 403,
        headers: vec![],
        body: r#"{"error":{"errors":[{"reason":"forbidden"}]}}"#.into(),
    });
    let (p, acct) = setup(&server, None);
    assert!(matches!(p.list_calendars(&acct), Err(Error::AuthRevoked)));
}

#[test]
fn profile_email_is_read_from_userinfo() {
    let server = TestServer::start(|r| {
        if r.header("authorization") == Some("Bearer AT") {
            Resp::json(r#"{"sub":"1","email":"me@gmail.com","name":"Me"}"#)
        } else {
            Resp::status(401)
        }
    });
    let http = ReqwestClient::new_allow_http_for_tests().unwrap();
    assert_eq!(
        GoogleProvider::fetch_email(&http, &server.url("/userinfo"), "AT").unwrap(),
        "me@gmail.com"
    );
    assert!(GoogleProvider::fetch_email(&http, &server.url("/userinfo"), "bad").is_err());
}

#[test]
fn oauth_config_requests_read_only_calendar_access() {
    let c = oauth_config("cid", Some("sec"));
    assert!(c.scopes.iter().any(|s| s.ends_with("calendar.readonly")));
    assert!(
        !c.scopes
            .iter()
            .any(|s| s.ends_with("/calendar") || s.contains("calendar.events")),
        "no write scopes"
    );
    assert!(c.auth_url.starts_with("https://") && c.token_url.starts_with("https://"));
}
