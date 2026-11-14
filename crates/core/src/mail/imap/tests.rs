use super::mime::{clean, decode_header, parse_headers};
use super::wire::{Piece, Tok, parse_value, quote, tokenize};
use super::*;
use crate::secrets::MemorySecretStore;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::Mutex;

/// A scripted server on a loopback socket. `respond` gets each command (without its tag) and returns
/// the raw bytes to send before the tagged reply; `None` means "close the connection".
struct Fake {
    port: u16,
    seen: Arc<Mutex<Vec<String>>>,
}

fn serve(
    greeting: &'static str,
    respond: impl Fn(&str) -> Option<(Vec<u8>, &'static str)> + Send + 'static,
) -> Fake {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        let Ok((mut sock, _)) = listener.accept() else {
            return;
        };
        let _ = sock.write_all(greeting.as_bytes());
        let mut reader = BufReader::new(sock.try_clone().unwrap());
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                return;
            }
            let line = line.trim_end().to_string();
            let (tag, cmd) = line.split_once(' ').unwrap_or((&line, ""));
            log.lock().unwrap().push(cmd.to_string());
            match respond(cmd) {
                None => return,
                Some((body, status)) => {
                    let _ = sock.write_all(&body);
                    let _ = sock.write_all(format!("{tag} {status}\r\n").as_bytes());
                }
            }
        }
    });
    Fake { port, seen }
}

struct Local(u16);
impl Connect for Local {
    fn connect(&self, _: ImapPreset) -> Result<Box<dyn Io>> {
        let s = TcpStream::connect(("127.0.0.1", self.0)).map_err(|_| Error::Offline)?;
        s.set_read_timeout(Some(StdDuration::from_secs(5))).unwrap();
        Ok(Box::new(s))
    }
}

fn source(port: u16, password: Option<&str>) -> ImapSource {
    let secrets = MemorySecretStore::default();
    if let Some(p) = password {
        secrets.set(&secret_key("m1"), p).unwrap();
    }
    ImapSource::new(Arc::new(Local(port)), Arc::new(secrets))
}

fn mailbox() -> ImapMailbox {
    ImapMailbox {
        id: "m1".into(),
        preset: ImapPreset::Yahoo,
        address: "me@yahoo.com".into(),
    }
}

const GOOD_HEADER: &str = "From: =?UTF-8?B?w4lsw6g=?= <ele@example.com>\r\nSubject: =?utf-8?Q?Invitation:_Lunch_=E2=98=95?=\r\n\r\n";

fn good_server(extra_caps: &'static str) -> Fake {
    serve("* OK ready\r\n", move |cmd| {
        let c = cmd.to_ascii_uppercase();
        if c == "CAPABILITY" {
            return Some((
                format!("* CAPABILITY IMAP4rev1 {extra_caps}\r\n").into_bytes(),
                "OK done",
            ));
        }
        if c.starts_with("ID ") || c.starts_with("LOGIN ") || c.starts_with("EXAMINE") {
            return Some((vec![], "OK done"));
        }
        if c.starts_with("STATUS") {
            return Some((b"* STATUS \"INBOX\" (UNSEEN 12)\r\n".to_vec(), "OK done"));
        }
        if c.starts_with("UID SEARCH") {
            return Some((b"* SEARCH 4 9\r\n* 40 EXISTS\r\n".to_vec(), "OK done"));
        }
        if c.starts_with("UID FETCH") {
            let mut b = Vec::new();
            for (uid, date) in [
                (4, "07-Nov-2026 08:00:00 +0000"),
                (9, "08-Nov-2026 09:30:00 -0500"),
            ] {
                b.extend_from_slice(
                    format!(
                        "* {uid} FETCH (UID {uid} FLAGS () INTERNALDATE \"{date}\" BODY[HEADER.FIELDS (FROM SUBJECT CONTENT-TYPE)] {{{}}}\r\n{GOOD_HEADER})\r\n",
                        GOOD_HEADER.len()
                    )
                    .as_bytes(),
                );
            }
            return Some((b, "OK done"));
        }
        if c == "LOGOUT" {
            return Some((b"* BYE\r\n".to_vec(), "OK done"));
        }
        Some((vec![], "BAD unknown"))
    })
}

