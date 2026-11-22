//! Phones: notifications handed to the phone's own system ahead of time, so meeting heads-ups, summaries, your own
//! event reminders and task reminders arrive even when Firstlight is closed (iOS and Android pause apps in the
//! background, which stops the notifier thread).
//!
//! The same rules decide as on a computer: `plan` asks them what would be due at each minute of the next hours and
//! keeps the first minute each notification would appear. `apply` schedules what is new, and cancels what is no
//! longer planned (a meeting moved or removed, a setting changed). Anything scheduled is marked shown, so the notifier
//! doesn't show it again while the app happens to be open; but planning ignores those marks for things this module
//! scheduled itself, so a moved meeting's reminder moves with it.

use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use firstlight_core::briefing::notify::{Kind, NotifySettings, due};
use firstlight_core::briefing::{BriefingSettings, rules::UnlockRules};
use firstlight_core::day::DaySettings;
use firstlight_core::model::Event;
use firstlight_core::store::Store;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Mutex;
use tauri::AppHandle;

/// How far ahead notifications are handed over (iOS keeps at most 64 waiting per app).
const AHEAD_MINUTES: i64 = 12 * 60;
const MAX_PLANNED: usize = 60;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Planned {
    pub id: String,
    pub at: DateTime<Utc>,
    pub title: String,
    pub body: String,
}

/// What this module has scheduled: the phone's number for each → (our id, when).
static SCHEDULED: Mutex<BTreeMap<i32, (String, i64)>> = Mutex::new(BTreeMap::new());

/// Our own ids scheduled so far, which planning must not treat as already shown.
pub(crate) fn scheduled_ids() -> HashSet<String> {
    SCHEDULED
        .lock()
        .map(|s| s.values().map(|(id, _)| id.clone()).collect())
        .unwrap_or_default()
}

/// A stable number for an id (the phone's system wants a 32-bit one): FNV-1a, kept positive.
fn number(id: &str) -> i32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in id.bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    (h & 0x7fff_ffff) as i32
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn plan(
    store: &Store,
    events: &[Event],
    now: DateTime<Utc>,
    tz: Tz,
    ns: &NotifySettings,
    bs: &BriefingSettings,
    rules: &UnlockRules,
    day_settings: &DaySettings,
    sent: &HashSet<String>,
) -> Vec<Planned> {
    let mut out: HashMap<String, Planned> = HashMap::new();
    let clock = if ns.time_24h { "%H:%M" } else { "%-I:%M %p" };
    let tasks = firstlight_core::todo::all(store).unwrap_or_default();
    let mut add = |id: String, at: DateTime<Utc>, title: String, body: String| {
        if !sent.contains(&id) {
            out.entry(id.clone()).or_insert(Planned {
                id,
                at,
                title,
                body,
            });
        }
    };
    for m in 1..=AHEAD_MINUTES {
        let t = now + Duration::minutes(m);
        let day = t.with_timezone(&tz).date_naive();
        let off = day_settings.off_on(day).is_some();
        let focusing = firstlight_core::local::focus_until(store, t)
            .ok()
            .flatten()
            .is_some();
        for n in due(events, t, tz, ns, bs, rules, sent) {
            // The same holds as for the notifier: during focus time only heads-ups; on a day off only what is about a
            // meeting you still have.
            if (focusing && n.kind != Kind::Before) || (off && !n.kind.on_time_off()) {
                continue;
            }
            add(n.id, t, n.title, n.body);
        }
        for r in firstlight_core::local::due_reminders(store, t).unwrap_or_default() {
            let title = if ns.hide_titles {
                "Event reminder".to_string()
            } else {
                r.title.clone()
            };
            add(
                r.id,
                t,
                title,
                format!("Starts at {}", r.start.with_timezone(&tz).format(clock)),
            );
        }
        if day_settings.task_reminders_now(focusing, day) {
            for r in firstlight_core::tasks::due_reminders(&tasks, t) {
                let title = if ns.hide_titles {
                    "Task due".to_string()
                } else {
                    r.title.clone()
                };
                add(
                    r.id,
                    t,
                    title,
                    format!("Task due at {}", r.at.with_timezone(&tz).format(clock)),
                );
            }
        }
    }
    let mut v: Vec<Planned> = out.into_values().collect();
    v.sort_by(|a, b| a.at.cmp(&b.at).then(a.id.cmp(&b.id)));
    v.truncate(MAX_PLANNED);
    v
}

/// Schedules what is newly planned and cancels what no longer is. Marks scheduled ones as shown.
pub(crate) fn apply(app: &AppHandle, store: &Store, plan: &[Planned], now: DateTime<Utc>) {
    use tauri_plugin_notification::NotificationExt;
    let Ok(mut scheduled) = SCHEDULED.lock() else {
        return;
    };
    let wanted: BTreeMap<i32, &Planned> = plan.iter().map(|p| (number(&p.id), p)).collect();
    let gone: Vec<i32> = scheduled
        .iter()
        .filter(|(n, (_, at))| wanted.get(n).is_none_or(|p| p.at.timestamp() != *at))
        .map(|(n, _)| *n)
        .collect();
    if !gone.is_empty() {
        #[cfg(mobile)]
        let _ = app.notification().cancel(gone.clone());
        for n in &gone {
            scheduled.remove(n);
        }
    }
    for (n, p) in wanted {
        if scheduled.contains_key(&n) {
            continue;
        }
        let Ok(date) = time::OffsetDateTime::from_unix_timestamp(p.at.timestamp()) else {
            continue;
        };
        let shown = app
            .notification()
            .builder()
            .id(n)
            .title(&p.title)
            .body(&p.body)
            .schedule(tauri_plugin_notification::Schedule::At {
                date,
                repeating: false,
                allow_while_idle: true,
            })
            .show();
        match shown {
            Ok(()) => {
                scheduled.insert(n, (p.id.clone(), p.at.timestamp()));
                let _ = store.mark_notification_sent(&p.id, now);
            }
            Err(e) => log::warn!("couldn't schedule a notification: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn numbers_are_stable_and_positive() {
        assert_eq!(super::number("before:abc"), super::number("before:abc"));
        assert_ne!(super::number("before:abc"), super::number("before:abd"));
        for id in ["", "x", "morning:2026-11-22", "task:🙂"] {
            assert!(super::number(id) >= 0);
        }
    }
}
