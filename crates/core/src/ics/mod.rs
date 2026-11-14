//! iCalendar (RFC 5545) parsing and recurrence expansion.
//!
//! Handles the parts of the format real calendars actually use: line folding, escaped text,
//! UTC / TZID / floating / all-day times, `DURATION`, `RRULE` / `RDATE` / `EXDATE`, and
//! `RECURRENCE-ID` overrides (moved or cancelled single occurrences). Malformed events are
//! skipped rather than failing the whole feed.

use crate::model::*;
use crate::providers::Window;
use crate::{Error, Result, join, tz};
use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, Utc};
use chrono_tz::Tz;
use std::collections::{HashMap, HashSet};

mod parse;
use parse::{Prop, parse_lines};

/// Hard cap on expanded instances per recurring event within one window.
const MAX_INSTANCES: u16 = 1000;
/// Cap on stored description length.
const MAX_DESCRIPTION: usize = 8000;

pub struct ParseOptions<'a> {
    pub calendar_id: &'a str,
    /// Used for "floating" times and unknown timezone names.
    pub default_tz: Tz,
    pub window: Window,
    /// The user's address, to work out their RSVP status. `None` for anonymous feeds.
    pub my_email: Option<&'a str>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Key {
    Timed(i64),
    Day(NaiveDate),
}

#[derive(Clone, Copy, Debug)]
enum Moment {
    Day(NaiveDate),
    Time(DateTime<Utc>),
}

impl Moment {
    fn key(self) -> Key {
        match self {
            Moment::Day(d) => Key::Day(d),
            Moment::Time(t) => Key::Timed(t.timestamp()),
        }
    }
}

#[derive(Clone, Debug)]
struct Start {
    moment: Moment,
    /// The zone the time was written in; `None` for UTC and all-day.
    tz: Option<Tz>,
    local: NaiveDateTime,
}

struct Raw {
    uid: String,
    recurrence_id: Option<Key>,
    summary: String,
    start: Start,
    /// Length of the event: whole days for all-day, a duration otherwise.
    length: Length,
    status: EventStatus,
    location: Option<String>,
    description: Option<String>,
    url: Option<String>,
    conference: Option<String>,
    organizer: Option<String>,
    response: Response,
    rrule: Option<String>,
    rdates: Vec<Moment>,
    exdates: HashSet<Key>,
    sequence_updated: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug)]
enum Length {
    Days(i64),
    Span(Duration),
}

fn parse_start(p: &Prop, default_tz: Tz) -> Option<Start> {
    let v = p.value.trim();
    let is_date = p
        .param("VALUE")
        .is_some_and(|x| x.eq_ignore_ascii_case("DATE"))
        || (v.len() == 8 && !v.contains('T'));
    if is_date {
        let d = NaiveDate::parse_from_str(v, "%Y%m%d").ok()?;
        return Some(Start {
            moment: Moment::Day(d),
            tz: None,
            local: d.and_hms_opt(0, 0, 0)?,
        });
    }
    let utc = v.ends_with('Z') || v.ends_with('z');
    let naive =
        NaiveDateTime::parse_from_str(v.trim_end_matches(['Z', 'z']), "%Y%m%dT%H%M%S").ok()?;
    if utc {
        return Some(Start {
            moment: Moment::Time(naive.and_utc()),
            tz: None,
            local: naive,
        });
    }
    let zone = p.param("TZID").and_then(tz::resolve).unwrap_or(default_tz);
    Some(Start {
        moment: Moment::Time(tz::local_to_utc(zone, naive)),
        tz: Some(zone),
        local: naive,
    })
}

fn parse_moments(p: &Prop, default_tz: Tz) -> Vec<Moment> {
    p.value
        .split(',')
        .filter_map(|v| {
            let single = Prop {
                name: p.name.clone(),
                params: p.params.clone(),
                value: v.to_string(),
            };
            parse_start(&single, default_tz).map(|s| s.moment)
        })
        .collect()
}

