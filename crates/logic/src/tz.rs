//! Timezone name resolution. Outlook and Exchange write Windows zone names
//! ("Eastern Standard Time") in ICS files; map the common ones to IANA.

use chrono::{DateTime, Duration, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;

const WINDOWS_TO_IANA: &[(&str, &str)] = &[
    ("UTC", "UTC"),
    ("GMT Standard Time", "Europe/London"),
    ("W. Europe Standard Time", "Europe/Berlin"),
    ("Central Europe Standard Time", "Europe/Budapest"),
    ("Romance Standard Time", "Europe/Paris"),
    ("Central European Standard Time", "Europe/Warsaw"),
    ("E. Europe Standard Time", "Europe/Chisinau"),
    ("GTB Standard Time", "Europe/Bucharest"),
    ("FLE Standard Time", "Europe/Kiev"),
    ("Russian Standard Time", "Europe/Moscow"),
    ("Turkey Standard Time", "Europe/Istanbul"),
    ("Eastern Standard Time", "America/New_York"),
    ("Central Standard Time", "America/Chicago"),
    ("Mountain Standard Time", "America/Denver"),
    ("US Mountain Standard Time", "America/Phoenix"),
    ("Pacific Standard Time", "America/Los_Angeles"),
    ("Alaskan Standard Time", "America/Anchorage"),
    ("Hawaiian Standard Time", "Pacific/Honolulu"),
    ("Atlantic Standard Time", "America/Halifax"),
    ("Canada Central Standard Time", "America/Regina"),
    ("Mexico Standard Time", "America/Mexico_City"),
    ("SA Pacific Standard Time", "America/Bogota"),
    ("E. South America Standard Time", "America/Sao_Paulo"),
    ("Argentina Standard Time", "America/Argentina/Buenos_Aires"),
    ("India Standard Time", "Asia/Kolkata"),
    ("China Standard Time", "Asia/Shanghai"),
    ("Tokyo Standard Time", "Asia/Tokyo"),
    ("Korea Standard Time", "Asia/Seoul"),
    ("Singapore Standard Time", "Asia/Singapore"),
    ("Arabian Standard Time", "Asia/Dubai"),
    ("Israel Standard Time", "Asia/Jerusalem"),
    ("Pakistan Standard Time", "Asia/Karachi"),
    ("SE Asia Standard Time", "Asia/Bangkok"),
    ("AUS Eastern Standard Time", "Australia/Sydney"),
    ("Cen. Australia Standard Time", "Australia/Adelaide"),
    ("W. Australia Standard Time", "Australia/Perth"),
    ("New Zealand Standard Time", "Pacific/Auckland"),
    ("South Africa Standard Time", "Africa/Johannesburg"),
    ("Egypt Standard Time", "Africa/Cairo"),
];

/// Resolves an IANA or Windows timezone name. Quotes and a leading slash
/// (some generators write "/Europe/London") are tolerated.
pub fn resolve(name: &str) -> Option<Tz> {
    let n = name.trim().trim_matches('"').trim_start_matches('/');
    if let Ok(tz) = n.parse::<Tz>() {
        return Some(tz);
    }
    WINDOWS_TO_IANA
        .iter()
        .find(|(w, _)| w.eq_ignore_ascii_case(n))
        .and_then(|(_, i)| i.parse().ok())
}

/// Converts a local wall-clock time to a UTC instant. On a daylight-saving gap
/// (the time doesn't exist) it moves forward by the gap; on a repeated hour it
/// takes the first occurrence.
pub fn local_to_utc(tz: Tz, naive: NaiveDateTime) -> DateTime<Utc> {
    if let Some(dt) = tz.from_local_datetime(&naive).earliest() {
        return dt.with_timezone(&Utc);
    }
    for hours in 1..=3 {
        if let Some(dt) = tz
            .from_local_datetime(&(naive + Duration::hours(hours)))
            .earliest()
        {
            return dt.with_timezone(&Utc);
        }
    }
    naive.and_utc()
}

// ---- VTIMEZONE ---------------------------------------------------------------------------

/// One change of clock: the instant, and the offsets (seconds east of UTC) before and after it.
struct Shift {
    at: DateTime<Utc>,
    from: i32,
    to: i32,
    name: String,
    dst: bool,
}

fn offset_at(tz: Tz, t: DateTime<Utc>) -> (i32, String, bool) {
    use chrono::Offset;
    use chrono_tz::{OffsetComponents, OffsetName};
    let o = tz.offset_from_utc_datetime(&t.naive_utc());
    (
        o.fix().local_minus_utc(),
        o.abbreviation().unwrap_or("").to_string(),
        o.dst_offset() != Duration::zero(),
    )
}

/// Every clock change in a calendar year (UTC), found by stepping through it and narrowing down to the second.
fn shifts_in(tz: Tz, year: i32) -> Vec<Shift> {
    let Some(start) =
        chrono::NaiveDate::from_ymd_opt(year, 1, 1).and_then(|d| d.and_hms_opt(0, 0, 0))
    else {
        return vec![];
    };
    let start = Utc.from_utc_datetime(&start);
    let mut out = Vec::new();
    let mut prev = start;
    let mut prev_off = offset_at(tz, prev).0;
    for h in 1..=(366 * 24) {
        let t = start + Duration::hours(h);
        if chrono::Datelike::year(&t) != year {
            break;
        }
        let off = offset_at(tz, t).0;
        if off != prev_off {
            let (mut lo, mut hi) = (prev, t);
            while hi - lo > Duration::seconds(1) {
                let mid = lo + (hi - lo) / 2;
                if offset_at(tz, mid).0 == prev_off {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let (to, name, dst) = offset_at(tz, hi);
            out.push(Shift {
                at: hi,
                from: prev_off,
                to,
                name,
                dst,
            });
            prev_off = off;
        }
        prev = t;
    }
    out
}

fn hhmm(secs: i32) -> String {
    let sign = if secs < 0 { '-' } else { '+' };
    let a = secs.abs();
    format!("{sign}{:02}{:02}", a / 3600, a % 3600 / 60)
}

/// "Second Sunday" and the like, as iCalendar writes it (`2SU`, `-1SU` for the last).
fn nth_weekday(d: chrono::NaiveDate) -> String {
    use chrono::Datelike;
    let last = (d + Duration::days(7)).month() != d.month();
    let n = if last {
        -1
    } else {
        (d.day() as i32 - 1) / 7 + 1
    };
    let w = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"][d.weekday().num_days_from_monday() as usize];
    format!("{n}{w}")
}

/// The `VTIMEZONE` block (as unfolded lines) that makes a calendar file with `TZID=<zone>` times readable by any
/// calendar program, built from the zone's real clock changes in `year`. A zone whose changes follow a yearly rule
/// (most do) gets that rule; any other gets its changes spelled out for the next ten years.
pub fn vtimezone_lines(tz: Tz, year: i32) -> Vec<String> {
    use chrono::Datelike;
    let local_of = |s: &Shift| s.at.naive_utc() + Duration::seconds(i64::from(s.from));
    let mut l = vec!["BEGIN:VTIMEZONE".to_string(), format!("TZID:{}", tz.name())];
    let this = shifts_in(tz, year);
    let observance = |l: &mut Vec<String>,
                      s: &Shift,
                      rule: Option<String>,
                      year_override: Option<chrono::NaiveDateTime>| {
        let kind = if s.dst { "DAYLIGHT" } else { "STANDARD" };
        l.push(format!("BEGIN:{kind}"));
        l.push(format!(
            "DTSTART:{}",
            year_override
                .unwrap_or_else(|| local_of(s))
                .format("%Y%m%dT%H%M%S")
        ));
        l.push(format!("TZOFFSETFROM:{}", hhmm(s.from)));
        l.push(format!("TZOFFSETTO:{}", hhmm(s.to)));
        if !s.name.is_empty() {
            l.push(format!("TZNAME:{}", s.name));
        }
        if let Some(r) = rule {
            l.push(format!("RRULE:{r}"));
        }
        l.push(format!("END:{kind}"));
    };
    if this.is_empty() {
        let (off, name, _) = offset_at(
            tz,
            Utc.with_ymd_and_hms(year, 1, 1, 0, 0, 0)
                .single()
                .unwrap_or_default(),
        );
        l.push("BEGIN:STANDARD".into());
        l.push("DTSTART:19700101T000000".into());
        l.push(format!("TZOFFSETFROM:{}", hhmm(off)));
        l.push(format!("TZOFFSETTO:{}", hhmm(off)));
        if !name.is_empty() {
            l.push(format!("TZNAME:{name}"));
        }
        l.push("END:STANDARD".into());
    } else {
        let key = |s: &Shift| {
            let d = local_of(s);
            (d.month(), nth_weekday(d.date()), d.time(), s.from, s.to)
        };
        // A yearly rule only if the next three years change the clocks the same way.
        let rule_holds = (1..=3).all(|k| {
            let next = shifts_in(tz, year + k);
            next.len() == this.len() && next.iter().zip(&this).all(|(a, b)| key(a) == key(b))
        });
        if rule_holds {
            for s in &this {
                let d = local_of(s);
                let rule = format!(
                    "FREQ=YEARLY;BYMONTH={};BYDAY={}",
                    d.month(),
                    nth_weekday(d.date())
                );
                observance(&mut l, s, Some(rule), None);
            }
        } else {
            let mut all: Vec<Shift> = (0..10).flat_map(|k| shifts_in(tz, year + k)).collect();
            all.sort_by_key(|s| s.at);
            for s in &all {
                observance(&mut l, s, None, None);
            }
        }
    }
    l.push("END:VTIMEZONE".into());
    l
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn a_zone_with_a_yearly_rule_gets_that_rule() {
        let v = vtimezone_lines(chrono_tz::America::New_York, 2026).join("\n");
        assert!(v.contains("TZID:America/New_York"), "{v}");
        assert!(v.contains("BEGIN:DAYLIGHT\nDTSTART:20260308T020000\nTZOFFSETFROM:-0500\nTZOFFSETTO:-0400\nTZNAME:EDT\nRRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=2SU"), "{v}");
        assert!(v.contains("BEGIN:STANDARD\nDTSTART:20261101T020000\nTZOFFSETFROM:-0400\nTZOFFSETTO:-0500\nTZNAME:EST\nRRULE:FREQ=YEARLY;BYMONTH=11;BYDAY=1SU"), "{v}");
    }

    #[test]
    fn the_last_sunday_is_written_as_minus_one() {
        let v = vtimezone_lines(chrono_tz::Europe::London, 2026).join("\n");
        assert!(v.contains("BYMONTH=3;BYDAY=-1SU"), "{v}");
        assert!(v.contains("BYMONTH=10;BYDAY=-1SU"), "{v}");
        assert!(
            v.contains("DTSTART:20260329T010000\nTZOFFSETFROM:+0000\nTZOFFSETTO:+0100"),
            "{v}"
        );
    }

    #[test]
    fn a_zone_without_clock_changes_is_one_fixed_offset() {
        let v = vtimezone_lines(chrono_tz::Asia::Kolkata, 2026).join("\n");
        assert_eq!(v.matches("BEGIN:STANDARD").count(), 1, "{v}");
        assert!(v.contains("TZOFFSETFROM:+0530\nTZOFFSETTO:+0530"), "{v}");
        assert!(!v.contains("DAYLIGHT") && !v.contains("RRULE"));
    }

    #[test]
    fn every_zone_gives_a_well_formed_block() {
        for tz in chrono_tz::TZ_VARIANTS {
            let v = vtimezone_lines(tz, 2026);
            assert_eq!(v.first().map(String::as_str), Some("BEGIN:VTIMEZONE"));
            assert_eq!(v.last().map(String::as_str), Some("END:VTIMEZONE"));
            let begins = v.iter().filter(|x| x.starts_with("BEGIN:")).count();
            let ends = v.iter().filter(|x| x.starts_with("END:")).count();
            assert_eq!(begins, ends, "{tz}");
            assert!(begins >= 2, "{tz}: needs at least one observance");
        }
    }

    #[test]
    fn resolves_iana_windows_and_sloppy_names() {
        assert_eq!(resolve("Europe/London"), Some(chrono_tz::Europe::London));
        assert_eq!(
            resolve("Eastern Standard Time"),
            Some(chrono_tz::America::New_York)
        );
        assert_eq!(
            resolve("\"Pacific Standard Time\""),
            Some(chrono_tz::America::Los_Angeles)
        );
        assert_eq!(resolve("/Europe/Paris"), Some(chrono_tz::Europe::Paris));
        assert_eq!(resolve("Mars/Olympus"), None);
    }

    #[test]
    fn dst_gap_moves_forward_and_overlap_takes_first() {
        let ny = chrono_tz::America::New_York;
        // 2026-03-08 02:30 does not exist in New York (clocks jump 02:00 -> 03:00).
        let gap = NaiveDate::from_ymd_opt(2026, 3, 8)
            .unwrap()
            .and_hms_opt(2, 30, 0)
            .unwrap();
        assert_eq!(local_to_utc(ny, gap).format("%H:%M").to_string(), "07:30"); // 03:30 EDT
        // 2026-11-01 01:30 happens twice; first is still daylight time (UTC-4).
        let overlap = NaiveDate::from_ymd_opt(2026, 11, 1)
            .unwrap()
            .and_hms_opt(1, 30, 0)
            .unwrap();
        assert_eq!(
            local_to_utc(ny, overlap).format("%H:%M").to_string(),
            "05:30"
        );
    }
}
