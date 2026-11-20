//! The automatic out-of-office reply in Gmail or Outlook, set from Time off. Both services take a start and an end,
//! so the reply switches itself on and off; Firstlight sets it once, when the person presses the button after
//! reading the exact text. Each needs its own permission, asked for separately (`gmail.settings.basic`,
//! `MailboxSettings.ReadWrite`).

use crate::http::{HttpClient, HttpRequest, HttpResponse};
use crate::oauth::TokenManager;
use crate::{Error, Result};
use chrono::{DateTime, Utc};
use serde_json::json;

pub const GMAIL_SETTINGS_SCOPE: &str = "https://www.googleapis.com/auth/gmail.settings.basic";
pub const GRAPH_MAILBOX_SCOPE: &str = "MailboxSettings.ReadWrite";
pub const MAX_TEXT: usize = 2000;

/// The reply and when it applies (`from` up to `to`).
pub struct AutoReply<'a> {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub subject: &'a str,
    pub text: &'a str,
}

fn check(resp: &HttpResponse, service: &str) -> Result<()> {
    match resp.status {
        200..=299 => Ok(()),
        401 => Err(Error::AuthRevoked),
        403 => Err(Error::PermissionNeeded),
        429 | 503 | 504 => Err(Error::Offline),
        s => Err(Error::Provider(format!("{service} returned {s}"))),
    }
}

fn tidy(r: &AutoReply) -> Result<(String, String)> {
    if r.to <= r.from {
        return Err(Error::Invalid("The reply has to end after it starts."));
    }
    let text: String = r
        .text
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .collect();
    let text = text.trim().to_string();
    if text.is_empty() || text.chars().count() > MAX_TEXT {
        return Err(Error::Invalid("Write a reply of up to 2,000 characters."));
    }
    let subject: String = r
        .subject
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(200)
        .collect();
    Ok((subject.trim().to_string(), text))
}

/// Outlook shows the reply as HTML: the text is escaped and its lines kept.
fn html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\n', "<br>")
}

/// Gmail's vacation responder, on from `from` to `to`, for everyone who writes.
pub fn set_gmail(
    http: &dyn HttpClient,
    tokens: &TokenManager,
    account_id: &str,
    r: &AutoReply,
    api_base: &str,
    now: DateTime<Utc>,
) -> Result<()> {
    let (subject, text) = tidy(r)?;
    let body = json!({
        "enableAutoReply": true,
        "responseSubject": subject,
        "responseBodyPlainText": text,
        "restrictToContacts": false,
        "restrictToDomain": false,
        "startTime": r.from.timestamp_millis().to_string(),
        "endTime": r.to.timestamp_millis().to_string(),
    })
    .to_string();
    let token = tokens.access_token(account_id, now)?;
    let resp = http.send(
        &HttpRequest::new(
            "PUT",
            format!(
                "{}/users/me/settings/vacation",
                api_base.trim_end_matches('/')
            ),
        )
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(body),
    )?;
    check(&resp, "Gmail")
}

/// Outlook's automatic replies, scheduled from `from` to `to`, inside and outside the organisation.
pub fn set_graph(
    http: &dyn HttpClient,
    tokens: &TokenManager,
    account_id: &str,
    r: &AutoReply,
    api_base: &str,
    now: DateTime<Utc>,
) -> Result<()> {
    let (_, text) = tidy(r)?;
    let when = |t: DateTime<Utc>| json!({ "dateTime": t.format("%Y-%m-%dT%H:%M:%S").to_string(), "timeZone": "UTC" });
    let body = json!({
        "automaticRepliesSetting": {
            "status": "scheduled",
            "externalAudience": "all",
            "scheduledStartDateTime": when(r.from),
            "scheduledEndDateTime": when(r.to),
            "internalReplyMessage": html(&text),
            "externalReplyMessage": html(&text),
        }
    })
    .to_string();
    let token = tokens.access_token(account_id, now)?;
    let resp = http.send(
        &HttpRequest::new(
            "PATCH",
            format!("{}/me/mailboxSettings", api_base.trim_end_matches('/')),
        )
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(body),
    )?;
    check(&resp, "Microsoft")
}

