//! The briefing engine run over the fixture provider's dataset (needs the provider, so it lives here, not in the pure crate).

use crate::briefing::{BriefingSettings, build, is_shown};
use crate::providers::fixture::FixtureProvider;
use chrono::{NaiveDate, TimeZone, Utc};

#[test]
fn works_on_the_fixture_dataset() {
    let london = chrono_tz::Europe::London;
    let p = FixtureProvider::new(NaiveDate::from_ymd_opt(2026, 3, 10).unwrap(), london);
    let now = Utc.with_ymd_and_hms(2026, 3, 10, 9, 30, 0).unwrap();
    let b = build(&p.events(), now, london, &BriefingSettings::default());
    assert!(!b.conflicts.is_empty(), "design review overlaps the 1:1");
    assert_eq!(b.all_day_today.len(), 1);
    assert!(b.tomorrow.early_start, "the 06:45 flight");
    assert!(
        b.remaining
            .iter()
            .all(|i| is_shown(&i.event, &BriefingSettings::default()))
    );
    assert_eq!(b.needs_response.len(), 1);
}
