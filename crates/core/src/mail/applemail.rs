//! Apple Mail, read through AppleScript.
//!
//! The AppleScript itself runs in the desktop shell (it needs the user's one-time Automation
//! permission). This module owns everything that can be checked without Mail: the script text,
//! parsing what it prints, and building the link that opens a message.
//!
//! Safety: the script only ever receives numbers and a directory path as arguments, never text from
//! a message, so there is nothing to inject into. It reads; it never changes, moves or sends mail.

use super::{OpenTarget, RawMessage, SourceBatch};
use chrono::DateTime;

/// How many days back the digest looks, and how many unread messages are fetched at most.
pub const DEFAULT_DAYS_BACK: i64 = 3;
pub const MAX_FETCH: usize = 40;

const RS: char = '\u{1e}';
const US: char = '\u{1f}';

/// Arguments: days back, maximum messages, and a directory for saving calendar attachments.
pub const SCRIPT: &str = r#"on run argv
  set daysBack to (item 1 of argv) as integer
  set maxCount to (item 2 of argv) as integer
  set outDir to item 3 of argv
  set RS to ASCII character 30
  set US to ASCII character 31
  if application "Mail" is not running then return "NOTRUNNING"
  set nowEpoch to (do shell script "date +%s") as integer
  set nowDate to current date
  tell application "Mail"
    set cutoff to nowDate - (daysBack * days)
    set totalUnread to unread count of inbox
    set out to "TOTAL" & US & (totalUnread as text) & RS
    set msgs to (messages of inbox whose read status is false and date received is greater than cutoff)
    set n to count of msgs
    if n > maxCount then set n to maxCount
    repeat with i from 1 to n
      try
        set m to item i of msgs
        set c to ""
        try
          set c to content of m
          if (length of c) > 400 then set c to text 1 thru 400 of c
        end try
        set epoch to nowEpoch + ((date received of m) - nowDate)
        set hasInvite to "0"
        set icsPath to ""
        try
          repeat with att in (mail attachments of m)
            set attName to name of att
            set attType to ""
            try
              set attType to MIME type of att
            end try
            if attType is "text/calendar" or attName ends with ".ics" then
              set hasInvite to "1"
              if icsPath is "" then
                try
                  set target to outDir & "/" & i & ".ics"
                  save att in (POSIX file target)
                  set icsPath to target
                end try
              end if
            end if
          end repeat
        end try
        set out to out & (message id of m) & US & (sender of m) & US & (subject of m) & US & (epoch as integer as text) & US & c & US & hasInvite & US & icsPath & RS
      end try
    end repeat
  end tell
  return out
end run"#;

/// The most-trivial script, used to trigger and check the permission without reading any mail.
pub const PROBE_SCRIPT: &str = r#"tell application "Mail" to return (count of accounts) as text"#;

#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// Mail is not running; nothing was read.
    NotRunning,
    Messages(ParsedMail),
}

#[derive(Debug, PartialEq)]
pub struct ParsedMail {
    pub unread_total: u32,
    pub rows: Vec<Row>,
}

#[derive(Debug)]
pub struct Row {
    pub message: RawMessage,
    /// Where the script saved a calendar attachment, to be read and deleted by the caller.
    pub ics_path: Option<String>,
}

impl PartialEq for Row {
    fn eq(&self, other: &Self) -> bool {
        self.message.id == other.message.id && self.ics_path == other.ics_path
    }
}

/// Mail ids look like `<abc@host>`; keep the part inside the brackets.
fn clean_id(raw: &str) -> String {
    raw.trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .to_string()
}

/// Ids are used inside a link, so accept only ids made of ordinary characters.
pub fn valid_message_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 512
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._%+-@$=!~*'()/:#&?,;".contains(&b))
}

/// The `message://` link that opens a message in Apple Mail, or `None` for an id we won't put in a link.
pub fn message_url(id: &str) -> Option<String> {
    if !valid_message_id(id) {
        return None;
    }
    let mut encoded = String::with_capacity(id.len() * 3);
    for b in id.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            encoded.push(b as char);
        } else {
            encoded.push_str(&format!("%{b:02X}"));
        }
    }
    Some(format!("message://%3C{encoded}%3E"))
}

