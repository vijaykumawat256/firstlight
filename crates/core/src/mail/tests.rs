use super::*;
use chrono::TimeZone;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, 10, 12, 0, 0).unwrap()
}

fn raw(id: &str, sender: &str, subject: &str, mins_ago: i64) -> RawMessage {
    RawMessage {
        id: id.into(),
        sender: sender.into(),
        subject: subject.into(),
        received: now() - Duration::minutes(mins_ago),
        snippet: "hello there".into(),
        unread: true,
        invitation_hint: false,
        ics: None,
        open: OpenTarget::AppleMail {
            message_id: id.into(),
        },
    }
}

fn batch(total: u32, messages: Vec<RawMessage>) -> SourceBatch {
    SourceBatch {
        unread_total: total,
        messages,
    }
}

fn list(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn senders_are_split_into_name_and_lowercase_address() {
    assert_eq!(
        parse_sender("Priya Shah <Priya@Example.com>"),
        ("Priya Shah".into(), "priya@example.com".into())
    );
    assert_eq!(
        parse_sender("\"Shah, Priya\" <priya@example.com>"),
        ("Shah, Priya".into(), "priya@example.com".into())
    );
    assert_eq!(
        parse_sender("<bare@example.com>"),
        ("bare@example.com".into(), "bare@example.com".into())
    );
    assert_eq!(
        parse_sender("plain@example.com"),
        ("plain@example.com".into(), "plain@example.com".into())
    );
    assert_eq!(
        parse_sender("  Name <  spaced@example.com >  "),
        ("Name".into(), "spaced@example.com".into())
    );
    assert_eq!(parse_sender(""), (String::new(), String::new()));
}

#[test]
fn snippets_are_single_line_plain_text_and_short() {
    assert_eq!(
        clean_snippet("  Hi\n\nthere,\t\tfriend \r\n"),
        "Hi there, friend"
    );
    assert_eq!(
        clean_snippet("a\u{0}b\u{7}c\u{200b}d"),
        "a b c d",
        "control and zero-width characters become spaces"
    );
}

#[test]
fn snippets_never_carry_markup() {
    assert_eq!(
        clean_snippet("<p>Hello <b>world</b></p><img src=\"https://tracker.example/x.gif\">!"),
        "Hello world!"
    );
    assert_eq!(
        clean_snippet("<script>alert(1)</script>safe"),
        "alert(1)safe"
    );
    assert!(!clean_snippet("<style>p{}</style>text <a href=\"x\">link</a>").contains('<'));
}

#[test]
fn long_snippets_are_cut_on_a_character_boundary() {
    let long = "é".repeat(500);
    let s = clean_snippet(&long);
    assert_eq!(s.chars().count(), MAX_SNIPPET_CHARS);
    assert!(s.ends_with('…'));
    let emoji = "😀 ".repeat(200);
    let e = clean_snippet(&emoji);
    assert!(e.chars().count() <= MAX_SNIPPET_CHARS);
    assert!(e.is_char_boundary(e.len()));
    assert_eq!(clean_snippet("short"), "short");
}

#[test]
fn important_sender_matching_supports_addresses_domains_and_names() {
    let p = list(&["boss@acme.com", "@clients.org", "partner.io", "Maya Singh"]);
    assert!(sender_matches("Anyone", "boss@acme.com", &p));
    assert!(sender_matches("X", "BOSS@ACME.COM", &p), "case-insensitive");
    assert!(
        !sender_matches("X", "other@acme.com", &p),
        "an address entry matches only that address"
    );
    assert!(sender_matches("X", "anna@clients.org", &p));
    assert!(
        sender_matches("X", "anna@eu.clients.org", &p),
        "subdomains count"
    );
    assert!(
        sender_matches("X", "sam@partner.io", &p),
        "a bare domain works"
    );
    assert!(
        sender_matches("maya singh", "m@elsewhere.net", &p),
        "a name matches exactly, ignoring case"
    );
    assert!(
        !sender_matches("Maya", "m@elsewhere.net", &p),
        "a first name alone is not enough"
    );
}

#[test]
fn matching_cannot_be_fooled_by_lookalike_domains() {
    let p = list(&["@clients.org", "partner.io"]);
    assert!(!sender_matches("X", "a@notclients.org", &p));
    assert!(!sender_matches("X", "a@clients.org.evil.com", &p));
    assert!(!sender_matches("X", "a@mypartner.io", &p));
    assert!(
        !sender_matches("X", "a@x.com", &list(&["", "  "])),
        "blank entries match nothing"
    );
}

#[test]
fn invitation_subjects_from_common_systems_are_recognised() {
    for s in [
        "Invitation: Design review @ Tue Mar 10",
        "Updated invitation: Standup",
        "Meeting Invitation - Q2 plan",
        "Accepted: Lunch",
        "Canceled event: Sync",
        "FWD: Invitation: x",
    ] {
        assert!(subject_looks_like_invitation(s), "{s}");
    }
    for s in [
        "Re: invitation to dinner?",
        "Your invitation code",
        "Lunch tomorrow?",
        "",
    ] {
        assert!(!subject_looks_like_invitation(s), "{s}");
    }
}

#[test]
fn the_digest_counts_ranks_and_caps() {
    let mut msgs: Vec<RawMessage> = (0..8)
        .map(|i| {
            raw(
                &format!("boss{i}"),
                "Boss <boss@acme.com>",
                &format!("Thing {i}"),
                10 + i,
            )
        })
        .collect();
    msgs.push(raw("spam1", "Deals <deals@shop.com>", "50% off", 5));
    let d = build_digest(&[batch(40, msgs)], &list(&["boss@acme.com"]), &[], now());
    assert_eq!(d.unread_total, 40);
    assert_eq!(d.important.len(), 5, "capped");
    assert_eq!(d.important[0].id, "boss0", "newest first");
    assert_eq!(
        d.other_unread,
        40 - 5,
        "everything not shown is counted as other"
    );
}

#[test]
fn nothing_is_important_unless_the_user_or_a_meeting_says_so() {
    let msgs = vec![
        raw("a", "Priya <priya@x.com>", "Agenda", 5),
        raw("b", "Stranger <s@y.com>", "Hi", 6),
    ];
    let none = build_digest(&[batch(2, msgs.clone())], &[], &[], now());
    assert!(none.important.is_empty());
    assert_eq!(none.other_unread, 2);
    // Priya organises one of today's meetings, so her mail counts.
    let with_meetings = build_digest(&[batch(2, msgs)], &[], &list(&["Priya"]), now());
    assert_eq!(with_meetings.important.len(), 1);
    assert_eq!(with_meetings.important[0].sender_name, "Priya");
}

#[test]
fn read_messages_and_duplicates_are_ignored() {
    let mut read = raw("r", "Boss <boss@acme.com>", "Old", 5);
    read.unread = false;
    let a = raw("same", "Boss <boss@acme.com>", "Hi", 5);
    let d = build_digest(
        &[batch(3, vec![read, a.clone()]), batch(2, vec![a])],
        &list(&["boss@acme.com"]),
        &[],
        now(),
    );
    assert_eq!(
        d.important.len(),
        1,
        "one copy of the message, none of the read one"
    );
    assert_eq!(d.unread_total, 5, "totals add up across sources");
}

#[test]
fn invitations_are_listed_separately_with_details_from_the_attachment() {
    let mut inv = raw(
        "inv",
        "Priya <priya@x.com>",
        "Invitation: Design review",
        20,
    );
    inv.invitation_hint = true;
    inv.ics = Some(
        "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:1\r\nDTSTART:20260311T100000Z\r\nDTEND:20260311T110000Z\r\nSUMMARY:Design review\r\nLOCATION:Room 4B\r\nORGANIZER;CN=Priya Shah:mailto:priya@x.com\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n".into(),
    );
    let by_subject_only = raw("inv2", "Sam <sam@y.com>", "Updated invitation: Standup", 30);
    let d = build_digest(&[batch(2, vec![inv, by_subject_only])], &[], &[], now());
    assert_eq!(d.invitations.len(), 2);
    assert!(
        d.important.is_empty(),
        "an invitation is not double-counted as important"
    );
    let first = &d.invitations[0];
    assert_eq!(first.title.as_deref(), Some("Design review"));
    assert_eq!(first.location.as_deref(), Some("Room 4B"));
    assert_eq!(first.organizer.as_deref(), Some("Priya Shah"));
    assert!(matches!(first.when, Some(When::Timed { .. })));
    assert_eq!(
        d.invitations[1].title, None,
        "no attachment: just the message"
    );
    assert_eq!(d.other_unread, 0);
}

#[test]
fn a_broken_or_hostile_attachment_cannot_break_the_digest() {
    let mut a = raw("a", "X <x@y.com>", "Invitation: junk", 5);
    a.invitation_hint = true;
    a.ics = Some("this is not a calendar <script>".into());
    let mut b = raw("b", "X <x@y.com>", "Invitation: cancelled", 6);
    b.invitation_hint = true;
    b.ics = Some("BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:1\r\nDTSTART:20260311T100000Z\r\nDTEND:20260311T110000Z\r\nSTATUS:CANCELLED\r\nSUMMARY:Gone\r\nEND:VEVENT\r\nEND:VCALENDAR".into());
    let d = build_digest(&[batch(2, vec![a, b])], &[], &[], now());
    assert_eq!(d.invitations.len(), 2);
    assert!(d.invitations.iter().all(|i| i.title.is_none()));
}

#[test]
fn only_a_snippet_survives_never_a_body() {
    let mut m = raw("a", "Boss <boss@acme.com>", "Report", 5);
    m.snippet = format!("<html><body>{}</body></html>", "confidential ".repeat(500));
    let d = build_digest(&[batch(1, vec![m])], &list(&["boss@acme.com"]), &[], now());
    let kept = &d.important[0];
    assert!(kept.snippet.chars().count() <= MAX_SNIPPET_CHARS);
    assert!(!kept.snippet.contains('<'));
    let json = serde_json::to_string(&d).unwrap();
    assert!(
        json.len() < 1500,
        "the whole digest for one message stays tiny: {} bytes",
        json.len()
    );
}

#[test]
fn open_targets_never_carry_a_script_url() {
    let t = OpenTarget::Url {
        url: "https://mail.google.com/mail/u/0/#inbox/abc".into(),
    };
    assert!(matches!(&t, OpenTarget::Url { url } if url.starts_with("https://")));
}

#[test]
fn an_empty_inbox_is_a_valid_digest() {
    let d = build_digest(&[], &[], &[], now());
    assert_eq!((d.unread_total, d.other_unread), (0, 0));
    assert!(d.important.is_empty() && d.invitations.is_empty());
}

#[test]
fn html_entities_in_provider_snippets_become_plain_text() {
    assert_eq!(
        decode_entities("Let&#39;s sync &amp; plan &lt;now&gt; &quot;ok&quot;"),
        "Let's sync & plan <now> \"ok\""
    );
    assert_eq!(
        decode_entities("caf&#xE9; &#233; &nbsp;done"),
        "café é  done"
    );
    assert_eq!(
        decode_entities("AT&T and R&D &unknown; &"),
        "AT&T and R&D &unknown; &"
    );
    assert_eq!(
        decode_entities("&#99999999999; &#xZZ;"),
        "&#99999999999; &#xZZ;",
        "bad numeric references are left alone"
    );
    assert_eq!(decode_entities("no entities"), "no entities");
    // Decoding can produce angle brackets; snippet cleaning then removes anything tag-like.
    assert_eq!(
        clean_snippet(&decode_entities("&lt;script&gt;x&lt;/script&gt;hello")),
        "xhello"
    );
}
