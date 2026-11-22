//! Pairing a phone with a computer by typing a 6-digit code.
//!
//! 1. The computer shows a fresh 6-digit code (valid 10 minutes, 5 tries) and listens for phones.
//! 2. The phone looks for it on the home network: it sends a tiny "who is pairing?" message ([`WHO`]) to the paired
//!    address it knows and to every address on its own network. Each goes to one address (no broadcast), which a phone
//!    may do with the ordinary local-network permission; iOS only allows broadcasts with a special permission from
//!    Apple. A computer that is pairing answers with the port to connect to ([`here_text`]); nothing personal.
//! 3. Over that connection both sides run SPAKE2 with the 6 digits. Each side ends up with the same 32-byte key only
//!    if both typed the same code, and someone listening learns nothing they could test guesses against offline: every
//!    guess needs a real attempt, and the computer allows 5.
//! 4. Sealed with that key, the computer hands over the sync code, its device id and its name ([`Offer`]); the phone
//!    answers with its own ([`Reply`]). Only when the phone's answer opens is the pairing done, so both screens can say
//!    so truthfully.

use super::{open, seal, valid_device};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use spake2::{Ed25519Group, Identity, Password, Spake2};
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, TcpStream};
use std::time::Duration;

/// How many digits a pairing code has.
pub const PIN_DIGITS: usize = 6;
/// How long a pairing code works.
pub const PIN_LIFETIME: Duration = Duration::from_secs(10 * 60);
/// Wrong codes allowed before the computer stops accepting this one.
pub const MAX_TRIES: u32 = 5;
/// "Is anyone pairing?", sent by the phone to the announcement port.
pub const WHO: &[u8] = b"FLWHO1";
const HERE: &str = "FLHERE1";
const HELLO: &[u8; 6] = b"FLPAK1";
const ID_PHONE: &[u8] = b"firstlight-pair-phone";
const ID_COMPUTER: &[u8] = b"firstlight-pair-computer";
const MAX_FRAME: usize = 4096;
const IO_LIMIT: Duration = Duration::from_secs(15);
/// At most this many remembered addresses for the other device.
pub const MAX_ADDRS: usize = 4;

/// A fresh code, every digit equally likely.
pub fn new_pin() -> Result<String> {
    let mut out = String::with_capacity(PIN_DIGITS);
    while out.len() < PIN_DIGITS {
        let mut b = [0u8; 1];
        getrandom::fill(&mut b).map_err(|_| Error::Invalid("Couldn't make a random number."))?;
        // 250 = 25 × 10: dropping 250..=255 keeps every digit equally likely.
        if b[0] < 250 {
            out.push(char::from(b'0' + b[0] % 10));
        }
    }
    Ok(out)
}

/// The code as typed ("482 913", "482-913") reduced to its digits, or `None` if it isn't 6 digits.
pub fn clean_pin(typed: &str) -> Option<String> {
    let digits: String = typed
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect();
    (digits.len() == PIN_DIGITS && digits.bytes().all(|b| b.is_ascii_digit())).then_some(digits)
}

/// The computer's answer to [`WHO`]: where to connect.
pub fn here_text(port: u16) -> String {
    format!("{HERE} {port}")
}

pub fn parse_here(data: &[u8]) -> Option<u16> {
    let text = std::str::from_utf8(data).ok()?;
    let port = text.trim().strip_prefix(HERE)?.strip_prefix(' ')?;
    port.parse().ok().filter(|p| *p > 0)
}

/// What the computer hands over once the code matched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Offer {
    pub code: String,
    pub device: String,
    pub name: String,
}

/// The phone's answer, which also proves to the computer that it typed the right code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reply {
    pub device: String,
    pub name: String,
}

/// A device name as shown on the other side: printable, trimmed, at most 80 characters.
pub fn clean_name(name: &str) -> String {
    let n: String = name.chars().filter(|c| !c.is_control()).take(80).collect();
    let n = n.trim();
    if n.is_empty() {
        "Your device".into()
    } else {
        n.into()
    }
}

fn io(_: std::io::Error) -> Error {
    Error::Offline
}

fn send(stream: &mut TcpStream, data: &[u8]) -> Result<()> {
    let len = u16::try_from(data.len()).map_err(|_| Error::Invalid("too big"))?;
    stream.write_all(&len.to_be_bytes()).map_err(io)?;
    stream.write_all(data).map_err(io)
}

fn receive(stream: &mut TcpStream) -> Result<Vec<u8>> {
    let mut len = [0u8; 2];
    stream.read_exact(&mut len).map_err(io)?;
    let len = u16::from_be_bytes(len) as usize;
    if len == 0 || len > MAX_FRAME {
        return Err(Error::Invalid("not a pairing message"));
    }
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).map_err(io)?;
    Ok(buf)
}

fn limits(stream: &TcpStream) -> Result<()> {
    stream.set_read_timeout(Some(IO_LIMIT)).map_err(io)?;
    stream.set_write_timeout(Some(IO_LIMIT)).map_err(io)
}

