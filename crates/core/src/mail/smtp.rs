//! Sending an invitation from a mailbox the user connected (Yahoo or iCloud), only when they ask.
//!
//! * Off unless the user turned on "Send invitations from Firstlight" in Settings, and then each message goes
//!   out only when they press Send in the invitation window, which shows the recipients, subject and text first.
//! * The servers are fixed (the two providers' own), reached with TLS from the first byte (Yahoo, port 465) or
//!   with `STARTTLS` (iCloud, port 587). The app password is sent only on a connection that is already
//!   encrypted: if `STARTTLS` is not offered or fails, nothing is sent. There is no plain-text fallback.
//! * Every address was checked by [`crate::invite::parse_address`] (no spaces, brackets, quotes or line
//!   breaks), subject text is encoded, and nothing from the page is put in a header unchecked.
//! * Nothing is stored or logged: not the message, not the recipients.

use super::imap::wire::Io;
use super::imap::{ImapPreset, secret_key};
use crate::secrets::SecretStore;
use crate::{Error, Result, invite};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::{DateTime, Utc};
use std::io::Write;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::{Duration, Instant};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(6);
const IO_TIMEOUT: Duration = Duration::from_secs(20);
const TOTAL_BUDGET: Duration = Duration::from_secs(60);
const MAX_LINE: usize = 2000;
const MAX_REPLY_LINES: usize = 60;
/// Most people one message can go to.
pub const MAX_RECIPIENTS: usize = 20;
const MAX_MESSAGE_BYTES: usize = 256 * 1024;

/// Where a provider takes outgoing mail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SmtpServer {
    pub host: &'static str,
    pub port: u16,
    /// Start in plain text and upgrade with `STARTTLS` (iCloud); otherwise encrypted from the first byte.
    pub starttls: bool,
}

/// The built-in server for a provider. A custom mailbox has none: sending is not offered for it.
pub fn server_for(preset: ImapPreset) -> Option<SmtpServer> {
    match preset {
        ImapPreset::Yahoo => Some(SmtpServer {
            host: "smtp.mail.yahoo.com",
            port: 465,
            starttls: false,
        }),
        ImapPreset::Icloud => Some(SmtpServer {
            host: "smtp.mail.me.com",
            port: 587,
            starttls: true,
        }),
        ImapPreset::Custom => None,
    }
}

/// Opens connections. A trait so tests can talk to a local fake server.
pub trait SmtpTransport: Send + Sync {
    /// A connection to the server: already encrypted when `!server.starttls`.
    fn open(&self, server: &SmtpServer) -> Result<Box<dyn Io>>;
    /// Wraps an open plain connection in TLS, after the server agreed to `STARTTLS`.
    fn upgrade(&self, io: Box<dyn Io>, server: &SmtpServer) -> Result<Box<dyn Io>>;
}

/// The real thing: the operating system checks the certificate for the provider's own name. One value per
/// message: for `STARTTLS` it keeps the open socket so the upgrade can wrap it.
#[derive(Default)]
pub struct TlsTransport {
    raw: std::sync::Mutex<Option<TcpStream>>,
}

fn tls_over(tcp: TcpStream, host: &str) -> Result<Box<dyn Io>> {
    let config = crate::tls::client_config()?;
    let name = rustls::pki_types::ServerName::try_from(host.to_string())
        .map_err(|_| Error::Invalid("bad server name"))?;
    let conn = rustls::ClientConnection::new(Arc::new(config), name)
        .map_err(|_| Error::Provider("could not set up secure connections".into()))?;
    Ok(Box::new(rustls::StreamOwned::new(conn, tcp)))
}

