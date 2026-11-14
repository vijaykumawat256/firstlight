use super::*;
use crate::http::ReqwestClient;
use crate::http::testserver::{Resp, TestServer};
use crate::oauth::{OAuthConfig, TokenSet, secret_key};
use crate::secrets::{MemorySecretStore, SecretStore};
use chrono::Duration;

fn source(server: &TestServer) -> GmailSource {
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
    GmailSource::with_base(
        http.clone(),
        TokenManager { http, secrets, cfg },
        &server.url("/gmail/v1"),
    )
}

fn api() -> TestServer {
    TestServer::start(|r| {
        if r.header("authorization") != Some("Bearer AT") {
            return Resp::status(401);
        }
        let p = r.path.as_str();
        if p.starts_with("/gmail/v1/users/me/labels/INBOX") {
            Resp::json(r#"{"id":"INBOX","messagesUnread":37}"#)
        } else if p.starts_with("/gmail/v1/users/me/messages?") {
            Resp::json(r#"{"messages":[{"id":"m1"},{"id":"m2"},{"id":"gone"},{"id":"bad/id"}]}"#)
        } else if p.starts_with("/gmail/v1/users/me/messages/m1") {
            Resp::json(
                r#"{"id":"m1","snippet":"Let&#39;s sync &amp; plan","internalDate":"1773144000000","payload":{"headers":[{"name":"From","value":"Priya Shah <priya@x.com>"},{"name":"Subject","value":"Agenda"},{"name":"Date","value":"Tue"}]}}"#,
            )
        } else if p.starts_with("/gmail/v1/users/me/messages/m2") {
            Resp::json(
                r#"{"id":"m2","snippet":"You are invited","internalDate":"1773140400000","payload":{"headers":[{"name":"from","value":"cal@x.com"},{"name":"SUBJECT","value":"Invitation: Design review @ Tue"}]}}"#,
            )
        } else {
            Resp::status(404)
        }
    })
}

#[test]
fn reads_the_unread_total_and_message_metadata() {
    let server = api();
    let b = source(&server)
        .fetch("acct", "me@gmail.com", 3, 25)
        .unwrap();
    assert_eq!(b.unread_total, 37);
    assert_eq!(
        b.messages.len(),
        2,
        "a message deleted since listing and an unsafe id are skipped"
    );
    let m1 = &b.messages[0];
    assert_eq!(
        (m1.id.as_str(), m1.sender.as_str(), m1.subject.as_str()),
        ("m1", "Priya Shah <priya@x.com>", "Agenda")
    );
    assert_eq!(
        m1.snippet, "Let's sync & plan",
        "entities are decoded to plain text"
    );
    assert_eq!(m1.received.timestamp(), 1_773_144_000);
    assert!(m1.unread && !m1.invitation_hint);
    let m2 = &b.messages[1];
    assert_eq!(
        m2.sender, "cal@x.com",
        "header names are matched ignoring case"
    );
    assert!(m2.invitation_hint, "recognised from the subject");
}

#[test]
fn the_query_asks_only_for_recent_unread_inbox_mail_and_never_for_bodies() {
    let server = api();
    source(&server)
        .fetch("acct", "me@gmail.com", 3, 25)
        .unwrap();
    let seen = server.requests.lock().unwrap();
    let list = seen
        .iter()
        .find(|r| r.path.starts_with("/gmail/v1/users/me/messages?"))
        .unwrap();
    assert!(
        list.path.contains("is%3Aunread+in%3Ainbox+newer_than%3A3d")
            || list.path.contains("is%3Aunread%20in%3Ainbox"),
        "{}",
        list.path
    );
    assert!(list.path.contains("maxResults=25"));
    for r in seen.iter().filter(|r| r.path.contains("/messages/")) {
        assert!(
            r.path.contains("format=metadata"),
            "never the full message: {}",
            r.path
        );
        assert!(!r.path.contains("format=full") && !r.path.contains("format=raw"));
    }
}

#[test]
fn messages_open_in_the_right_gmail_account_over_https() {
    let server = api();
    let b = source(&server)
        .fetch("acct", "me@gmail.com", 3, 25)
        .unwrap();
    match &b.messages[0].open {
        OpenTarget::Url { url } => {
            assert!(
                url.starts_with("https://mail.google.com/mail/u/me@gmail.com/"),
                "{url}"
            );
            assert!(url.ends_with("#inbox/m1"), "{url}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn missing_mail_permission_is_reported_as_such_not_as_revoked_access() {
    let server = TestServer::start(|_| {
        Resp { status: 403, headers: vec![], body: r#"{"error":{"errors":[{"reason":"insufficientPermissions"}],"message":"Insufficient Permission"}}"#.into() }
    });
    assert!(matches!(
        source(&server).fetch("acct", "", 3, 25),
        Err(Error::PermissionNeeded)
    ));
}

#[test]
fn error_statuses_are_classified() {
    for (status, want) in [(401u16, "auth"), (429, "offline"), (500, "other")] {
        let server = TestServer::start(move |_| Resp::status(status));
        let got = match source(&server).fetch("acct", "", 3, 25).unwrap_err() {
            Error::AuthRevoked => "auth",
            Error::Offline => "offline",
            _ => "other",
        };
        assert_eq!(got, want, "status {status}");
    }
}

#[test]
fn an_empty_inbox_is_fine() {
    let server = TestServer::start(|r| {
        if r.path.contains("/labels/") {
            Resp::json(r#"{"messagesUnread":0}"#)
        } else {
            Resp::json("{}")
        }
    });
    let b = source(&server).fetch("acct", "", 3, 25).unwrap();
    assert_eq!((b.unread_total, b.messages.len()), (0, 0));
}

#[test]
fn requests_ask_for_the_least_access_scope_that_works() {
    assert!(SCOPE.ends_with("gmail.readonly"));
    assert!(
        !SCOPE.contains("modify")
            && !SCOPE.contains("send")
            && !SCOPE.contains("compose")
            && !SCOPE.contains("mail.google.com/"),
        "read-only"
    );
}
