use super::*;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::sync::Mutex;

struct Fake {
    port: u16,
    seen: Arc<Mutex<Vec<String>>>,
    data: Arc<Mutex<String>>,
}

/// A scripted SMTP server on a loopback socket. `reply` gets each command line and returns the raw reply.
fn serve(reply: impl Fn(&str) -> String + Send + 'static) -> Fake {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let data = Arc::new(Mutex::new(String::new()));
    let (log, dlog) = (seen.clone(), data.clone());
    std::thread::spawn(move || {
        let Ok((mut sock, _)) = listener.accept() else {
            return;
        };
        let _ = sock.write_all(b"220 ready\r\n");
        let mut r = BufReader::new(sock.try_clone().unwrap());
        loop {
            let mut line = String::new();
            if r.read_line(&mut line).unwrap_or(0) == 0 {
                return;
            }
            let line = line.trim_end().to_string();
            log.lock().unwrap().push(line.clone());
            let out = reply(&line);
            if line == "DATA" && out.starts_with("354") {
                let _ = sock.write_all(out.as_bytes());
                let mut body = String::new();
                loop {
                    let mut l = String::new();
                    if r.read_line(&mut l).unwrap_or(0) == 0 {
                        return;
                    }
                    if l == ".\r\n" {
                        break;
                    }
                    body.push_str(&l);
                }
                *dlog.lock().unwrap() = body;
                let _ = sock.write_all(b"250 queued\r\n");
                continue;
            }
            if out.is_empty() {
                return;
            }
            let _ = sock.write_all(out.as_bytes());
        }
    });
    Fake { port, seen, data }
}

struct Local(u16);
impl SmtpTransport for Local {
    fn open(&self, _: &SmtpServer) -> Result<Box<dyn Io>> {
        let s = TcpStream::connect(("127.0.0.1", self.0)).map_err(|_| Error::Offline)?;
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        Ok(Box::new(s))
    }
    fn upgrade(&self, io: Box<dyn Io>, _: &SmtpServer) -> Result<Box<dyn Io>> {
        Ok(io) // a real transport would start TLS here
    }
}

const IMPLICIT: SmtpServer = SmtpServer {
    host: "smtp.mail.yahoo.com",
    port: 465,
    starttls: false,
};
const STARTTLS: SmtpServer = SmtpServer {
    host: "smtp.mail.me.com",
    port: 587,
    starttls: true,
};

fn happy(starttls_offered: bool) -> impl Fn(&str) -> String + Send + 'static {
    move |cmd| {
        let c = cmd.to_ascii_uppercase();
        if c.starts_with("EHLO") {
            let tls = if starttls_offered {
                "250-STARTTLS\r\n"
            } else {
                ""
            };
            format!("250-hello\r\n{tls}250 AUTH PLAIN LOGIN\r\n")
        } else if c == "STARTTLS" {
            "220 go ahead\r\n".into()
        } else if c.starts_with("AUTH PLAIN") {
            "235 ok\r\n".into()
        } else if c.starts_with("MAIL FROM") || c.starts_with("RCPT TO") {
            "250 ok\r\n".into()
        } else if c == "DATA" {
            "354 go\r\n".into()
        } else if c == "QUIT" {
            "221 bye\r\n".into()
        } else {
            "500 what\r\n".into()
        }
    }
}

fn to() -> Vec<String> {
    vec!["sam@example.com".into(), "Pat@Example.org".into()]
}

fn out<'a>(to: &'a [String], subject: &'a str) -> Outgoing<'a> {
    Outgoing {
        from: "me@yahoo.com",
        to,
        subject,
        text: "Lunch\n\nWhen: Friday\n.hidden dot line\n",
        ics: Some("BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nEND:VCALENDAR\r\n"),
        method: "REQUEST",
        now: DateTime::from_timestamp(1_800_000_000, 0).unwrap(),
    }
}