impl SmtpTransport for TlsTransport {
    fn open(&self, server: &SmtpServer) -> Result<Box<dyn Io>> {
        if ![ImapPreset::Yahoo, ImapPreset::Icloud]
            .iter()
            .filter_map(|p| server_for(*p))
            .any(|s| s == *server)
        {
            return Err(Error::Invalid("That mail server isn't allowed."));
        }
        let tcp = (server.host, server.port)
            .to_socket_addrs()
            .map_err(|_| Error::Offline)?
            .take(4)
            .find_map(|a| TcpStream::connect_timeout(&a, CONNECT_TIMEOUT).ok())
            .ok_or(Error::Offline)?;
        let _ = tcp.set_read_timeout(Some(IO_TIMEOUT));
        let _ = tcp.set_write_timeout(Some(IO_TIMEOUT));
        let _ = tcp.set_nodelay(true);
        if server.starttls {
            let clone = tcp.try_clone().map_err(|_| Error::Offline)?;
            *self.raw.lock().map_err(|_| Error::Offline)? = Some(clone);
            Ok(Box::new(tcp))
        } else {
            tls_over(tcp, server.host)
        }
    }

    fn upgrade(&self, io: Box<dyn Io>, server: &SmtpServer) -> Result<Box<dyn Io>> {
        drop(io); // the plain handle; the socket itself lives on in `raw`
        let tcp = self
            .raw
            .lock()
            .map_err(|_| Error::Offline)?
            .take()
            .ok_or(Error::Invalid("There is no connection to secure."))?;
        tls_over(tcp, server.host)
    }
}

/// What goes out.
pub struct Outgoing<'a> {
    pub from: &'a str,
    pub to: &'a [String],
    pub subject: &'a str,
    pub text: &'a str,
    /// The calendar invitation (`REQUEST`) or cancellation (`CANCEL`) as a file; it travels inside the message
    /// where calendar apps look for it and again as an attachment. `None` for a plain note (running late, a recap).
    pub ics: Option<&'a str>,
    /// The iTIP method named in the calendar part's header: must match the file.
    pub method: &'a str,
    pub now: DateTime<Utc>,
}

fn b64_lines(data: &[u8]) -> String {
    let enc = STANDARD.encode(data);
    enc.as_bytes()
        .chunks(76)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join("\r\n")
}

