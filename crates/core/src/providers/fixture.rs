//! Fixture provider: realistic fake calendars for development, tests and demos.
//! No network, no credentials, no real data.

use super::{Capabilities, Provider, SyncResult, Window};
use crate::model::*;
use crate::{Error, Result};
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;

pub const FIXTURE_ACCOUNT_ID: &str = "fixture-account";
const WORK_CAL: &str = "fixture-work";
const PERSONAL_CAL: &str = "fixture-personal";

pub struct FixtureProvider {
    /// The user's "today", in their timezone.
    today: NaiveDate,
    tz: Tz,
}

impl FixtureProvider {
    pub fn new(today: NaiveDate, tz: Tz) -> Self {
        Self { today, tz }
    }

    pub fn account(now: DateTime<Utc>) -> Account {
        Account {
            id: FIXTURE_ACCOUNT_ID.into(),
            provider: ProviderKind::Fixture,
            display_name: "Demo account".into(),
            email: None,
            status: AccountStatus::Ok,
            secret_ref: None,
            created_at: now,
        }
    }

    /// Local wall-clock time `h:m` on `today + day_offset`, as a UTC instant in the given zone.
    fn at(&self, tz: Tz, day_offset: i64, h: u32, m: u32) -> DateTime<Utc> {
        let date = self.today + Duration::days(day_offset);
        let naive = date.and_time(NaiveTime::from_hms_opt(h, m, 0).unwrap());
        // On a DST gap the local time doesn't exist: nudge forward an hour.
        tz.from_local_datetime(&naive)
            .earliest()
            .or_else(|| {
                tz.from_local_datetime(&(naive + Duration::hours(1)))
                    .earliest()
            })
            .expect("valid local time")
            .with_timezone(&Utc)
    }

    #[allow(clippy::too_many_arguments)]
    fn timed(
        &self,
        key: &str,
        cal: &str,
        title: &str,
        tz: Tz,
        day: i64,
        (h1, m1): (u32, u32),
        (h2, m2): (u32, u32),
    ) -> Event {
        let mut e = self.base(key, cal, title);
        e.when = When::Timed {
            start: self.at(tz, day, h1, m1),
            end: self.at(tz, day, h2, m2),
            tzid: Some(tz.name().to_string()),
        };
        e
    }

    fn base(&self, key: &str, cal: &str, title: &str) -> Event {
        Event {
            id: format!("{cal}:{key}:{}", self.today),
            calendar_id: cal.into(),
            remote_id: key.into(),
            ical_uid: format!("{key}@fixture"),
            title: title.into(),
            when: When::AllDay {
                start: self.today,
                end: self.today + Duration::days(1),
            },
            location: None,
            description: None,
            join_url: None,
            status: EventStatus::Confirmed,
            my_response: Response::Accepted,
            organizer: None,
            recurrence_master_id: None,
            updated_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
        }
    }

    /// The full fake dataset: a busy, realistic day plus tomorrow.
    pub fn events(&self) -> Vec<Event> {
        let tz = self.tz;
        let la: Tz = "America/Los_Angeles".parse().unwrap();
        let mut v = Vec::new();

        let mut e = self.timed("standup", WORK_CAL, "Team standup", tz, 0, (9, 0), (9, 15));
        e.recurrence_master_id = Some("standup-series".into());
        e.join_url = Some("https://meet.example.com/standup".into());
        v.push(e);

        let mut e = self.timed(
            "design-review",
            WORK_CAL,
            "Design review",
            tz,
            0,
            (10, 30),
            (11, 30),
        );
        e.location = Some("Room 4B".into());
        e.join_url = Some("https://zoom.example.com/j/123456789".into());
        e.organizer = Some("Priya".into());
        v.push(e);

        // Overlaps the design review on purpose: a conflict.
        let mut e = self.timed(
            "one-on-one",
            WORK_CAL,
            "1:1 with Sam",
            tz,
            0,
            (11, 0),
            (11, 30),
        );
        e.my_response = Response::NeedsAction;
        v.push(e);

        v.push(self.timed(
            "lunch",
            PERSONAL_CAL,
            "Lunch with Alex",
            tz,
            0,
            (12, 30),
            (13, 30),
        ));

        let mut e = self.timed("focus", WORK_CAL, "Focus time", tz, 0, (14, 0), (16, 0));
        e.status = EventStatus::Tentative;
        e.my_response = Response::Tentative;
        v.push(e);

        // Back-to-back with a different timezone (organiser is on the west coast).
        let mut e = self.timed(
            "planning",
            WORK_CAL,
            "Roadmap planning",
            la,
            0,
            (8, 0),
            (9, 0),
        );
        e.join_url = Some("https://teams.example.com/l/meetup-join/abc".into());
        e.organizer = Some("Jordan (PST)".into());
        v.push(e);

        v.push(self.timed(
            "dentist",
            PERSONAL_CAL,
            "Dentist",
            tz,
            0,
            (17, 30),
            (18, 15),
        ));

        // Cancelled: must never be shown as a normal event.
        let mut e = self.timed(
            "cancelled",
            WORK_CAL,
            "Vendor demo (cancelled)",
            tz,
            0,
            (15, 0),
            (15, 30),
        );
        e.status = EventStatus::Cancelled;
        v.push(e);

        // Declined: hidden by default.
        let mut e = self.timed(
            "declined",
            WORK_CAL,
            "All-hands rehearsal",
            tz,
            0,
            (16, 30),
            (17, 0),
        );
        e.my_response = Response::Declined;
        v.push(e);

        // All-day today.
        v.push(self.base("birthday", PERSONAL_CAL, "Maya's birthday"));

        // Tomorrow: an unusually early start, and a lunch.
        let mut e = self.timed(
            "flight",
            PERSONAL_CAL,
            "Flight to Berlin",
            tz,
            1,
            (6, 45),
            (9, 30),
        );
        e.location = Some("Terminal 2".into());
        v.push(e);
        v.push(self.timed(
            "tomorrow-sync",
            WORK_CAL,
            "Partner sync",
            tz,
            1,
            (11, 0),
            (11, 45),
        ));
        v.push(self.timed(
            "tomorrow-dinner",
            PERSONAL_CAL,
            "Dinner at Nopi",
            tz,
            1,
            (19, 30),
            (21, 0),
        ));

        v
    }
}

