//! Classic Outlook's inbox on Windows, read the same way as its calendars (`providers::system::outlook`): no
//! sign-in, through Outlook's automation interface, in a short PowerShell script whose answer is JSON.
//!
//! What is read: the inbox's unread count, and for unread messages from the last few days the subject, the time it
//! arrived and whether it is a meeting request (with the meeting's time and place). What is NOT read: the sender,
//! the text and the recipients. Outlook guards those against outside programs, and asking for one makes it wait on
//! a security prompt until the time limit (seen with the calendar's organiser on a real PC). So Outlook mail adds the
//! unread count and invitations to the digest, but cannot match "people who matter". A test keeps guarded fields out.
//!
//! Read-only: nothing is marked read, moved or changed. Messages can't be opened from the briefing (classic Outlook
//! has no address for a message that is safe to hand to the system), so their rows aren't clickable.

use super::{OpenTarget, RawMessage, SourceBatch};
use crate::providers::system::outlook::{PRELUDE, ScriptRunner, read_answer};
use crate::{Error, Result};
use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use serde::Deserialize;

/// Most messages read per refresh (newest first).
pub const MAX_FETCH: usize = 50;

const INBOX: &str = "$f=$ns.GetDefaultFolder(6)\n\
$unread=[int]$f.UnReadItemCount\n\
$items=$f.Items\n\
$items.Sort('[ReceivedTime]',$true)\n\
$since=[DateTimeOffset]::FromUnixTimeSeconds(@SINCE@).LocalDateTime\n\
$ci=[System.Globalization.CultureInfo]::CurrentCulture\n\
$flt='[UnRead] = True AND [ReceivedTime] >= ''' + $since.ToString('g',$ci) + ''''\n\
$epoch=[DateTime]::SpecifyKind([DateTime]'1970-01-01',[DateTimeKind]::Utc)\n\
$utc={param($d) [long]([DateTime]::SpecifyKind($d,[DateTimeKind]::Utc)-$epoch).TotalSeconds}\n\
$list=New-Object System.Collections.ArrayList\n\
$n=0\n\
foreach($m in $items.Restrict($flt)){\n\
  $n++\n\
  if($n -gt @MAX@){break}\n\
  try{\n\
    $cls=[string]$m.MessageClass\n\
    $o=[pscustomobject]@{id=[string]$m.EntryID; subject=[string]$m.Subject; received=(& $utc $m.ReceivedTime.ToUniversalTime()); cls=$cls; start=$null; end=$null; all_day=$false; location=''}\n\
    if($cls -like 'IPM.Schedule.Meeting.Request*'){\n\
      try{\n\
        $a=$m.GetAssociatedAppointment($false)\n\
        if($a){$o.start=(& $utc $a.StartUTC); $o.end=(& $utc $a.EndUTC); $o.all_day=[bool]$a.AllDayEvent; $o.location=[string]$a.Location}\n\
      }catch{}\n\
    }\n\
    [void]$list.Add($o)\n\
  }catch{}\n\
}\n\
$json=ConvertTo-Json -InputObject ([pscustomobject]@{unread=$unread; messages=@($list)}) -Depth 4 -Compress\n\
Write-Output ('FLJSON:'+[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($json)))\n\
}catch{\n\
$m=[string]$_.Exception.Message\n\
Write-Output ('FLERR:'+[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($m)))\n\
exit 1\n\
}\n";

/// The script for unread mail since `since` (seconds), at most `max` messages. Only numbers go into it.
pub fn script(since: i64, max: usize) -> String {
    let body = INBOX
        .replace("@SINCE@", &since.to_string())
        .replace("@MAX@", &max.to_string());
    format!("{PRELUDE}{body}")
}

#[derive(Debug, Deserialize)]
struct Answer {
    #[serde(default)]
    unread: i64,
    #[serde(default)]
    messages: Vec<Msg>,
}