/// `[-]P[nW][nD][T[nH][nM][nS]]`
fn parse_duration(s: &str) -> Option<Duration> {
    let (neg, rest) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let rest = rest.strip_prefix('P')?;
    let mut total = Duration::zero();
    let mut num = String::new();
    let mut in_time = false;
    for c in rest.chars() {
        match c {
            'T' => in_time = true,
            '0'..='9' => num.push(c),
            unit => {
                let n: i64 = num.parse().ok()?;
                num.clear();
                total += match (unit, in_time) {
                    ('W', false) => Duration::weeks(n),
                    ('D', false) => Duration::days(n),
                    ('H', true) => Duration::hours(n),
                    ('M', true) => Duration::minutes(n),
                    ('S', true) => Duration::seconds(n),
                    _ => return None,
                };
            }
        }
    }
    if !num.is_empty() {
        return None;
    }
    Some(if neg { -total } else { total })
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n' | 'N') => out.push('\n'),
            Some(o) => out.push(o),
            None => out.push('\\'),
        }
    }
    out
}

fn text(props: &[Prop], name: &str) -> Option<String> {
    props
        .iter()
        .find(|p| p.name == name)
        .map(|p| unescape(&p.value))
        .filter(|s| !s.trim().is_empty())
}

fn mailto(v: &str) -> String {
    v.trim()
        .trim_start_matches("mailto:")
        .trim_start_matches("MAILTO:")
        .to_ascii_lowercase()
}

fn to_raw(props: &[Prop], opts: &ParseOptions) -> Option<Raw> {
    let uid = props
        .iter()
        .find(|p| p.name == "UID")?
        .value
        .trim()
        .to_string();
    if uid.is_empty() {
        return None;
    }
    let start = parse_start(props.iter().find(|p| p.name == "DTSTART")?, opts.default_tz)?;

    let length = match (
        props.iter().find(|p| p.name == "DTEND"),
        props.iter().find(|p| p.name == "DURATION"),
    ) {
        (Some(e), _) => match (parse_start(e, opts.default_tz)?.moment, start.moment) {
            (Moment::Day(e), Moment::Day(s)) => Length::Days((e - s).num_days().max(1)),
            (Moment::Time(e), Moment::Time(s)) => Length::Span((e - s).max(Duration::zero())),
            // Mixed date/time is malformed; treat the end as the start.
            _ => Length::Span(Duration::zero()),
        },
        (None, Some(d)) => {
            let dur = parse_duration(d.value.trim())?;
            match start.moment {
                Moment::Day(_) => Length::Days(dur.num_days().max(1)),
                Moment::Time(_) => Length::Span(dur.max(Duration::zero())),
            }
        }
        (None, None) => match start.moment {
            Moment::Day(_) => Length::Days(1),
            Moment::Time(_) => Length::Span(Duration::zero()),
        },
    };

    let status = match text(props, "STATUS")
        .map(|s| s.to_ascii_uppercase())
        .as_deref()
    {
        Some("CANCELLED") => EventStatus::Cancelled,
        Some("TENTATIVE") => EventStatus::Tentative,
        _ => EventStatus::Confirmed,
    };

    let organizer = props.iter().find(|p| p.name == "ORGANIZER").map(|p| {
        p.param("CN")
            .map(unescape)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| mailto(&p.value))
    });
    let organizer_addr = props
        .iter()
        .find(|p| p.name == "ORGANIZER")
        .map(|p| mailto(&p.value));

    let me = opts.my_email.map(str::to_ascii_lowercase);
    let response = match &me {
        None => Response::None,
        Some(me) if organizer_addr.as_deref() == Some(me.as_str()) => Response::None,
        Some(me) => props
            .iter()
            .filter(|p| p.name == "ATTENDEE" && mailto(&p.value) == *me)
            .find_map(|p| {
                Some(match p.param("PARTSTAT")?.to_ascii_uppercase().as_str() {
                    "ACCEPTED" => Response::Accepted,
                    "DECLINED" => Response::Declined,
                    "TENTATIVE" => Response::Tentative,
                    "NEEDS-ACTION" => Response::NeedsAction,
                    _ => return None,
                })
            })
            .unwrap_or(Response::None),
    };

    let recurrence_id = props
        .iter()
        .find(|p| p.name == "RECURRENCE-ID")
        .and_then(|p| parse_start(p, opts.default_tz))
        .map(|s| s.moment.key());

    let mut rdates = Vec::new();
    let mut exdates = HashSet::new();
    for p in props {
        match p.name.as_str() {
            "RDATE" => rdates.extend(parse_moments(p, opts.default_tz)),
            "EXDATE" => exdates.extend(
                parse_moments(p, opts.default_tz)
                    .into_iter()
                    .map(Moment::key),
            ),
            _ => {}
        }
    }

    let updated = ["LAST-MODIFIED", "DTSTAMP"]
        .iter()
        .find_map(|n| props.iter().find(|p| p.name == *n))
        .and_then(|p| parse_start(p, opts.default_tz))
        .and_then(|s| match s.moment {
            Moment::Time(t) => Some(t),
            Moment::Day(_) => None,
        })
        .unwrap_or_default();

    Some(Raw {
        uid,
        recurrence_id,
        summary: text(props, "SUMMARY").unwrap_or_else(|| "(No title)".into()),
        start,
        length,
        status,
        location: text(props, "LOCATION"),
        description: text(props, "DESCRIPTION").map(|d| d.chars().take(MAX_DESCRIPTION).collect()),
        url: props
            .iter()
            .find(|p| p.name == "URL")
            .map(|p| p.value.trim().to_string()),
        conference: props
            .iter()
            .find(|p| {
                matches!(
                    p.name.as_str(),
                    "CONFERENCE" | "X-GOOGLE-CONFERENCE" | "X-MICROSOFT-SKYPETEAMSMEETINGURL"
                )
            })
            .map(|p| p.value.trim().to_string()),
        organizer,
        response,
        rrule: props
            .iter()
            .find(|p| p.name == "RRULE")
            .map(|p| p.value.trim().to_string()),
        rdates,
        exdates,
        sequence_updated: updated,
    })
}

