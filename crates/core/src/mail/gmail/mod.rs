//! Gmail through its REST API, read-only (`gmail.readonly`).
//!
//! Mail access is a separate permission from calendar access and is only requested when the user
//! turns email on for the account. Only message metadata and Google's own short snippet are read.

use super::{OpenTarget, RawMessage, SourceBatch, decode_entities, subject_looks_like_invitation};
use crate::http::{HttpClient, HttpRequest, HttpResponse};
use crate::oauth::TokenManager;
use crate::{Error, Result};
use chrono::{DateTime, Utc};
use reqwest::Url;
use serde_json::Value;
use std::sync::Arc;

pub const SCOPE: &str = "https://www.googleapis.com/auth/gmail.readonly";
pub const API_BASE: &str = "https://gmail.googleapis.com/gmail/v1";

pub struct GmailSource {
    http: Arc<dyn HttpClient>,
    tokens: TokenManager,
    api_base: String,
}

fn check(resp: &HttpResponse) -> Result<()> {
    match resp.status {
        200..=299 => Ok(()),
        401 => Err(Error::AuthRevoked),
        403 | 429 => {
            let body = resp.text();
            if resp.status == 429
                || body.contains("rateLimitExceeded")
                || body.contains("userRateLimitExceeded")
            {
                Err(Error::Offline)
            } else {
                // Typically "insufficient authentication scopes": mail access was never granted.
                Err(Error::PermissionNeeded)
            }
        }
        404 => Err(Error::Provider("message not found".into())),
        s => Err(Error::Provider(format!("Gmail returned {s}"))),
    }
}

fn header<'a>(msg: &'a Value, name: &str) -> Option<&'a str> {
    msg.pointer("/payload/headers")?
        .as_array()?
        .iter()
        .find(|h| {
            h.get("name")
                .and_then(Value::as_str)
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })?
        .get("value")?
        .as_str()
}

impl GmailSource {
    pub fn new(http: Arc<dyn HttpClient>, tokens: TokenManager) -> Self {
        Self::with_base(http, tokens, API_BASE)
    }

    pub fn with_base(http: Arc<dyn HttpClient>, tokens: TokenManager, api_base: &str) -> Self {
        Self {
            http,
            tokens,
            api_base: api_base.trim_end_matches('/').into(),
        }
    }

    fn get(&self, account_id: &str, url: &str) -> Result<Value> {
        let token = self.tokens.access_token(account_id, Utc::now())?;
        let resp = self.http.send(
            &HttpRequest::get(url)
                .header("Authorization", format!("Bearer {token}"))
                .header("Accept", "application/json"),
        )?;
        check(&resp)?;
        serde_json::from_slice(&resp.body)
            .map_err(|_| Error::Provider("Gmail sent a response we could not read".into()))
    }

    /// Unread inbox mail from the last `days_back` days, newest first, at most `max` messages.
    pub fn fetch(
        &self,
        account_id: &str,
        email: &str,
        days_back: i64,
        max: usize,
    ) -> Result<SourceBatch> {
        let unread_total = self
            .get(
                account_id,
                &format!("{}/users/me/labels/INBOX", self.api_base),
            )?
            .get("messagesUnread")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32;

        let mut list = Url::parse(&format!("{}/users/me/messages", self.api_base))
            .map_err(|_| Error::Invalid("bad API address"))?;
        list.query_pairs_mut()
            .append_pair(
                "q",
                &format!("is:unread in:inbox newer_than:{}d", days_back.clamp(1, 30)),
            )
            .append_pair("maxResults", &max.to_string());
        let ids: Vec<String> = self
            .get(account_id, list.as_str())?
            .get("messages")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|m| m.get("id").and_then(Value::as_str).map(String::from))
                    .take(max)
                    .collect()
            })
            .unwrap_or_default();

        let mut messages = Vec::new();
        for id in ids {
            // Only characters Gmail itself uses in ids go into a request.
            if !id.bytes().all(|b| b.is_ascii_alphanumeric()) {
                continue;
            }
            let mut u = Url::parse(&format!("{}/users/me/messages/{id}", self.api_base))
                .map_err(|_| Error::Invalid("bad API address"))?;
            u.query_pairs_mut()
                .append_pair("format", "metadata")
                .append_pair("metadataHeaders", "From")
                .append_pair("metadataHeaders", "Subject");
            let msg = match self.get(account_id, u.as_str()) {
                Ok(m) => m,
                Err(Error::Provider(_)) => continue, // deleted since listing
                Err(e) => return Err(e),
            };
            let subject = header(&msg, "Subject").unwrap_or("").to_string();
            let received = msg
                .get("internalDate")
                .and_then(Value::as_str)
                .and_then(|ms| ms.parse::<i64>().ok())
                .and_then(DateTime::from_timestamp_millis)
                .unwrap_or_default();
            let mailbox = Url::parse("https://mail.google.com/mail/u/0/")
                .ok()
                .map(|mut u| {
                    // `/u/<address>/` selects the right account for people signed in to several.
                    u.set_path(&format!(
                        "/mail/u/{}/",
                        if email.is_empty() { "0" } else { email }
                    ));
                    u.set_fragment(Some(&format!("inbox/{id}")));
                    u.to_string()
                });
            messages.push(RawMessage {
                id: id.clone(),
                sender: header(&msg, "From").unwrap_or("").to_string(),
                invitation_hint: subject_looks_like_invitation(&subject),
                subject,
                received,
                snippet: decode_entities(msg.get("snippet").and_then(Value::as_str).unwrap_or("")),
                unread: true,
                ics: None,
                open: OpenTarget::Url {
                    url: mailbox.unwrap_or_else(|| "https://mail.google.com/".into()),
                },
            });
        }
        Ok(SourceBatch {
            unread_total,
            messages,
        })
    }
}

#[cfg(test)]
mod tests;
