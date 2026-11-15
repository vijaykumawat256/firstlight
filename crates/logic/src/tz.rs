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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

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
