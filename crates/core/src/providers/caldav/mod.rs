//! CalDAV (RFC 4791): iCloud, Yahoo, Fastmail, Nextcloud and most other calendar servers.
//!
//! Sign-in is a username and an *app password*; both live in the secret store, never the database.
//! Every request carries the password, so every URL the server hands back is confined to the
//! origin the user typed in: a malicious server cannot redirect our credentials elsewhere.

use super::{Capabilities, Provider, SyncResult, Window};
use crate::http::{HttpClient, HttpRequest, HttpResponse};
use crate::ics::{ParseOptions, events_from_ics};
use crate::model::*;
use crate::secrets::SecretStore;
use crate::xml::{self, Node};
use crate::{Error, Result};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono_tz::Tz;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Well-known servers, offered as one-tap presets in the UI.
pub const PRESETS: [(&str, &str, &str); 3] = [
    ("icloud", "iCloud", "https://caldav.icloud.com"),
    ("yahoo", "Yahoo", "https://caldav.calendar.yahoo.com"),
    ("fastmail", "Fastmail", "https://caldav.fastmail.com"),
];

/// What is stored (as JSON) in the secret store for one account.
#[derive(Serialize, Deserialize, Clone)]
pub struct CaldavSecret {
    pub server: String,
    pub username: String,
    pub password: String,
}

impl std::fmt::Debug for CaldavSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CaldavSecret([redacted])")
    }
}

pub struct CaldavProvider {
    http: Arc<dyn HttpClient>,
    secrets: Arc<dyn SecretStore>,
    default_tz: Tz,
}

#[derive(Serialize, Deserialize, Default)]
struct State {
    ctag: Option<String>,
    window_from: String,
}

const NS: &str = r#"xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav" xmlns:cs="http://calendarserver.org/ns/" xmlns:a="http://apple.com/ns/ical/""#;

fn propfind_body(props: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?><d:propfind {NS}><d:prop>{props}</d:prop></d:propfind>"#
    )
}