#[test]
fn a_message_goes_out_in_the_expected_order_with_the_login_sent_once() {
    let f = serve(happy(false));
    let to = to();
    send(
        &Local(f.port),
        &IMPLICIT,
        "me@yahoo.com",
        "app-pass-123",
        &out(&to, "Invitation: Lunch"),
    )
    .unwrap();
    std::thread::sleep(Duration::from_millis(150)); // QUIT is sent after we return
    let seen = f.seen.lock().unwrap().clone();
    let cmds: Vec<&str> = seen.iter().map(|l| l.split(' ').next().unwrap()).collect();
    assert_eq!(
        cmds,
        ["EHLO", "AUTH", "MAIL", "RCPT", "RCPT", "DATA", "QUIT"]
    );
    assert_eq!(
        seen[1],
        format!(
            "AUTH PLAIN {}",
            STANDARD.encode("\0me@yahoo.com\0app-pass-123")
        )
    );
    assert_eq!(seen[2], "MAIL FROM:<me@yahoo.com>");
    assert_eq!(
        seen[4], "RCPT TO:<pat@example.org>",
        "addresses are lower-cased"
    );
    let data = f.data.lock().unwrap().clone();
    assert!(data.contains("Subject: Invitation: Lunch\r\n"));
    assert!(data.contains("To: sam@example.com, pat@example.org\r\n"));
    assert!(!data.to_ascii_lowercase().contains("bcc:"));
    assert!(
        !data.contains("app-pass-123"),
        "the password is never in the message"
    );
}

#[test]
fn icloud_style_servers_are_upgraded_before_the_login() {
    let f = serve(happy(true));
    let to = to();
    send(
        &Local(f.port),
        &STARTTLS,
        "me@icloud.com",
        "pw",
        &out(&to, "x"),
    )
    .unwrap();
    std::thread::sleep(Duration::from_millis(150)); // QUIT is sent after we return
    let seen = f.seen.lock().unwrap().clone();
    let cmds: Vec<&str> = seen.iter().map(|l| l.split(' ').next().unwrap()).collect();
    assert_eq!(
        cmds,
        [
            "EHLO", "STARTTLS", "EHLO", "AUTH", "MAIL", "RCPT", "RCPT", "DATA", "QUIT"
        ]
    );
}

#[test]
fn without_starttls_nothing_secret_is_sent() {
    let f = serve(happy(false)); // does not advertise STARTTLS
    let to = to();
    let e = send(
        &Local(f.port),
        &STARTTLS,
        "me@icloud.com",
        "pw",
        &out(&to, "x"),
    )
    .unwrap_err();
    assert!(matches!(e, Error::Provider(_)));
    let seen = f.seen.lock().unwrap().clone();
    assert!(seen.iter().all(|l| !l.starts_with("AUTH")), "{seen:?}");
}

#[test]
fn a_refused_login_means_reconnect() {
    let f = serve(|cmd| {
        if cmd.starts_with("EHLO") {
            "250 AUTH PLAIN\r\n".into()
        } else {
            "535 bad credentials\r\n".into()
        }
    });
    let to = to();
    let e = send(
        &Local(f.port),
        &IMPLICIT,
        "me@yahoo.com",
        "wrong",
        &out(&to, "x"),
    )
    .unwrap_err();
    assert!(matches!(e, Error::AuthRevoked));
    assert!(!format!("{e}").contains("wrong"));
}

#[test]
fn a_refused_recipient_stops_before_any_message_is_sent() {
    let f = serve(|cmd| {
        let c = cmd.to_ascii_uppercase();
        if c.starts_with("EHLO") {
            "250 AUTH PLAIN\r\n".into()
        } else if c.starts_with("AUTH") {
            "235 ok\r\n".into()
        } else if c.starts_with("MAIL") {
            "250 ok\r\n".into()
        } else {
            "550 no such user\r\n".into()
        }
    });
    let to = to();
    let e = send(
        &Local(f.port),
        &IMPLICIT,
        "me@yahoo.com",
        "pw",
        &out(&to, "x"),
    )
    .unwrap_err();
    assert!(matches!(e, Error::Provider(_)));
    assert!(
        !format!("{e}").contains("example"),
        "no address in the error"
    );
    assert!(f.seen.lock().unwrap().iter().all(|l| l != "DATA"));
    assert!(f.data.lock().unwrap().is_empty());
}

