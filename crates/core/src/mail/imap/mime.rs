//! Reads the few mail headers the digest shows: From, Subject, Content-Type.
//! Output is plain text with control characters removed and a hard length limit.

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};

const MAX_HEADER_CHARS: usize = 500;

/// Header name (lower-case) and its unfolded, decoded value. Only the first of each name is kept.
pub fn parse_headers(raw: &[u8]) -> Vec<(String, String)> {
    let text = String::from_utf8_lossy(raw);
    let mut fields: Vec<(String, String)> = Vec::new();
    for line in text.split('\n') {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            break; // end of the header block
        }
        if line.starts_with([' ', '\t']) {
            if let Some((_, v)) = fields.last_mut()
                && v.len() < 8 * MAX_HEADER_CHARS
            {
                v.push(' ');
                v.push_str(line.trim());
            }
        } else if let Some((name, value)) = line.split_once(':') {
            fields.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for (n, v) in fields {
        if !seen.contains(&n) {
            seen.push(n.clone());
            out.push((n, v));
        }
    }
    out
}

pub fn get<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.as_str())
}

/// Decodes RFC 2047 words (`=?UTF-8?B?...?=`) and cleans the result into one line of plain text.
pub fn decode_header(value: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    let mut after_word = false;
    while let Some(start) = rest.find("=?") {
        let (before, tail) = rest.split_at(start);
        match decode_word(tail) {
            Some((decoded, used)) => {
                // Whitespace between two encoded words is not part of the text.
                if !(after_word && before.trim().is_empty()) {
                    out.push_str(before);
                }
                out.push_str(&decoded);
                rest = &tail[used..];
                after_word = true;
            }
            None => {
                out.push_str(before);
                out.push_str("=?");
                rest = &tail[2..];
                after_word = false;
            }
        }
    }
    out.push_str(rest);
    clean(&out)
}

fn decode_word(s: &str) -> Option<(String, usize)> {
    // =?charset?encoding?text?=
    let body = s.strip_prefix("=?")?;
    let end = body.find("?=")?;
    let mut parts = body[..end].splitn(3, '?');
    let (charset, enc, text) = (parts.next()?, parts.next()?, parts.next()?);
    let bytes = match enc {
        "B" | "b" => STANDARD
            .decode(text)
            .or_else(|_| STANDARD_NO_PAD.decode(text.trim_end_matches('=')))
            .ok()?,
        "Q" | "q" => decode_q(text),
        _ => return None,
    };
    let charset = charset.split('*').next().unwrap_or("").to_ascii_lowercase();
    let decoded = match charset.as_str() {
        "iso-8859-1" | "latin1" | "windows-1252" | "cp1252" => {
            bytes.iter().map(|b| *b as char).collect()
        }
        _ => String::from_utf8_lossy(&bytes).into_owned(),
    };
    Some((decoded, 2 + end + 2))
}

fn decode_q(text: &str) -> Vec<u8> {
    let b = text.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'_' => out.push(b' '),
            b'=' => match (
                b.get(i + 1).and_then(|c| (*c as char).to_digit(16)),
                b.get(i + 2).and_then(|c| (*c as char).to_digit(16)),
            ) {
                (Some(h), Some(l)) => {
                    out.push((h * 16 + l) as u8);
                    i += 2;
                }
                _ => out.push(b'='),
            },
            c => out.push(c),
        }
        i += 1;
    }
    out
}

/// One line, no control characters, bounded length.
pub fn clean(s: &str) -> String {
    let mut out = String::new();
    let mut last_space = true;
    for c in s.chars() {
        let c = if c.is_control() || c == '\u{2028}' || c == '\u{2029}' {
            ' '
        } else {
            c
        };
        if c.is_whitespace() {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(c);
            last_space = false;
        }
        if out.chars().count() >= MAX_HEADER_CHARS {
            break;
        }
    }
    out.trim().to_string()
}