/// Validates a user-entered server address: https only, no credentials in the URL.
pub fn validate_server(input: &str) -> Result<String> {
    let url = Url::parse(input.trim())
        .map_err(|_| Error::Invalid("server address is not a valid link"))?;
    if url.scheme() != "https" || url.host_str().is_none_or(str::is_empty) {
        return Err(Error::Invalid("server address must start with https://"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(Error::Invalid(
            "do not put a username or password in the address",
        ));
    }
    Ok(url.to_string())
}

fn origin(u: &Url) -> (String, Option<String>, Option<u16>) {
    (
        u.scheme().to_string(),
        u.host_str().map(str::to_ascii_lowercase),
        u.port_or_known_default(),
    )
}

/// Maps a CalDAV colour ("#RRGGBB" or "#RRGGBBAA") to the nearest palette name.
pub fn nearest_palette(hex: &str) -> Option<&'static str> {
    const PALETTE: [(&str, [i32; 3]); 10] = [
        ("red", [0xff, 0x3b, 0x30]),
        ("orange", [0xff, 0x95, 0x00]),
        ("yellow", [0xff, 0xcc, 0x00]),
        ("green", [0x34, 0xc7, 0x59]),
        ("teal", [0x30, 0xb0, 0xc7]),
        ("blue", [0x00, 0x7a, 0xff]),
        ("indigo", [0x58, 0x56, 0xd6]),
        ("purple", [0xaf, 0x52, 0xde]),
        ("pink", [0xff, 0x2d, 0x55]),
        ("brown", [0xa2, 0x84, 0x5e]),
    ];
    let h = hex.trim().trim_start_matches('#');
    if !(h.len() == 6 || h.len() == 8) || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let ch = |i: usize| i32::from_str_radix(&h[i..i + 2], 16).ok();
    let rgb = [ch(0)?, ch(2)?, ch(4)?];
    PALETTE
        .iter()
        .min_by_key(|(_, p)| (0..3).map(|i| (p[i] - rgb[i]).pow(2)).sum::<i32>())
        .map(|(n, _)| *n)
}

use super::short_hash;

impl CaldavProvider {
    pub fn new(http: Arc<dyn HttpClient>, secrets: Arc<dyn SecretStore>, default_tz: Tz) -> Self {
        Self {
            http,
            secrets,
            default_tz,
        }
    }

    pub fn secret_key(account_id: &str) -> String {
        format!("caldav:{account_id}")
    }

    fn load(&self, account_id: &str) -> Result<CaldavSecret> {
        let raw = self
            .secrets
            .get(&Self::secret_key(account_id))?
            .ok_or(Error::AuthRevoked)?;
        serde_json::from_str(&raw)
            .map_err(|_| Error::Secret("stored credentials are unreadable".into()))
    }

    fn request(
        &self,
        s: &CaldavSecret,
        method: &str,
        url: &str,
        depth: Option<&str>,
        body: String,
    ) -> Result<HttpResponse> {
        let auth = format!(
            "Basic {}",
            STANDARD.encode(format!("{}:{}", s.username, s.password))
        );
        let mut req = HttpRequest::new(method, url)
            .header("Authorization", auth)
            .header("Content-Type", "application/xml; charset=utf-8")
            .body(body);
        if let Some(d) = depth {
            req = req.header("Depth", d);
        }
        let resp = self.http.send(&req)?;
        match resp.status {
            401 | 403 => Err(Error::AuthRevoked),
            429 => Err(Error::Offline), // rate limited: back off like an outage
            s if s >= 500 => Err(Error::Provider(format!("server error {s}"))),
            _ => Ok(resp),
        }
    }

    /// Resolves a server-supplied link against `base`, refusing anything off the server's origin.
    fn resolve(base: &Url, href: &str) -> Result<Url> {
        let url = base
            .join(href.trim())
            .map_err(|_| Error::Provider("the server sent an invalid link".into()))?;
        if origin(&url) != origin(base) {
            return Err(Error::Provider(
                "the server pointed to a different host; refusing to send credentials there".into(),
            ));
        }
        Ok(url)
    }

    fn propfind(&self, s: &CaldavSecret, url: &Url, depth: &str, props: &str) -> Result<Node> {
        let resp = self.request(
            s,
            "PROPFIND",
            url.as_str(),
            Some(depth),
            propfind_body(props),
        )?;
        if resp.status == 404 {
            return Err(Error::Provider("calendar server path not found".into()));
        }
        if resp.status != 207 && !resp.is_success() {
            return Err(Error::Provider(format!("server returned {}", resp.status)));
        }
        xml::parse(&resp.text())
    }

    /// Property nodes of a response whose status is 200.
    fn ok_props(resp: &Node) -> Vec<&Node> {
        resp.children_named("propstat")
            .filter(|ps| ps.child("status").is_none_or(|s| s.text.contains(" 200")))
            .filter_map(|ps| ps.child("prop"))
            .collect()
    }

    fn prop<'a>(resp: &'a Node, name: &str) -> Option<&'a Node> {
        Self::ok_props(resp).into_iter().find_map(|p| p.child(name))
    }

    fn discover_home(&self, s: &CaldavSecret) -> Result<Url> {
        let base = Url::parse(&s.server).map_err(|_| Error::Invalid("bad server address"))?;
        let principal = match self.propfind(s, &base, "0", "<d:current-user-principal/>") {
            Ok(doc) => doc
                .children_named("response")
                .find_map(|r| Self::prop(r, "current-user-principal").and_then(|p| p.child("href")))
                .map(|h| Self::resolve(&base, h.trimmed()))
                .transpose()?
                .unwrap_or_else(|| base.clone()),
            Err(Error::Provider(_)) => base.clone(), // some servers answer only at deeper paths
            Err(e) => return Err(e),
        };
        let doc = self.propfind(s, &principal, "0", "<c:calendar-home-set/>")?;
        let home = doc
            .children_named("response")
            .find_map(|r| Self::prop(r, "calendar-home-set").and_then(|p| p.child("href")))
            .map(|h| Self::resolve(&principal, h.trimmed()))
            .transpose()?;
        Ok(home.unwrap_or(principal))
    }

    fn fetch_ctag(&self, s: &CaldavSecret, cal: &Url) -> Option<String> {
        let doc = self.propfind(s, cal, "0", "<cs:getctag/>").ok()?;
        doc.children_named("response")
            .find_map(|r| Self::prop(r, "getctag").map(|n| n.trimmed().to_string()))
            .filter(|c| !c.is_empty())
    }
}

