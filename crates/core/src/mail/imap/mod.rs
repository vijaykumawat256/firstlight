//! IMAP mail for Yahoo and iCloud, read-only.
//!
//! * The servers are fixed presets (`imap.mail.yahoo.com`, `imap.mail.me.com`, port 993, TLS from the
//!   first byte). A settings file or the page can never point this at another host.
//! * The mailbox is opened with `EXAMINE`, which cannot change anything, and headers are fetched with
//!   `BODY.PEEK`, so nothing is marked read. No command that writes (`STORE`, `APPEND`, `COPY`,
//!   `DELETE`, `EXPUNGE`) exists in this module, and a test checks the commands actually sent.
//! * Only sender, subject and date are read. Message bodies are never fetched, so there is no snippet.
//! * Both providers require an app-specific password; the password lives in the OS keychain.

mod mime;
#[cfg(test)]
mod tests;
mod wire;

use super::{OpenTarget, RawMessage, SourceBatch, subject_looks_like_invitation};
use crate::secrets::SecretStore;
use crate::{Error, Result};
use chrono::{DateTime, Datelike, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::{Duration as StdDuration, Instant};
use wire::{Conn, Io, Piece, Tok, Value, parse_value, quote, tokenize};

const PORT: u16 = 993;
const CONNECT_TIMEOUT: StdDuration = StdDuration::from_secs(6);
/// Addresses tried before giving up, so a dead network fails in well under a minute.
const MAX_ADDRESSES: usize = 4;
const IO_TIMEOUT: StdDuration = StdDuration::from_secs(20);
/// The whole fetch, however slow the server is.
const TOTAL_BUDGET: StdDuration = StdDuration::from_secs(45);
pub const MAX_MAILBOXES: usize = 4;

/// The providers we know. Adding one is a code change on purpose: the host list is part of the trust model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImapPreset {
    Yahoo,
    Icloud,
}

impl ImapPreset {
    pub fn host(self) -> &'static str {
        match self {
            Self::Yahoo => "imap.mail.yahoo.com",
            Self::Icloud => "imap.mail.me.com",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Yahoo => "Yahoo Mail",
            Self::Icloud => "iCloud Mail",
        }
    }

    pub fn webmail(self) -> &'static str {
        match self {
            Self::Yahoo => "https://mail.yahoo.com/",
            Self::Icloud => "https://www.icloud.com/mail/",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "yahoo" => Some(Self::Yahoo),
            "icloud" => Some(Self::Icloud),
            _ => None,
        }
    }
}

/// A mailbox the user connected. Contains no secret: the app password is in the keychain under
/// [`secret_key`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImapMailbox {
    pub id: String,
    pub preset: ImapPreset,
    /// The login name, normally the full email address.
    pub address: String,
}

impl ImapMailbox {
    /// Whether the fields are sane: bounded, printable ASCII, and an id we generate ourselves.
    pub fn is_valid(&self) -> bool {
        valid_address(&self.address)
            && !self.id.is_empty()
            && self.id.len() <= 64
            && self
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    }
}

pub fn valid_address(a: &str) -> bool {
    !a.is_empty() && a.len() <= 254 && a.bytes().all(|b| (0x21..0x7f).contains(&b))
}

pub fn secret_key(mailbox_id: &str) -> String {
    format!("imap-mail:{mailbox_id}")
}

/// Opens connections. A trait so tests can talk to a local fake server.
pub trait Connect: Send + Sync {
    fn connect(&self, preset: ImapPreset) -> Result<Box<dyn Io>>;
}

/// The real thing: TLS to the preset's host, certificate checked by the operating system.
pub struct TlsConnect;

impl Connect for TlsConnect {
    fn connect(&self, preset: ImapPreset) -> Result<Box<dyn Io>> {
        use rustls_platform_verifier::BuilderVerifierExt;
        let host = preset.host();
        // Try every address the name resolves to: the first may be IPv6 on a network without it.
        let tcp = (host, PORT)
            .to_socket_addrs()
            .map_err(|_| Error::Offline)?
            .take(MAX_ADDRESSES)
            .find_map(|addr| TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).ok())
            .ok_or(Error::Offline)?;
        let _ = tcp.set_read_timeout(Some(IO_TIMEOUT));
        let _ = tcp.set_write_timeout(Some(IO_TIMEOUT));
        let _ = tcp.set_nodelay(true);
        let config = rustls::ClientConfig::builder()
            .with_platform_verifier()
            .map_err(|_| Error::Provider("could not set up secure connections".into()))?
            .with_no_client_auth();
        let name = rustls::pki_types::ServerName::try_from(host.to_string())
            .map_err(|_| Error::Invalid("bad server name"))?;
        let conn = rustls::ClientConnection::new(Arc::new(config), name)
            .map_err(|_| Error::Provider("could not set up secure connections".into()))?;
        Ok(Box::new(rustls::StreamOwned::new(conn, tcp)))
    }
}

