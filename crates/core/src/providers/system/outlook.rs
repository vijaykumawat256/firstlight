//! Classic Outlook for Windows, read the way the Calendar app is read on a Mac: no sign-in, no setup.
//!
//! Outlook already holds every account the person added (Exchange, Microsoft 365, Outlook.com, Gmail, IMAP, .pst
//! files). We ask it, through its own automation interface (COM), for the calendars and for the events in a
//! window. The COM calls run in a short PowerShell script whose answer is JSON; this module builds the scripts,
//! parses the answer and maps it. Only the small [`ScriptRunner`] (start PowerShell, wait with a limit) lives in
//! the Windows shell, so everything here is tested on any computer with a fake runner.
//!
//! Read-only. The "new Outlook" (the web-based app) has no automation interface and is not supported; the
//! classic one is detected by its registration and, when absent, the Calendar app tile is simply not offered.

use super::{Access, SysCalendar, SysCalendarKind, SysEvent, SysResponse, SysStatus, SystemSource};
use crate::{Error, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;

/// Runs a PowerShell script and returns what it printed. Implemented by the Windows shell.
pub trait ScriptRunner: Send + Sync {
    /// Whether classic Outlook is installed (its COM registration exists). Must be cheap: it is asked often.
    fn installed(&self) -> bool;
    /// Runs the script hidden, with a time limit, and returns standard output.
    fn run(&self, script: &str) -> Result<String>;
}

pub struct OutlookSource<R: ScriptRunner> {
    runner: R,
}

impl<R: ScriptRunner> OutlookSource<R> {
    pub fn new(runner: R) -> Self {
        Self { runner }
    }
}

const PRELUDE: &str = "$ErrorActionPreference='Stop'\n\
[Console]::OutputEncoding=[System.Text.Encoding]::UTF8\n\
$ol=New-Object -ComObject Outlook.Application\n\
$ns=$ol.GetNamespace('MAPI')\n";

fn calendars_script() -> String {
    format!(
        "{PRELUDE}\
$out=New-Object System.Collections.ArrayList\n\
foreach($s in $ns.Stores){{\n\
  try{{\n\
    $f=$s.GetDefaultFolder(9)\n\
    [void]$out.Add([pscustomobject]@{{store=$s.StoreID;entry=$f.EntryID;title=[string]$f.Name;source=[string]$s.DisplayName;exchange=($s.ExchangeStoreType -lt 3)}})\n\
  }}catch{{}}\n\
}}\n\
ConvertTo-Json -InputObject @($out) -Compress\n"
    )
}

/// Ids from Outlook are hexadecimal. Anything else is refused before it can reach a script.
fn is_hex(s: &str) -> bool {
    !s.is_empty() && s.len() <= 512 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn events_script(store: &str, entry: &str, from: i64, to: i64) -> String {
    format!(
        "{PRELUDE}\
$f=$ns.GetFolderFromID('{entry}','{store}')\n\
$items=$f.Items\n\
$items.Sort('[Start]')\n\
$items.IncludeRecurrences=$true\n\
$from=[DateTimeOffset]::FromUnixTimeSeconds({from}).LocalDateTime\n\
$to=[DateTimeOffset]::FromUnixTimeSeconds({to}).LocalDateTime\n\
$ci=[System.Globalization.CultureInfo]::CurrentCulture\n\
$flt=\"[Start] < '\"+$to.ToString('g',$ci)+\"' AND [End] > '\"+$from.ToString('g',$ci)+\"'\"\n\
$epoch=[DateTime]::SpecifyKind([DateTime]'1970-01-01',[DateTimeKind]::Utc)\n\
$utc={{param($d) [long]([DateTime]::SpecifyKind($d,[DateTimeKind]::Utc)-$epoch).TotalSeconds}}\n\
$out=New-Object System.Collections.ArrayList\n\
$n=0\n\
foreach($a in $items.Restrict($flt)){{\n\
  $n++\n\
  if($n -gt 5000){{break}}\n\
  try{{\n\
    $org=''\n\
    try{{$org=[string]$a.Organizer}}catch{{}}\n\
    $body=''\n\
    try{{$body=[string]$a.Body; if($body.Length -gt 8000){{$body=$body.Substring(0,8000)}}}}catch{{}}\n\
    [void]$out.Add([pscustomobject]@{{\n\
      id=[string]$a.EntryID; uid=[string]$a.GlobalAppointmentID; title=[string]$a.Subject;\n\
      start=(& $utc $a.StartUTC); end=(& $utc $a.EndUTC); all_day=[bool]$a.AllDayEvent;\n\
      location=[string]$a.Location; notes=$body; organizer=$org;\n\
      busy=[int]$a.BusyStatus; meeting=[int]$a.MeetingStatus; response=[int]$a.ResponseStatus;\n\
      recurring=[bool]$a.IsRecurring; modified=(& $utc $a.LastModificationTime.ToUniversalTime())\n\
    }})\n\
  }}catch{{}}\n\
}}\n\
ConvertTo-Json -InputObject @($out) -Compress\n"
    )
}

#[derive(Deserialize)]
struct RawCalendar {
    store: String,
    entry: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    exchange: bool,
}

#[derive(Deserialize)]
struct RawEvent {
    #[serde(default)]
    id: String,
    #[serde(default)]
    uid: String,
    #[serde(default)]
    title: String,
    start: i64,
    end: i64,
    #[serde(default)]
    all_day: bool,
    #[serde(default)]
    location: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    organizer: String,
    #[serde(default)]
    busy: i32,
    #[serde(default)]
    meeting: i32,
    #[serde(default)]
    response: i32,
    #[serde(default)]
    recurring: bool,
    #[serde(default)]
    modified: i64,
}

/// PowerShell may print a banner or a warning before the JSON; the answer starts at the first line that opens an array.
fn json_line(out: &str) -> Option<&str> {
    let mut offset = 0;
    for line in out.split_inclusive('\n') {
        if line.trim_start().starts_with('[') {
            return Some(out[offset..].trim());
        }
        offset += line.len();
    }
    None
}

fn parse<T: for<'de> Deserialize<'de>>(out: &str) -> Result<Vec<T>> {
    let line = json_line(out)
        .ok_or_else(|| Error::Provider("Outlook gave an answer Firstlight couldn't read".into()))?;
    serde_json::from_str(line)
        .map_err(|_| Error::Provider("Outlook gave an answer Firstlight couldn't read".into()))
}

/// Outlook calendar ids are `<store id>:<folder id>`, both hexadecimal.
fn split_id(id: &str) -> Result<(&str, &str)> {
    id.split_once(':')
        .filter(|(s, e)| is_hex(s) && is_hex(e))
        .ok_or(Error::Invalid("That Outlook calendar isn't valid."))
}

fn map_event(r: RawEvent) -> SysEvent {
    let at = |t: i64| DateTime::<Utc>::from_timestamp(t, 0).unwrap_or_default();
    // MeetingStatus: 5 = the organiser cancelled it, 7 = a cancellation was received.
    let cancelled = matches!(r.meeting, 5 | 7);
    SysEvent {
        id: r.id,
        uid: Some(r.uid).filter(|u| !u.is_empty()),
        title: r.title,
        start: at(r.start),
        end: at(r.end),
        all_day: r.all_day,
        location: Some(r.location).filter(|s| !s.trim().is_empty()),
        notes: Some(r.notes).filter(|s| !s.trim().is_empty()),
        url: None,
        status: if cancelled {
            SysStatus::Canceled
        } else if r.busy == 1 {
            SysStatus::Tentative
        } else {
            SysStatus::Confirmed
        },
        // ResponseStatus: 0 none, 1 organised by me, 2 tentative, 3 accepted, 4 declined, 5 not yet answered.
        response: match r.response {
            2 => SysResponse::Tentative,
            3 => SysResponse::Accepted,
            4 => SysResponse::Declined,
            5 => SysResponse::Pending,
            _ => SysResponse::NotInvited,
        },
        organizer: Some(r.organizer).filter(|s| !s.is_empty()),
        is_recurring: r.recurring,
        tzid: None,
        modified: (r.modified > 0).then_some(r.modified),
    }
}

impl<R: ScriptRunner> SystemSource for OutlookSource<R> {
    fn name(&self) -> &'static str {
        "Outlook"
    }

    fn writable(&self) -> bool {
        false
    }

    fn access(&self) -> Access {
        // Outlook has no permission prompt: if it is installed and has a profile, it answers.
        if self.runner.installed() {
            Access::Granted
        } else {
            Access::Unsupported
        }
    }

    fn request_access(&self) -> Result<Access> {
        Ok(self.access())
    }

    fn calendars(&self) -> Result<Vec<SysCalendar>> {
        let out = self.runner.run(&calendars_script())?;
        Ok(parse::<RawCalendar>(&out)?
            .into_iter()
            .filter(|c| is_hex(&c.store) && is_hex(&c.entry))
            .map(|c| SysCalendar {
                id: format!("{}:{}", c.store, c.entry),
                title: if c.title.is_empty() {
                    "Calendar".into()
                } else {
                    c.title
                },
                source: c.source,
                kind: if c.exchange {
                    SysCalendarKind::Exchange
                } else {
                    SysCalendarKind::Local
                },
                // Reading only for now.
                read_only: true,
            })
            .collect())
    }

    fn events(
        &self,
        calendar_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<SysEvent>> {
        let (store, entry) = split_id(calendar_id)?;
        let out = self.runner.run(&events_script(
            store,
            entry,
            from.timestamp(),
            to.timestamp(),
        ))?;
        Ok(parse::<RawEvent>(&out)?
            .into_iter()
            .map(map_event)
            .collect())
    }
}

#[cfg(test)]
mod tests;
