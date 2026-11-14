use super::*;
use crate::http::ReqwestClient;
use crate::http::testserver::{Req, Resp, TestServer};
use crate::secrets::MemorySecretStore;
use chrono::{TimeZone, Utc};

const USER: &str = "me@example.com";
const PASS: &str = "app-specific-pass";

fn multistatus(body: &str) -> Resp {
    Resp::status(207).with_header("Content-Type", "application/xml").with_body(&format!(r#"<?xml version="1.0"?><d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav" xmlns:cs="http://calendarserver.org/ns/" xmlns:a="http://apple.com/ns/ical/">{body}</d:multistatus>"#))
}

fn ok_prop(inner: &str) -> String {
    format!("<d:propstat><d:prop>{inner}</d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat>")
}

fn ics(uid: &str, summary: &str, day: u32) -> String {
    format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:{uid}\r\nDTSTART:202603{day:02}T090000Z\r\nDTEND:202603{day:02}T100000Z\r\nSUMMARY:{summary}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
    )
}

fn authed(r: &Req) -> bool {
    use base64::Engine;
    r.header("authorization")
        == Some(&format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!("{USER}:{PASS}"))
        ))
}

/// A mini CalDAV server: principal -> home -> calendars (one events calendar, one tasks-only, one inbox).
fn caldav_server(home_href: &'static str) -> TestServer {
    TestServer::start(move |r| {
        if !authed(r) {
            return Resp::status(401);
        }
        match (r.method.as_str(), r.path.as_str()) {
            ("PROPFIND", "/") => multistatus(&format!(
                "<d:response><d:href>/</d:href>{}</d:response>",
                ok_prop(
                    "<d:current-user-principal><d:href>/principals/me/</d:href></d:current-user-principal>"
                )
            )),
            ("PROPFIND", "/principals/me/") => multistatus(&format!(
                "<d:response><d:href>/principals/me/</d:href>{}</d:response>",
                ok_prop(&format!(
                    "<c:calendar-home-set><d:href>{home_href}</d:href></c:calendar-home-set>"
                ))
            )),
            ("PROPFIND", "/cals/me/") => multistatus(&format!(
                "<d:response><d:href>/cals/me/</d:href>{}</d:response>\
                 <d:response><d:href>/cals/me/work/</d:href>{}</d:response>\
                 <d:response><d:href>/cals/me/tasks/</d:href>{}</d:response>\
                 <d:response><d:href>/cals/me/inbox/</d:href>{}</d:response>",
                ok_prop("<d:resourcetype><d:collection/></d:resourcetype>"),
                ok_prop(
                    "<d:resourcetype><d:collection/><c:calendar/></d:resourcetype><d:displayname>Work &amp; Life</d:displayname><a:calendar-color>#FF2968FF</a:calendar-color><c:supported-calendar-component-set><c:comp name=\"VEVENT\"/></c:supported-calendar-component-set>"
                ),
                ok_prop(
                    "<d:resourcetype><d:collection/><c:calendar/></d:resourcetype><d:displayname>Reminders</d:displayname><c:supported-calendar-component-set><c:comp name=\"VTODO\"/></c:supported-calendar-component-set>"
                ),
                ok_prop(
                    "<d:resourcetype><d:collection/><c:schedule-inbox/></d:resourcetype><d:displayname>Inbox</d:displayname>"
                ),
            )),
            ("PROPFIND", "/cals/me/work/") => multistatus(&format!(
                "<d:response><d:href>/cals/me/work/</d:href>{}</d:response>",
                ok_prop("<cs:getctag>ctag-1</cs:getctag>")
            )),
            ("REPORT", "/cals/me/work/") => multistatus(&format!(
                "<d:response><d:href>/cals/me/work/a.ics</d:href>{}</d:response>\
                 <d:response><d:href>/cals/me/work/b.ics</d:href>{}</d:response>\
                 <d:response><d:href>/cals/me/work/bad.ics</d:href>{}</d:response>",
                ok_prop(&format!(
                    "<d:getetag>\"1\"</d:getetag><c:calendar-data>{}</c:calendar-data>",
                    ics("a", "Planning", 10)
                )),
                ok_prop(&format!(
                    "<d:getetag>\"2\"</d:getetag><c:calendar-data><![CDATA[{}]]></c:calendar-data>",
                    ics("b", "Review", 11)
                )),
                ok_prop("<c:calendar-data>garbage, not ics</c:calendar-data>"),
            )),
            _ => Resp::status(404),
        }
    })
}

fn setup(server: &TestServer, password: &str) -> (CaldavProvider, Account) {
    let secrets = Arc::new(MemorySecretStore::default());
    secrets
        .set(
            &CaldavProvider::secret_key("acct"),
            &serde_json::to_string(&CaldavSecret {
                server: server.url("/"),
                username: USER.into(),
                password: password.into(),
            })
            .unwrap(),
        )
        .unwrap();
    let p = CaldavProvider::new(
        Arc::new(ReqwestClient::new_allow_http_for_tests().unwrap()),
        secrets,
        chrono_tz::UTC,
    );
    let account = Account {
        id: "acct".into(),
        provider: ProviderKind::Caldav,
        display_name: "Test".into(),
        email: Some(USER.into()),
        status: AccountStatus::Ok,
        secret_ref: Some(CaldavProvider::secret_key("acct")),
        created_at: Utc::now(),
    };
    (p, account)
}

fn window() -> Window {
    Window {
        from: Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap(),
        to: Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap(),
    }
}