/// Parses what [`SCRIPT`] prints. Malformed records are skipped, never fatal.
pub fn parse_output(out: &str) -> Result<Outcome, String> {
    let out = out.trim_matches(|c: char| c == '\n' || c == '\r');
    if out == "NOTRUNNING" {
        return Ok(Outcome::NotRunning);
    }
    let mut records = out.split(RS).filter(|r| !r.trim().is_empty());
    let header = records.next().ok_or("Mail returned nothing")?;
    let mut h = header.split(US);
    if h.next() != Some("TOTAL") {
        return Err("Mail returned an unexpected response".into());
    }
    let unread_total = h
        .next()
        .and_then(|n| n.trim().parse().ok())
        .ok_or("Mail returned an unexpected response")?;

    let mut rows = Vec::new();
    for rec in records {
        let f: Vec<&str> = rec.split(US).collect();
        if f.len() < 7 {
            continue;
        }
        let (Some(received), id) = (
            f[3].trim()
                .parse::<i64>()
                .ok()
                .and_then(|t| DateTime::from_timestamp(t, 0)),
            clean_id(f[0]),
        ) else {
            continue;
        };
        if !valid_message_id(&id) {
            continue;
        }
        let ics_path = (!f[6].trim().is_empty()).then(|| f[6].trim().to_string());
        rows.push(Row {
            message: RawMessage {
                id: id.clone(),
                sender: f[1].to_string(),
                subject: f[2].to_string(),
                received,
                snippet: f[4].to_string(),
                unread: true, // the script only asks for unread messages
                invitation_hint: f[5].trim() == "1",
                ics: None,
                open: OpenTarget::AppleMail { message_id: id },
            },
            ics_path,
        });
    }
    Ok(Outcome::Messages(ParsedMail { unread_total, rows }))
}