impl Provider for CaldavProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Caldav
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            incremental_sync: true,
            email: false,
            needs_polling: true,
        }
    }

    fn list_calendars(&self, account: &Account) -> Result<Vec<Calendar>> {
        let s = self.load(&account.id)?;
        let home = self.discover_home(&s)?;
        let doc = self.propfind(
            &s,
            &home,
            "1",
            "<d:resourcetype/><d:displayname/><a:calendar-color/><c:supported-calendar-component-set/>",
        )?;
        let mut out = Vec::new();
        for r in doc.children_named("response") {
            let Some(href) = r.child("href") else {
                continue;
            };
            let is_calendar =
                Self::prop(r, "resourcetype").is_some_and(|t| t.child("calendar").is_some());
            let supports_events =
                Self::prop(r, "supported-calendar-component-set").is_none_or(|set| {
                    set.children_named("comp").any(|c| {
                        c.attr("name")
                            .is_some_and(|n| n.eq_ignore_ascii_case("VEVENT"))
                    })
                });
            if !is_calendar || !supports_events {
                continue;
            }
            let url = Self::resolve(&home, href.trimmed())?;
            let name = Self::prop(r, "displayname")
                .map(|n| n.trimmed().to_string())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| "Calendar".into());
            let colour = Self::prop(r, "calendar-color")
                .and_then(|c| nearest_palette(c.trimmed()))
                .map_or_else(|| default_colour_for(url.as_str()), |c| c);
            out.push(Calendar {
                id: format!("{}:{}", account.id, short_hash(url.as_str())),
                account_id: account.id.clone(),
                remote_id: url.to_string(),
                name,
                colour: colour.to_string(),
                visible: true,
                read_only: true,
            });
        }
        if out.is_empty() {
            return Err(Error::Provider(
                "no calendars were found on that server".into(),
            ));
        }
        Ok(out)
    }

    fn sync_events(
        &self,
        calendar: &Calendar,
        state: Option<&str>,
        window: &Window,
    ) -> Result<SyncResult> {
        let s = self.load(&calendar.account_id)?;
        let base = Url::parse(&s.server).map_err(|_| Error::Invalid("bad server address"))?;
        // The stored remote id is a URL we produced from a confined href; re-check anyway.
        let cal_url = Self::resolve(&base, &calendar.remote_id)?;

        let window_from = window.from.date_naive().to_string();
        let prev: State = state
            .and_then(|t| serde_json::from_str(t).ok())
            .unwrap_or_default();
        let ctag = self.fetch_ctag(&s, &cal_url);
        let unchanged = matches!((&prev.ctag, &ctag), (Some(old), Some(new)) if old == new)
            && prev.window_from == window_from;
        if unchanged {
            return Ok(SyncResult {
                changed: vec![],
                deleted: vec![],
                next_state: state.map(String::from),
                full: false,
            });
        }

        let fmt = |t: chrono::DateTime<chrono::Utc>| t.format("%Y%m%dT%H%M%SZ").to_string();
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?><c:calendar-query {NS}><d:prop><d:getetag/><c:calendar-data/></d:prop>
               <c:filter><c:comp-filter name="VCALENDAR"><c:comp-filter name="VEVENT"><c:time-range start="{}" end="{}"/></c:comp-filter></c:comp-filter></c:filter></c:calendar-query>"#,
            fmt(window.from),
            fmt(window.to)
        );
        let resp = self.request(&s, "REPORT", cal_url.as_str(), Some("1"), body)?;
        if resp.status == 404 || resp.status == 410 {
            return Err(Error::Provider("calendar not found".into()));
        }
        if resp.status != 207 {
            return Err(Error::Provider(format!("server returned {}", resp.status)));
        }
        let doc = xml::parse(&resp.text())?;

        // iCloud/Yahoo identify the user by their sign-in; use it to work out RSVP status.
        let me = s.username.contains('@').then_some(s.username.as_str());
        let mut events = Vec::new();
        for r in doc.children_named("response") {
            let Some(data) = Self::prop(r, "calendar-data")
                .map(|n| n.text.as_str())
                .filter(|t| !t.trim().is_empty())
            else {
                continue;
            };
            // One broken resource must not hide the rest of the calendar.
            if let Ok(mut evs) = events_from_ics(
                data,
                &ParseOptions {
                    calendar_id: &calendar.id,
                    default_tz: self.default_tz,
                    window: *window,
                    my_email: me,
                },
            ) {
                events.append(&mut evs);
            }
        }
        events.sort_by(|a, b| a.id.cmp(&b.id));
        events.dedup_by(|a, b| a.id == b.id);

        let next = State { ctag, window_from };
        Ok(SyncResult {
            changed: events,
            deleted: vec![],
            next_state: serde_json::to_string(&next).ok(),
            full: true,
        })
    }
}

#[cfg(test)]
mod tests;