#[derive(Debug, Deserialize)]
struct Msg {
    id: String,
    #[serde(default)]
    subject: Option<String>,
    received: i64,
    #[serde(default)]
    cls: Option<String>,
    #[serde(default)]
    start: Option<i64>,
    #[serde(default)]
    end: Option<i64>,
    #[serde(default)]
    all_day: bool,
    #[serde(default)]
    location: Option<String>,
}

/// A meeting request's details as a tiny calendar file, so the digest reads it with the same, tested parser as an
/// invitation attachment from any other source.
fn ics_for(subject: &str, m: &Msg, tz: Tz) -> Option<String> {
    let (start, end) = (
        DateTime::from_timestamp(m.start?, 0)?,
        DateTime::from_timestamp(m.end?, 0)?,
    );
    if end < start {
        return None;
    }
    let (dtstart, dtend) = if m.all_day {
        let day = |d: DateTime<Utc>| d.with_timezone(&tz).format("%Y%m%d").to_string();
        (
            format!("DTSTART;VALUE=DATE:{}", day(start)),
            format!("DTEND;VALUE=DATE:{}", day(end)),
        )
    } else {
        let at = |d: DateTime<Utc>| d.format("%Y%m%dT%H%M%SZ").to_string();
        (
            format!("DTSTART:{}", at(start)),
            format!("DTEND:{}", at(end)),
        )
    };
    let location = m
        .location
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| format!("LOCATION:{}\r\n", crate::invite::escape_text(l)))
        .unwrap_or_default();
    Some(format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:outlook-invitation\r\n{dtstart}\r\n{dtend}\r\nSUMMARY:{}\r\n{location}END:VEVENT\r\nEND:VCALENDAR\r\n",
        crate::invite::escape_text(subject)
    ))
}

/// Reads the answer of [`script`].
pub fn parse(out: &str, tz: Tz) -> Result<SourceBatch> {
    let json = read_answer(out)?;
    let answer: Answer = serde_json::from_str(&json)
        .map_err(|_| Error::Provider("Outlook gave an answer Firstlight couldn't read".into()))?;
    let messages = answer
        .messages
        .iter()
        .take(MAX_FETCH)
        .filter_map(|m| {
            let received = DateTime::from_timestamp(m.received, 0)?;
            let subject: String = m
                .subject
                .as_deref()
                .unwrap_or("")
                .chars()
                .filter(|c| !c.is_control())
                .take(300)
                .collect();
            let invitation = m
                .cls
                .as_deref()
                .is_some_and(|c| c.starts_with("IPM.Schedule.Meeting.Request"));
            Some(RawMessage {
                id: format!("outlook:{}", m.id.chars().take(512).collect::<String>()),
                sender: String::new(),
                ics: invitation.then(|| ics_for(&subject, m, tz)).flatten(),
                subject,
                received,
                snippet: String::new(),
                unread: true,
                invitation_hint: invitation,
                open: OpenTarget::None,
            })
        })
        .collect();
    Ok(SourceBatch {
        unread_total: answer.unread.clamp(0, i64::from(u32::MAX)) as u32,
        messages,
    })
}