/// The message for a wrong code (or anything that fails the handshake, which looks the same from outside).
pub const WRONG_CODE: &str =
    "That code isn't right. Check the 6 digits on your computer and try again.";

fn wrong() -> Error {
    Error::Invalid(WRONG_CODE)
}

fn session_key(bytes: Vec<u8>) -> Result<[u8; 32]> {
    bytes.try_into().map_err(|_| wrong())
}

/// The computer's side of one attempt. `Ok` means the phone had the right code and is now paired; an `Err` counts as
/// a try.
pub fn host(mut stream: TcpStream, pin: &str, offer: &Offer) -> Result<Reply> {
    limits(&stream)?;
    let mut hello = [0u8; HELLO.len()];
    stream.read_exact(&mut hello).map_err(io)?;
    if &hello != HELLO {
        return Err(Error::Invalid("not a pairing request"));
    }
    let (spake, mine) = Spake2::<Ed25519Group>::start_b(
        &Password::new(pin.as_bytes()),
        &Identity::new(ID_PHONE),
        &Identity::new(ID_COMPUTER),
    );
    let theirs = receive(&mut stream)?;
    send(&mut stream, &mine)?;
    let key = session_key(spake.finish(&theirs).map_err(|_| wrong())?)?;
    let offer_bytes = serde_json::to_vec(offer).map_err(|_| wrong())?;
    send(&mut stream, &seal(&key, &offer_bytes)?)?;
    let answer = receive(&mut stream)?;
    let reply: Reply = open(&key, &answer)
        .ok()
        .and_then(|p| serde_json::from_slice(&p).ok())
        .ok_or_else(wrong)?;
    if !valid_device(&reply.device) {
        return Err(wrong());
    }
    let _ = send(&mut stream, b"OK");
    Ok(Reply {
        device: reply.device,
        name: clean_name(&reply.name),
    })
}

/// The phone's side: connects with the typed code and returns what the computer handed over.
pub fn join(mut stream: TcpStream, pin: &str, reply: &Reply) -> Result<Offer> {
    limits(&stream)?;
    stream.write_all(HELLO).map_err(io)?;
    let (spake, mine) = Spake2::<Ed25519Group>::start_a(
        &Password::new(pin.as_bytes()),
        &Identity::new(ID_PHONE),
        &Identity::new(ID_COMPUTER),
    );
    send(&mut stream, &mine)?;
    let theirs = receive(&mut stream)?;
    let key = session_key(spake.finish(&theirs).map_err(|_| wrong())?)?;
    let sealed = receive(&mut stream)?;
    let offer: Offer = open(&key, &sealed)
        .ok()
        .and_then(|p| serde_json::from_slice(&p).ok())
        .ok_or_else(wrong)?;
    super::parse_code(&offer.code).map_err(|_| wrong())?;
    if !valid_device(&offer.device) {
        return Err(wrong());
    }
    let reply_bytes = serde_json::to_vec(reply).map_err(|_| wrong())?;
    send(&mut stream, &seal(&key, &reply_bytes)?)?;
    // The computer's "OK" means it took our answer; without it the pairing isn't finished.
    if receive(&mut stream)? != b"OK" {
        return Err(wrong());
    }
    Ok(Offer {
        name: clean_name(&offer.name),
        ..offer
    })
}

/// Only addresses on a home or office network are remembered or contacted.
pub fn local_network_addr(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || is_shared_v4(v4),
        // Unique local addresses (fc00::/7), the IPv6 version of a home network's 192.168.x.x.
        IpAddr::V6(v6) => (v6.segments()[0] & 0xfe00) == 0xfc00 && !is_v4_mapped(v6),
    }
}

/// 100.64.0.0/10, used by some home routers and carriers behind a shared address.
fn is_shared_v4(ip: &Ipv4Addr) -> bool {
    let o = ip.octets();
    o[0] == 100 && (o[1] & 0xc0) == 64
}

fn is_v4_mapped(ip: &Ipv6Addr) -> bool {
    ip.to_ipv4_mapped().is_some()
}

/// Where the phone looks for a computer that is pairing: every other address of its own home network (the usual
/// 255.255.255.0 one). Nothing outside the local network.
pub fn neighbours(own: Ipv4Addr) -> Vec<Ipv4Addr> {
    if !local_network_addr(&IpAddr::V4(own)) {
        return vec![];
    }
    let [a, b, c, me] = own.octets();
    (1..=254u8)
        .filter(|d| *d != me)
        .map(|d| Ipv4Addr::new(a, b, c, d))
        .collect()
}

/// The other device's addresses as kept in settings: valid local-network ones, each once, at most [`MAX_ADDRS`].
pub fn clean_peers(addrs: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for a in addrs {
        if let Ok(ip) = a.trim().parse::<IpAddr>()
            && local_network_addr(&ip)
            && !out.contains(&ip.to_string())
        {
            out.push(ip.to_string());
        }
    }
    out.truncate(MAX_ADDRS);
    out
}

#[cfg(test)]
mod tests;
