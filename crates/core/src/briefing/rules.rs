//! When should the briefing appear on its own? Pure decision logic, driven by the unlock event.
//!
//! The rules exist so Firstlight is never annoying: people unlock their computer dozens of times a day.

use chrono::{DateTime, Datelike, Duration, NaiveDate, Timelike, Utc, Weekday};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct UnlockRules {
    /// The first unlock of the day at or after this local hour opens the full briefing.
    pub first_unlock_after_hour: u32,
    /// Never appear again within this many minutes of the last automatic appearance.
    pub min_interval_minutes: i64,
    /// Local `(from_hour, to_hour)` during which nothing appears. May wrap past midnight (22 to 6).
    pub quiet_hours: Option<(u32, u32)>,
    /// Monday first. `false` disables automatic appearances that day.
    pub days: [bool; 7],
    /// On later unlocks, show a compact "up next" panel if a meeting starts within this many minutes.
    pub up_next_lead_minutes: i64,
    /// Stay out of the way when another app is full screen (presentations, video, games).
    pub skip_when_fullscreen: bool,
}

impl Default for UnlockRules {
    fn default() -> Self {
        Self {
            first_unlock_after_hour: 6,
            min_interval_minutes: 30,
            quiet_hours: Some((22, 6)),
            days: [true; 7],
            up_next_lead_minutes: 15,
            skip_when_fullscreen: true,
        }
    }
}

/// What has already happened, from the log.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RuleState {
    /// The local date of the last full briefing.
    pub last_full_date: Option<NaiveDate>,
    /// The last time anything appeared automatically.
    pub last_shown_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    None,
    ShowFull,
    ShowUpNext,
}

pub(crate) fn weekday_index(d: Weekday) -> usize {
    d.num_days_from_monday() as usize
}

pub(crate) fn in_quiet_hours(hour: u32, quiet: Option<(u32, u32)>) -> bool {
    match quiet {
        None => false,
        Some((from, to)) if from == to => false,
        Some((from, to)) if from < to => hour >= from && hour < to,
        Some((from, to)) => hour >= from || hour < to, // wraps midnight
    }
}

