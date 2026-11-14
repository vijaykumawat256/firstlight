//! Google Calendar via the REST API (OAuth, read-only scope).
//!
//! Each sync fetches the whole window with `singleEvents=true`, so Google expands recurrences
//! for us. Incremental sync tokens are deliberately not used yet: they cannot be combined with
//! a time window and the interaction needs verifying against the live API first.

use super::{Capabilities, Provider, SyncResult, Window, short_hash};
use crate::http::{HttpClient, HttpRequest, HttpResponse};
use crate::join;
use crate::model::*;
use crate::oauth::{OAuthConfig, TokenManager};
use crate::providers::caldav::nearest_palette;
use crate::secrets::SecretStore;
use crate::{Error, Result};
use chrono::{DateTime, NaiveDate, Utc};
use reqwest::Url;
use serde_json::Value;
use std::sync::Arc;

pub const API_BASE: &str = "https://www.googleapis.com/calendar/v3";
pub const USERINFO_URL: &str = "https://openidconnect.googleapis.com/v1/userinfo";
const MAX_PAGES: usize = 20;
const MAX_DESCRIPTION: usize = 8000;

pub fn oauth_config(client_id: &str, client_secret: Option<&str>) -> OAuthConfig {
    OAuthConfig {
        client_id: client_id.into(),
        client_secret: client_secret.map(String::from),
        auth_url: "https://accounts.google.com/o/oauth2/v2/auth".into(),
        token_url: "https://oauth2.googleapis.com/token".into(),
        scopes: [
            "openid",
            "email",
            "https://www.googleapis.com/auth/calendar.readonly",
        ]
        .map(String::from)
        .to_vec(),
    }
}

/// Extra authorisation parameters: ask for a refresh token and make sure one is issued.
pub const AUTH_EXTRA: [(&str, &str); 2] = [("access_type", "offline"), ("prompt", "consent")];

pub struct GoogleProvider {
    http: Arc<dyn HttpClient>,
    tokens: TokenManager,
    api_base: String,
}

impl GoogleProvider {
    pub fn new(http: Arc<dyn HttpClient>, secrets: Arc<dyn SecretStore>, cfg: OAuthConfig) -> Self {
        Self::with_base(http, secrets, cfg, API_BASE)
    }

    pub fn with_base(
        http: Arc<dyn HttpClient>,
        secrets: Arc<dyn SecretStore>,
        cfg: OAuthConfig,
        api_base: &str,
    ) -> Self {
        Self {
            tokens: TokenManager {
                http: http.clone(),
                secrets,
                cfg,
            },
            http,
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
            .map_err(|_| Error::Provider("Google sent a response we could not read".into()))
    }

    /// The signed-in address, to label the new account.
    pub fn fetch_email(
        http: &dyn HttpClient,
        userinfo_url: &str,
        access_token: &str,
    ) -> Result<String> {
        let resp = http.send(
            &HttpRequest::get(userinfo_url)
                .header("Authorization", format!("Bearer {access_token}")),
        )?;
        check(&resp)?;
        let v: Value = serde_json::from_slice(&resp.body)
            .map_err(|_| Error::Provider("could not read the account details".into()))?;
        v.get("email")
            .and_then(Value::as_str)
            .map(String::from)
            .ok_or_else(|| Error::Provider("Google did not share an email address".into()))
    }
}

fn check(resp: &HttpResponse) -> Result<()> {
    match resp.status {
        200..=299 => Ok(()),
        401 => Err(Error::AuthRevoked),
        403 | 429 => {
            // Quota and rate limits arrive as 403/429 with a reason; they are not revocation.
            let body = resp.text();
            if resp.status == 429
                || body.contains("rateLimitExceeded")
                || body.contains("userRateLimitExceeded")
                || body.contains("quotaExceeded")
            {
                Err(Error::Offline)
            } else {
                Err(Error::AuthRevoked) // permission withdrawn
            }
        }
        404 | 410 => Err(Error::Provider("calendar not found".into())),
        s => Err(Error::Provider(format!("Google returned {s}"))),
    }
}

fn str_of<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

impl Provider for GoogleProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Google
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            incremental_sync: false,
            email: true,
            needs_polling: true,
        }
    }

    fn list_calendars(&self, account: &Account) -> Result<Vec<Calendar>> {
        let mut out = Vec::new();
        let mut page: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut url = Url::parse(&format!("{}/users/me/calendarList", self.api_base))
                .map_err(|_| Error::Invalid("bad API address"))?;
            url.query_pairs_mut()
                .append_pair("minAccessRole", "reader")
                .append_pair("maxResults", "250");
            if let Some(p) = &page {
                url.query_pairs_mut().append_pair("pageToken", p);
            }
            let v = self.get(&account.id, url.as_str())?;
            for item in v
                .get("items")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(id) = str_of(item, "id") else {
                    continue;
                };
                if item.get("hidden").and_then(Value::as_bool) == Some(true)
                    || item.get("deleted").and_then(Value::as_bool) == Some(true)
                {
                    continue;
                }
                let name = str_of(item, "summaryOverride")
                    .or_else(|| str_of(item, "summary"))
                    .filter(|n| !n.trim().is_empty())
                    .unwrap_or("Calendar");
                let colour = str_of(item, "backgroundColor")
                    .and_then(nearest_palette)
                    .unwrap_or_else(|| default_colour_for(id));
                let role = str_of(item, "accessRole").unwrap_or("reader");
                out.push(Calendar {
                    id: format!("{}:{}", account.id, short_hash(id)),
                    account_id: account.id.clone(),
                    remote_id: id.to_string(),
                    name: name.to_string(),
                    colour: colour.to_string(),
                    visible: true,
                    read_only: !matches!(role, "owner" | "writer"),
                });
            }
            match str_of(&v, "nextPageToken") {
                Some(t) => page = Some(t.to_string()),
                None => break,
            }
        }
        Ok(out)
    }

    fn sync_events(
        &self,
        calendar: &Calendar,
        _state: Option<&str>,
        window: &Window,
    ) -> Result<SyncResult> {
        let rfc = |t: DateTime<Utc>| t.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let mut changed = Vec::new();
        let mut page: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut url = Url::parse(&format!("{}/calendars/x/events", self.api_base))
                .map_err(|_| Error::Invalid("bad API address"))?;
            // Set the calendar id as a path segment so it is percent-encoded correctly.
            url.path_segments_mut()
                .map_err(|_| Error::Invalid("bad API address"))?
                .pop()
                .pop()
                .push(&calendar.remote_id)
                .push("events");
            {
                let mut q = url.query_pairs_mut();
                q.append_pair("singleEvents", "true")
                    .append_pair("showDeleted", "false")
                    .append_pair("maxResults", "2500")
                    .append_pair("timeMin", &rfc(window.from))
                    .append_pair("timeMax", &rfc(window.to));
                if let Some(p) = &page {
                    q.append_pair("pageToken", p);
                }
            }
            let v = self.get(&calendar.account_id, url.as_str())?;
            for item in v
                .get("items")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                changed.extend(
                    map_event(item, &calendar.id).filter(|e| e.overlaps(window.from, window.to)),
                );
            }
            match str_of(&v, "nextPageToken") {
                Some(t) => page = Some(t.to_string()),
                None => break,
            }
        }
        changed.sort_by(|a, b| a.id.cmp(&b.id));
        changed.dedup_by(|a, b| a.id == b.id);
        Ok(SyncResult {
            changed,
            deleted: vec![],
            next_state: None,
            full: true,
        })
    }
}

