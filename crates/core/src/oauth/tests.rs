use super::*;
use crate::http::ReqwestClient;
use crate::http::testserver::{Req, Resp, TestServer};
use crate::secrets::MemorySecretStore;
use chrono::TimeZone;

fn cfg(server: &TestServer) -> OAuthConfig {
    OAuthConfig {
        client_id: "client-123".into(),
        client_secret: Some("shh".into()),
        auth_url: "https://accounts.example.com/auth".into(),
        token_url: server.url("/token"),
        scopes: vec!["calendar.read".into(), "email".into()],
    }
}

fn client() -> Arc<dyn HttpClient> {
    Arc::new(ReqwestClient::new_allow_http_for_tests().unwrap())
}

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, 10, 12, 0, 0).unwrap()
}

fn form_of(r: &Req) -> std::collections::HashMap<String, String> {
    Url::parse(&format!("http://x/?{}", r.body))
        .unwrap()
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

#[test]
fn pkce_matches_the_rfc_7636_test_vector() {
    // RFC 7636, Appendix B.
    assert_eq!(
        challenge_for("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
}

#[test]
fn generated_verifiers_are_valid_unique_and_unpadded() {
    let (a, b) = (Pkce::generate(), Pkce::generate());
    assert_ne!(a.verifier, b.verifier);
    assert!((43..=128).contains(&a.verifier.len()));
    assert!(
        a.verifier
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    );
    assert_eq!(a.challenge, challenge_for(&a.verifier));
    assert_ne!(random_state(), random_state());
}

#[test]
fn authorization_url_carries_pkce_state_and_scopes() {
    let server = TestServer::start(|_| Resp::ok(""));
    let url = authorization_url(
        &cfg(&server),
        "http://127.0.0.1:5555/callback",
        "st4te",
        "chal",
        &[("access_type", "offline")],
    )
    .unwrap();
    let u = Url::parse(&url).unwrap();
    let q: std::collections::HashMap<_, _> = u.query_pairs().into_owned().collect();
    assert_eq!(u.host_str(), Some("accounts.example.com"));
    assert_eq!(q["response_type"], "code");
    assert_eq!(q["client_id"], "client-123");
    assert_eq!(q["code_challenge"], "chal");
    assert_eq!(q["code_challenge_method"], "S256");
    assert_eq!(q["state"], "st4te");
    assert_eq!(q["scope"], "calendar.read email");
    assert_eq!(q["redirect_uri"], "http://127.0.0.1:5555/callback");
    assert_eq!(q["access_type"], "offline");
    assert!(
        !url.contains("client_secret"),
        "the secret never goes through the browser"
    );
    // https only
    let mut bad = cfg(&server);
    bad.auth_url = "http://accounts.example.com/auth".into();
    assert!(authorization_url(&bad, "x", "s", "c", &[]).is_err());
}

#[test]
fn code_exchange_sends_the_verifier_and_parses_tokens() {
    let server = TestServer::start(|_| {
        Resp::json(
            r#"{"access_token":"AT","refresh_token":"RT","expires_in":3599,"token_type":"Bearer"}"#,
        )
    });
    let t = exchange_code(
        client().as_ref(),
        &cfg(&server),
        "the-code",
        "the-verifier",
        "http://127.0.0.1:1/callback",
        now(),
    )
    .unwrap();
    assert_eq!(
        (t.access_token.as_str(), t.refresh_token.as_str()),
        ("AT", "RT")
    );
    assert_eq!(t.expires_at, now() + Duration::seconds(3599));
    let seen = server.requests.lock().unwrap();
    assert_eq!(seen[0].method, "POST");
    assert_eq!(
        seen[0].header("content-type"),
        Some("application/x-www-form-urlencoded")
    );
    let f = form_of(&seen[0]);
    assert_eq!(f["grant_type"], "authorization_code");
    assert_eq!(f["code"], "the-code");
    assert_eq!(f["code_verifier"], "the-verifier");
    assert_eq!(f["client_id"], "client-123");
    assert_eq!(f["client_secret"], "shh");
}

#[test]
fn exchange_without_a_refresh_token_is_an_error() {
    let server = TestServer::start(|_| Resp::json(r#"{"access_token":"AT","expires_in":3600}"#));
    assert!(exchange_code(client().as_ref(), &cfg(&server), "c", "v", "r", now()).is_err());
}

#[test]
fn error_responses_are_classified() {
    for (status, body, want) in [
        (400u16, r#"{"error":"invalid_grant"}"#, "revoked"),
        (401, r#"{"error":"invalid_client"}"#, "revoked"),
        (400, r#"{"error":"unauthorized_client"}"#, "revoked"),
        (503, "", "offline"),
        (429, "", "offline"),
        (400, r#"{"error":"invalid_request"}"#, "other"),
    ] {
        let body = body.to_string();
        let server = TestServer::start(move |_| Resp {
            status,
            headers: vec![],
            body: body.clone(),
        });
        let err = refresh(client().as_ref(), &cfg(&server), "RT", now()).unwrap_err();
        let got = match err {
            Error::AuthRevoked => "revoked",
            Error::Offline => "offline",
            _ => "other",
        };
        assert_eq!(got, want, "status {status}");
    }
}

#[test]
fn refresh_keeps_the_old_refresh_token_unless_a_new_one_arrives() {
    let server = TestServer::start(|r| {
        if form_of(r)["refresh_token"] == "RT1" {
            Resp::json(r#"{"access_token":"AT2","expires_in":3600}"#)
        } else {
            Resp::json(r#"{"access_token":"AT3","refresh_token":"RT3","expires_in":3600}"#)
        }
    });
    let kept = refresh(client().as_ref(), &cfg(&server), "RT1", now()).unwrap();
    assert_eq!(kept.refresh_token, "RT1");
    let rotated = refresh(client().as_ref(), &cfg(&server), "RT2", now()).unwrap();
    assert_eq!(rotated.refresh_token, "RT3");
}

fn manager(server: &TestServer, tokens: Option<TokenSet>) -> TokenManager {
    let m = TokenManager {
        http: client(),
        secrets: Arc::new(MemorySecretStore::default()),
        cfg: cfg(server),
    };
    if let Some(t) = tokens {
        m.store("acct", &t).unwrap();
    }
    m
}

fn tokens(expires_in: i64) -> TokenSet {
    TokenSet {
        access_token: "AT-old".into(),
        refresh_token: "RT-old".into(),
        expires_at: now() + Duration::seconds(expires_in),
    }
}

#[test]
fn a_valid_access_token_is_reused_without_any_request() {
    let server = TestServer::start(|_| Resp::status(500));
    let m = manager(&server, Some(tokens(3600)));
    assert_eq!(m.access_token("acct", now()).unwrap(), "AT-old");
    assert_eq!(server.count(), 0);
}

#[test]
fn an_expiring_token_is_refreshed_and_the_rotated_refresh_token_is_saved() {
    let server = TestServer::start(|_| {
        Resp::json(r#"{"access_token":"AT-new","refresh_token":"RT-new","expires_in":3600}"#)
    });
    let m = manager(&server, Some(tokens(30))); // inside the 90 s safety margin
    assert_eq!(m.access_token("acct", now()).unwrap(), "AT-new");
    let saved = m.load("acct").unwrap();
    assert_eq!(
        (saved.access_token.as_str(), saved.refresh_token.as_str()),
        ("AT-new", "RT-new")
    );
    assert_eq!(m.access_token("acct", now()).unwrap(), "AT-new");
    assert_eq!(server.count(), 1, "the second call used the cache");
}

#[test]
fn a_revoked_grant_surfaces_as_auth_revoked_and_keeps_nothing_new() {
    let server = TestServer::start(|_| Resp {
        status: 400,
        headers: vec![],
        body: r#"{"error":"invalid_grant"}"#.into(),
    });
    let m = manager(&server, Some(tokens(-10)));
    assert!(matches!(
        m.access_token("acct", now()),
        Err(Error::AuthRevoked)
    ));
}

#[test]
fn a_missing_secret_means_sign_in_again() {
    let server = TestServer::start(|_| Resp::status(500));
    assert!(matches!(
        manager(&server, None).access_token("acct", now()),
        Err(Error::AuthRevoked)
    ));
}

#[test]
fn tokens_never_print() {
    let t = tokens(100);
    assert!(!format!("{t:?}").contains("AT-old") && !format!("{t:?}").contains("RT-old"));
}
