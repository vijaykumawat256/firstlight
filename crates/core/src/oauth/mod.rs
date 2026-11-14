//! OAuth 2.0 authorization-code flow with PKCE for desktop apps (RFC 6749, 7636, 8252).
//!
//! * The user signs in in their own browser; Daybrief never sees a password.
//! * Tokens live only in the secret store. A rotated refresh token is saved *before* it is used.
//! * A rejected refresh token (`invalid_grant`) means [`Error::AuthRevoked`]: one clear prompt, no retry loop.

use crate::http::{HttpClient, HttpRequest};
use crate::secrets::SecretStore;
use crate::{Error, Result};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Duration, Utc};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub mod loopback;

/// Refresh this long before the access token actually expires.
const EXPIRY_MARGIN: Duration = Duration::seconds(90);

#[derive(Debug, Clone)]
pub struct OAuthConfig {
    pub client_id: String,
    /// Google issues desktop clients a "secret" that is not confidential; it is sent but may ship in the app.
    pub client_secret: Option<String>,
    pub auth_url: String,
    pub token_url: String,
    pub scopes: Vec<String>,
}

/// PKCE verifier and its S256 challenge.
#[derive(Clone)]
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

pub fn challenge_for(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

impl Pkce {
    pub fn generate() -> Self {
        let mut b = [0u8; 48]; // 64 base64url characters, within RFC 7636's 43-128
        getrandom::fill(&mut b).expect("system randomness unavailable");
        let verifier = URL_SAFE_NO_PAD.encode(b);
        let challenge = challenge_for(&verifier);
        Self {
            verifier,
            challenge,
        }
    }
}

/// An unguessable value tying the redirect back to the request that started it (CSRF protection).
pub fn random_state() -> String {
    let mut b = [0u8; 24];
    getrandom::fill(&mut b).expect("system randomness unavailable");
    URL_SAFE_NO_PAD.encode(b)
}

/// The tokens for one account, stored as JSON in the secret store.
#[derive(Clone, Serialize, Deserialize)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: DateTime<Utc>,
}

impl std::fmt::Debug for TokenSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TokenSet([redacted])")
    }
}

pub fn secret_key(account_id: &str) -> String {
    format!("oauth:{account_id}")
}

/// The URL to open in the user's browser.
pub fn authorization_url(
    cfg: &OAuthConfig,
    redirect_uri: &str,
    state: &str,
    challenge: &str,
    extra: &[(&str, &str)],
) -> Result<String> {
    let mut url =
        Url::parse(&cfg.auth_url).map_err(|_| Error::Invalid("bad authorisation address"))?;
    if url.scheme() != "https" {
        return Err(Error::Invalid("authorisation address must be https"));
    }
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("client_id", &cfg.client_id)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", &cfg.scopes.join(" "))
            .append_pair("state", state)
            .append_pair("code_challenge", challenge)
            .append_pair("code_challenge_method", "S256");
        for (k, v) in extra {
            q.append_pair(k, v);
        }
    }
    Ok(url.to_string())
}

fn form(pairs: &[(&str, &str)]) -> String {
    let mut u = Url::parse("http://x/").expect("static");
    u.query_pairs_mut().extend_pairs(pairs.iter().copied());
    u.query().unwrap_or_default().to_string()
}

fn token_request<'a>(
    http: &dyn HttpClient,
    cfg: &'a OAuthConfig,
    mut pairs: Vec<(&'a str, &'a str)>,
) -> Result<serde_json::Value> {
    pairs.push(("client_id", &cfg.client_id));
    if let Some(s) = &cfg.client_secret {
        pairs.push(("client_secret", s));
    }
    let req = HttpRequest::new("POST", cfg.token_url.clone())
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .body(form(&pairs));
    let resp = http.send(&req)?;
    let json: serde_json::Value =
        serde_json::from_slice(&resp.body).unwrap_or(serde_json::Value::Null);
    if resp.is_success() {
        return Ok(json);
    }
    // 400/401 with invalid_grant / invalid_client / unauthorized_client: the grant is dead.
    let code = json.get("error").and_then(|e| e.as_str()).unwrap_or("");
    if matches!(
        code,
        "invalid_grant" | "invalid_client" | "unauthorized_client" | "interaction_required"
    ) || resp.status == 401
    {
        return Err(Error::AuthRevoked);
    }
    if resp.status == 429 || resp.status >= 500 {
        return Err(Error::Offline); // transient: try again later, do not mark the account broken
    }
    Err(Error::Provider(format!(
        "sign-in server returned {}",
        resp.status
    )))
}

fn parse_tokens(
    json: &serde_json::Value,
    previous_refresh: Option<&str>,
    now: DateTime<Utc>,
) -> Result<TokenSet> {
    let access = json
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Provider("sign-in response had no access token".into()))?;
    // Providers only sometimes rotate the refresh token; keep the old one when no new one arrives.
    let refresh = json
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .or(previous_refresh)
        .ok_or_else(|| Error::Provider("sign-in response had no refresh token".into()))?;
    let secs = json
        .get("expires_in")
        .and_then(|v| v.as_i64())
        .unwrap_or(3600)
        .clamp(60, 86_400);
    Ok(TokenSet {
        access_token: access.into(),
        refresh_token: refresh.into(),
        expires_at: now + Duration::seconds(secs),
    })
}

/// Trades the authorization code from the redirect for tokens.
pub fn exchange_code(
    http: &dyn HttpClient,
    cfg: &OAuthConfig,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
    now: DateTime<Utc>,
) -> Result<TokenSet> {
    let json = token_request(
        http,
        cfg,
        vec![
            ("grant_type", "authorization_code"),
            ("code", code),
            ("code_verifier", verifier),
            ("redirect_uri", redirect_uri),
        ],
    )?;
    parse_tokens(&json, None, now)
}

pub fn refresh(
    http: &dyn HttpClient,
    cfg: &OAuthConfig,
    refresh_token: &str,
    now: DateTime<Utc>,
) -> Result<TokenSet> {
    let json = token_request(
        http,
        cfg,
        vec![
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ],
    )?;
    parse_tokens(&json, Some(refresh_token), now)
}

/// Hands out valid access tokens for an account, refreshing and persisting as needed.
pub struct TokenManager {
    pub http: Arc<dyn HttpClient>,
    pub secrets: Arc<dyn SecretStore>,
    pub cfg: OAuthConfig,
}

impl TokenManager {
    pub fn load(&self, account_id: &str) -> Result<TokenSet> {
        let raw = self
            .secrets
            .get(&secret_key(account_id))?
            .ok_or(Error::AuthRevoked)?;
        serde_json::from_str(&raw).map_err(|_| Error::Secret("stored sign-in is unreadable".into()))
    }

    pub fn store(&self, account_id: &str, t: &TokenSet) -> Result<()> {
        let raw =
            serde_json::to_string(t).map_err(|_| Error::Secret("could not save sign-in".into()))?;
        self.secrets.set(&secret_key(account_id), &raw)
    }

    pub fn access_token(&self, account_id: &str, now: DateTime<Utc>) -> Result<String> {
        let tokens = self.load(account_id)?;
        if tokens.expires_at - EXPIRY_MARGIN > now {
            return Ok(tokens.access_token);
        }
        let fresh = refresh(self.http.as_ref(), &self.cfg, &tokens.refresh_token, now)?;
        // Save before use: if the provider rotated the refresh token, losing it would force a re-login.
        self.store(account_id, &fresh)?;
        Ok(fresh.access_token)
    }
}

#[cfg(test)]
mod tests;
