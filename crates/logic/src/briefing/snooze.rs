//! Snoozed heads-ups: "remind me again in ten minutes". The text kept is exactly what was shown the first
//! time, so a snoozed notification can never reveal more than the original did (for instance after the
//! user hid event names). Held in memory only: a restart forgets pending snoozes, which at most costs one
//! repeat reminder.

use chrono::{DateTime, Duration, Utc};

pub const SNOOZE_MINUTES: i64 = 10;
const MAX_PENDING: usize = 20;
const MAX_TEXT_CHARS: usize = 300;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snoozed {
    pub due: DateTime<Utc>,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Default)]
pub struct SnoozeQueue {
    items: Vec<Snoozed>,
}

impl SnoozeQueue {
    /// An empty queue, usable in a `static`.
    pub const fn new() -> Self {
        Self { items: Vec::new() }
    }
}

fn limit(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .take(MAX_TEXT_CHARS)
        .collect()
}

impl SnoozeQueue {
    /// Snoozes a notification for [`SNOOZE_MINUTES`]. Snoozing the same text again replaces the earlier
    /// snooze rather than stacking a second one. At most [`MAX_PENDING`] are kept (the oldest is dropped).
    pub fn add(&mut self, now: DateTime<Utc>, title: &str, body: &str) {
        let (title, body) = (limit(title), limit(body));
        self.items.retain(|s| !(s.title == title && s.body == body));
        if self.items.len() >= MAX_PENDING {
            self.items.remove(0);
        }
        self.items.push(Snoozed {
            due: now + Duration::minutes(SNOOZE_MINUTES),
            title,
            body,
        });
    }

    /// Removes and returns the snoozes whose time has come, earliest first.
    pub fn take_due(&mut self, now: DateTime<Utc>) -> Vec<Snoozed> {
        let (mut due, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut self.items)
            .into_iter()
            .partition(|s| s.due <= now);
        self.items = keep;
        due.sort_by_key(|s| s.due);
        due
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    #[test]
    fn a_snooze_comes_back_after_ten_minutes_and_only_once() {
        let mut q = SnoozeQueue::default();
        q.add(
            t("2026-11-20T10:00:00Z"),
            "Design review",
            "Starts at 10:15 AM",
        );
        assert!(q.take_due(t("2026-11-20T10:09:59Z")).is_empty());
        let due = q.take_due(t("2026-11-20T10:10:00Z"));
        assert_eq!(due.len(), 1);
        assert_eq!(
            (due[0].title.as_str(), due[0].body.as_str()),
            ("Design review", "Starts at 10:15 AM")
        );
        assert!(
            q.take_due(t("2026-11-20T11:00:00Z")).is_empty(),
            "shown once"
        );
        assert!(q.is_empty());
    }

    #[test]
    fn snoozing_the_same_notification_again_does_not_stack() {
        let mut q = SnoozeQueue::default();
        q.add(t("2026-11-20T10:00:00Z"), "A", "b");
        q.add(t("2026-11-20T10:05:00Z"), "A", "b");
        assert_eq!(q.len(), 1);
        assert!(
            q.take_due(t("2026-11-20T10:10:00Z")).is_empty(),
            "the later snooze replaced it"
        );
        assert_eq!(q.take_due(t("2026-11-20T10:15:00Z")).len(), 1);
    }

    #[test]
    fn it_is_bounded_and_ordered_and_text_is_cleaned() {
        let mut q = SnoozeQueue::default();
        for i in 0..30 {
            q.add(
                t("2026-11-20T10:00:00Z") + Duration::minutes(i),
                &format!("E{i}"),
                "x",
            );
        }
        assert_eq!(q.len(), MAX_PENDING);
        let due = q.take_due(t("2026-11-21T00:00:00Z"));
        assert_eq!(due.first().unwrap().title, "E10", "the oldest were dropped");
        assert!(due.windows(2).all(|w| w[0].due <= w[1].due));
        q.add(t("2026-11-20T10:00:00Z"), &"a\nb\u{7}".repeat(400), "");
        let s = q.take_due(t("2026-11-21T00:00:00Z")).remove(0);
        assert!(
            s.title.chars().count() <= MAX_TEXT_CHARS && !s.title.chars().any(char::is_control)
        );
    }
}