#[test]
fn reads_unread_mail_read_only() {
    let fake = good_server("ID");
    let now = "2026-11-09T12:00:00Z".parse().unwrap();
    let batch = source(fake.port, Some("app-pass"))
        .fetch(&mailbox(), 3, 10, now)
        .unwrap();
    assert_eq!(batch.unread_total, 12);
    assert_eq!(batch.messages.len(), 2);
    let m = &batch.messages[0];
    assert_eq!(m.id, "imap:m1:9"); // newest first
    assert_eq!(m.sender, "Élè <ele@example.com>");
    assert_eq!(m.subject, "Invitation: Lunch ☕");
    assert!(m.invitation_hint);
    assert!(m.snippet.is_empty() && m.unread && m.ics.is_none());
    assert_eq!(
        m.received,
        "2026-11-08T14:30:00Z".parse::<DateTime<Utc>>().unwrap()
    );
    assert!(matches!(&m.open, OpenTarget::Url { url } if url == "https://mail.yahoo.com/"));

    let sent = fake.seen.lock().unwrap().join("\n").to_ascii_uppercase();
    assert!(sent.contains("EXAMINE"));
    assert!(sent.contains("BODY.PEEK[HEADER.FIELDS"));
    assert!(sent.contains("SINCE 06-NOV-2026"));
    assert!(sent.contains("ID (\"NAME\" \"DAYBRIEF\""));
    // Nothing that writes, selects read-write, or fetches a body.
    for forbidden in [
        "SELECT",
        "STORE",
        "APPEND",
        "COPY",
        "MOVE",
        "DELETE",
        "EXPUNGE",
        "CREATE",
        "RENAME",
        "BODY[",
        "RFC822",
        "BODY.PEEK[TEXT",
        "BODY.PEEK[]",
    ] {
        assert!(!sent.contains(forbidden), "sent {forbidden}: {sent}");
    }
}

#[test]
fn id_is_only_sent_when_the_server_offers_it() {
    let fake = good_server("");
    source(fake.port, Some("p"))
        .fetch(&mailbox(), 3, 10, Utc::now())
        .unwrap();
    assert!(
        !fake
            .seen
            .lock()
            .unwrap()
            .iter()
            .any(|c| c.starts_with("ID "))
    );
}

#[test]
fn a_refused_login_means_reconnect_and_does_not_echo_anything() {
    let fake = serve("* OK hi\r\n", |cmd| {
        if cmd.starts_with("LOGIN") {
            Some((
                vec![],
                "NO [AUTHENTICATIONFAILED] bad me@yahoo.com app-pass",
            ))
        } else {
            Some((b"* CAPABILITY IMAP4rev1\r\n".to_vec(), "OK"))
        }
    });
    let err = source(fake.port, Some("app-pass"))
        .fetch(&mailbox(), 3, 10, Utc::now())
        .unwrap_err();
    assert!(matches!(err, Error::AuthRevoked));
    assert!(!format!("{err} {err:?}").contains("app-pass"));
}

#[test]
fn no_stored_password_means_reconnect_without_touching_the_network() {
    let err = source(1, None)
        .fetch(&mailbox(), 3, 10, Utc::now())
        .unwrap_err();
    assert!(matches!(err, Error::AuthRevoked));
}

#[test]
fn cannot_reach_the_server_means_offline() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    drop(l);
    let err = source(port, Some("p"))
        .fetch(&mailbox(), 3, 10, Utc::now())
        .unwrap_err();
    assert!(matches!(err, Error::Offline));
}

#[test]
fn a_server_that_hangs_up_is_offline_not_a_crash() {
    let fake = serve("* OK hi\r\n", |_| None);
    let err = source(fake.port, Some("p"))
        .fetch(&mailbox(), 3, 10, Utc::now())
        .unwrap_err();
    assert!(matches!(err, Error::Offline));
}