/// What one mailbox contributes.
pub struct ImapSource {
    connect: Arc<dyn Connect>,
    secrets: Arc<dyn SecretStore>,
}

#[derive(Debug, Default)]
struct Inbox {
    unseen_total: u32,
    messages: Vec<Header>,
}

#[derive(Debug, Default, Clone)]
struct Header {
    uid: u32,
    received: Option<DateTime<Utc>>,
    from: String,
    subject: String,
    content_type: String,
}

impl ImapSource {
    pub fn new(connect: Arc<dyn Connect>, secrets: Arc<dyn SecretStore>) -> Self {
        Self { connect, secrets }
    }

    /// Unread inbox mail from the last `days_back` days, newest first, at most `max` messages.
    pub fn fetch(
        &self,
        mailbox: &ImapMailbox,
        days_back: i64,
        max: usize,
        now: DateTime<Utc>,
    ) -> Result<SourceBatch> {
        let password = self
            .secrets
            .get(&secret_key(&mailbox.id))?
            .ok_or(Error::AuthRevoked)?;
        let io = self.connect.connect(mailbox.preset)?;
        let since = now - Duration::days(days_back.clamp(1, 30));
        let inbox = fetch_inbox(
            io,
            &mailbox.address,
            &password,
            since,
            max,
            Instant::now() + TOTAL_BUDGET,
        )?;
        let messages = inbox
            .messages
            .into_iter()
            .map(|h| RawMessage {
                id: format!("imap:{}:{}", mailbox.id, h.uid),
                invitation_hint: subject_looks_like_invitation(&h.subject)
                    || h.content_type.to_ascii_lowercase().contains("calendar"),
                sender: h.from,
                subject: h.subject,
                received: h.received.unwrap_or(now),
                snippet: String::new(),
                unread: true,
                ics: None,
                open: OpenTarget::Url {
                    url: mailbox.preset.webmail().into(),
                },
            })
            .collect();
        Ok(SourceBatch {
            unread_total: inbox.unseen_total,
            messages,
        })
    }

    /// Signs in and out, to check an address and app password before saving them.
    pub fn check_login(&self, preset: ImapPreset, address: &str, password: &str) -> Result<()> {
        let io = self.connect.connect(preset)?;
        let mut conn = Conn::new(io, Instant::now() + TOTAL_BUDGET);
        conn.greeting()?;
        login(&mut conn, address, password)?;
        let _ = conn.run("LOGOUT");
        Ok(())
    }
}