fn build(raw: &Raw, calendar_id: &str, at: Moment, instance: bool) -> Event {
    let when = match (at, raw.length) {
        (Moment::Day(s), Length::Days(n)) => When::AllDay {
            start: s,
            end: s + Duration::days(n),
        },
        (Moment::Day(s), Length::Span(_)) => When::AllDay {
            start: s,
            end: s + Duration::days(1),
        },
        (Moment::Time(s), Length::Span(d)) => When::Timed {
            start: s,
            end: s + d,
            tzid: raw.start.tz.map(|t| t.name().to_string()),
        },
        (Moment::Time(s), Length::Days(n)) => When::Timed {
            start: s,
            end: s + Duration::days(n),
            tzid: raw.start.tz.map(|t| t.name().to_string()),
        },
    };
    let suffix = match at.key() {
        Key::Timed(t) => t.to_string(),
        Key::Day(d) => d.to_string(),
    };
    let join_url = join::extract(
        raw.conference.as_deref(),
        raw.location.as_deref(),
        raw.description.as_deref(),
    )
    .or_else(|| raw.url.as_deref().and_then(join::find_in_text));
    Event {
        id: format!("{calendar_id}:{}:{suffix}", raw.uid),
        calendar_id: calendar_id.to_string(),
        remote_id: if instance {
            format!("{}#{suffix}", raw.uid)
        } else {
            raw.uid.clone()
        },
        ical_uid: raw.uid.clone(),
        title: raw.summary.clone(),
        when,
        location: raw.location.clone(),
        description: raw.description.clone(),
        join_url,
        status: raw.status,
        my_response: raw.response,
        organizer: raw.organizer.clone(),
        recurrence_master_id: instance.then(|| raw.uid.clone()),
        updated_at: raw.sequence_updated,
        also_in: Vec::new(),
    }
}

