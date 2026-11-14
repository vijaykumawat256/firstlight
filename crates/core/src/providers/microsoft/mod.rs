//! Outlook.com and Microsoft 365 via Microsoft Graph (OAuth, read-only scope).
//!
//! Uses `calendarView`, which returns recurring events already expanded. Times are read in each
//! event's own timezone (Windows or IANA names) rather than forcing UTC: Graph shifts all-day events
//! by a day when asked for UTC, which would show them on the wrong date for most of the world.
//! Delta queries are not used yet; each sync fetches the whole window.

use super::{Capabilities, Provider, SyncResult, Window, short_hash};
use crate::http::{HttpClient, HttpRequest, HttpResponse};
use crate::join;
use crate::model::*;
use crate::oauth::{OAuthConfig, TokenManager};
use crate::providers::caldav::nearest_palette;
use crate::secrets::SecretStore;
use crate::{Error, Result, tz};
use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use reqwest::Url;
use serde_json::Value;
use std::sync::Arc;

pub const API_BASE: &str = "https://graph.microsoft.com/v1.0";
const MAX_PAGES: usize = 40;
const MAX_DESCRIPTION: usize = 8000;

pub fn oauth_config(client_id: &str) -> OAuthConfig {
    OAuthConfig {
        client_id: client_id.into(),
        client_secret: None, // public client: PKCE only
        auth_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize".into(),
        token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token".into(),
        scopes: ["offline_access", "Calendars.Read", "User.Read"]
            .map(String::from)
            .to_vec(),
    }
}

pub struct MicrosoftProvider {
    http: Arc<dyn HttpClient>,
    tokens: TokenManager,
    api_base: String,
}

impl MicrosoftProvider {
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
            .map_err(|_| Error::Provider("Microsoft sent a response we could not read".into()))
    }

    /// Follows `@odata.nextLink`, but only within the API's own origin: the bearer token goes with every request.
    fn next_link(&self, v: &Value) -> Result<Option<String>> {
        let Some(link) = v.get("@odata.nextLink").and_then(Value::as_str) else {
            return Ok(None);
        };
        let base = Url::parse(&self.api_base).map_err(|_| Error::Invalid("bad API address"))?;
        let next = Url::parse(link)
            .map_err(|_| Error::Provider("Microsoft sent an invalid link".into()))?;
        if (next.scheme(), next.host_str(), next.port_or_known_default())
            != (base.scheme(), base.host_str(), base.port_or_known_default())
        {
            return Err(Error::Provider(
                "the server pointed to a different host; refusing to send credentials there".into(),
            ));
        }
        Ok(Some(next.to_string()))
    }

    /// The signed-in address, to label the new account.
    pub fn fetch_email(
        http: &dyn HttpClient,
        api_base: &str,
        access_token: &str,
    ) -> Result<String> {
        let url = format!(
            "{}/me?$select=mail,userPrincipalName",
            api_base.trim_end_matches('/')
        );
        let resp = http.send(
            &HttpRequest::get(url).header("Authorization", format!("Bearer {access_token}")),
        )?;
        check(&resp)?;
        let v: Value = serde_json::from_slice(&resp.body)
            .map_err(|_| Error::Provider("could not read the account details".into()))?;
        ["mail", "userPrincipalName"]
            .iter()
            .find_map(|k| v.get(*k).and_then(Value::as_str).filter(|s| !s.is_empty()))
            .map(String::from)
            .ok_or_else(|| Error::Provider("Microsoft did not share an email address".into()))
    }
}

fn check(resp: &HttpResponse) -> Result<()> {
    match resp.status {
        200..=299 => Ok(()),
        401 => Err(Error::AuthRevoked),
        403 => Err(Error::AuthRevoked), // consent withdrawn or admin policy changed
        429 | 503 | 504 => Err(Error::Offline),
        404 | 410 => Err(Error::Provider("calendar not found".into())),
        s => Err(Error::Provider(format!("Microsoft returned {s}"))),
    }
}

fn str_of<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

/// Graph's named colours, for calendars that have no `hexColor`.
fn named_colour(name: &str) -> Option<&'static str> {
    Some(match name {
        "lightBlue" | "maxColor" => "blue",
        "lightGreen" => "green",
        "lightOrange" => "orange",
        "lightGray" | "gray" => "indigo",
        "lightYellow" => "yellow",
        "lightTeal" => "teal",
        "lightPink" => "pink",
        "lightBrown" => "brown",
        "lightRed" => "red",
        _ => return None,
    })
}

