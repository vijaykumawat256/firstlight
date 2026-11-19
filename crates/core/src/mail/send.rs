//! Sending a short plain note (running late, a meeting recap) from a Google or Microsoft account, after the person has
//! seen the exact message and pressed Send. Each needs its own permission, asked for separately (`gmail.send`,
//! `Mail.Send`); Yahoo and iCloud mailboxes send through `smtp`. Nothing here runs on its own.

use crate::http::{HttpClient, HttpRequest, HttpResponse};
use crate::invite;
use crate::mail::smtp::{self, Outgoing};
use crate::oauth::TokenManager;
use crate::{Error, Result};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use serde_json::json;

pub const GMAIL_SEND_SCOPE: &str = "https://www.googleapis.com/auth/gmail.send";
pub const GRAPH_SEND_SCOPE: &str = "Mail.Send";
pub const GMAIL_API: &str = "https://gmail.googleapis.com/gmail/v1";
pub const GRAPH_API: &str = "https://graph.microsoft.com/v1.0";

/// The note itself. Addresses are checked again here, whatever the page sent.
pub struct Note<'a> {
    pub from: &'a str,
    pub to: &'a [String],
    pub subject: &'a str,
    pub text: &'a str,
    pub now: DateTime<Utc>,
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

fn checked_to(to: &[String]) -> Result<Vec<String>> {
    if to.is_empty() || to.len() > smtp::MAX_RECIPIENTS {
        return Err(Error::Invalid("Choose between 1 and 20 recipients."));
    }
    to.iter()
        .map(|a| {
            invite::parse_address(a).ok_or(Error::Invalid("One of the addresses isn't valid."))
        })
        .collect()
}

/// Gmail: the message built exactly as for a mailbox (one text part), sent as `raw`.
pub fn send_gmail(
    http: &dyn HttpClient,
    tokens: &TokenManager,
    account_id: &str,
    note: &Note,
    api_base: &str,
) -> Result<()> {
    let to = checked_to(note.to)?;
    let raw = smtp::build_message(&Outgoing {
        from: note.from,
        to: &to,
        subject: note.subject,
        text: note.text,
        ics: None,
        method: "",
        now: note.now,
    })?;
    let token = tokens.access_token(account_id, note.now)?;
    let body = json!({ "raw": URL_SAFE_NO_PAD.encode(raw.as_bytes()) }).to_string();
    let resp = http.send(
        &HttpRequest::new(
            "POST",
            format!("{}/users/me/messages/send", api_base.trim_end_matches('/')),
        )
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(body),
    )?;
    check(&resp, "Gmail")
}

/// Microsoft Graph: `sendMail` with a plain-text body; a copy is kept in Sent Items.
pub fn send_graph(
    http: &dyn HttpClient,
    tokens: &TokenManager,
    account_id: &str,
    note: &Note,
    api_base: &str,
) -> Result<()> {
    let to = checked_to(note.to)?;
    let subject: String = note
        .subject
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let body = json!({
        "message": {
            "subject": subject.trim(),
            "body": { "contentType": "Text", "content": note.text },
            "toRecipients": to.iter().map(|a| json!({ "emailAddress": { "address": a } })).collect::<Vec<_>>(),
        },
        "saveToSentItems": true
    })
    .to_string();
    let token = tokens.access_token(account_id, note.now)?;
    let resp = http.send(
        &HttpRequest::new(
            "POST",
            format!("{}/me/sendMail", api_base.trim_end_matches('/')),
        )
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(body),
    )?;
    check(&resp, "Microsoft")
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

    fn note<'a>(to: &'a [String]) -> Note<'a> {
        Note {
            from: "me@gmail.com",
            to,
            subject: "Running late: Design review",
            text: "Hi,\n\nI'm running about 10 minutes late.\n",
            now: Utc::now(),
        }
    }

    #[test]
    fn gmail_gets_the_whole_message_as_raw() {
        let seen = Arc::new(Mutex::new(None::<(String, String, String)>));
        let s2 = seen.clone();
        let server = TestServer::start(move |r| {
            *s2.lock().unwrap() = Some((r.method.clone(), r.path.clone(), r.body.clone()));
            if r.header("authorization") != Some("Bearer AT") {
                return Resp::status(401);
            }
            Resp::json(r#"{"id":"m1"}"#)
        });
        let (http, tm) = tokens(&server);
        let to = vec!["sam@example.com".to_string()];
        send_gmail(
            http.as_ref(),
            &tm,
            "acct",
            &note(&to),
            &server.url("/gmail/v1"),
        )
        .unwrap();
        let (method, path, body) = seen.lock().unwrap().clone().unwrap();
        assert_eq!(
            (method.as_str(), path.as_str()),
            ("POST", "/gmail/v1/users/me/messages/send")
        );
        let raw: serde_json::Value = serde_json::from_str(&body).unwrap();
        let decoded = String::from_utf8(
            URL_SAFE_NO_PAD
                .decode(raw["raw"].as_str().unwrap())
                .unwrap(),
        )
        .unwrap();
        assert!(decoded.contains("To: sam@example.com"));
        assert!(decoded.contains("Subject: Running late: Design review"));
    }

    #[test]
    fn microsoft_gets_a_plain_text_message_and_keeps_a_copy() {
        let seen = Arc::new(Mutex::new(String::new()));
        let s2 = seen.clone();
        let server = TestServer::start(move |r| {
            *s2.lock().unwrap() = r.body.clone();
            if r.path == "/v1.0/me/sendMail" && r.method == "POST" {
                Resp::status(202)
            } else {
                Resp::status(404)
            }
        });
        let (http, tm) = tokens(&server);
        let to = vec!["sam@example.com".to_string(), "kim@example.com".to_string()];
        send_graph(http.as_ref(), &tm, "acct", &note(&to), &server.url("/v1.0")).unwrap();
        let body: serde_json::Value = serde_json::from_str(&seen.lock().unwrap()).unwrap();
        assert_eq!(body["message"]["body"]["contentType"], "Text");
        assert_eq!(
            body["message"]["toRecipients"][1]["emailAddress"]["address"],
            "kim@example.com"
        );
        assert_eq!(body["saveToSentItems"], true);
    }

    #[test]
    fn refusals_and_bad_addresses_are_reported_not_sent() {
        let server = TestServer::start(|_| Resp::status(403));
        let (http, tm) = tokens(&server);
        let to = vec!["sam@example.com".to_string()];
        assert!(matches!(
            send_graph(http.as_ref(), &tm, "acct", &note(&to), &server.url("/v1.0")),
            Err(Error::PermissionNeeded)
        ));
        let bad = vec!["not an address".to_string()];
        assert!(send_gmail(http.as_ref(), &tm, "acct", &note(&bad), &server.url("/g")).is_err());
        assert!(send_gmail(http.as_ref(), &tm, "acct", &note(&[]), &server.url("/g")).is_err());
    }
}
