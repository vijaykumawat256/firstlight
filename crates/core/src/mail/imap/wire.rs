//! The IMAP wire format, reduced to what a read-only digest needs.
//!
//! Everything a server sends is untrusted. Every length is capped before anything is allocated, and the
//! parser never indexes blindly, so a hostile or broken server can make a request fail but not hang,
//! exhaust memory or panic.

use crate::{Error, Result};
use std::io::{BufRead, BufReader, Read, Write};
use std::time::Instant;

/// A byte stream to the server: a TLS stream in production, a plain socket in tests.
pub trait Io: Read + Write + Send {}
impl<T: Read + Write + Send> Io for T {}

pub const MAX_LINE: usize = 8 * 1024;
pub const MAX_LITERAL: usize = 64 * 1024;
pub const MAX_RESPONSE: usize = 1024 * 1024;
const MAX_NESTING: usize = 8;
const MAX_TOKENS: usize = 50_000;

/// One piece of an untagged response: text, or a length-prefixed literal that follows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    Literal(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    Atom(String),
    Quoted(String),
    Literal(Vec<u8>),
    Open,
    Close,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Atom(String),
    Str(String),
    Bytes(Vec<u8>),
    List(Vec<Value>),
}

impl Value {
    pub fn text(&self) -> Option<String> {
        match self {
            Value::Atom(s) | Value::Str(s) => Some(s.clone()),
            Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
            Value::List(_) => None,
        }
    }
}

/// Splits text into tokens. Literals are substituted at their position.
pub fn tokenize(pieces: &[Piece]) -> Vec<Tok> {
    let mut out = Vec::new();
    for piece in pieces {
        match piece {
            Piece::Literal(b) => out.push(Tok::Literal(b.clone())),
            Piece::Text(t) => tokenize_text(t, &mut out),
        }
        if out.len() > MAX_TOKENS {
            out.truncate(MAX_TOKENS);
            break;
        }
    }
    out
}

fn tokenize_text(text: &str, out: &mut Vec<Tok>) {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            c if c.is_whitespace() => i += 1,
            '(' => {
                out.push(Tok::Open);
                i += 1;
            }
            ')' => {
                out.push(Tok::Close);
                i += 1;
            }
            '"' => {
                i += 1;
                let mut s = String::new();
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        i += 1;
                    }
                    s.push(chars[i]);
                    i += 1;
                }
                i += 1; // closing quote (or end of input)
                out.push(Tok::Quoted(s));
            }
            _ => {
                let mut s = String::new();
                while i < chars.len() {
                    let c = chars[i];
                    if c.is_whitespace() || c == '(' || c == ')' || c == '"' {
                        break;
                    }
                    s.push(c);
                    i += 1;
                    // A section such as BODY[HEADER.FIELDS (FROM SUBJECT)] is part of one atom.
                    if c == '[' {
                        while i < chars.len() && chars[i] != ']' {
                            s.push(chars[i]);
                            i += 1;
                        }
                        if i < chars.len() {
                            s.push(']');
                            i += 1;
                        }
                    }
                }
                out.push(Tok::Atom(s));
            }
        }
    }
}

/// Parses one value starting at `*pos`: an atom, string, literal or a (nested) parenthesised list.
pub fn parse_value(toks: &[Tok], pos: &mut usize, depth: usize) -> Option<Value> {
    if depth > MAX_NESTING {
        return None;
    }
    let t = toks.get(*pos)?;
    *pos += 1;
    Some(match t {
        Tok::Atom(a) => Value::Atom(a.clone()),
        Tok::Quoted(q) => Value::Str(q.clone()),
        Tok::Literal(b) => Value::Bytes(b.clone()),
        Tok::Close => return None,
        Tok::Open => {
            let mut items = Vec::new();
            loop {
                match toks.get(*pos) {
                    None => break,
                    Some(Tok::Close) => {
                        *pos += 1;
                        break;
                    }
                    Some(_) => items.push(parse_value(toks, pos, depth + 1)?),
                }
            }
            Value::List(items)
        }
    })
}

/// A finished command: the status word and the untagged lines that came before it.
#[derive(Debug)]
pub struct Reply {
    pub ok: bool,
    pub untagged: Vec<Vec<Piece>>,
}

pub struct Conn {
    io: BufReader<Box<dyn Io>>,
    deadline: Instant,
    tag: u32,
}

