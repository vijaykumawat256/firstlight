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

/// How a script ends, whatever happens: either `FLJSON:<base64 of the JSON>` on success or `FLERR:<base64 of the
/// error text>` on failure. Base64 keeps the answer safe from the console's code page, and the marker keeps it
/// apart from any banner or warning PowerShell prints. The error text comes from Outlook or Windows (for example a
/// COM error code), never from the calendar's contents.
pub(crate) const PRELUDE: &str = "$ErrorActionPreference='Stop'\n\
try{\n\
$ol=New-Object -ComObject Outlook.Application\n\
$ns=$ol.GetNamespace('MAPI')\n";

const EPILOGUE: &str = "$json=ConvertTo-Json -InputObject @($out) -Compress\n\
Write-Output ('FLJSON:'+[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($json)))\n\
}catch{\n\
$m=[string]$_.Exception.Message\n\
Write-Output ('FLERR:'+[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($m)))\n\
exit 1\n\
}\n";

const CALENDARS: &str = "$out=New-Object System.Collections.ArrayList\n\
$seen=@{}\n\
$errs=New-Object System.Collections.ArrayList\n\
$add={param($f,$x)\n\
  $eid=[string]$f.EntryID\n\
  if($seen.ContainsKey($eid)){return}\n\
  $seen[$eid]=$true\n\
  $src=''\n\
  try{$src=[string]$f.Store.DisplayName}catch{}\n\
  [void]$out.Add([pscustomobject]@{store=[string]$f.StoreID;entry=$eid;title=[string]$f.Name;source=$src;exchange=$x})\n\
}\n\
$stores=0\n\
foreach($s in $ns.Stores){\n\
  $stores++\n\
  try{\n\
    $f=$s.GetDefaultFolder(9)\n\
    $x=$false\n\
    try{$x=([int]$s.ExchangeStoreType -lt 3)}catch{}\n\
    & $add $f $x\n\
  }catch{[void]$errs.Add([string]$_.Exception.Message)}\n\
}\n\
try{& $add ($ns.GetDefaultFolder(9)) $false}catch{[void]$errs.Add([string]$_.Exception.Message)}\n\
if($out.Count -eq 0){\n\
  $why=''\n\
  if($errs.Count -gt 0){$why=' '+[string]$errs[0]}\n\
  throw ('No calendar folder found in '+$stores+' Outlook data file(s).'+$why)\n\
}\n";

// Outlook guards some fields against outside programs (the notes, the organiser, recipients, anything through the
// property accessor): asking for one makes it wait on a security prompt and the whole read stalls until the time
// limit (seen on a real PC, 2026-11-15). So the script asks for none of them: Outlook events have no notes and no
// organiser. A test keeps them out.
const EVENTS: &str = "$f=$ns.GetFolderFromID('@ENTRY@','@STORE@')\n\
$items=$f.Items\n\
$items.Sort('[Start]')\n\
$items.IncludeRecurrences=$true\n\
$from=[DateTimeOffset]::FromUnixTimeSeconds(@FROM@).LocalDateTime\n\
$to=[DateTimeOffset]::FromUnixTimeSeconds(@TO@).LocalDateTime\n\
$ci=[System.Globalization.CultureInfo]::CurrentCulture\n\
$flt='[Start] < ''' + $to.ToString('g',$ci) + ''' AND [End] > ''' + $from.ToString('g',$ci) + ''''\n\
$epoch=[DateTime]::SpecifyKind([DateTime]'1970-01-01',[DateTimeKind]::Utc)\n\
$utc={param($d) [long]([DateTime]::SpecifyKind($d,[DateTimeKind]::Utc)-$epoch).TotalSeconds}\n\
$out=New-Object System.Collections.ArrayList\n\
$n=0\n\
foreach($a in $items.Restrict($flt)){\n\
  $n++\n\
  if($n -gt 5000){break}\n\
  try{\n\
    [void]$out.Add([pscustomobject]@{\n\
      id=[string]$a.EntryID; uid=[string]$a.GlobalAppointmentID; title=[string]$a.Subject;\n\
      start=(& $utc $a.StartUTC); end=(& $utc $a.EndUTC); all_day=[bool]$a.AllDayEvent;\n\
      location=[string]$a.Location;\n\
      busy=[int]$a.BusyStatus; meeting=[int]$a.MeetingStatus; response=[int]$a.ResponseStatus;\n\
      recurring=[bool]$a.IsRecurring; modified=(& $utc $a.LastModificationTime.ToUniversalTime())\n\
    })\n\
  }catch{}\n\
}\n";

