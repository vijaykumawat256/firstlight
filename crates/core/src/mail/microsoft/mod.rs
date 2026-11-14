//! Outlook.com and Microsoft 365 mail through Microsoft Graph, read-only (`Mail.Read`).
//!
//! Requested only when the user turns email on for the account. Reads the sender, subject, time and
//! Microsoft's own short plain-text `bodyPreview`; never the message body.

use super::{OpenTarget, RawMessage, SourceBatch, subject_looks_like_invitation};
use crate::http::{HttpClient, HttpRequest, HttpResponse};
use crate::oauth::TokenManager;
use crate::{Error, Result};
use chrono::{DateTime, Duration, Utc};
use reqwest::Url;
use serde_json::Value;
use std::sync::Arc;

pub const SCOPE: &str = "Mail.Read";
pub const API_BASE: &str = "https://graph.microsoft.com/v1.0";

pub struct MicrosoftMailSource {
    http: Arc<dyn HttpClient>,
    tokens: TokenManager,
    api_base: String,
}

fn check(resp: &HttpResponse) -> Result<()> {
    match resp.status {
        200..=299 => Ok(()),
        401 => Err(Error::AuthRevoked),
        403 => Err(Error::PermissionNeeded), // Mail.Read not granted, or blocked by the organisation
        429 | 503 | 504 => Err(Error::Offline),
        s => Err(Error::Provider(format!("Microsoft returned {s}"))),
    }
}

fn is_https(url: &str) -> bool {
    Url::parse(url).is_ok_and(|u| u.scheme() == "https" && u.host_str().is_some())
}

impl MicrosoftMailSource {
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
            .map_err(|_| Error::Provider("Microsoft sent a response we could not read".into()))
    }

    pub fn fetch(
        &self,
        account_id: &str,
        days_back: i64,
        max: usize,
        now: DateTime<Utc>,
    ) -> Result<SourceBatch> {
        let unread_total = self
            .get(
                account_id,
                &format!(
                    "{}/me/mailFolders/inbox?$select=unreadItemCount",
                    self.api_base
                ),
            )?
            .get("unreadItemCount")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32;

        let since = (now - Duration::days(days_back.clamp(1, 30)))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let mut u = Url::parse(&format!("{}/me/mailFolders/inbox/messages", self.api_base))
            .map_err(|_| Error::Invalid("bad API address"))?;
        u.query_pairs_mut()
            // Graph only accepts $orderby on a property that comes first in $filter.
            .append_pair(
                "$filter",
                &format!("receivedDateTime ge {since} and isRead eq false"),
            )
            .append_pair("$orderby", "receivedDateTime desc")
            .append_pair("$top", &max.to_string())
            .append_pair(
                "$select",
                "id,subject,from,receivedDateTime,bodyPreview,webLink",
            );
        let list = self.get(account_id, u.as_str())?;

        let messages = list
            .get("value")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .take(max)
            .filter_map(|m| {
                let id = m.get("id")?.as_str()?.to_string();
                let subject = m
                    .get("subject")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let received = DateTime::parse_from_rfc3339(m.get("receivedDateTime")?.as_str()?)
                    .ok()?
                    .with_timezone(&Utc);
                let from = m.pointer("/from/emailAddress");
                let sender = match (
                    from.and_then(|f| f.get("name")).and_then(Value::as_str),
                    from.and_then(|f| f.get("address")).and_then(Value::as_str),
                ) {
                    (Some(n), Some(a)) if !n.is_empty() => format!("{n} <{a}>"),
                    (_, Some(a)) => a.to_string(),
                    _ => String::new(),
                };
                let meeting_request = m
                    .get("@odata.type")
                    .and_then(Value::as_str)
                    .is_some_and(|t| t.contains("eventMessage"));
                // The link must be https or the message cannot be opened from the digest.
                let open = m
                    .get("webLink")
                    .and_then(Value::as_str)
                    .filter(|l| is_https(l))?
                    .to_string();
                Some(RawMessage {
                    id,
                    sender,
                    invitation_hint: meeting_request || subject_looks_like_invitation(&subject),
                    subject,
                    received,
                    snippet: m
                        .get("bodyPreview")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    unread: true,
                    ics: None,
                    open: OpenTarget::Url { url: open },
                })
            })
            .collect();
        Ok(SourceBatch {
            unread_total,
            messages,
        })
    }
}

#[cfg(test)]
mod tests;