/// Header text that is not plain ASCII, as an RFC 2047 encoded word (one word per 40 bytes, on character
/// boundaries). Control characters are dropped first, so no value can end a header early.
fn encode_header(s: &str) -> String {
    let clean: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let clean = clean.trim();
    if clean.is_ascii() {
        return clean.to_string();
    }
    let mut words = Vec::new();
    let mut cur = String::new();
    for c in clean.chars() {
        if cur.len() + c.len_utf8() > 40 {
            words.push(std::mem::take(&mut cur));
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
        .iter()
        .map(|w| format!("=?UTF-8?B?{}?=", STANDARD.encode(w)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The whole message, ready for `DATA` (CRLF line ends, no dot-stuffing yet). Errors if an address is not a
/// plain address.
pub fn build_message(m: &Outgoing) -> Result<String> {
    let from = invite::parse_address(m.from).ok_or(Error::Invalid("Your address isn't valid."))?;
    if m.to.is_empty() || m.to.len() > MAX_RECIPIENTS {
        return Err(Error::Invalid("Choose between 1 and 20 recipients."));
    }
    let mut to = Vec::new();
    for a in m.to {
        to.push(
            invite::parse_address(a).ok_or(Error::Invalid("One of the addresses isn't valid."))?,
        );
    }
    let domain = from.split_once('@').map_or("localhost", |(_, d)| d);
    let id = crate::model::new_id();
    let Some(ics) = m.ics else {
        // A plain note: one text part.
        let body = m.text.replace("\r\n", "\n").replace('\n', "\r\n");
        let msg = format!(
            "From: {from}\r\nTo: {to}\r\nSubject: {subject}\r\nDate: {date}\r\nMessage-ID: <{id}@{domain}>\r\nMIME-Version: 1.0\r\n\
             Content-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: base64\r\n\r\n{text}\r\n",
            to = to.join(", "),
            subject = encode_header(m.subject),
            date = m.now.to_rfc2822(),
            text = b64_lines(body.as_bytes()),
        );
        if msg.len() > MAX_MESSAGE_BYTES {
            return Err(Error::Invalid("That message is too long to send."));
        }
        return Ok(msg);
    };
    if !matches!(m.method, "REQUEST" | "CANCEL") {
        return Err(Error::Invalid("That kind of message can't be sent."));
    }
    let outer = format!("=_firstlight_{}", crate::model::new_id());
    let inner = format!("=_firstlight_{}", crate::model::new_id());
    let body = m.text.replace("\r\n", "\n").replace('\n', "\r\n");
    // The shape calendar apps expect: the readable text and the calendar part side by side (so the app can show
    // Accept and Decline), and the same file attached for apps that only look for attachments.
    let msg = format!(
        "From: {from}\r\nTo: {to}\r\nSubject: {subject}\r\nDate: {date}\r\nMessage-ID: <{id}@{domain}>\r\nMIME-Version: 1.0\r\n\
         Content-Type: multipart/mixed; boundary=\"{outer}\"\r\n\r\n\
         --{outer}\r\nContent-Type: multipart/alternative; boundary=\"{inner}\"\r\n\r\n\
         --{inner}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: base64\r\n\r\n{text}\r\n\
         --{inner}\r\nContent-Type: text/calendar; charset=utf-8; method={method}\r\nContent-Transfer-Encoding: base64\r\n\r\n{ics}\r\n\
         --{inner}--\r\n\
         --{outer}\r\nContent-Type: application/ics; name=\"invite.ics\"\r\n\
         Content-Disposition: attachment; filename=\"invite.ics\"\r\nContent-Transfer-Encoding: base64\r\n\r\n{ics}\r\n\
         --{outer}--\r\n",
        to = to.join(", "),
        subject = encode_header(m.subject),
        date = m.now.to_rfc2822(),
        method = m.method,
        text = b64_lines(body.as_bytes()),
        ics = b64_lines(ics.as_bytes()),
    );
    if msg.len() > MAX_MESSAGE_BYTES {
        return Err(Error::Invalid("That message is too long to send."));
    }
    Ok(msg)
}

/// Dot-stuffs a message and ends it with the `DATA` terminator.
fn data_block(msg: &str) -> String {
    let mut out = String::with_capacity(msg.len() + 8);
    for line in msg.strip_suffix("\r\n").unwrap_or(msg).split("\r\n") {
        if line.starts_with('.') {
            out.push('.');
        }
        out.push_str(line);
        out.push_str("\r\n");
    }
    out.push_str(".\r\n");
    out
}

struct Session {
    io: Box<dyn Io>,
    started: Instant,
}

struct Reply {
    code: u16,
    lines: Vec<String>,
}

impl Session {
    fn check_time(&self) -> Result<()> {
        if self.started.elapsed() > TOTAL_BUDGET {
            return Err(Error::Offline);
        }
        Ok(())
    }

    fn read_line(&mut self) -> Result<String> {
        let mut line = Vec::new();
        let mut b = [0u8; 1];
        loop {
            self.check_time()?;
            match self.io.read(&mut b) {
                Ok(0) | Err(_) => return Err(Error::Offline),
                Ok(_) => {}
            }
            if b[0] == b'\n' {
                break;
            }
            if line.len() >= MAX_LINE {
                return Err(Error::Provider("the mail server sent too much".into()));
            }
            line.push(b[0]);
        }
        Ok(String::from_utf8_lossy(&line)
            .trim_end_matches('\r')
            .to_string())
    }

    fn reply(&mut self) -> Result<Reply> {
        let mut lines = Vec::new();
        for _ in 0..MAX_REPLY_LINES {
            let l = self.read_line()?;
            let code: u16 = l.get(..3).and_then(|c| c.parse().ok()).ok_or_else(|| {
                Error::Provider("the mail server sent something unexpected".into())
            })?;
            let more = l.as_bytes().get(3) == Some(&b'-');
            lines.push(l.get(4..).unwrap_or("").to_string());
            if !more {
                return Ok(Reply { code, lines });
            }
        }
        Err(Error::Provider("the mail server sent too much".into()))
    }

    fn send(&mut self, line: &str) -> Result<()> {
        self.check_time()?;
        self.io
            .write_all(format!("{line}\r\n").as_bytes())
            .and_then(|_| self.io.flush())
            .map_err(|_| Error::Offline)
    }

    fn command(&mut self, line: &str) -> Result<Reply> {
        self.send(line)?;
        self.reply()
    }
}

fn refused(code: u16) -> Error {
    match code {
        535 | 534 | 530 | 454 => Error::AuthRevoked,
        400..=499 => Error::Provider(format!("the mail server asked to try later ({code})")),
        _ => Error::Provider(format!("the mail server refused it ({code})")),
    }
}

fn expect(r: &Reply, ok: &[u16]) -> Result<()> {
    if ok.contains(&r.code) {
        Ok(())
    } else {
        Err(refused(r.code))
    }
}

/// Sends one message. `username` and `password` are the mailbox's own login.
pub fn send(
    transport: &dyn SmtpTransport,
    server: &SmtpServer,
    username: &str,
    password: &str,
    m: &Outgoing,
) -> Result<()> {
    let msg = build_message(m)?;
    let from = invite::parse_address(m.from).ok_or(Error::Invalid("Your address isn't valid."))?;
    let mut s = Session {
        io: transport.open(server)?,
        started: Instant::now(),
    };
    expect(&s.reply()?, &[220])?;
    let mut caps = s.command("EHLO firstlight")?;
    expect(&caps, &[250])?;
    if server.starttls {
        let advertised = caps
            .lines
            .iter()
            .any(|l| l.eq_ignore_ascii_case("STARTTLS"));
        if !advertised {
            // Never log in over an unencrypted connection.
            return Err(Error::Provider(
                "the mail server doesn't offer an encrypted connection".into(),
            ));
        }
        expect(&s.command("STARTTLS")?, &[220])?;
        s.io = transport.upgrade(s.io, server)?;
        caps = s.command("EHLO firstlight")?;
        expect(&caps, &[250])?;
    }
    let auth: Vec<String> = caps
        .lines
        .iter()
        .filter_map(|l| l.strip_prefix("AUTH ").or_else(|| l.strip_prefix("AUTH=")))
        .flat_map(|l| l.split_whitespace().map(str::to_ascii_uppercase))
        .collect();
    if auth.iter().any(|a| a == "PLAIN") {
        let token = STANDARD.encode(format!("\0{username}\0{password}"));
        expect(&s.command(&format!("AUTH PLAIN {token}"))?, &[235])?;
    } else if auth.iter().any(|a| a == "LOGIN") {
        expect(&s.command("AUTH LOGIN")?, &[334])?;
        expect(&s.command(&STANDARD.encode(username))?, &[334])?;
        expect(&s.command(&STANDARD.encode(password))?, &[235])?;
    } else {
        return Err(Error::Provider(
            "the mail server doesn't offer a sign-in method Firstlight supports".into(),
        ));
    }
    expect(&s.command(&format!("MAIL FROM:<{from}>"))?, &[250])?;
    for a in m.to {
        let a =
            invite::parse_address(a).ok_or(Error::Invalid("One of the addresses isn't valid."))?;
        expect(&s.command(&format!("RCPT TO:<{a}>"))?, &[250, 251])?;
    }
    expect(&s.command("DATA")?, &[354])?;
    s.check_time()?;
    s.io.write_all(data_block(&msg).as_bytes())
        .and_then(|_| s.io.flush())
        .map_err(|_| Error::Offline)?;
    expect(&s.reply()?, &[250])?;
    let _ = s.send("QUIT");
    Ok(())
}

/// The mailbox's saved app password.
pub fn password_for(secrets: &dyn SecretStore, mailbox_id: &str) -> Result<String> {
    secrets
        .get(&secret_key(mailbox_id))?
        .ok_or(Error::AuthRevoked)
}

#[cfg(test)]
mod tests;
