//! Portable data types shared by the store, providers and the UI.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Calendar colour names. The actual colours live in the UI's design tokens.
pub const CALENDAR_COLOURS: [&str; 10] = [
    "red", "orange", "yellow", "green", "teal", "blue", "indigo", "purple", "pink", "brown",
];

/// A stable, pleasant default colour for an id (so a new account always gets the same one).
pub fn default_colour_for(id: &str) -> &'static str {
    // Skip yellow: poor contrast as a thin bar on white.
    const PICK: [&str; 9] = [
        "blue", "green", "orange", "purple", "teal", "pink", "indigo", "red", "brown",
    ];
    let h = id
        .bytes()
        .fold(5381u32, |h, b| h.wrapping_mul(33) ^ u32::from(b));
    PICK[(h as usize) % PICK.len()]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Fixture,
    Google,
    Microsoft,
    Caldav,
    Ics,
    System,
}

impl ProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fixture => "fixture",
            Self::Google => "google",
            Self::Microsoft => "microsoft",
            Self::Caldav => "caldav",
            Self::Ics => "ics",
            Self::System => "system",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "fixture" => Self::Fixture,
            "google" => Self::Google,
            "microsoft" => Self::Microsoft,
            "caldav" => Self::Caldav,
            "ics" => Self::Ics,
            "system" => Self::System,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    Ok,
    /// Revoked or expired credentials; the user must act (never retried in a loop).
    NeedsAttention,
}

/// A connected account. This is a portable record: `secret_ref` names where the
/// secret lives, it does not assume this machine's keychain (PLAN.md, section 3).
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub provider: ProviderKind,
    pub display_name: String,
    pub email: Option<String>,
    pub status: AccountStatus,
    pub secret_ref: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl fmt::Debug for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Account")
            .field("id", &self.id)
            .field("provider", &self.provider)
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Calendar {
    pub id: String,
    pub account_id: String,
    pub remote_id: String,
    pub name: String,
    /// One of the fixed palette names (see `src/design`), never a raw colour.
    pub colour: String,
    pub visible: bool,
    pub read_only: bool,
}

impl fmt::Debug for Calendar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Calendar")
            .field("id", &self.id)
            .field("account_id", &self.account_id)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventStatus {
    Confirmed,
    Tentative,
    Cancelled,
}

impl EventStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Confirmed => "confirmed",
            Self::Tentative => "tentative",
            Self::Cancelled => "cancelled",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "confirmed" => Self::Confirmed,
            "tentative" => Self::Tentative,
            "cancelled" => Self::Cancelled,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Response {
    Accepted,
    Declined,
    Tentative,
    NeedsAction,
    /// The user organises the event or no response applies.
    None,
}

impl Response {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Declined => "declined",
            Self::Tentative => "tentative",
            Self::NeedsAction => "needs_action",
            Self::None => "none",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "accepted" => Self::Accepted,
            "declined" => Self::Declined,
            "tentative" => Self::Tentative,
            "needs_action" => Self::NeedsAction,
            "none" => Self::None,
            _ => return None,
        })
    }
}

/// When an event happens. All-day events are dates, never midnight timestamps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum When {
    Timed {
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        /// Original IANA timezone of the event, kept for display and recurrence.
        tzid: Option<String>,
    },
    AllDay {
        start: NaiveDate,
        /// Exclusive end date, as in iCalendar.
        end: NaiveDate,
    },
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub calendar_id: String,
    pub remote_id: String,
    pub ical_uid: String,
    pub title: String,
    pub when: When,
    pub location: Option<String>,
    pub description: Option<String>,
    /// Only `https` links are ever stored or opened.
    pub join_url: Option<String>,
    pub status: EventStatus,
    pub my_response: Response,
    pub organizer: Option<String>,
    pub recurrence_master_id: Option<String>,
    pub updated_at: DateTime<Utc>,
    /// Other calendars containing the same meeting; filled by `dedupe`, never stored.
    #[serde(default)]
    pub also_in: Vec<String>,
}

// Event content is private. Debug output (and therefore logs) shows IDs only.
impl fmt::Debug for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Event")
            .field("id", &self.id)
            .field("calendar_id", &self.calendar_id)
            .finish_non_exhaustive()
    }
}

impl Event {
    /// True if the event touches the half-open UTC range `[from, to)`.
    /// All-day events are compared by their UTC dates, which is adequate for
    /// the broad queries the store makes; the UI refines by local day.
    pub fn overlaps(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> bool {
        match &self.when {
            When::Timed { start, end, .. } => *start < to && *end > from,
            When::AllDay { start, end } => {
                let s = start.and_hms_opt(0, 0, 0).unwrap().and_utc();
                let e = end.and_hms_opt(0, 0, 0).unwrap().and_utc();
                s < to && e > from
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_never_contains_content() {
        let ev = Event {
            id: "e1".into(),
            calendar_id: "c1".into(),
            remote_id: "r1".into(),
            ical_uid: "u1".into(),
            title: "Secret merger talks".into(),
            when: When::AllDay {
                start: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                end: NaiveDate::from_ymd_opt(2026, 1, 2).unwrap(),
            },
            location: Some("Boardroom".into()),
            description: Some("confidential".into()),
            join_url: None,
            status: EventStatus::Confirmed,
            my_response: Response::None,
            organizer: Some("boss@example.com".into()),
            recurrence_master_id: None,
            updated_at: Utc::now(),
            also_in: vec![],
        };
        let out = format!("{ev:?}");
        for secret in ["Secret", "Boardroom", "confidential", "boss@"] {
            assert!(!out.contains(secret), "leaked {secret}: {out}");
        }
        assert!(out.contains("e1"));
    }

    #[test]
    fn enums_round_trip_through_strings() {
        for s in [
            EventStatus::Confirmed,
            EventStatus::Tentative,
            EventStatus::Cancelled,
        ] {
            assert_eq!(EventStatus::parse(s.as_str()), Some(s));
        }
        for r in [
            Response::Accepted,
            Response::Declined,
            Response::Tentative,
            Response::NeedsAction,
            Response::None,
        ] {
            assert_eq!(Response::parse(r.as_str()), Some(r));
        }
        assert_eq!(ProviderKind::parse("nope"), None);
    }
}
