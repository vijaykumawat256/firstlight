use super::*;
use crate::http::ReqwestClient;
use crate::http::testserver::{Resp, TestServer};
use crate::oauth::{OAuthConfig, TokenSet, secret_key};
use crate::secrets::{MemorySecretStore, SecretStore};
use chrono::TimeZone;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, 10, 12, 0, 0).unwrap()
}

fn source(server: &TestServer) -> MicrosoftMailSource {
    let secrets = Arc::new(MemorySecretStore::default());
    let t = TokenSet {
        access_token: "AT".into(),
        refresh_token: "RT".into(),
        expires_at: Utc::now() + Duration::hours(1),
    };
    secrets
        .set(&secret_key("acct"), &serde_json::to_string(&t).unwrap())
        .unwrap();
    let cfg = OAuthConfig {
        client_id: "c".into(),
        client_secret: None,
        auth_url: "https://a.example/auth".into(),
        token_url: server.url("/token"),
        scopes: vec![],
    };
    let http: Arc<dyn HttpClient> = Arc::new(ReqwestClient::new_allow_http_for_tests().unwrap());
    MicrosoftMailSource::with_base(
        http.clone(),
        TokenManager { http, secrets, cfg },
        &server.url("/v1.0"),
    )
}

const MESSAGES: &str = r##"{"value":[
 {"id":"AAA1","subject":"Agenda for Tuesday","receivedDateTime":"2026-03-10T10:15:00Z","bodyPreview":"Hi all, attached is the agenda","webLink":"https://outlook.office.com/owa/?ItemID=AAA1&exvsurl=1","from":{"emailAddress":{"name":"Priya Shah","address":"priya@x.com"}}},
 {"@odata.type":"#microsoft.graph.eventMessageRequest","id":"AAA2","subject":"Design review","receivedDateTime":"2026-03-10T09:00:00Z","bodyPreview":"Priya has invited you","webLink":"https://outlook.office.com/owa/?ItemID=AAA2","from":{"emailAddress":{"name":"","address":"cal@x.com"}}},
 {"id":"AAA3","subject":"No link","receivedDateTime":"2026-03-10T08:00:00Z","bodyPreview":"x","webLink":"http://insecure.example/owa","from":{"emailAddress":{"name":"A","address":"a@x.com"}}},
 {"id":"AAA4","subject":"Script link","receivedDateTime":"2026-03-10T07:00:00Z","bodyPreview":"x","webLink":"javascript:alert(1)","from":{"emailAddress":{"name":"A","address":"a@x.com"}}},
 {"id":"AAA5","subject":"Bad time","receivedDateTime":"yesterday","bodyPreview":"x","webLink":"https://outlook.office.com/owa/?ItemID=AAA5"}
]}"##;

fn api() -> TestServer {
    TestServer::start(|r| {
        if r.header("authorization") != Some("Bearer AT") {
            return Resp::status(401);
        }
        if r.path.starts_with("/v1.0/me/mailFolders/inbox?") {
            Resp::json(r#"{"unreadItemCount":12}"#)
        } else if r.path.starts_with("/v1.0/me/mailFolders/inbox/messages?") {
            Resp::json(MESSAGES)
        } else {
            Resp::status(404)
        }
    })
}

#[test]
fn reads_unread_total_senders_and_previews() {
    let server = api();
    let b = source(&server).fetch("acct", 3, 25, now()).unwrap();
    assert_eq!(b.unread_total, 12);
    let ids: Vec<_> = b.messages.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(
        ids,
        ["AAA1", "AAA2"],
        "messages with a non-https link or no valid time are dropped"
    );
    assert_eq!(b.messages[0].sender, "Priya Shah <priya@x.com>");
    assert_eq!(b.messages[0].snippet, "Hi all, attached is the agenda");
    assert_eq!(
        b.messages[1].sender, "cal@x.com",
        "no display name: just the address"
    );
    assert!(
        b.messages[1].invitation_hint,
        "meeting requests are recognised by their Graph type"
    );
    assert!(!b.messages[0].invitation_hint);
    assert!(
        matches!(&b.messages[0].open, OpenTarget::Url { url } if url.starts_with("https://outlook.office.com/"))
    );
}

#[test]
fn the_request_is_windowed_ordered_validly_and_selects_only_what_it_needs() {
    let server = api();
    source(&server).fetch("acct", 3, 25, now()).unwrap();
    let seen = server.requests.lock().unwrap();
    let req = seen.iter().find(|r| r.path.contains("/messages?")).unwrap();
    let decoded = req
        .path
        .replace("%20", " ")
        .replace("+", " ")
        .replace("%3A", ":")
        .replace("%24", "$");
    assert!(
        decoded.contains("receivedDateTime ge 2026-03-07T12:00:00Z and isRead eq false"),
        "{decoded}"
    );
    assert!(
        decoded.contains("$orderby=receivedDateTime desc"),
        "orderby property comes first in the filter, as Graph requires"
    );
    assert!(decoded.contains("$top=25"));
    let select = decoded.split("$select=").nth(1).unwrap();
    assert!(
        !select.contains("body,") && !select.ends_with("body") && !select.contains("uniqueBody"),
        "never the body: {select}"
    );
    assert!(select.contains("bodyPreview"));
}

#[test]
fn permission_and_error_statuses_are_classified() {
    for (status, want) in [
        (401u16, "auth"),
        (403, "permission"),
        (429, "offline"),
        (503, "offline"),
        (500, "other"),
    ] {
        let server = TestServer::start(move |_| Resp::status(status));
        let got = match source(&server).fetch("acct", 3, 25, now()).unwrap_err() {
            Error::AuthRevoked => "auth",
            Error::PermissionNeeded => "permission",
            Error::Offline => "offline",
            _ => "other",
        };
        assert_eq!(got, want, "status {status}");
    }
}

#[test]
fn an_empty_inbox_is_fine() {
    let server = TestServer::start(|r| {
        if r.path.contains("/messages?") {
            Resp::json(r#"{"value":[]}"#)
        } else {
            Resp::json(r#"{"unreadItemCount":0}"#)
        }
    });
    let b = source(&server).fetch("acct", 3, 25, now()).unwrap();
    assert_eq!((b.unread_total, b.messages.len()), (0, 0));
}

#[test]
fn the_scope_is_read_only() {
    assert_eq!(SCOPE, "Mail.Read");
    assert!(!SCOPE.contains("Write") && !SCOPE.contains("Send"));
}