/// Turns the automatic reply off now.
pub fn clear(
    http: &dyn HttpClient,
    tokens: &TokenManager,
    account_id: &str,
    google: bool,
    api_base: &str,
    now: DateTime<Utc>,
) -> Result<()> {
    let token = tokens.access_token(account_id, now)?;
    let base = api_base.trim_end_matches('/');
    let req = if google {
        HttpRequest::new("PUT", format!("{base}/users/me/settings/vacation"))
            .body(json!({ "enableAutoReply": false }).to_string())
    } else {
        HttpRequest::new("PATCH", format!("{base}/me/mailboxSettings"))
            .body(json!({ "automaticRepliesSetting": { "status": "disabled" } }).to_string())
    };
    let resp = http.send(
        &req.header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json"),
    )?;
    check(&resp, if google { "Gmail" } else { "Microsoft" })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::ReqwestClient;
    use crate::http::testserver::{Resp, TestServer};
    use crate::oauth::{OAuthConfig, TokenSet, secret_key};
    use crate::secrets::{MemorySecretStore, SecretStore};
    use std::sync::{Arc, Mutex};

    fn tokens(server: &TestServer) -> (Arc<dyn HttpClient>, TokenManager) {
        let secrets = Arc::new(MemorySecretStore::default());
        let t = TokenSet {
            access_token: "AT".into(),
            refresh_token: "RT".into(),
            expires_at: Utc::now() + chrono::Duration::hours(1),
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
        let http: Arc<dyn HttpClient> =
            Arc::new(ReqwestClient::new_allow_http_for_tests().unwrap());
        (http.clone(), TokenManager { http, secrets, cfg })
    }

    fn reply<'a>(text: &'a str) -> AutoReply<'a> {
        AutoReply {
            from: "2026-12-24T05:00:00Z".parse().unwrap(),
            to: "2027-01-01T05:00:00Z".parse().unwrap(),
            subject: "Out of office",
            text,
        }
    }

    /// Method, path and body of each request the server saw.
    type Seen = Arc<Mutex<Vec<(String, String, String)>>>;

    fn recording() -> (TestServer, Seen) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s2 = seen.clone();
        let server = TestServer::start(move |r| {
            s2.lock()
                .unwrap()
                .push((r.method.clone(), r.path.clone(), r.body.clone()));
            if r.header("authorization") != Some("Bearer AT") {
                return Resp::status(401);
            }
            Resp::json("{}")
        });
        (server, seen)
    }

    #[test]
    fn gmail_gets_a_vacation_responder_with_its_dates() {
        let (server, seen) = recording();
        let (http, tm) = tokens(&server);
        set_gmail(
            http.as_ref(),
            &tm,
            "acct",
            &reply("I'm away until 4 Jan."),
            &server.url("/gmail/v1"),
            Utc::now(),
        )
        .unwrap();
        let (method, path, body) = seen.lock().unwrap()[0].clone();
        assert_eq!(
            (method.as_str(), path.as_str()),
            ("PUT", "/gmail/v1/users/me/settings/vacation")
        );
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["enableAutoReply"], true);
        assert_eq!(v["responseBodyPlainText"], "I'm away until 4 Jan.");
        assert_eq!(
            v["startTime"],
            "2026-12-24T05:00:00Z"
                .parse::<DateTime<Utc>>()
                .unwrap()
                .timestamp_millis()
                .to_string()
        );
        assert_eq!(v["restrictToContacts"], false);
    }

    #[test]
    fn outlook_gets_a_scheduled_reply_with_the_text_escaped() {
        let (server, seen) = recording();
        let (http, tm) = tokens(&server);
        set_graph(
            http.as_ref(),
            &tm,
            "acct",
            &reply("Away <until> 4 Jan & back\nThanks"),
            &server.url("/v1.0"),
            Utc::now(),
        )
        .unwrap();
        let (method, path, body) = seen.lock().unwrap()[0].clone();
        assert_eq!(
            (method.as_str(), path.as_str()),
            ("PATCH", "/v1.0/me/mailboxSettings")
        );
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        let s = &v["automaticRepliesSetting"];
        assert_eq!(s["status"], "scheduled");
        assert_eq!(
            s["scheduledStartDateTime"]["dateTime"],
            "2026-12-24T05:00:00"
        );
        assert_eq!(s["scheduledEndDateTime"]["timeZone"], "UTC");
        assert_eq!(
            s["externalReplyMessage"],
            "Away &lt;until&gt; 4 Jan &amp; back<br>Thanks"
        );
    }

    #[test]
    fn turning_off_and_bad_input_and_refusals() {
        let (server, seen) = recording();
        let (http, tm) = tokens(&server);
        clear(
            http.as_ref(),
            &tm,
            "acct",
            false,
            &server.url("/v1.0"),
            Utc::now(),
        )
        .unwrap();
        clear(
            http.as_ref(),
            &tm,
            "acct",
            true,
            &server.url("/gmail/v1"),
            Utc::now(),
        )
        .unwrap();
        let calls = seen.lock().unwrap().clone();
        assert!(calls[0].2.contains("\"disabled\""));
        assert!(calls[1].2.contains("\"enableAutoReply\":false"));

        let backwards = AutoReply {
            from: "2027-01-01T00:00:00Z".parse().unwrap(),
            to: "2026-12-24T00:00:00Z".parse().unwrap(),
            subject: "x",
            text: "x",
        };
        assert!(
            set_gmail(
                http.as_ref(),
                &tm,
                "acct",
                &backwards,
                &server.url("/g"),
                Utc::now()
            )
            .is_err()
        );
        assert!(
            set_gmail(
                http.as_ref(),
                &tm,
                "acct",
                &reply("   "),
                &server.url("/g"),
                Utc::now()
            )
            .is_err()
        );
        let long = "x".repeat(MAX_TEXT + 1);
        assert!(
            set_graph(
                http.as_ref(),
                &tm,
                "acct",
                &reply(&long),
                &server.url("/g"),
                Utc::now()
            )
            .is_err()
        );

        let refusing = TestServer::start(|_| Resp::status(403));
        let (http, tm) = tokens(&refusing);
        assert!(matches!(
            set_graph(
                http.as_ref(),
                &tm,
                "acct",
                &reply("Away"),
                &refusing.url("/v1.0"),
                Utc::now()
            ),
            Err(Error::PermissionNeeded)
        ));
    }
}