fn io_error(e: std::io::Error) -> Error {
    use std::io::ErrorKind::*;
    match e.kind() {
        TimedOut | WouldBlock | UnexpectedEof | ConnectionReset | ConnectionAborted
        | BrokenPipe | NotConnected => Error::Offline,
        _ => Error::Provider("the secure connection to the mail server failed".into()),
    }
}

fn protocol(msg: &str) -> Error {
    Error::Provider(format!("the mail server sent something unexpected ({msg})"))
}

impl Conn {
    pub fn new(io: Box<dyn Io>, deadline: Instant) -> Self {
        Self {
            io: BufReader::new(io),
            deadline,
            tag: 0,
        }
    }

    fn check_time(&self) -> Result<()> {
        if Instant::now() > self.deadline {
            Err(Error::Offline)
        } else {
            Ok(())
        }
    }

    fn read_line(&mut self) -> Result<String> {
        self.check_time()?;
        let mut buf = Vec::new();
        (&mut self.io)
            .take(MAX_LINE as u64 + 1)
            .read_until(b'\n', &mut buf)
            .map_err(io_error)?;
        if buf.is_empty() {
            return Err(Error::Offline);
        }
        if !buf.ends_with(b"\n") {
            return Err(protocol("line too long"));
        }
        while buf.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
            buf.pop();
        }
        Ok(String::from_utf8_lossy(&buf).into_owned())
    }

    /// The server's first line.
    pub fn greeting(&mut self) -> Result<()> {
        let line = self.read_line()?;
        if line.starts_with("* OK") {
            Ok(())
        } else {
            Err(protocol("no greeting"))
        }
    }

    /// Sends one command and reads until its tagged reply. `command` must already be safe to send.
    pub fn run(&mut self, command: &str) -> Result<Reply> {
        debug_assert!(!command.contains('\r') && !command.contains('\n'));
        self.tag += 1;
        let tag = format!("d{}", self.tag);
        self.check_time()?;
        let w = self.io.get_mut();
        w.write_all(format!("{tag} {command}\r\n").as_bytes())
            .and_then(|_| w.flush())
            .map_err(io_error)?;

        let mut untagged = Vec::new();
        let mut total = 0usize;
        loop {
            let line = self.read_line()?;
            total += line.len();
            if let Some(rest) = line.strip_prefix(&format!("{tag} ")) {
                let status = rest.split(' ').next().unwrap_or("");
                return Ok(Reply {
                    ok: status.eq_ignore_ascii_case("OK"),
                    untagged,
                });
            }
            if line.starts_with('+') {
                return Err(protocol("unexpected continuation"));
            }
            if !line.starts_with("* ") {
                continue; // a stray line; ignore it
            }
            if line.starts_with("* BYE") {
                return Err(Error::Offline);
            }
            let mut pieces = Vec::new();
            let mut current = line;
            loop {
                match literal_marker(&current) {
                    Some((start, n)) => {
                        if n > MAX_LITERAL {
                            return Err(protocol("message part too large"));
                        }
                        total += n;
                        if total > MAX_RESPONSE {
                            return Err(protocol("response too large"));
                        }
                        pieces.push(Piece::Text(current[..start].to_string()));
                        let mut data = vec![0u8; n];
                        self.check_time()?;
                        self.io.read_exact(&mut data).map_err(io_error)?;
                        pieces.push(Piece::Literal(data));
                        current = self.read_line()?;
                        total += current.len();
                        if total > MAX_RESPONSE {
                            return Err(protocol("response too large"));
                        }
                    }
                    None => {
                        pieces.push(Piece::Text(current));
                        break;
                    }
                }
            }
            untagged.push(pieces);
            if untagged.len() > 10_000 {
                return Err(protocol("too many lines"));
            }
        }
    }
}

/// If the line ends with `{123}`, the index where the marker starts and the length.
fn literal_marker(line: &str) -> Option<(usize, usize)> {
    let inner = line.strip_suffix('}')?;
    let open = inner.rfind('{')?;
    let digits = &inner[open + 1..];
    if digits.is_empty() || digits.len() > 9 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((open, digits.parse().ok()?))
}

/// Quotes a string for LOGIN. Only printable ASCII is accepted, so no literal is ever needed and
/// nothing can break out of the quotes into a second command.
pub fn quote(s: &str) -> Result<String> {
    if s.is_empty() || s.len() > 256 || !s.bytes().all(|b| (0x20..0x7f).contains(&b)) {
        return Err(Error::Invalid(
            "use only plain letters, numbers and symbols",
        ));
    }
    let mut out = String::from("\"");
    for c in s.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    Ok(out)
}