fn calendars_script() -> String {
    format!("{PRELUDE}{CALENDARS}{EPILOGUE}")
}

/// Ids from Outlook are hexadecimal. Anything else is refused before it can reach a script.
fn is_hex(s: &str) -> bool {
    !s.is_empty() && s.len() <= 4096 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// `store` and `entry` must already have passed [`is_hex`]; the two numbers are written as numbers.
fn events_script(store: &str, entry: &str, from: i64, to: i64) -> String {
    let body = EVENTS
        .replace("@ENTRY@", entry)
        .replace("@STORE@", store)
        .replace("@FROM@", &from.to_string())
        .replace("@TO@", &to.to_string());
    format!("{PRELUDE}{body}{EPILOGUE}")
}

/// A script as PowerShell's `-EncodedCommand` wants it: UTF-16 little-endian, base64. Passing it this way runs it as
/// one piece (reading a script from standard input runs it line by line, which behaves differently when a line
/// fails) and puts no script text where a shell could reinterpret it.
pub fn encode_command(script: &str) -> String {
    use base64::Engine;
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn unreadable() -> Error {
    Error::Provider("Outlook gave an answer Firstlight couldn't read".into())
}

/// One short, plain line of what Outlook or Windows said, safe to show: control characters out, cut short.
fn tidy_error(raw: &str) -> String {
    let one: String = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let one = one.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() <= 300 {
        return one;
    }
    format!("{}…", one.chars().take(299).collect::<String>().trim_end())
}

/// What a failure means for the person, with what Windows said after it.
pub fn explain_failure(raw: &str) -> Error {
    let said = tidy_error(raw);
    let lower = said.to_ascii_lowercase();
    let hint = if lower.contains("80040154")
        || lower.contains("class not registered")
        || lower.contains("regdb_e_classnotreg")
    {
        "Classic Outlook isn't installed on this computer. (The new Outlook can't be read this way: use Sign in with Microsoft instead.)"
    } else if lower.contains("80080005")
        || lower.contains("server execution failed")
        || lower.contains("co_e_server_exec_failure")
    {
        "Outlook wouldn't start for Firstlight. Open Outlook yourself once and wait for it to finish loading, then try again. If you run Outlook as administrator, run Firstlight the same way."
    } else if lower.contains("mapi")
        || lower.contains("profile")
        || lower.contains("operation failed")
        || lower.contains("80040")
    {
        "Outlook isn't ready: it may be waiting for you to choose a profile or finish setting up an account. Open Outlook, complete anything it asks, then try again."
    } else {
        "Outlook couldn't be read."
    };
    Error::Provider(format!("{hint} (Windows said: {said})"))
}

/// Finds the answer in what the script printed: the JSON it produced, or the failure it reported.
pub(crate) fn read_answer(out: &str) -> Result<String> {
    use base64::Engine;
    let decode = |b64: &str| {
        base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
    };
    for line in out.lines().map(str::trim) {
        if let Some(b64) = line.strip_prefix("FLERR:") {
            return Err(explain_failure(&decode(b64).unwrap_or_default()));
        }
        if let Some(b64) = line.strip_prefix("FLJSON:") {
            return decode(b64).ok_or_else(unreadable);
        }
    }
    Err(unreadable())
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

fn parse<T: for<'de> Deserialize<'de>>(out: &str) -> Result<Vec<T>> {
    serde_json::from_str(&read_answer(out)?).map_err(|_| unreadable())
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
        // Outlook keeps addresses in a form that needs Exchange to resolve; the guest list is left out for now.
        attendees: vec![],
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
        let raw = parse::<RawCalendar>(&out)?;
        if let Some(c) = raw.first()
            && !raw.iter().any(|c| is_hex(&c.store) && is_hex(&c.entry))
        {
            // Says what was odd without showing the ids themselves.
            return Err(Error::Provider(format!(
                "Outlook listed {} calendar(s), but their ids were not in the form Firstlight expects (lengths {} and {})",
                raw.len(),
                c.store.len(),
                c.entry.len()
            )));
        }
        Ok(raw
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

#[cfg(test)]
mod live;
