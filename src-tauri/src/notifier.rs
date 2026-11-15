//! Shows the notifications the briefing module says are due.
//!
//! A background thread with its own database connection checks every 30 seconds. All the rules (what is
//! due, never twice, quiet hours) live in `firstlight_core::briefing::notify`; this file only fetches the
//! inputs, shows the result, and records that it was shown.

use chrono::{Duration, Utc};
use firstlight_core::briefing::day_bounds;
use firstlight_core::briefing::notify::due;
use firstlight_core::dedupe::dedupe;
use firstlight_core::secrets::Redacted;
use firstlight_core::store::Store;
use std::path::PathBuf;
use std::time::Duration as StdDuration;
use tauri::AppHandle;
use tauri_plugin_notification::{NotificationExt, PermissionState};

const TICK: StdDuration = StdDuration::from_secs(30);
const KEEP_SENT_DAYS: i64 = 3;

/// Whether the system is set to a 24-hour clock (macOS). Used only for the default; the user can override it.
pub(crate) fn system_uses_24h() -> bool {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("defaults")
            .args(["read", "-g", "AppleICUForce24HourTime"])
            .output()
            .ok()
            .is_some_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "1")
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// The action-type id of notifications that offer a "Snooze" button.
pub(crate) const SNOOZABLE: &str = "snoozable";
/// The id of the snooze button's action.
pub(crate) const SNOOZE_ACTION: &str = "snooze";

/// Snoozed notifications waiting for their time (in memory; see `briefing::snooze`).
static SNOOZED: std::sync::Mutex<firstlight_core::briefing::snooze::SnoozeQueue> =
    std::sync::Mutex::new(firstlight_core::briefing::snooze::SnoozeQueue::new());

/// Called when the user presses Snooze on a notification: it comes back in ten minutes with the same words.
pub(crate) fn snooze(title: &str, body: &str) {
    if let Ok(mut q) = SNOOZED.lock() {
        q.add(Utc::now(), title, body);
    }
}

/// Shows one notification, asking macOS for permission the first time it is needed. `snoozable` adds a
/// "Snooze 10 min" button (heads-ups and reminders; the daily summaries have none).
fn show(app: &AppHandle, title: &str, body: &str, snoozable: bool) {
    let n = app.notification();
    if matches!(
        n.permission_state(),
        Ok(PermissionState::Prompt) | Ok(PermissionState::PromptWithRationale)
    ) {
        let _ = n.request_permission();
    }
    let mut builder = n.builder().title(title).body(body);
    if snoozable {
        builder = builder.action_type_id(SNOOZABLE);
    }
    if let Err(e) = builder.show() {
        // The error says nothing about the content; the content itself is never logged.
        log::warn!("could not show a notification: {e}");
    }
}

/// Puts text next to the menu-bar icon (macOS) and in its tooltip, or clears it. Only touches the tray when
/// the text changed, so an idle app does no work.
fn set_tray_text(app: &AppHandle, text: Option<&str>) {
    use std::sync::Mutex;
    static LAST: Mutex<Option<String>> = Mutex::new(None);
    let owned = text.map(String::from);
    {
        let Ok(mut last) = LAST.lock() else { return };
        if *last == owned {
            return;
        }
        *last = owned.clone();
    }
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_title(owned.as_deref());
        let _ = tray.set_tooltip(Some(owned.as_deref().unwrap_or("Firstlight")));
    }
}

pub fn spawn(app: AppHandle, db_path: PathBuf, key: Redacted) {
    std::thread::Builder::new()
        .name("notifier".into())
        .spawn(move || {
            let mut store = match Store::open(&db_path, &key) {
                Ok(s) => s,
                Err(e) => {
                    log::error!("notifier cannot open the database: {e}");
                    return;
                }
            };
            let system_24h = system_uses_24h();
            let mut refreshed_on = None;
            // Give the first sync a moment so we don't announce from stale data at launch.
            std::thread::sleep(StdDuration::from_secs(8));
            loop {
                if firstlight_core::contain::contained(|| {
                    let now = Utc::now();
                    // Read each tick: a changed system time zone applies without a restart.
                    let tz = crate::local_tz();
                    // Re-read every tick so a change in Settings applies at once, without a restart.
                    let settings = store.load_settings();
                    let ns = settings.notify_for_engine(system_24h);
                    let general = settings.general.clone();
                    let (bs, rules) = (settings.briefing, settings.unlock);
                    let today = now.with_timezone(&tz).date_naive();
                    // Repeating events of the user's own are generated for a rolling window: top it up
                    // once a day (and the first time round after launch).
                    if refreshed_on != Some(today) {
                        refreshed_on = Some(today);
                        let _ = firstlight_core::local::refresh(&mut store, now);
                    }
                    let (from, _) = day_bounds(today, tz);
                    // Eight days, so the Sunday week preview can see all of the week ahead; everything else only looks at
                    // today and tomorrow.
                    let (_, to) = day_bounds(today + Duration::days(8), tz);

                    if let (Ok(events), Ok(sent)) =
                        (store.events_between(from, to), store.sent_notifications())
                    {
                        // The menu-bar text, refreshed every tick (empty when the setting is off).
                        let next =
                            firstlight_core::briefing::build(&dedupe(events.clone()), now, tz, &bs)
                                .next;
                        let label = general
                            .tray_next_meeting
                            .then(|| {
                                firstlight_core::briefing::glance::tray_label(
                                    next.as_ref(),
                                    now,
                                    ns.hide_titles,
                                    ns.time_24h,
                                    tz,
                                )
                            })
                            .flatten();
                        set_tray_text(&app, label.as_deref());
                        // Events with a reminder of their own are announced by it, not by the general heads-up.
                        let own = firstlight_core::local::reminder_map(&store).unwrap_or_default();
                        let events: Vec<_> = dedupe(events)
                            .into_iter()
                            .filter(|e| !firstlight_core::local::has_own_reminder(&own, e))
                            .collect();
                        for r in
                            firstlight_core::local::due_reminders(&store, now).unwrap_or_default()
                        {
                            if sent.contains(&r.id) {
                                continue;
                            }
                            let _ = store.mark_notification_sent(&r.id, now);
                            let clock = if ns.time_24h { "%H:%M" } else { "%-I:%M %p" };
                            let title = if ns.hide_titles {
                                "Event reminder"
                            } else {
                                r.title.as_str()
                            };
                            show(
                                &app,
                                title,
                                &format!("Starts at {}", r.start.with_timezone(&tz).format(clock)),
                                true,
                            );
                            log::info!("own reminder shown");
                        }
                        // Snoozed ones whose ten minutes are up (shown again with the same words).
                        let again = SNOOZED
                            .lock()
                            .map(|mut q| q.take_due(now))
                            .unwrap_or_default();
                        for s in again {
                            show(&app, &s.title, &s.body, true);
                            log::info!("snoozed notification shown again");
                        }
                        for n in due(&events, now, tz, &ns, &bs, &rules, &sent) {
                            // Record first: whatever happens next, we never try this one again.
                            let _ = store.mark_notification_sent(&n.id, now);
                            show(
                                &app,
                                &n.title,
                                &n.body,
                                n.kind == firstlight_core::briefing::notify::Kind::Before,
                            );
                            log::info!("notification shown: {:?}", n.kind);
                        }
                    }
                })
                .is_none()
                {
                    log::error!("a notifier tick panicked and was contained");
                }
                let _ = store.prune_notifications(Utc::now() - Duration::days(KEEP_SENT_DAYS));
                std::thread::sleep(TICK);
            }
        })
        .expect("failed to start the notifier thread");
}
