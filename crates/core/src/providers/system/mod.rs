//! The calendars already set up in the operating system (macOS Calendar app via EventKit).
//!
//! Any account added to the Mac (iCloud, Gmail, Outlook/Exchange, Yahoo, ...) appears here with no
//! extra sign-in. This module holds the platform-independent half: permission handling, the mapping
//! to Firstlight events, and the rules around all-day dates. The operating-system half implements
//! [`SystemSource`] in the desktop shell.

use super::{Capabilities, Provider, SyncResult, Window, short_hash};
use crate::model::*;
use crate::{Error, Result, join};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use chrono_tz::Tz;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Granted,
    /// The user has not been asked yet.
    NotDetermined,
    Denied,
    /// Blocked by a device policy (for example a managed Mac).
    Restricted,
    /// This platform or OS version has no system calendar support.
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysCalendarKind {
    Local,
    CalDav,
    Exchange,
    Subscription,
    Birthday,
}

#[derive(Debug, Clone)]
pub struct SysCalendar {
    pub id: String,
    pub title: String,
    /// The account the calendar belongs to ("iCloud", "Gmail", ...).
    pub source: String,
    pub kind: SysCalendarKind,
    pub read_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysStatus {
    None,
    Confirmed,
    Tentative,
    Canceled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysResponse {
    /// No attendee entry for the user (their own event, or no invitation).
    NotInvited,
    Pending,
    Accepted,
    Declined,
    Tentative,
    Other,
}

#[derive(Debug, Clone)]
pub struct SysEvent {
    /// The store's identifier. Occurrences of a recurring event share it.
    pub id: String,
    /// The iCalendar UID when the store exposes one; used to merge duplicates across accounts.
    pub uid: Option<String>,
    pub title: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub all_day: bool,
    pub location: Option<String>,
    pub notes: Option<String>,
    pub url: Option<String>,
    pub status: SysStatus,
    pub response: SysResponse,
    pub organizer: Option<String>,
    pub is_recurring: bool,
    pub tzid: Option<String>,
    /// When the store last changed it, if known (Unix seconds).
    pub modified: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysFreq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

/// How an event Firstlight adds repeats: every `interval` days, weeks, months or years, up to an optional last day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SysRepeat {
    pub freq: SysFreq,
    pub interval: u32,
    /// The last day it can happen (local date, inclusive).
    pub until: Option<NaiveDate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SysWhen {
    Timed {
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    },
    /// Local dates; `last` is the final day (inclusive).
    AllDay { first: NaiveDate, last: NaiveDate },
}

/// An event Firstlight asks the Calendar app to keep. Only what the form collects: no attendees (the
/// Calendar app cannot add people to an event from outside; "Send invitation" is how they are told).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SysNewEvent {
    pub title: String,
    pub when: SysWhen,
    pub location: Option<String>,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub repeat: Option<SysRepeat>,
    /// An alert this many minutes before the start (timed events only).
    pub alarm_minutes: Option<u32>,
}

fn cannot_write() -> Error {
    Error::Invalid("Adding events to the Calendar app isn't available on this computer.")
}

/// What the operating-system layer must provide. Implemented per platform in the shell.
pub trait SystemSource: Send + Sync {
    fn access(&self) -> Access;
    /// Shows the permission prompt if the user has not been asked; blocks until they answer.
    fn request_access(&self) -> Result<Access>;
    fn calendars(&self) -> Result<Vec<SysCalendar>>;
    /// Events of one calendar touching the window, recurrences already expanded.
    fn events(
        &self,
        calendar_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<SysEvent>>;

    /// Creates an event in `calendar_id`, or (with `existing`) replaces what an event Firstlight created before
    /// holds, and returns the store's identifier for it. A repeating event is changed as a whole.
    fn save_event(
        &self,
        _calendar_id: &str,
        _existing: Option<&str>,
        _event: &SysNewEvent,
        _tz: Tz,
    ) -> Result<String> {
        Err(cannot_write())
    }

    /// Removes an event Firstlight created (all of it, if it repeats).
    fn delete_event(&self, _event_id: &str) -> Result<()> {
        Err(cannot_write())
    }
}

/// Something that can keep events Firstlight creates: the Calendar app, or a CalDAV account. Only what the
/// form collects goes in; only events Firstlight created are ever changed or removed through it.
pub trait CalendarWriter {
    /// Creates an event in `calendar_id`, or (with `existing`) replaces one Firstlight made before, and
    /// returns the store's identifier for it. A repeating event is changed as a whole.
    fn save_event(
        &self,
        calendar_id: &str,
        existing: Option<&str>,
        event: &SysNewEvent,
        tz: Tz,
    ) -> Result<String>;
    /// Removes an event Firstlight created (all of it, if it repeats). Already gone counts as done.
    fn delete_event(&self, event_id: &str) -> Result<()>;
}

impl<T: SystemSource + ?Sized> CalendarWriter for T {
    fn save_event(
        &self,
        calendar_id: &str,
        existing: Option<&str>,
        event: &SysNewEvent,
        tz: Tz,
    ) -> Result<String> {
        SystemSource::save_event(self, calendar_id, existing, event, tz)
    }
    fn delete_event(&self, event_id: &str) -> Result<()> {
        SystemSource::delete_event(self, event_id)
    }
}

pub struct SystemProvider {
    source: Arc<dyn SystemSource>,
    tz: Tz,
}

/// The id of the single "Calendar app" account.
pub const SYSTEM_ACCOUNT_ID: &str = "system-account";

pub fn account(now: DateTime<Utc>) -> Account {
    Account {
        id: SYSTEM_ACCOUNT_ID.into(),
        provider: ProviderKind::System,
        display_name: "Calendar app".into(),
        email: None,
        status: AccountStatus::Ok,
        secret_ref: None,
        created_at: now,
    }
}

impl SystemProvider {
    pub fn new(source: Arc<dyn SystemSource>, tz: Tz) -> Self {
        Self { source, tz }
    }

    fn require_access(&self) -> Result<()> {
        match self.source.access() {
            Access::Granted => Ok(()),
            // Withdrawn or never given: the account shows "needs attention" so the user can re-allow it.
            Access::Denied | Access::Restricted | Access::NotDetermined => Err(Error::AuthRevoked),
            Access::Unsupported => Err(Error::Provider(
                "the Calendar app is not available on this computer".into(),
            )),
        }
    }
}

/// Calendars the user rarely wants in a daily briefing start hidden (they can turn them on).
fn visible_by_default(kind: SysCalendarKind) -> bool {
    !matches!(
        kind,
        SysCalendarKind::Birthday | SysCalendarKind::Subscription
    )
}

/// Dates for an all-day event. Stores report the end as the last instant of the final day
/// (23:59:59) or as the next midnight; both must become an exclusive end date.
pub fn all_day_dates(start: DateTime<Utc>, end: DateTime<Utc>, tz: Tz) -> (NaiveDate, NaiveDate) {
    let first = start.with_timezone(&tz).date_naive();
    let last = (end - Duration::seconds(1)).with_timezone(&tz).date_naive();
    (
        first,
        (last + Duration::days(1)).max(first + Duration::days(1)),
    )
}

fn map_event(e: SysEvent, calendar_id: &str, tz: Tz) -> Event {
    let (when, key) = if e.all_day {
        let (s, en) = all_day_dates(e.start, e.end, tz);
        (When::AllDay { start: s, end: en }, s.to_string())
    } else {
        (
            When::Timed {
                start: e.start,
                end: e.end.max(e.start),
                tzid: e.tzid.clone(),
            },
            e.start.timestamp().to_string(),
        )
    };
    let status = match e.status {
        SysStatus::Canceled => EventStatus::Cancelled,
        SysStatus::Tentative => EventStatus::Tentative,
        SysStatus::None | SysStatus::Confirmed => EventStatus::Confirmed,
    };
    let my_response = match e.response {
        SysResponse::Pending => Response::NeedsAction,
        SysResponse::Accepted => Response::Accepted,
        SysResponse::Declined => Response::Declined,
        SysResponse::Tentative => Response::Tentative,
        SysResponse::NotInvited | SysResponse::Other => Response::None,
    };
    let description = e
        .notes
        .map(|n| n.chars().take(8000).collect::<String>())
        .filter(|n| !n.trim().is_empty());
    let join_url = join::extract(
        e.url.as_deref(),
        e.location.as_deref(),
        description.as_deref(),
    );
    let uid = e
        .uid
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| e.id.clone());
    Event {
        id: format!("{calendar_id}:{}:{key}", e.id),
        calendar_id: calendar_id.into(),
        remote_id: if e.is_recurring {
            format!("{}#{key}", e.id)
        } else {
            e.id.clone()
        },
        recurrence_master_id: e.is_recurring.then(|| uid.clone()),
        ical_uid: uid,
        title: if e.title.trim().is_empty() {
            "(No title)".into()
        } else {
            e.title
        },
        when,
        location: e.location.filter(|l| !l.trim().is_empty()),
        description,
        join_url,
        status,
        my_response,
        organizer: e.organizer.filter(|o| !o.is_empty()),
        updated_at: e
            .modified
            .and_then(|t| DateTime::from_timestamp(t, 0))
            .unwrap_or_default(),
        also_in: Vec::new(),
    }
}

impl Provider for SystemProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::System
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            incremental_sync: false,
            email: false,
            needs_polling: true,
        }
    }

    fn list_calendars(&self, account: &Account) -> Result<Vec<Calendar>> {
        self.require_access()?;
        let mut cals = self.source.calendars()?;
        cals.sort_by(|a, b| {
            (a.source.as_str(), a.title.as_str(), a.id.as_str()).cmp(&(
                b.source.as_str(),
                b.title.as_str(),
                b.id.as_str(),
            ))
        });
        Ok(cals
            .into_iter()
            .map(|c| {
                // "Work (Gmail)": say which account a calendar comes from unless the name already does.
                let name = if c.source.is_empty() || c.title.contains(&c.source) {
                    c.title.clone()
                } else {
                    format!("{} ({})", c.title, c.source)
                };
                Calendar {
                    id: format!("{}:{}", account.id, short_hash(&c.id)),
                    account_id: account.id.clone(),
                    colour: default_colour_for(&c.id).into(),
                    remote_id: c.id,
                    name,
                    visible: visible_by_default(c.kind),
                    read_only: c.read_only,
                }
            })
            .collect())
    }

    fn sync_events(
        &self,
        calendar: &Calendar,
        _state: Option<&str>,
        window: &Window,
    ) -> Result<SyncResult> {
        self.require_access()?;
        let raw = self
            .source
            .events(&calendar.remote_id, window.from, window.to)?;
        let mut changed: Vec<Event> = raw
            .into_iter()
            .map(|e| map_event(e, &calendar.id, self.tz))
            .filter(|e| e.overlaps(window.from, window.to))
            .collect();
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

#[cfg(test)]
mod tests;