#[test]
fn discovers_principal_home_and_only_event_calendars() {
    let server = caldav_server("/cals/me/");
    let (p, acct) = setup(&server, PASS);
    let cals = p.list_calendars(&acct).unwrap();
    assert_eq!(
        cals.len(),
        1,
        "tasks-only calendars, inboxes and the home collection are skipped"
    );
    assert_eq!(cals[0].name, "Work & Life");
    assert_eq!(cals[0].colour, "pink", "#FF2968 is nearest to pink");
    assert!(cals[0].remote_id.ends_with("/cals/me/work/"));
    assert!(cals[0].id.starts_with("acct:"));
    // Ids are stable across calls.
    assert_eq!(p.list_calendars(&acct).unwrap()[0].id, cals[0].id);
}

#[test]
fn syncs_events_and_skips_broken_resources() {
    let server = caldav_server("/cals/me/");
    let (p, acct) = setup(&server, PASS);
    let cal = p.list_calendars(&acct).unwrap().remove(0);
    let r = p.sync_events(&cal, None, &window()).unwrap();
    let mut titles: Vec<_> = r.changed.iter().map(|e| e.title.as_str()).collect();
    titles.sort();
    assert_eq!(
        titles,
        ["Planning", "Review"],
        "the garbage resource is ignored, not fatal"
    );
    assert!(r.full);
    assert!(r.next_state.as_deref().unwrap().contains("ctag-1"));
    // The query asked for exactly our window.
    let seen = server.requests.lock().unwrap();
    let report = seen.iter().find(|q| q.method == "REPORT").unwrap();
    assert!(report.body.contains("20260301T000000Z") && report.body.contains("20260401T000000Z"));
    assert_eq!(report.header("depth"), Some("1"));
}

#[test]
fn an_unchanged_ctag_skips_the_expensive_report() {
    let server = caldav_server("/cals/me/");
    let (p, acct) = setup(&server, PASS);
    let cal = p.list_calendars(&acct).unwrap().remove(0);
    let first = p.sync_events(&cal, None, &window()).unwrap();
    let second = p
        .sync_events(&cal, first.next_state.as_deref(), &window())
        .unwrap();
    assert!(!second.full && second.changed.is_empty());
    let reports = server
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|q| q.method == "REPORT")
        .count();
    assert_eq!(reports, 1, "only the first sync fetched events");
    // A moved window must refetch even if the ctag matches.
    let moved = Window {
        from: window().from + chrono::Duration::days(1),
        to: window().to + chrono::Duration::days(1),
    };
    assert!(
        p.sync_events(&cal, first.next_state.as_deref(), &moved)
            .unwrap()
            .full
    );
}

#[test]
fn wrong_password_means_reconnect_and_is_never_echoed() {
    let server = caldav_server("/cals/me/");
    let (p, acct) = setup(&server, "WRONG-PASSWORD-123");
    let err = p.list_calendars(&acct).unwrap_err();
    assert!(matches!(err, Error::AuthRevoked));
    assert!(!format!("{err} {err:?}").contains("WRONG-PASSWORD"));
}

#[test]
fn a_server_cannot_redirect_our_credentials_to_another_host() {
    let evil = TestServer::start(|_| Resp::status(207));
    // The server claims the calendar home lives on another origin.
    let server = caldav_server(Box::leak(
        format!("{}/cals/me/", evil.base).into_boxed_str(),
    ));
    let (p, acct) = setup(&server, PASS);
    let err = p.list_calendars(&acct).unwrap_err();
    assert!(err.to_string().contains("different host"), "{err}");
    assert_eq!(
        evil.count(),
        0,
        "no request, and so no password, ever reached the other host"
    );
}

#[test]
fn missing_credentials_mean_reconnect() {
    let server = caldav_server("/cals/me/");
    let (p, acct) = setup(&server, PASS);
    p.secrets
        .delete(&CaldavProvider::secret_key("acct"))
        .unwrap();
    assert!(matches!(p.list_calendars(&acct), Err(Error::AuthRevoked)));
}

#[test]
fn rate_limiting_and_server_errors_are_not_treated_as_revoked_access() {
    for (status, offline) in [(429u16, true), (503, false)] {
        let server = TestServer::start(move |_| Resp::status(status));
        let (p, acct) = setup(&server, PASS);
        let err = p.list_calendars(&acct).unwrap_err();
        assert!(!matches!(err, Error::AuthRevoked), "{status}");
        assert_eq!(matches!(err, Error::Offline), offline, "{status}");
    }
}

#[test]
fn server_address_validation() {
    assert_eq!(
        validate_server(" https://caldav.icloud.com ").unwrap(),
        "https://caldav.icloud.com/"
    );
    for bad in [
        "http://caldav.example.com",
        "caldav.example.com",
        "https://user:pw@example.com",
        "ftp://x.test",
        "",
        "https://",
    ] {
        assert!(validate_server(bad).is_err(), "{bad}");
    }
}

#[test]
fn colour_matching() {
    assert_eq!(nearest_palette("#FF0000FF"), Some("red"));
    assert_eq!(nearest_palette("#0000ff"), Some("blue"));
    assert_eq!(nearest_palette("34c759"), Some("green"));
    assert_eq!(nearest_palette("not-a-colour"), None);
    assert_eq!(nearest_palette("#FFF"), None);
}

#[test]
fn secrets_never_appear_in_debug_output() {
    let s = CaldavSecret {
        server: "https://x".into(),
        username: "u@x".into(),
        password: "hunter2".into(),
    };
    assert!(!format!("{s:?}").contains("hunter2"));
}

#[test]
fn presets_are_valid_https_servers() {
    for (_, _, url) in PRESETS {
        assert!(validate_server(url).is_ok(), "{url}");
    }
}