#[test]
fn refuses_servers_that_disable_login() {
    let fake = serve("* OK hi\r\n", |cmd| {
        if cmd == "CAPABILITY" {
            Some((b"* CAPABILITY IMAP4rev1 LOGINDISABLED\r\n".to_vec(), "OK"))
        } else {
            Some((vec![], "OK"))
        }
    });
    let err = source(fake.port, Some("p"))
        .fetch(&mailbox(), 3, 10, Utc::now())
        .unwrap_err();
    assert!(matches!(err, Error::Provider(_)));
    assert!(
        !fake
            .seen
            .lock()
            .unwrap()
            .iter()
            .any(|c| c.starts_with("LOGIN"))
    );
}

#[test]
fn hostile_responses_fail_cleanly() {
    // A literal far bigger than allowed, an endless line, a continuation request, a bad greeting.
    let cases: Vec<(&'static str, Vec<u8>)> = vec![
        (
            "* OK hi\r\n",
            b"* 1 FETCH (BODY[HEADER] {999999999}\r\n".to_vec(),
        ),
        ("* OK hi\r\n", vec![b'x'; 20_000]),
        ("* OK hi\r\n", b"+ send me more\r\n".to_vec()),
        ("garbage\r\n", vec![]),
        ("* PREAUTH hi\r\n", vec![]),
    ];
    for (greeting, reply) in cases {
        let reply2 = reply.clone();
        let fake = serve(greeting, move |_| Some((reply2.clone(), "OK")));
        let r = source(fake.port, Some("p")).fetch(&mailbox(), 3, 10, Utc::now());
        assert!(r.is_err(), "should have failed for {reply:?}");
    }
}

