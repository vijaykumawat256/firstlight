//! Shows the notifications the briefing module says are due.
//!
//! A background thread with its own database connection checks every 30 seconds. All the rules (what is
//! due, never twice, quiet hours) live in `daybrief_core::briefing::notify`; this file only fetches the
//! inputs, shows the result, and records that it was shown.

use chrono::{Duration, Utc};
use chrono_tz::Tz;
use daybrief_core::briefing::day_bounds;
use daybrief_core::briefing::notify::due;
use daybrief_core::dedupe::dedupe;
use daybrief_core::secrets::Redacted;
use daybrief_core::store::Store;
use std::path::PathBuf;
use std::time::Duration as StdDuration;
use tauri::AppHandle;
use tauri_plugin_notification::{NotificationExt, PermissionState};

const TICK: StdDuration = StdDuration::from_secs(30);
const KEEP_SENT_DAYS: i64 = 3;

/// Whether the system is set to a 24-hour clock (macOS). Used only for the default; the user can override it.
fn system_uses_24h() -> bool {
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

/// Shows one notification, asking macOS for permission the first time it is needed.
fn show(app: &AppHandle, title: &str, body: &str) {
    let n = app.notification();
    if matches!(
        n.permission_state(),
        Ok(PermissionState::Prompt) | Ok(PermissionState::PromptWithRationale)
    ) {
        let _ = n.request_permission();
    }
    if let Err(e) = n.builder().title(title).body(body).show() {
        // The error says nothing about the content; the content itself is never logged.
        log::warn!("could not show a notification: {e}");
    }
}

pub fn spawn(app: AppHandle, db_path: PathBuf, key: Redacted, tz: Tz) {
    std::thread::Builder::new()
        .name("notifier".into())
        .spawn(move || {
            let store = match Store::open(&db_path, &key) {
                Ok(s) => s,
                Err(e) => {
                    log::error!("notifier cannot open the database: {e}");
                    return;
                }
            };
            let system_24h = system_uses_24h();
            // Give the first sync a moment so we don't announce from stale data at launch.
            std::thread::sleep(StdDuration::from_secs(8));
            loop {
                let now = Utc::now();
                // Re-read every tick so a change in Settings applies at once, without a restart.
                let settings = store.load_settings();
                let ns = settings.notify_for_engine(system_24h);
                let (bs, rules) = (settings.briefing, settings.unlock);
                let today = now.with_timezone(&tz).date_naive();
                let (from, _) = day_bounds(today, tz);
                let (_, to) = day_bounds(today + Duration::days(1), tz);

                if let (Ok(events), Ok(sent)) =
                    (store.events_between(from, to), store.sent_notifications())
                {
                    for n in due(&dedupe(events), now, tz, &ns, &bs, &rules, &sent) {
                        // Record first: whatever happens next, we never try this one again.
                        let _ = store.mark_notification_sent(&n.id, now);
                        show(&app, &n.title, &n.body);
                        log::info!("notification shown: {:?}", n.kind);
                    }
                }
                let _ = store.prune_notifications(now - Duration::days(KEEP_SENT_DAYS));
                std::thread::sleep(TICK);
            }
        })
        .expect("failed to start the notifier thread");
}