#[test]
fn a_server_with_no_sign_in_method_we_know_is_refused() {
    let f = serve(|cmd| {
        if cmd.starts_with("EHLO") {
            "250 AUTH XOAUTH2\r\n".into()
        } else {
            "500\r\n".into()
        }
    });
    let to = to();
    assert!(
        send(
            &Local(f.port),
            &IMPLICIT,
            "me@yahoo.com",
            "pw",
            &out(&to, "x")
        )
        .is_err()
    );
}

#[test]
fn a_hostile_server_cannot_make_us_read_forever() {
    let long = format!("250 {}\r\n", "A".repeat(5000));
    let f = serve(move |_| long.clone());
    let to = to();
    assert!(
        send(
            &Local(f.port),
            &IMPLICIT,
            "me@yahoo.com",
            "pw",
            &out(&to, "x")
        )
        .is_err()
    );
}

#[test]
fn nothing_from_the_page_can_add_a_header_or_a_hidden_recipient() {
    let now = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
    let bad = [
        "a@b.com\r\nBcc: evil@x.com",
        "a@b.com>\r\nRCPT TO:<evil@x.com",
        "a b@c.com",
        "<a@b.com>",
        "\"a\"@b.com",
        "a@b",
        "",
    ];
    for b in bad {
        let to = vec![b.to_string()];
        let o = Outgoing {
            to: &to,
            ..out(&to, "x")
        };
        assert!(build_message(&o).is_err(), "{b:?}");
    }
    let to = vec!["a@b.com".to_string()];
    let o = Outgoing {
        from: "me@yahoo.com\r\nBcc: x@y.com",
        ..out(&to, "x")
    };
    assert!(build_message(&o).is_err());
    // A subject with a line break and non-ASCII text stays one header line and is encoded.
    let o = Outgoing {
        subject: "Lunch ☕\r\nBcc: evil@x.com",
        now,
        ..out(&to, "x")
    };
    let m = build_message(&o).unwrap();
    let header_block = m.split("\r\n\r\n").next().unwrap();
    assert!(!header_block.to_ascii_lowercase().contains("\r\nbcc:"));
    assert!(header_block.contains("Subject: =?UTF-8?B?"));
    assert!(m.is_ascii(), "everything on the wire is 7-bit");
}

#[test]
fn the_recipient_count_is_limited() {
    let many: Vec<String> = (0..21).map(|i| format!("p{i}@example.com")).collect();
    assert!(build_message(&out(&many, "x")).is_err());
    assert!(build_message(&out(&[], "x")).is_err());
}

#[test]
fn the_text_and_the_calendar_part_sit_side_by_side_and_the_file_is_attached_too() {
    let to = vec!["a@b.com".to_string()];
    let m = build_message(&out(&to, "x")).unwrap();
    let boundaries: Vec<String> = m
        .split("boundary=\"")
        .skip(1)
        .map(|r| r.split('"').next().unwrap().to_string())
        .collect();
    assert_eq!(boundaries.len(), 2, "mixed outside, alternative inside");
    let dec = |p: &str| {
        let body = p.split_once("\r\n\r\n").unwrap().1.replace("\r\n", "");
        String::from_utf8(STANDARD.decode(body.trim_end_matches('-')).unwrap()).unwrap()
    };
    let ics = "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nEND:VCALENDAR\r\n";
    let inner: Vec<&str> = m.split(&format!("--{}", boundaries[1])).collect();
    assert!(inner[1].contains("Content-Type: text/plain"));
    assert_eq!(
        dec(inner[1]),
        "Lunch\r\n\r\nWhen: Friday\r\n.hidden dot line\r\n"
    );
    assert!(inner[2].contains("Content-Type: text/calendar; charset=utf-8; method=REQUEST"));
    assert_eq!(dec(inner[2]), ics);
    // After the alternative: the same file as an attachment.
    let attachment = m.rsplit(&format!("--{}", boundaries[0])).nth(1).unwrap();
    assert!(attachment.contains("filename=\"invite.ics\""));
    assert_eq!(dec(attachment), ics);
    assert!(m.lines().all(|l| l.len() <= 998));
}