#[test]
fn credentials_cannot_inject_commands() {
    assert_eq!(quote("a\"b\\c").unwrap(), r#""a\"b\\c""#);
    for bad in ["a\r\nb", "a\nb", "a\0b", "pässword", ""] {
        assert!(quote(bad).is_err(), "{bad:?}");
    }
    assert!(quote(&"x".repeat(300)).is_err());
    assert!(valid_address("me@yahoo.com"));
    assert!(
        !valid_address("me @yahoo.com") && !valid_address("a\r\nb@x.com") && !valid_address("")
    );
}

#[test]
fn presets_are_fixed_hosts_and_unknown_names_are_refused() {
    assert_eq!(ImapPreset::Yahoo.host(), "imap.mail.yahoo.com");
    assert_eq!(ImapPreset::Icloud.host(), "imap.mail.me.com");
    assert_eq!(ImapPreset::parse("yahoo"), Some(ImapPreset::Yahoo));
    assert_eq!(ImapPreset::parse("evil.example.com"), None);
    assert!(serde_json::from_str::<ImapPreset>("\"evil.example.com\"").is_err());
}

#[test]
fn tokenizer_handles_sections_quotes_and_literals() {
    let pieces = vec![
        Piece::Text("* 1 FETCH (UID 7 BODY[HEADER.FIELDS (FROM SUBJECT)] ".into()),
        Piece::Literal(b"From: a\r\n".to_vec()),
        Piece::Text(" X \"q\\\"uote\")".into()),
    ];
    let toks = tokenize(&pieces);
    assert!(toks.contains(&Tok::Atom("BODY[HEADER.FIELDS (FROM SUBJECT)]".into())));
    assert!(toks.contains(&Tok::Literal(b"From: a\r\n".to_vec())));
    assert!(toks.contains(&Tok::Quoted("q\"uote".into())));
    let mut pos = 3;
    assert!(parse_value(&toks, &mut pos, 0).is_some());
}

#[test]
fn nesting_is_bounded() {
    let deep = "(".repeat(5000);
    let toks = tokenize(&[Piece::Text(deep)]);
    let mut pos = 0;
    assert!(parse_value(&toks, &mut pos, 0).is_none());
}

#[test]
fn encoded_words_and_folding() {
    assert_eq!(
        decode_header("=?UTF-8?B?SGVsbG8=?= =?UTF-8?Q?_World?="),
        "Hello World"
    );
    assert_eq!(decode_header("=?ISO-8859-1?Q?caf=E9?="), "café");
    assert_eq!(decode_header("=?UTF-8?B?SGk?="), "Hi"); // missing padding
    assert_eq!(decode_header("plain =?broken"), "plain =?broken");
    assert_eq!(decode_header("a\u{0007}b\r\n\tc"), "a b c");
    assert_eq!(clean(&"y".repeat(5000)).len(), 500);
    let h =
        parse_headers(b"Subject: one\r\n two\r\nFrom: a@b.c\r\nSubject: second\r\n\r\nBody: no");
    assert_eq!(h.len(), 2);
    assert_eq!(h[0], ("subject".into(), "one two".into()));
}

#[test]
fn garbage_never_panics() {
    let mut x: u64 = 0x2545F4914F6CDD1D;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    const BITS: &[&str] = &[
        "(",
        ")",
        "\"",
        "\\",
        "{5}",
        "{99999999999}",
        "[",
        "]",
        "=?",
        "?=",
        "B?",
        "Q?",
        "=",
        "=É",
        "UID",
        "FETCH",
        "*",
        " ",
        "\r\n",
        "é",
        "💥",
        "BODY[HEADER",
        "INTERNALDATE",
        "12",
    ];
    for _ in 0..3000 {
        let n = (next() % 40) as usize;
        let s: String = (0..n)
            .map(|_| BITS[(next() % BITS.len() as u64) as usize])
            .collect();
        let _ = decode_header(&s);
        let _ = clean(&s);
        let _ = parse_headers(s.as_bytes());
        let pieces = vec![
            Piece::Text(s.clone()),
            Piece::Literal(s.clone().into_bytes()),
            Piece::Text(s),
        ];
        let _ = super::parse_fetch(&pieces);
        let toks = tokenize(&pieces);
        let mut pos = 0;
        let _ = parse_value(&toks, &mut pos, 0);
    }
}

#[test]
fn stale_dates_and_formats() {
    assert_eq!(
        imap_date("2026-01-05T00:00:00Z".parse().unwrap()),
        "05-Jan-2026"
    );
    assert!(parse_internal_date(" 7-Jul-1996 02:44:25 -0700").is_some());
    assert!(parse_internal_date("nonsense").is_none());
}

#[test]
fn check_login_signs_in_and_out() {
    let fake = good_server("");
    source(fake.port, None)
        .check_login(ImapPreset::Yahoo, "me@yahoo.com", "p")
        .unwrap();
    let seen = fake.seen.lock().unwrap().clone();
    assert!(seen.iter().any(|c| c.starts_with("LOGIN")));
    assert!(seen.iter().any(|c| c == "LOGOUT"));
    assert!(!seen.iter().any(|c| c.starts_with("EXAMINE")));
}

#[test]
fn unused_read_import_guard() {
    // Keeps `Read` in scope for the socket helpers above on all platforms.
    let mut empty: &[u8] = &[];
    let mut b = [0u8; 1];
    assert_eq!(empty.read(&mut b).unwrap(), 0);
}

/// Live check, no credentials: TLS to the real servers must verify and greet us.
/// Run: `cargo test -p daybrief-core live_imap -- --ignored --nocapture`
#[test]
#[ignore]
fn live_imap_greeting_over_verified_tls() {
    for preset in [ImapPreset::Yahoo, ImapPreset::Icloud] {
        let io = TlsConnect.connect(preset).expect("tls connect");
        let mut conn = wire::Conn::new(io, Instant::now() + StdDuration::from_secs(20));
        conn.greeting().expect("greeting");
        let caps = conn.run("CAPABILITY").expect("capability");
        let text: Vec<String> = caps
            .untagged
            .iter()
            .flat_map(|p| p.iter())
            .filter_map(|p| match p {
                Piece::Text(t) => Some(t.clone()),
                _ => None,
            })
            .collect();
        println!("{}: {}", preset.host(), text.join(" | "));
        let _ = conn.run("LOGOUT");
    }
}
