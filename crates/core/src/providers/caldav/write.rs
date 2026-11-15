//! Adding, changing and removing the events Firstlight itself creates in a CalDAV calendar.
//!
//! Nothing here reads or touches any other event: a new event is a `PUT` of a file under a name we make up
//! (and `If-None-Match: *`, so an existing file is never overwritten), a change is a `PUT` to the address we
//! stored for that event, and a removal is a `DELETE` of the same address. Every address is checked to be on
//! the server the user signed in to, and to sit inside the calendar being written, before credentials are sent.

use super::{CaldavProvider, CaldavSecret, load_secret, origin};
use crate::http::{HttpClient, HttpRequest};
use crate::invite::{escape_text, fold};
use crate::providers::system::{CalendarWriter, SysFreq, SysNewEvent, SysRepeat, SysWhen};
use crate::secrets::SecretStore;
use crate::{Error, Result};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use reqwest::Url;
use std::sync::Arc;

/// The UID of an event we made, taken from the last part of its address (`.../firstlight-<id>.ics`). The
/// server keeps the UID we wrote, so a synced copy can be matched back to the event it came from.
pub fn uid_for(event_url: &str) -> String {
    let last = event_url
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .rsplit('/')
        .next()
        .unwrap_or("");
    last.strip_suffix(".ics").unwrap_or(last).to_string()
}

/// Writes to one calendar of one CalDAV account.
pub struct CaldavWriter {
    http: Arc<dyn HttpClient>,
    secrets: Arc<dyn SecretStore>,
    account_id: String,
    /// The address of the calendar (as the server listed it).
    calendar_url: String,
    now: fn() -> DateTime<Utc>,
}

impl CaldavWriter {
    pub fn new(
        http: Arc<dyn HttpClient>,
        secrets: Arc<dyn SecretStore>,
        account_id: &str,
        calendar_url: &str,
    ) -> Self {
        Self {
            http,
            secrets,
            account_id: account_id.to_string(),
            calendar_url: calendar_url.to_string(),
            now: Utc::now,
        }
    }

    fn send(
        &self,
        s: &CaldavSecret,
        method: &str,
        url: &Url,
        body: Option<String>,
        new: bool,
    ) -> Result<u16> {
        let auth = format!(
            "Basic {}",
            STANDARD.encode(format!("{}:{}", s.username, s.password))
        );
        let mut req = HttpRequest::new(method, url.as_str()).header("Authorization", auth);
        if let Some(b) = body {
            req = req
                .header("Content-Type", "text/calendar; charset=utf-8")
                .body(b);
            if new {
                req = req.header("If-None-Match", "*");
            }
        }
        let status = self.http.send(&req)?.status;
        match status {
            401 => Err(Error::AuthRevoked),
            429 => Err(Error::Offline),
            s if s >= 500 => Err(Error::Provider(format!("server error {s}"))),
            s => Ok(s),
        }
    }

    /// The calendar's address and the secret, with the address confined to the user's server.
    fn target(&self) -> Result<(CaldavSecret, Url)> {
        let s = load_secret(self.secrets.as_ref(), &self.account_id)?;
        let base = Url::parse(&s.server).map_err(|_| Error::Invalid("bad server address"))?;
        let mut cal = CaldavProvider::resolve(&base, &self.calendar_url)?;
        if !cal.path().ends_with('/') {
            cal.set_path(&format!("{}/", cal.path()));
        }
        Ok((s, cal))
    }

    /// An event address inside this calendar, on the same server.
    fn inside(&self, cal: &Url, address: &str) -> Result<Url> {
        let url = CaldavProvider::resolve(cal, address)?;
        let name = url.path().strip_prefix(cal.path()).unwrap_or("");
        let plain = !name.is_empty()
            && !name.contains('/')
            && name.ends_with(".ics")
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
        if origin(&url) != origin(cal) || !plain {
            return Err(Error::Invalid("That event can't be changed here."));
        }
        Ok(url)
    }
}

impl CalendarWriter for CaldavWriter {
    fn save_event(
        &self,
        _calendar_id: &str,
        existing: Option<&str>,
        event: &SysNewEvent,
        tz: Tz,
    ) -> Result<String> {
        let (s, cal) = self.target()?;
        let (url, new) = match existing {
            Some(e) => (self.inside(&cal, e)?, false),
            None => {
                let name = format!("firstlight-{}.ics", crate::model::new_id());
                (self.inside(&cal, &name)?, true)
            }
        };
        let uid = uid_for(url.as_str());
        let body = event_file(&uid, event, tz, (self.now)());
        match self.send(&s, "PUT", &url, Some(body), new)? {
            200 | 201 | 204 => Ok(url.to_string()),
            403 => Err(Error::Provider(
                "the server doesn't allow changes to that calendar".into(),
            )),
            404 | 410 if !new => Err(Error::Provider(
                "that event is no longer on the server".into(),
            )),
            412 => Err(Error::Provider(
                "the server refused to overwrite a file".into(),
            )),
            c => Err(Error::Provider(format!("server returned {c}"))),
        }
    }