/// All start moments of a recurring event that could touch the window.
fn expand(raw: &Raw, window: &Window) -> Vec<Moment> {
    let mut out = vec![raw.start.moment];
    out.extend(raw.rdates.iter().copied());
    if let Some(rule) = &raw.rrule {
        let dtstart = match (raw.start.moment, raw.start.tz) {
            (Moment::Day(d), _) => format!("DTSTART:{}T000000Z", d.format("%Y%m%d")),
            (Moment::Time(t), None) => format!("DTSTART:{}", t.format("%Y%m%dT%H%M%SZ")),
            (Moment::Time(_), Some(z)) => {
                format!(
                    "DTSTART;TZID={}:{}",
                    z.name(),
                    raw.start.local.format("%Y%m%dT%H%M%S")
                )
            }
        };
        let slack = match raw.length {
            Length::Days(n) => Duration::days(n + 1),
            Length::Span(d) => d + Duration::days(1),
        };
        let from = (window.from - slack).with_timezone(&rrule::Tz::UTC);
        let to = window.to.with_timezone(&rrule::Tz::UTC);
        // An unparseable rule must not hide the event: keep just the first occurrence.
        if let Ok(set) = format!("{dtstart}\nRRULE:{rule}").parse::<rrule::RRuleSet>() {
            let res = set.after(from).before(to).all(MAX_INSTANCES);
            for d in res.dates {
                out.push(match raw.start.moment {
                    Moment::Day(_) => Moment::Day(d.naive_utc().date()),
                    Moment::Time(_) => Moment::Time(d.with_timezone(&Utc)),
                });
            }
        }
    }
    out.sort_by_key(|m| match m {
        Moment::Day(d) => d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp(),
        Moment::Time(t) => t.timestamp(),
    });
    out.dedup_by_key(|m| m.key());
    out
}

/// Parses an `.ics` document into events overlapping `opts.window`, with recurrences expanded.
pub fn events_from_ics(source: &str, opts: &ParseOptions) -> Result<Vec<Event>> {
    let (lines, saw_calendar) = parse_lines(source);
    if !saw_calendar {
        return Err(Error::Provider("not an iCalendar document".into()));
    }

    let raws: Vec<Raw> = lines
        .iter()
        .filter_map(|props| to_raw(props, opts))
        .collect();

    let mut overrides: HashMap<(String, Key), &Raw> = HashMap::new();
    for r in &raws {
        if let Some(k) = r.recurrence_id {
            overrides.insert((r.uid.clone(), k), r);
        }
    }

    let mut out: Vec<Event> = Vec::new();
    for raw in &raws {
        if raw.recurrence_id.is_some() {
            // A moved/changed single occurrence stands on its own.
            if raw.status != EventStatus::Cancelled {
                out.push(build(raw, opts.calendar_id, raw.start.moment, true));
            }
            continue;
        }
        if raw.rrule.is_none() && raw.rdates.is_empty() {
            out.push(build(raw, opts.calendar_id, raw.start.moment, false));
            continue;
        }
        for at in expand(raw, &opts.window) {
            let key = at.key();
            if raw.exdates.contains(&key) || overrides.contains_key(&(raw.uid.clone(), key)) {
                continue;
            }
            out.push(build(raw, opts.calendar_id, at, true));
        }
    }

    out.retain(|e| e.overlaps(opts.window.from, opts.window.to));
    out.sort_by(|a, b| sort_key(a).cmp(&sort_key(b)).then_with(|| a.id.cmp(&b.id)));
    out.dedup_by(|a, b| a.id == b.id);
    Ok(out)
}

fn sort_key(e: &Event) -> i64 {
    match &e.when {
        When::Timed { start, .. } => start.timestamp(),
        When::AllDay { start, .. } => start.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp(),
    }
}

#[cfg(test)]
mod tests;
