//! "Sign in with OpenRouter": the one service here that lets someone authorise an app with their account
//! instead of pasting a key. It is the standard browser sign-in (PKCE, a redirect to a port on this
//! computer) and it ends with an API key that belongs to the user's own OpenRouter account and bills their
//! own credits. Firstlight keeps that key in the keychain exactly like a pasted one.
//!
//! OpenRouter reaches models from many companies (OpenAI, Anthropic, Google, xAI and more), so this is also
//! the simplest way to use several of them with a single sign-in.

use crate::http::{HttpClient, HttpRequest};
use crate::secrets::Redacted;
use crate::{Error, Result};
use serde_json::{Value, json};

pub const AUTH_URL: &str = "https://openrouter.ai/auth";
pub const KEY_URL: &str = "https://openrouter.ai/api/v1/auth/keys";
pub const HOST: &str = "openrouter.ai";

/// The address to open in the browser. `redirect` is the loopback address we are listening on.
pub fn authorization_url(redirect: &str, challenge: &str, state: &str) -> String {
    let enc = |s: &str| {
        let mut out = String::new();
        for b in s.bytes() {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                out.push(b as char);
            } else {
                out.push_str(&format!("%{b:02X}"));
            }
        }
        out
    };
    format!(
        "{AUTH_URL}?callback_url={}&code_challenge={}&code_challenge_method=S256&state={}&key_label=Firstlight",
        enc(redirect),
        enc(challenge),
        enc(state)
    )
}

/// Trades the one-time code for the user's key. The key is returned wrapped so it cannot be printed.
pub fn exchange(http: &dyn HttpClient, url: &str, code: &str, verifier: &str) -> Result<Redacted> {
    let body = json!({ "code": code, "code_verifier": verifier, "code_challenge_method": "S256" });
    let req = HttpRequest::new("POST", url)
        .header("content-type", "application/json")
        .body(body.to_string());
    let resp = http.send(&req)?;
    match resp.status {
        200..=299 => {}
        400 | 403 => return Err(Error::AuthRevoked), // the code was wrong, used, or expired
        429 | 500..=599 => return Err(Error::Offline),
        _ => return Err(Error::Provider("OpenRouter refused the sign-in".into())),
    }
    let v: Value = serde_json::from_slice(&resp.body)
        .map_err(|_| Error::Provider("OpenRouter sent an unreadable reply".into()))?;
    let key = v
        .get("key")
        .and_then(Value::as_str)
        .filter(|k| (10..=300).contains(&k.len()) && k.bytes().all(|b| b.is_ascii_graphic()))
        .ok_or_else(|| Error::Provider("OpenRouter did not send a key".into()))?;
    Ok(Redacted::new(key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::ReqwestClient;
    use crate::http::testserver::{Resp, TestServer};

    #[test]
    fn the_sign_in_address_carries_the_challenge_and_state_and_never_the_verifier() {
        let url = authorization_url("http://localhost:51423/", "CHALLENGE_abc-123", "STATE xyz");
        assert!(url.starts_with("https://openrouter.ai/auth?"));
        assert!(url.contains("callback_url=http%3A%2F%2Flocalhost%3A51423%2F"));
        assert!(url.contains("code_challenge=CHALLENGE_abc-123"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("state=STATE%20xyz"));
        assert!(!url.contains("verifier"));
        assert!(!url.contains(' '));
    }

    fn http() -> ReqwestClient {
        ReqwestClient::new_allow_http_for_tests().unwrap()
    }

    #[test]
    fn the_code_is_traded_for_the_users_key() {
        let server = TestServer::start(|_| Resp::json(r#"{"key":"sk-or-v1-ABCDEFGHIJ"}"#));
        let key = exchange(
            &http(),
            &server.url("/api/v1/auth/keys"),
            "THE-CODE",
            "THE-VERIFIER",
        )
        .unwrap();
        assert_eq!(key.expose(), "sk-or-v1-ABCDEFGHIJ");
        assert!(
            !format!("{key:?}").contains("ABCDEFGHIJ"),
            "never printable"
        );
        let seen = server.requests.lock().unwrap();
        assert_eq!(
            (seen[0].method.as_str(), seen[0].path.as_str()),
            ("POST", "/api/v1/auth/keys")
        );
        let body: Value = serde_json::from_str(&seen[0].body).unwrap();
        assert_eq!(body["code"], "THE-CODE");
        assert_eq!(body["code_verifier"], "THE-VERIFIER");
        assert_eq!(body["code_challenge_method"], "S256");
    }

    #[test]
    fn a_bad_code_a_busy_service_and_a_strange_reply_each_fail_clearly() {
        let case = |resp: fn() -> Resp| {
            let server = TestServer::start(move |_| resp());
            exchange(&http(), &server.url("/k"), "c", "v").unwrap_err()
        };
        assert!(matches!(case(|| Resp::status(403)), Error::AuthRevoked));
        assert!(matches!(case(|| Resp::status(400)), Error::AuthRevoked));
        assert!(matches!(case(|| Resp::status(503)), Error::Offline));
        assert!(matches!(case(|| Resp::status(418)), Error::Provider(_)));
        assert!(matches!(
            case(|| Resp::json("not json")),
            Error::Provider(_)
        ));
        assert!(matches!(case(|| Resp::json(r#"{}"#)), Error::Provider(_)));
        assert!(matches!(
            case(|| Resp::json(r#"{"key":"short"}"#)),
            Error::Provider(_)
        ));
        assert!(matches!(
            case(|| Resp::json(r#"{"key":"has a space in it, bad"}"#)),
            Error::Provider(_)
        ));
    }
}