#[test]
fn only_an_invitation_or_a_cancellation_can_be_sent() {
    let to = vec!["a@b.com".to_string()];
    for method in ["PUBLISH", "REPLY", "request", "REQUEST\r\nBcc: x@y.com", ""] {
        let o = Outgoing {
            method,
            ..out(&to, "x")
        };
        assert!(build_message(&o).is_err(), "{method:?}");
    }
    let o = Outgoing {
        method: "CANCEL",
        ..out(&to, "x")
    };
    assert!(build_message(&o).unwrap().contains("method=CANCEL"));
}

#[test]
fn lines_that_start_with_a_dot_are_doubled_so_they_cannot_end_the_message() {
    let b = data_block("a\r\n.\r\n..x\r\nb\r\n");
    assert_eq!(b, "a\r\n..\r\n...x\r\nb\r\n.\r\n");
}

#[test]
fn only_yahoo_and_icloud_have_a_server_and_the_transport_refuses_others() {
    assert!(server_for(ImapPreset::Yahoo).is_some() && server_for(ImapPreset::Icloud).is_some());
    assert!(server_for(ImapPreset::Custom).is_none());
    let evil = SmtpServer {
        host: "evil.example.com",
        port: 465,
        starttls: false,
    };
    assert!(TlsTransport::default().open(&evil).is_err());
}

/// Talks to the real servers up to (not including) the login: the greeting, the encrypted connection and,
/// for iCloud, that STARTTLS is offered and accepted. `cargo test -p firstlight-core live_smtp -- --ignored --nocapture`
#[test]
#[ignore = "needs the network"]
fn live_smtp_greeting_and_starttls() {
    for preset in [ImapPreset::Yahoo, ImapPreset::Icloud] {
        let server = server_for(preset).unwrap();
        let t = TlsTransport::default();
        let mut s = Session {
            io: t.open(&server).unwrap(),
            started: Instant::now(),
        };
        let greeting = s.reply().unwrap();
        assert_eq!(greeting.code, 220, "{}", server.host);
        let caps = s.command("EHLO firstlight").unwrap();
        assert_eq!(caps.code, 250);
        if server.starttls {
            assert!(
                caps.lines
                    .iter()
                    .any(|l| l.eq_ignore_ascii_case("STARTTLS")),
                "{:?}",
                caps.lines
            );
            assert_eq!(s.command("STARTTLS").unwrap().code, 220);
            s.io = t.upgrade(s.io, &server).unwrap();
            caps_after_tls(&mut s);
        } else {
            assert!(
                caps.lines
                    .iter()
                    .any(|l| l.to_ascii_uppercase().starts_with("AUTH")),
                "{:?}",
                caps.lines
            );
        }
        let _ = s.send("QUIT");
        println!("{}: ok", server.host);
    }
}

fn caps_after_tls(s: &mut Session) {
    let caps = s.command("EHLO firstlight").unwrap();
    assert_eq!(caps.code, 250);
    assert!(
        caps.lines
            .iter()
            .any(|l| l.to_ascii_uppercase().starts_with("AUTH")),
        "{:?}",
        caps.lines
    );
}

#[test]
fn a_plain_note_is_one_text_part_with_no_calendar_file() {
    let to = vec!["sam@example.com".to_string()];
    let msg = build_message(&Outgoing {
        ics: None,
        method: "",
        ..out(&to, "Running late: Café chat")
    })
    .unwrap();
    assert!(msg.contains("Content-Type: text/plain; charset=utf-8"));
    assert!(!msg.contains("multipart"));
    assert!(!msg.contains("text/calendar"));
    assert!(msg.contains("Subject: =?UTF-8?B?"));
    assert!(msg.contains("To: sam@example.com"));
}
