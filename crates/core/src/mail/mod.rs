//! The email digest: a few facts about your inbox, never a mail client.
//!
//! What it keeps per message is deliberately small: sender, subject, time and a short plain-text
//! snippet. No HTML is ever rendered, no remote content is fetched, and no message body is stored
//! beyond that snippet. Opening a message hands off to the user's own mail app or webmail.
//!
//! Everything here is pure: the sources (Apple Mail, Gmail, Microsoft) turn their data into
//! [`RawMessage`]s, and [`build_digest`] does the rest.

use crate::ics::{ParseOptions, events_from_ics};
use crate::model::{Event, EventStatus, When};
use crate::providers::Window;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub mod applemail;
pub mod gmail;
pub mod microsoft;

/// The longest snippet kept. Enough to recognise a message, far too little to be a copy of it.
pub const MAX_SNIPPET_CHARS: usize = 160;
const MAX_IMPORTANT: usize = 5;
const MAX_INVITATIONS: usize = 5;

/// Where a message can be opened. Only `https` links and Apple Mail message ids exist, by construction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OpenTarget {
    /// A web link (Gmail, Outlook on the web). Always https.
    Url { url: String },
    /// Apple Mail, opened by its message-id.
    AppleMail { message_id: String },
}

/// What a source reports for one message.
#[derive(Debug, Clone)]
pub struct RawMessage {
    /// A stable id for de-duplication (the provider's message id).
    pub id: String,
    /// The raw sender, "Name <address>" or just an address.
    pub sender: String,
    pub subject: String,
    pub received: DateTime<Utc>,
    pub snippet: String,
    pub unread: bool,
    /// The message carries a calendar attachment or is a native meeting request.
    pub invitation_hint: bool,
    /// The calendar attachment's text, when the source could fetch it.
    pub ics: Option<String>,
    pub open: OpenTarget,
}

/// One source's contribution (a mailbox, an account).
#[derive(Debug, Clone, Default)]
pub struct SourceBatch {
    /// All unread messages in the inbox, which is more than `messages` holds.
    pub unread_total: u32,
    pub messages: Vec<RawMessage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MailMessage {
    pub id: String,
    pub sender_name: String,
    pub sender_address: String,
    pub subject: String,
    pub received: DateTime<Utc>,
    pub snippet: String,
    pub open: OpenTarget,
}

/// A meeting invitation sitting in the inbox.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvitationItem {
    pub message: MailMessage,
    /// Details read from the calendar attachment, when there was one we could parse.
    pub title: Option<String>,
    pub when: Option<When>,
    pub location: Option<String>,
    pub organizer: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MailDigest {
    pub generated_at: DateTime<Utc>,
    pub unread_total: u32,
    pub important: Vec<MailMessage>,
    pub invitations: Vec<InvitationItem>,
    /// Unread messages that are neither important nor invitations (so the UI can say "and 12 more").
    pub other_unread: u32,
}

/// Decodes the HTML entities some providers leave in plain-text snippets (`&amp;`, `&#39;`, ...).
/// Anything unrecognised is left as it is; the result is still plain text.
pub fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';').filter(|e| *e <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let name = &rest[1..end];
        let decoded = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            n if n.starts_with("#x") || n.starts_with("#X") => u32::from_str_radix(&n[2..], 16)
                .ok()
                .and_then(char::from_u32),
            n if n.starts_with('#') => n[1..].parse::<u32>().ok().and_then(char::from_u32),
            _ => None,
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Splits `"Priya Shah" <priya@example.com>` into display name and lower-cased address.
pub fn parse_sender(raw: &str) -> (String, String) {
    let raw = raw.trim();
    if let (Some(open), Some(close)) = (raw.rfind('<'), raw.rfind('>'))
        && open < close
    {
        let address = raw[open + 1..close].trim().to_ascii_lowercase();
        let name = raw[..open].trim().trim_matches('"').trim().to_string();
        return (
            if name.is_empty() {
                address.clone()
            } else {
                name
            },
            address,
        );
    }
    let address = raw
        .trim_matches(|c| c == '<' || c == '>')
        .to_ascii_lowercase();
    (address.clone(), address)
}

/// A short, single-line, plain-text snippet. Control characters and markup noise are removed.
pub fn clean_snippet(raw: &str) -> String {
    let mut out = String::with_capacity(MAX_SNIPPET_CHARS + 4);
    let mut last_space = true;
    let mut in_tag = false;
    for c in raw.chars() {
        // Anything tag-like is dropped: the digest never carries markup, even by accident.
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if in_tag => {}
            c if c.is_control() || c.is_whitespace() || c == '\u{200b}' || c == '\u{feff}' => {
                if !last_space {
                    out.push(' ');
                    last_space = true;
                }
            }
            c => {
                out.push(c);
                last_space = false;
            }
        }
    }
    let out = out.trim();
    if out.chars().count() <= MAX_SNIPPET_CHARS {
        return out.to_string();
    }
    let cut: String = out.chars().take(MAX_SNIPPET_CHARS - 1).collect();
    format!("{}…", cut.trim_end())
}