/// Unread mail from the last `days` days, through `runner`.
pub fn fetch<R: ScriptRunner + ?Sized>(
    runner: &R,
    days: i64,
    now: DateTime<Utc>,
    tz: Tz,
) -> Result<SourceBatch> {
    if !runner.installed() {
        return Err(Error::Provider(
            "Classic Outlook isn't installed on this computer.".into(),
        ));
    }
    let since = (now - Duration::days(days.clamp(1, 30))).timestamp();
    parse(&runner.run(&script(since, MAX_FETCH))?, tz)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use chrono::TimeZone;

    const NY: Tz = chrono_tz::America::New_York;

    fn answer(json: &str) -> String {
        format!(
            "WARNING: some banner\r\nFLJSON:{}\r\n",
            base64::engine::general_purpose::STANDARD.encode(json)
        )
    }

    #[test]
    fn the_script_reads_nothing_outlook_guards() {
        let s = script(1_700_000_000, 50);
        // Every property or method used on a message ($m) or its meeting ($a) is on this list of unguarded ones.
        let allowed = [
            "EntryID",
            "Subject",
            "ReceivedTime",
            "MessageClass",
            "GetAssociatedAppointment",
            "StartUTC",
            "EndUTC",
            "AllDayEvent",
            "Location",
            "ToUniversalTime",
        ];
        for var in ["$m.", "$a."] {
            for part in s.split(var).skip(1) {
                let name: String = part
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect();
                assert!(
                    allowed.contains(&name.as_str()),
                    "the script reads {var}{name}, which isn't on the list"
                );
            }
        }
        let lower = s.to_ascii_lowercase();
        for write in [
            ".save(",
            ".delete(",
            ".move(",
            "unread=$false",
            "unread = $false",
            ".send(",
            ".respond(",
        ] {
            assert!(!lower.contains(write), "the script must not {write}");
        }
        assert!(lower.contains("getdefaultfolder(6)"), "the inbox");
        assert!(lower.contains("1700000000") && lower.contains("-gt 50"));
    }

    #[test]
    fn unread_mail_and_meeting_requests_come_through_without_senders() {
        let start = Utc
            .with_ymd_and_hms(2026, 11, 23, 15, 0, 0)
            .unwrap()
            .timestamp();
        let out = answer(&format!(
            r#"{{"unread":42,"messages":[
              {{"id":"00AA","subject":"Lunch, Monday?","received":1795000000,"cls":"IPM.Note"}},
              {{"id":"00BB","subject":"Design review","received":1795000100,"cls":"IPM.Schedule.Meeting.Request","start":{start},"end":{},"all_day":false,"location":"Room 4; 2nd floor"}},
              {{"id":"00CC","subject":"Offsite","received":1795000200,"cls":"IPM.Schedule.Meeting.Request","start":{},"end":{},"all_day":true,"location":""}}
            ]}}"#,
            start + 3600,
            Utc.with_ymd_and_hms(2026, 12, 1, 5, 0, 0)
                .unwrap()
                .timestamp(),
            Utc.with_ymd_and_hms(2026, 12, 2, 5, 0, 0)
                .unwrap()
                .timestamp(),
        ));
        let b = parse(&out, NY).unwrap();
        assert_eq!(b.unread_total, 42);
        assert_eq!(b.messages.len(), 3);
        let note = &b.messages[0];
        assert_eq!(
            (note.id.as_str(), note.sender.as_str(), note.invitation_hint),
            ("outlook:00AA", "", false)
        );
        assert!(note.ics.is_none() && note.open == OpenTarget::None);

        // The digest turns the meeting requests into invitations with their time and place.
        let now = Utc.with_ymd_and_hms(2026, 11, 21, 12, 0, 0).unwrap();
        let d = super::super::build_digest(&[b], &[], &[], now);
        assert_eq!(d.invitations.len(), 2);
        let review = d
            .invitations
            .iter()
            .find(|i| i.message.subject == "Design review")
            .unwrap();
        assert_eq!(review.title.as_deref(), Some("Design review"));
        assert_eq!(review.location.as_deref(), Some("Room 4; 2nd floor"));
        assert!(review.when.is_some());
        let offsite = d
            .invitations
            .iter()
            .find(|i| i.message.subject == "Offsite")
            .unwrap();
        assert!(offsite.when.is_some(), "an all-day meeting keeps its day");
        assert_eq!(d.unread_total, 42);
        assert!(
            d.important.is_empty(),
            "no senders, so no one can match as important"
        );
    }

    #[test]
    fn a_failure_says_what_outlook_said() {
        let out = format!(
            "FLERR:{}\n",
            base64::engine::general_purpose::STANDARD
                .encode("Server execution failed (0x80080005)")
        );
        let e = parse(&out, NY).unwrap_err().to_string();
        assert!(e.contains("wouldn't start"), "{e}");
        assert!(parse("nothing useful", NY).is_err());
    }
}