impl Provider for MicrosoftProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Microsoft
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
        let mut url = Some(format!("{}/me/calendars?$top=100", self.api_base));
        for _ in 0..MAX_PAGES {
            let Some(u) = url.take() else { break };
            let v = self.get(&account.id, &u)?;
            for item in v
                .get("value")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(id) = str_of(item, "id") else {
                    continue;
                };
                let colour = str_of(item, "hexColor")
                    .and_then(nearest_palette)
                    .or_else(|| str_of(item, "color").and_then(named_colour))
                    .unwrap_or_else(|| default_colour_for(id));
                out.push(Calendar {
                    id: format!("{}:{}", account.id, short_hash(id)),
                    account_id: account.id.clone(),
                    remote_id: id.to_string(),
                    name: str_of(item, "name")
                        .filter(|n| !n.trim().is_empty())
                        .unwrap_or("Calendar")
                        .to_string(),
                    colour: colour.to_string(),
                    visible: true,
                    read_only: item.get("canEdit").and_then(Value::as_bool) != Some(true),
                });
            }
            url = self.next_link(&v)?;
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
        let mut first = Url::parse(&format!("{}/me/calendars/x/calendarView", self.api_base))
            .map_err(|_| Error::Invalid("bad API address"))?;
        first
            .path_segments_mut()
            .map_err(|_| Error::Invalid("bad API address"))?
            .pop()
            .pop()
            .push(&calendar.remote_id)
            .push("calendarView");
        first
            .query_pairs_mut()
            .append_pair("startDateTime", &rfc(window.from))
            .append_pair("endDateTime", &rfc(window.to))
            .append_pair("$top", "200");

        let mut changed = Vec::new();
        let mut url = Some(first.to_string());
        for _ in 0..MAX_PAGES {
            let Some(u) = url.take() else { break };
            let v = self.get(&calendar.account_id, &u)?;
            for item in v
                .get("value")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                changed.extend(
                    map_event(item, &calendar.id).filter(|e| e.overlaps(window.from, window.to)),
                );
            }
            url = self.next_link(&v)?;
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

/// Graph writes local wall-clock text plus a zone name: "2026-03-10T09:00:00.0000000" / "Pacific Standard Time".
fn moment(v: &Value) -> Option<(NaiveDateTime, Option<chrono_tz::Tz>, DateTime<Utc>)> {
    let text = str_of(v, "dateTime")?;
    let naive = NaiveDateTime::parse_from_str(text.split('.').next()?, "%Y-%m-%dT%H:%M:%S").ok()?;
    let zone = str_of(v, "timeZone").and_then(tz::resolve);
    let utc = tz::local_to_utc(zone.unwrap_or(chrono_tz::UTC), naive);
    Some((naive, zone, utc))
}

fn map_event(item: &Value, calendar_id: &str) -> Option<Event> {
    let id = str_of(item, "id")?;
    let (s_naive, s_zone, s_utc) = moment(item.get("start")?)?;
    let (e_naive, _, e_utc) = moment(item.get("end")?)?;

    let when = if item.get("isAllDay").and_then(Value::as_bool) == Some(true) {
        // All-day events are whole dates; use the wall-clock date as written.
        let start: NaiveDate = s_naive.date();
        let end = e_naive.date().max(start.succ_opt()?);
        When::AllDay { start, end }
    } else {
        When::Timed {
            start: s_utc,
            end: e_utc.max(s_utc),
            tzid: s_zone.map(|z| z.name().to_string()),
        }
    };

    let response = item
        .pointer("/responseStatus/response")
        .and_then(Value::as_str);
    let my_response = match response {
        Some("accepted") => Response::Accepted,
        Some("tentativelyAccepted") => Response::Tentative,
        Some("declined") => Response::Declined,
        Some("notResponded") => Response::NeedsAction,
        _ => Response::None, // organizer / none
    };
    let status = if item.get("isCancelled").and_then(Value::as_bool) == Some(true) {
        EventStatus::Cancelled
    } else if my_response == Response::Tentative {
        EventStatus::Tentative
    } else {
        EventStatus::Confirmed
    };

    let location = item
        .pointer("/location/displayName")
        .and_then(Value::as_str)
        .filter(|l| !l.is_empty())
        .map(String::from);
    let description = str_of(item, "bodyPreview")
        .filter(|d| !d.is_empty())
        .map(|d| d.chars().take(MAX_DESCRIPTION).collect::<String>());
    let online = item
        .pointer("/onlineMeeting/joinUrl")
        .and_then(Value::as_str)
        .or_else(|| str_of(item, "onlineMeetingUrl"));
    let join_url = join::extract(online, location.as_deref(), description.as_deref());

    Some(Event {
        id: format!("{calendar_id}:{id}"),
        calendar_id: calendar_id.into(),
        remote_id: id.into(),
        ical_uid: str_of(item, "iCalUId").unwrap_or(id).into(),
        title: str_of(item, "subject")
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("(No title)")
            .into(),
        when,
        location,
        description,
        join_url,
        status,
        my_response,
        organizer: item
            .pointer("/organizer/emailAddress/name")
            .and_then(Value::as_str)
            .filter(|n| !n.is_empty())
            .map(String::from),
        recurrence_master_id: str_of(item, "seriesMasterId").map(String::from),
        updated_at: str_of(item, "lastModifiedDateTime")
            .and_then(|u| DateTime::parse_from_rfc3339(u).ok())
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_default(),
        also_in: Vec::new(),
    })
}

#[cfg(test)]
mod tests;