/// Does this sender match any entry? Entries may be a full address, `@domain.com`, a bare domain,
/// or a name. Comparisons ignore case.
pub fn sender_matches(name: &str, address: &str, patterns: &[String]) -> bool {
    let (name, address) = (name.to_ascii_lowercase(), address.to_ascii_lowercase());
    patterns
        .iter()
        .map(|p| p.trim().to_ascii_lowercase())
        .filter(|p| !p.is_empty())
        .any(|p| {
            if p.contains('@') && !p.starts_with('@') {
                address == p
            } else if let Some(domain) = p.strip_prefix('@') {
                address.ends_with(&format!("@{domain}")) || address.ends_with(&format!(".{domain}"))
            } else if p.contains('.') && !p.contains(' ') {
                address.ends_with(&format!("@{p}")) || address.ends_with(&format!(".{p}"))
            } else {
                name == p
            }
        })
}

/// Subjects that the common calendar systems use for invitations and their updates.
pub fn subject_looks_like_invitation(subject: &str) -> bool {
    let s = subject.trim().to_ascii_lowercase();
    [
        "invitation:",
        "invitation from",
        "updated invitation",
        "meeting invitation",
        "new event:",
        "meeting request",
        "invitation to",
        "canceled event",
        "cancelled event",
        "accepted:",
        "declined:",
        "tentative:",
    ]
    .iter()
    .any(|p| s.starts_with(p) || s.starts_with(&format!("fwd: {p}")))
}

fn to_message(m: &RawMessage) -> MailMessage {
    let (sender_name, sender_address) = parse_sender(&m.sender);
    MailMessage {
        id: m.id.clone(),
        sender_name,
        sender_address,
        subject: m.subject.trim().chars().take(200).collect(),
        received: m.received,
        snippet: clean_snippet(&m.snippet),
        open: m.open.clone(),
    }
}

fn invitation_from(m: &RawMessage, window: &Window) -> InvitationItem {
    let message = to_message(m);
    let parsed: Option<Event> = m.ics.as_deref().and_then(|text| {
        let opts = ParseOptions {
            calendar_id: "mail",
            default_tz: chrono_tz::UTC,
            window: *window,
            my_email: None,
        };
        events_from_ics(text, &opts)
            .ok()?
            .into_iter()
            .find(|e| e.status != EventStatus::Cancelled)
    });
    match parsed {
        Some(e) => InvitationItem {
            message,
            title: Some(e.title),
            when: Some(e.when),
            location: e.location,
            organizer: e.organizer,
        },
        None => InvitationItem {
            message,
            title: None,
            when: None,
            location: None,
            organizer: None,
        },
    }
}

/// Builds the digest from every source.
///
/// * `important`: the user's list (addresses, `@domain`s, names);
/// * `meeting_people`: names of today's meeting organisers, treated as important too.
pub fn build_digest(
    batches: &[SourceBatch],
    important: &[String],
    meeting_people: &[String],
    now: DateTime<Utc>,
) -> MailDigest {
    let unread_total = batches.iter().map(|b| b.unread_total).sum();
    let window = Window {
        from: now - Duration::days(1),
        to: now + Duration::days(60),
    };

    let mut seen = HashSet::new();
    let mut messages: Vec<&RawMessage> = batches
        .iter()
        .flat_map(|b| &b.messages)
        .filter(|m| m.unread && seen.insert(m.id.clone()))
        .collect();
    messages.sort_by(|a, b| b.received.cmp(&a.received).then_with(|| a.id.cmp(&b.id)));

    let everyone: Vec<String> = important.iter().chain(meeting_people).cloned().collect();
    let mut digest = MailDigest {
        generated_at: now,
        unread_total,
        ..Default::default()
    };
    let mut accounted = 0u32;

    for m in &messages {
        let is_invitation = m.invitation_hint || subject_looks_like_invitation(&m.subject);
        if is_invitation {
            // Only what is actually shown is "accounted for"; the rest still counts as unread.
            if digest.invitations.len() < MAX_INVITATIONS {
                digest.invitations.push(invitation_from(m, &window));
                accounted += 1;
            }
            continue;
        }
        let (name, addr) = parse_sender(&m.sender);
        if sender_matches(&name, &addr, &everyone) && digest.important.len() < MAX_IMPORTANT {
            digest.important.push(to_message(m));
            accounted += 1;
        }
    }
    digest.other_unread = unread_total.saturating_sub(accounted);
    digest
}

#[cfg(test)]
mod tests;