    fn delete_event(&self, event_id: &str) -> Result<()> {
        let (s, cal) = self.target()?;
        let url = self.inside(&cal, event_id)?;
        match self.send(&s, "DELETE", &url, None, false)? {
            200 | 202 | 204 | 404 | 410 => Ok(()),
            403 => Err(Error::Provider(
                "the server doesn't allow changes to that calendar".into(),
            )),
            c => Err(Error::Provider(format!("server returned {c}"))),
        }
    }
}

fn stamp(d: DateTime<Utc>) -> String {
    d.format("%Y%m%dT%H%M%SZ").to_string()
}

/// The end of `day` in `tz`, in UTC: `UNTIL` for a timed series must be an instant, and the last day counts.
fn end_of_day(day: NaiveDate, tz: Tz) -> DateTime<Utc> {
    let t = NaiveTime::from_hms_opt(23, 59, 59).unwrap_or_default();
    tz.from_local_datetime(&day.and_time(t))
        .latest()
        .map_or_else(|| day.and_time(t).and_utc(), |d| d.with_timezone(&Utc))
}

fn rrule(r: &SysRepeat, timed: bool, tz: Tz) -> String {
    let freq = match r.freq {
        SysFreq::Daily => "DAILY",
        SysFreq::Weekly => "WEEKLY",
        SysFreq::Monthly => "MONTHLY",
        SysFreq::Yearly => "YEARLY",
    };
    let mut out = format!("FREQ={freq};INTERVAL={}", r.interval.max(1));
    if let Some(u) = r.until {
        if timed {
            out.push_str(&format!(";UNTIL={}", stamp(end_of_day(u, tz))));
        } else {
            out.push_str(&format!(";UNTIL={}", u.format("%Y%m%d")));
        }
    }
    out
}

/// The calendar file for one event. A timed event that repeats is written in the zone it was entered in, so a
/// 9:00 meeting stays at 9:00 across daylight saving; every other timed event is in UTC. No `METHOD`, no
/// organiser and no attendees: this is a calendar entry, not an invitation, and the server sends no mail for it.
pub(crate) fn event_file(uid: &str, e: &SysNewEvent, tz: Tz, now: DateTime<Utc>) -> String {
    let local = |d: DateTime<Utc>| d.with_timezone(&tz).format("%Y%m%dT%H%M%S").to_string();
    let mut l: Vec<String> = vec![
        "BEGIN:VCALENDAR".into(),
        "VERSION:2.0".into(),
        "PRODID:-//Firstlight//Calendar//EN".into(),
        "CALSCALE:GREGORIAN".into(),
        "BEGIN:VEVENT".into(),
        format!("UID:{}", escape_text(uid)),
        format!("DTSTAMP:{}", stamp(now)),
    ];
    let timed = matches!(e.when, SysWhen::Timed { .. });
    match &e.when {
        SysWhen::Timed { start, end } if e.repeat.is_some() => {
            l.push(format!("DTSTART;TZID={}:{}", tz.name(), local(*start)));
            l.push(format!("DTEND;TZID={}:{}", tz.name(), local(*end)));
        }
        SysWhen::Timed { start, end } => {
            l.push(format!("DTSTART:{}", stamp(*start)));
            l.push(format!("DTEND:{}", stamp(*end)));
        }
        SysWhen::AllDay { first, last } => {
            l.push(format!("DTSTART;VALUE=DATE:{}", first.format("%Y%m%d")));
            l.push(format!(
                "DTEND;VALUE=DATE:{}",
                (*last + Duration::days(1)).format("%Y%m%d")
            ));
        }
    }
    if let Some(r) = &e.repeat {
        l.push(format!("RRULE:{}", rrule(r, timed, tz)));
    }
    l.push(format!("SUMMARY:{}", escape_text(&e.title)));
    if let Some(v) = &e.location {
        l.push(format!("LOCATION:{}", escape_text(v)));
    }
    if let Some(u) = &e.url {
        l.push(format!("URL:{}", escape_text(u)));
    }
    if let Some(n) = &e.notes {
        l.push(format!("DESCRIPTION:{}", escape_text(n)));
    }
    if let (Some(m), true) = (e.alarm_minutes, timed) {
        l.push("BEGIN:VALARM".into());
        l.push("ACTION:DISPLAY".into());
        l.push(format!("DESCRIPTION:{}", escape_text(&e.title)));
        l.push(format!("TRIGGER:-PT{m}M"));
        l.push("END:VALARM".into());
    }
    l.push("END:VEVENT".into());
    l.push("END:VCALENDAR".into());
    l.iter().map(|x| fold(x)).collect::<Vec<_>>().join("\r\n") + "\r\n"
}

#[cfg(test)]
mod tests;