impl Provider for FixtureProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Fixture
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            incremental_sync: false,
            email: false,
            needs_polling: false,
        }
    }

    fn list_calendars(&self, account: &Account) -> Result<Vec<Calendar>> {
        if account.provider != ProviderKind::Fixture {
            return Err(Error::Provider("not a fixture account".into()));
        }
        let cal = |id: &str, name: &str, colour: &str| Calendar {
            id: id.into(),
            account_id: account.id.clone(),
            remote_id: id.into(),
            name: name.into(),
            colour: colour.into(),
            visible: true,
            read_only: true,
        };
        Ok(vec![
            cal(WORK_CAL, "Work", "blue"),
            cal(PERSONAL_CAL, "Personal", "green"),
        ])
    }

    fn sync_events(
        &self,
        calendar: &Calendar,
        _state: Option<&str>,
        window: &Window,
    ) -> Result<SyncResult> {
        let changed = self
            .events()
            .into_iter()
            .filter(|e| e.calendar_id == calendar.id && e.overlaps(window.from, window.to))
            .collect();
        Ok(SyncResult {
            changed,
            deleted: vec![],
            next_state: None,
            full: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(date: (i32, u32, u32), tz: &str) -> FixtureProvider {
        FixtureProvider::new(
            NaiveDate::from_ymd_opt(date.0, date.1, date.2).unwrap(),
            tz.parse().unwrap(),
        )
    }

    #[test]
    fn dataset_covers_the_awkward_cases() {
        let evs = provider((2026, 3, 10), "Europe/London").events();
        assert!(
            evs.iter().any(|e| matches!(e.when, When::AllDay { .. })),
            "all-day"
        );
        assert!(
            evs.iter().any(|e| e.status == EventStatus::Cancelled),
            "cancelled"
        );
        assert!(
            evs.iter().any(|e| e.my_response == Response::Declined),
            "declined"
        );
        assert!(
            evs.iter().any(|e| e.recurrence_master_id.is_some()),
            "recurring"
        );
        assert!(evs.iter().any(|e| e.join_url.is_some()), "join link");
        assert!(
            evs.iter().any(|e| matches!(&e.when, When::Timed { tzid: Some(t), .. } if t == "America/Los_Angeles")),
            "other timezone"
        );
        // Event IDs are unique.
        let mut ids: Vec<_> = evs.iter().map(|e| e.id.clone()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), evs.len());
    }

    #[test]
    fn contains_a_genuine_conflict() {
        let evs = provider((2026, 3, 10), "Europe/London").events();
        let a = evs.iter().find(|e| e.remote_id == "design-review").unwrap();
        let b = evs.iter().find(|e| e.remote_id == "one-on-one").unwrap();
        let (
            When::Timed {
                start: s1, end: e1, ..
            },
            When::Timed {
                start: s2, end: e2, ..
            },
        ) = (&a.when, &b.when)
        else {
            panic!()
        };
        assert!(s1 < e2 && s2 < e1);
    }

    #[test]
    fn local_times_are_correct_across_a_dst_change() {
        // US clocks spring forward on 2026-03-08: 09:00 local is UTC-4 afterwards, UTC-5 before.
        let before = provider((2026, 3, 7), "America/New_York").at(
            "America/New_York".parse().unwrap(),
            0,
            9,
            0,
        );
        let after = provider((2026, 3, 9), "America/New_York").at(
            "America/New_York".parse().unwrap(),
            0,
            9,
            0,
        );
        assert_eq!(before.format("%H:%M").to_string(), "14:00");
        assert_eq!(after.format("%H:%M").to_string(), "13:00");
    }

    #[test]
    fn join_links_are_https_only() {
        for e in provider((2026, 3, 10), "UTC").events() {
            if let Some(u) = e.join_url {
                assert!(u.starts_with("https://"));
            }
        }
    }

    #[test]
    fn sync_returns_only_the_calendars_events_in_window() {
        let p = provider((2026, 3, 10), "UTC");
        let acct = FixtureProvider::account(Utc::now());
        let cals = p.list_calendars(&acct).unwrap();
        let work = cals.iter().find(|c| c.id == WORK_CAL).unwrap();
        let w = Window {
            from: Utc.with_ymd_and_hms(2026, 3, 10, 0, 0, 0).unwrap(),
            to: Utc.with_ymd_and_hms(2026, 3, 11, 0, 0, 0).unwrap(),
        };
        let r = p.sync_events(work, None, &w).unwrap();
        assert!(r.full);
        assert!(r.changed.iter().all(|e| e.calendar_id == WORK_CAL));
        assert!(r.changed.iter().all(|e| e.overlaps(w.from, w.to)));
        assert!(!r.changed.is_empty());
    }
}