fn parse_when(start: &Value, end: &Value) -> Option<When> {
    if let (Some(s), Some(e)) = (str_of(start, "date"), str_of(end, "date")) {
        return Some(When::AllDay {
            start: NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()?,
            end: NaiveDate::parse_from_str(e, "%Y-%m-%d").ok()?,
        });
    }
    let s = DateTime::parse_from_rfc3339(str_of(start, "dateTime")?)
        .ok()?
        .with_timezone(&Utc);
    let e = DateTime::parse_from_rfc3339(str_of(end, "dateTime")?)
        .ok()?
        .with_timezone(&Utc);
    Some(When::Timed {
        start: s,
        end: e.max(s),
        tzid: str_of(start, "timeZone").map(String::from),
    })
}

fn map_event(item: &Value, calendar_id: &str) -> Option<Event> {
    let id = str_of(item, "id")?;
    let when = parse_when(item.get("start")?, item.get("end")?)?;
    let status = match str_of(item, "status") {
        Some("cancelled") => EventStatus::Cancelled,
        Some("tentative") => EventStatus::Tentative,
        _ => EventStatus::Confirmed,
    };
    let my_response = item
        .get("attendees")
        .and_then(Value::as_array)
        .and_then(|a| {
            a.iter()
                .find(|x| x.get("self").and_then(Value::as_bool) == Some(true))
        })
        .map(|me| match str_of(me, "responseStatus") {
            Some("accepted") => Response::Accepted,
            Some("declined") => Response::Declined,
            Some("tentative") => Response::Tentative,
            Some("needsAction") => Response::NeedsAction,
            _ => Response::None,
        })
        .unwrap_or(Response::None);

    let video = item
        .pointer("/conferenceData/entryPoints")
        .and_then(Value::as_array)
        .and_then(|eps| {
            eps.iter()
                .find(|e| str_of(e, "entryPointType") == Some("video"))
        })
        .and_then(|e| str_of(e, "uri"));
    let location = str_of(item, "location").map(String::from);
    let description =
        str_of(item, "description").map(|d| d.chars().take(MAX_DESCRIPTION).collect::<String>());
    let join_url = join::extract(
        video.or_else(|| str_of(item, "hangoutLink")),
        location.as_deref(),
        description.as_deref(),
    );
    let organizer = item
        .get("organizer")
        .and_then(|o| str_of(o, "displayName").or_else(|| str_of(o, "email")))
        .map(String::from);

    Some(Event {
        id: format!("{calendar_id}:{id}"),
        calendar_id: calendar_id.into(),
        remote_id: id.into(),
        ical_uid: str_of(item, "iCalUID").unwrap_or(id).into(),
        title: str_of(item, "summary")
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("(No title)")
            .into(),
        when,
        location,
        description,
        join_url,
        status,
        my_response,
        organizer,
        recurrence_master_id: str_of(item, "recurringEventId").map(String::from),
        updated_at: str_of(item, "updated")
            .and_then(|u| DateTime::parse_from_rfc3339(u).ok())
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_default(),
        also_in: Vec::new(),
    })
}

#[cfg(test)]
mod tests;