/// Decides what to do on an unlock. `next_start` is the start of the next upcoming event, if any.
pub fn decide(
    now: DateTime<Utc>,
    tz: Tz,
    rules: &UnlockRules,
    state: &RuleState,
    next_start: Option<DateTime<Utc>>,
    fullscreen: bool,
) -> Action {
    let local = now.with_timezone(&tz);
    if !rules.days[weekday_index(local.weekday())] {
        return Action::None;
    }
    if in_quiet_hours(local.hour(), rules.quiet_hours) {
        return Action::None;
    }
    if fullscreen && rules.skip_when_fullscreen {
        return Action::None;
    }
    if state
        .last_shown_at
        .is_some_and(|t| now - t < Duration::minutes(rules.min_interval_minutes))
    {
        return Action::None;
    }
    if local.hour() >= rules.first_unlock_after_hour
        && state.last_full_date != Some(local.date_naive())
    {
        return Action::ShowFull;
    }
    match next_start {
        Some(s) if s > now && s - now <= Duration::minutes(rules.up_next_lead_minutes) => {
            Action::ShowUpNext
        }
        _ => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    const LONDON: Tz = chrono_tz::Europe::London;

    // 2026-03-10 is a Tuesday; London is on GMT, so UTC == local.
    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 3, 10, h, m, 0).unwrap()
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()
    }

    fn rules() -> UnlockRules {
        UnlockRules::default()
    }

    #[test]
    fn first_unlock_of_the_day_opens_the_full_briefing() {
        assert_eq!(
            decide(
                at(8, 30),
                LONDON,
                &rules(),
                &RuleState::default(),
                None,
                false
            ),
            Action::ShowFull
        );
    }

    #[test]
    fn only_one_full_briefing_per_day() {
        let st = RuleState {
            last_full_date: Some(today()),
            last_shown_at: Some(at(8, 30)),
        };
        assert_eq!(
            decide(at(14, 0), LONDON, &rules(), &st, None, false),
            Action::None
        );
        // The next morning it opens again.
        let tomorrow = at(8, 30) + Duration::days(1);
        assert_eq!(
            decide(tomorrow, LONDON, &rules(), &st, None, false),
            Action::ShowFull
        );
    }

    #[test]
    fn an_unlock_before_the_start_hour_does_not_use_up_the_days_briefing() {
        assert_eq!(
            decide(
                at(5, 0),
                LONDON,
                &UnlockRules {
                    quiet_hours: None,
                    ..rules()
                },
                &RuleState::default(),
                None,
                false
            ),
            Action::None
        );
        assert_eq!(
            decide(
                at(7, 15),
                LONDON,
                &rules(),
                &RuleState::default(),
                None,
                false
            ),
            Action::ShowFull
        );
    }

    #[test]
    fn later_unlocks_show_a_compact_panel_only_when_a_meeting_is_close() {
        let st = RuleState {
            last_full_date: Some(today()),
            last_shown_at: Some(at(8, 30)),
        };
        assert_eq!(
            decide(at(13, 50), LONDON, &rules(), &st, Some(at(14, 0)), false),
            Action::ShowUpNext
        );
        assert_eq!(
            decide(at(13, 30), LONDON, &rules(), &st, Some(at(14, 0)), false),
            Action::None,
            "30 minutes away is too early"
        );
        assert_eq!(
            decide(at(14, 5), LONDON, &rules(), &st, Some(at(14, 0)), false),
            Action::None,
            "already started"
        );
        assert_eq!(
            decide(at(13, 50), LONDON, &rules(), &st, None, false),
            Action::None
        );
    }

    #[test]
    fn never_appears_twice_within_the_minimum_interval() {
        let st = RuleState {
            last_full_date: Some(today()),
            last_shown_at: Some(at(13, 40)),
        };
        assert_eq!(
            decide(at(13, 55), LONDON, &rules(), &st, Some(at(14, 0)), false),
            Action::None,
            "shown 15 minutes ago"
        );
        assert_eq!(
            decide(at(14, 11), LONDON, &rules(), &st, Some(at(14, 20)), false),
            Action::ShowUpNext
        );
        // The interval also guards a would-be first briefing.
        let st = RuleState {
            last_full_date: None,
            last_shown_at: Some(at(8, 10)),
        };
        assert_eq!(
            decide(at(8, 20), LONDON, &rules(), &st, None, false),
            Action::None
        );
    }

    #[test]
    fn quiet_hours_suppress_everything_including_across_midnight() {
        let r = UnlockRules {
            quiet_hours: Some((22, 6)),
            ..rules()
        };
        for h in [22, 23, 0, 3, 5] {
            assert_eq!(
                decide(
                    at(h, 30),
                    LONDON,
                    &r,
                    &RuleState::default(),
                    Some(at(h, 40)),
                    false
                ),
                Action::None,
                "{h}:30"
            );
        }
        assert_eq!(
            decide(at(6, 30), LONDON, &r, &RuleState::default(), None, false),
            Action::ShowFull
        );
        // A same-day window, and "no quiet hours".
        let day = UnlockRules {
            quiet_hours: Some((12, 14)),
            ..rules()
        };
        assert_eq!(
            decide(at(13, 0), LONDON, &day, &RuleState::default(), None, false),
            Action::None
        );
        assert_eq!(
            decide(at(14, 0), LONDON, &day, &RuleState::default(), None, false),
            Action::ShowFull
        );
        assert_eq!(
            decide(
                at(23, 0),
                LONDON,
                &UnlockRules {
                    quiet_hours: None,
                    ..rules()
                },
                &RuleState::default(),
                None,
                false
            ),
            Action::ShowFull
        );
    }

    #[test]
    fn disabled_weekdays_are_respected() {
        let mut days = [true; 7];
        days[1] = false; // Tuesday
        assert_eq!(
            decide(
                at(9, 0),
                LONDON,
                &UnlockRules { days, ..rules() },
                &RuleState::default(),
                None,
                false
            ),
            Action::None
        );
        let wednesday = at(9, 0) + Duration::days(1);
        assert_eq!(
            decide(
                wednesday,
                LONDON,
                &UnlockRules { days, ..rules() },
                &RuleState::default(),
                None,
                false
            ),
            Action::ShowFull
        );
    }

    #[test]
    fn stays_out_of_the_way_of_full_screen_apps_unless_told_otherwise() {
        assert_eq!(
            decide(
                at(9, 0),
                LONDON,
                &rules(),
                &RuleState::default(),
                None,
                true
            ),
            Action::None
        );
        let r = UnlockRules {
            skip_when_fullscreen: false,
            ..rules()
        };
        assert_eq!(
            decide(at(9, 0), LONDON, &r, &RuleState::default(), None, true),
            Action::ShowFull
        );
    }

    #[test]
    fn the_day_is_the_users_local_day_not_utc() {
        // 23:30 UTC on the 9th is 08:30 on the 10th in Tokyo (UTC+9).
        let tokyo = chrono_tz::Asia::Tokyo;
        let now = Utc.with_ymd_and_hms(2026, 3, 9, 23, 30, 0).unwrap();
        assert_eq!(
            decide(now, tokyo, &rules(), &RuleState::default(), None, false),
            Action::ShowFull
        );
        let st = RuleState {
            last_full_date: Some(NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()),
            last_shown_at: None,
        };
        assert_eq!(
            decide(now + Duration::hours(2), tokyo, &rules(), &st, None, false),
            Action::None
        );
    }

    #[test]
    fn defaults_are_conservative() {
        let r = rules();
        assert_eq!(
            (
                r.first_unlock_after_hour,
                r.min_interval_minutes,
                r.up_next_lead_minutes
            ),
            (6, 30, 15)
        );
        assert!(r.skip_when_fullscreen && r.days.iter().all(|d| *d));
    }
}