fn login(conn: &mut Conn, address: &str, password: &str) -> Result<()> {
    let caps = conn.run("CAPABILITY")?;
    let caps_text: String = caps
        .untagged
        .iter()
        .flat_map(|p| p.iter())
        .filter_map(|p| match p {
            Piece::Text(t) => Some(t.to_ascii_uppercase()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    if caps_text.contains("LOGINDISABLED") {
        return Err(Error::Provider(
            "this mail server needs a different sign-in method".into(),
        ));
    }
    // Yahoo wants to know who is connecting before it lets anyone in. Name and version only.
    if caps_text.split_whitespace().any(|w| w == "ID") {
        let _ = conn.run(&format!(
            "ID (\"name\" \"Daybrief\" \"version\" \"{}\")",
            env!("CARGO_PKG_VERSION")
        ));
    }
    let reply = conn.run(&format!("LOGIN {} {}", quote(address)?, quote(password)?))?;
    if reply.ok {
        Ok(())
    } else {
        // The server's wording is dropped: it can echo the name, and it is never useful to show.
        Err(Error::AuthRevoked)
    }
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// `07-Nov-2026`, always English month names as IMAP requires.
fn imap_date(d: DateTime<Utc>) -> String {
    format!(
        "{:02}-{}-{}",
        d.day(),
        MONTHS[d.month0() as usize],
        d.year()
    )
}

/// `17-Jul-1996 02:44:25 -0700`, with a space instead of a leading zero on the day for some servers.
fn parse_internal_date(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_str(s.trim(), "%d-%b-%Y %H:%M:%S %z")
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

fn fetch_inbox(
    io: Box<dyn Io>,
    address: &str,
    password: &str,
    since: DateTime<Utc>,
    max: usize,
    deadline: Instant,
) -> Result<Inbox> {
    let mut conn = Conn::new(io, deadline);
    conn.greeting()?;
    login(&mut conn, address, password)?;

    // The whole unread count, whatever its age, so "12 unread" is true.
    let mut unseen_total = 0;
    if let Ok(r) = conn.run("STATUS \"INBOX\" (UNSEEN)") {
        for resp in &r.untagged {
            let toks = tokenize(resp);
            if let Some(i) = toks
                .iter()
                .position(|t| matches!(t, Tok::Atom(a) if a.eq_ignore_ascii_case("UNSEEN")))
                && let Some(Tok::Atom(n)) = toks.get(i + 1)
            {
                unseen_total = n.parse().unwrap_or(0);
            }
        }
    }

    // EXAMINE, never SELECT: read-only by definition.
    if !conn.run("EXAMINE \"INBOX\"")?.ok {
        return Err(Error::Provider("the inbox could not be opened".into()));
    }
    let found = conn.run(&format!("UID SEARCH UNSEEN SINCE {}", imap_date(since)))?;
    if !found.ok {
        return Err(Error::Provider("the inbox could not be searched".into()));
    }
    let mut uids: Vec<u32> = found
        .untagged
        .iter()
        .map(|resp| tokenize(resp))
        .filter(|t| matches!(t.get(1), Some(Tok::Atom(a)) if a.eq_ignore_ascii_case("SEARCH")))
        .flat_map(|t| t.into_iter().skip(2))
        .filter_map(|t| match t {
            Tok::Atom(a) => a.parse().ok(),
            _ => None,
        })
        .collect();
    uids.sort_unstable_by(|a, b| b.cmp(a));
    uids.dedup();
    uids.truncate(max);
    let unseen_total = unseen_total.max(uids.len() as u32);

    let mut messages = Vec::new();
    if !uids.is_empty() {
        let list = uids
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let reply = conn.run(&format!(
            "UID FETCH {list} (FLAGS INTERNALDATE BODY.PEEK[HEADER.FIELDS (FROM SUBJECT CONTENT-TYPE)])"
        ))?;
        if !reply.ok {
            return Err(Error::Provider("the inbox could not be read".into()));
        }
        for resp in &reply.untagged {
            if let Some(h) = parse_fetch(resp) {
                messages.push(h);
            }
        }
    }
    messages.sort_by(|a, b| b.received.cmp(&a.received).then(b.uid.cmp(&a.uid)));
    let _ = conn.run("LOGOUT");
    Ok(Inbox {
        unseen_total,
        messages,
    })
}

/// Reads one `* n FETCH (...)` response; `None` for anything else or anything incomplete.
fn parse_fetch(resp: &[Piece]) -> Option<Header> {
    let toks = tokenize(resp);
    if !matches!(toks.first(), Some(Tok::Atom(a)) if a == "*")
        || !matches!(toks.get(2), Some(Tok::Atom(a)) if a.eq_ignore_ascii_case("FETCH"))
    {
        return None;
    }
    let mut pos = 3;
    let Value::List(items) = parse_value(&toks, &mut pos, 0)? else {
        return None;
    };
    let mut h = Header::default();
    let mut have_uid = false;
    let mut it = items.into_iter();
    while let (Some(key), Some(val)) = (it.next(), it.next()) {
        let Some(key) = key.text().map(|k| k.to_ascii_uppercase()) else {
            continue;
        };
        if key == "UID" {
            if let Some(n) = val.text().and_then(|t| t.parse().ok()) {
                h.uid = n;
                have_uid = true;
            }
        } else if key == "INTERNALDATE" {
            h.received = val.text().and_then(|t| parse_internal_date(&t));
        } else if key.starts_with("BODY[HEADER") || key.starts_with("BODY.PEEK[HEADER") {
            let raw = match val {
                Value::Bytes(b) => b,
                Value::Str(s) => s.into_bytes(),
                _ => continue,
            };
            let headers = mime::parse_headers(&raw);
            h.from = mime::get(&headers, "from")
                .map(mime::decode_header)
                .unwrap_or_default();
            h.subject = mime::get(&headers, "subject")
                .map(mime::decode_header)
                .unwrap_or_default();
            h.content_type = mime::get(&headers, "content-type")
                .map(mime::clean)
                .unwrap_or_default();
        }
    }
    have_uid.then_some(h)
}