impl ParsedMail {
    /// Turns the parsed rows into a batch once the caller has read each attachment.
    pub fn into_batch(self, mut read_ics: impl FnMut(&str) -> Option<String>) -> SourceBatch {
        let messages = self
            .rows
            .into_iter()
            .map(|mut r| {
                if let Some(p) = &r.ics_path {
                    r.message.ics = read_ics(p);
                }
                r.message
            })
            .collect();
        SourceBatch {
            unread_total: self.unread_total,
            messages,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(fields: &[&str]) -> String {
        format!("{}{RS}", fields.join(&US.to_string()))
    }

    fn output(total: u32, records: &[String]) -> String {
        format!("TOTAL{US}{total}{RS}{}", records.concat())
    }

    #[test]
    fn parses_totals_and_messages() {
        let out = output(
            37,
            &[
                record(&[
                    "<abc123@mail.example.com>",
                    "Priya Shah <priya@x.com>",
                    "Agenda for Tuesday",
                    "1773144000",
                    "Hi all, attached is the agenda",
                    "0",
                    "",
                ]),
                record(&[
                    "def456@host",
                    "Calendar <invite@x.com>",
                    "Invitation: Design review",
                    "1773140400",
                    "You are invited",
                    "1",
                    "/tmp/dir/2.ics",
                ]),
            ],
        );
        let Outcome::Messages(p) = parse_output(&out).unwrap() else {
            panic!()
        };
        assert_eq!(
            p.unread_total, 37,
            "the total counts the whole inbox, not just the fetched messages"
        );
        assert_eq!(p.rows.len(), 2);
        assert_eq!(
            p.rows[0].message.id, "abc123@mail.example.com",
            "angle brackets are stripped"
        );
        assert_eq!(p.rows[0].message.subject, "Agenda for Tuesday");
        assert_eq!(p.rows[0].message.received.timestamp(), 1_773_144_000);
        assert!(!p.rows[0].message.invitation_hint);
        assert!(p.rows[1].message.invitation_hint);
        assert_eq!(p.rows[1].ics_path.as_deref(), Some("/tmp/dir/2.ics"));
        assert!(
            matches!(&p.rows[0].message.open, OpenTarget::AppleMail { message_id } if message_id == "abc123@mail.example.com")
        );
    }

    #[test]
    fn subjects_and_snippets_may_contain_newlines_and_tabs() {
        let out = output(
            1,
            &[record(&[
                "id1@h",
                "A <a@b.com>",
                "Line one\nline two",
                "1773144000",
                "snippet\twith\ttabs\nand lines",
                "0",
                "",
            ])],
        );
        let Outcome::Messages(p) = parse_output(&out).unwrap() else {
            panic!()
        };
        assert_eq!(p.rows.len(), 1);
        assert!(
            p.rows[0].message.subject.contains('\n'),
            "kept raw here; cleaning happens when the digest is built"
        );
    }

    #[test]
    fn mail_not_running_is_a_state_not_an_error() {
        assert_eq!(parse_output("NOTRUNNING\n").unwrap(), Outcome::NotRunning);
    }

    #[test]
    fn an_empty_inbox_parses() {
        let Outcome::Messages(p) = parse_output(&output(0, &[])).unwrap() else {
            panic!()
        };
        assert_eq!((p.unread_total, p.rows.len()), (0, 0));
    }

    #[test]
    fn malformed_records_are_skipped_not_fatal() {
        let out = output(
            3,
            &[
                record(&["ok@h", "A <a@b.com>", "fine", "1773144000", "s", "0", ""]),
                record(&["too", "few", "fields"]),
                record(&[
                    "bad-time@h",
                    "A <a@b.com>",
                    "x",
                    "not-a-number",
                    "s",
                    "0",
                    "",
                ]),
                record(&[
                    "has space@h",
                    "A <a@b.com>",
                    "unsafe id",
                    "1773144000",
                    "s",
                    "0",
                    "",
                ]),
                record(&[
                    "<a href=x>@h",
                    "A <a@b.com>",
                    "markup id",
                    "1773144000",
                    "s",
                    "0",
                    "",
                ]),
            ],
        );
        let Outcome::Messages(p) = parse_output(&out).unwrap() else {
            panic!()
        };
        assert_eq!(
            p.rows
                .iter()
                .map(|r| r.message.id.as_str())
                .collect::<Vec<_>>(),
            ["ok@h"]
        );
    }

    #[test]
    fn garbage_output_is_an_error_not_a_crash() {
        for bad in [
            "",
            "hello",
            "TOTAL",
            "TOTAL\u{1f}abc",
            "ERROR: not authorized",
        ] {
            assert!(parse_output(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn message_links_are_built_only_from_safe_ids_and_are_percent_encoded() {
        assert_eq!(
            message_url("abc@mail.example.com").as_deref(),
            Some("message://%3Cabc%40mail.example.com%3E")
        );
        assert_eq!(
            message_url("a+b=c/d").as_deref(),
            Some("message://%3Ca%2Bb%3Dc%2Fd%3E")
        );
        for bad in [
            "",
            "has space",
            "<script>",
            "a\nb",
            "javascript:alert(1)\"",
            &"x".repeat(600),
            "quote\"inside",
            "back\\slash",
        ] {
            assert_eq!(message_url(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn attachments_are_read_through_the_callback_and_only_for_rows_that_have_one() {
        let out = output(
            2,
            &[
                record(&[
                    "a@h",
                    "A <a@b.com>",
                    "Invitation: x",
                    "1773144000",
                    "s",
                    "1",
                    "/tmp/d/1.ics",
                ]),
                record(&["b@h", "B <b@b.com>", "plain", "1773144001", "s", "0", ""]),
            ],
        );
        let Outcome::Messages(p) = parse_output(&out).unwrap() else {
            panic!()
        };
        let mut asked = Vec::new();
        let batch = p.into_batch(|path| {
            asked.push(path.to_string());
            Some("BEGIN:VCALENDAR".into())
        });
        assert_eq!(asked, ["/tmp/d/1.ics"]);
        assert_eq!(batch.messages[0].ics.as_deref(), Some("BEGIN:VCALENDAR"));
        assert_eq!(batch.messages[1].ics, None);
        assert_eq!(batch.unread_total, 2);
    }

    #[test]
    fn the_script_reads_and_never_changes_mail() {
        for forbidden in [
            "delete ",
            "move ",
            "send ",
            "set read status",
            "set flagged",
            "make new",
            "reply",
            "forward",
            "mark as",
        ] {
            assert!(
                !SCRIPT.to_ascii_lowercase().contains(forbidden),
                "script contains {forbidden:?}"
            );
        }
        assert!(
            SCRIPT.contains("is not running"),
            "must not launch Mail by surprise"
        );
        assert!(
            !PROBE_SCRIPT.to_ascii_lowercase().contains("message"),
            "the probe reads no messages"
        );
    }
}
