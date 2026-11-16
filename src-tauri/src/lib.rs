//! Firstlight desktop shell: windows, tray, autostart, unlock detection, and the
//! small set of commands the UI is allowed to call. All calendar logic lives in
//! `firstlight-core`.

mod commands;
mod crash_reports;
#[cfg(not(windows))]
mod eventkit;
mod fullscreen;
mod keychain;
mod mail_ai;
mod notifier;
#[cfg(windows)]
mod outlook_win;
mod providers;
mod scheduler;
mod unlock;
mod updates;

use chrono::Utc;
use chrono_tz::Tz;
use firstlight_core::http::{ReqwestClient, Transport};
use firstlight_core::secrets::{Redacted, load_or_create_db_key};
use firstlight_core::store::Store;
use providers::Deps;
use scheduler::{SyncHandle, Trigger};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

const KEYCHAIN_SERVICE: &str = "com.firstlight.desktop";
const MAIN_WINDOW: &str = "main";
const UP_NEXT_WINDOW: &str = "upnext";
/// The small panel hides itself after this long if the user ignores it.
const UP_NEXT_SECONDS: u64 = 14;

pub(crate) struct AppState {
    /// The UI's own connection. Sync work uses separate connections so it never blocks this one.
    pub store: Mutex<Store>,
    pub db_path: PathBuf,
    pub key: Redacted,
    pub deps: Deps,
    pub sync: SyncHandle,
    /// Set to abort a sign-in that is waiting for the browser.
    pub oauth_cancel: Arc<std::sync::atomic::AtomicBool>,
    /// Held for the duration of a sign-in so only one runs at a time.
    pub oauth_lock: Arc<Mutex<()>>,
    pub mail_cache: Mutex<Option<(std::time::Instant, mail_ai::MailResult)>>,
    /// (when, what was summarised, the summary) so an unchanged day isn't re-sent.
    pub ai_cache: Mutex<Option<(std::time::Instant, String, Option<String>)>>,
}

// ---- helpers --------------------------------------------------------------

/// The system's time zone, read fresh on every call so travelling (or changing the zone in System
/// Settings) takes effect without restarting. `iana_time_zone` resets the OS's cached value itself.
pub(crate) fn local_tz() -> Tz {
    iana_time_zone::get_timezone()
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(chrono_tz::UTC)
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN_WINDOW) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
    // Opening the window is a good moment to look for new events. The scheduler's unlock cooldown keeps
    // this from syncing more than once every couple of minutes, however often the window is opened.
    if let Some(state) = app.try_state::<AppState>() {
        state.sync.trigger(scheduler::Trigger::Unlock);
    }
}

/// Which addresses a window of this app may show. Only the app's own pages: no remote site can ever be
/// loaded into a window that has our commands available, whatever a link or a script tries.
pub(crate) fn navigation_allowed(url: &tauri::Url) -> bool {
    match url.scheme() {
        "tauri" => url.host_str() == Some("localhost"),
        // Windows and Android serve the app from this host.
        "http" | "https" if url.host_str() == Some("tauri.localhost") => true,
        "about" => url.as_str() == "about:blank",
        // The development server, never in a release build.
        "http" => {
            cfg!(debug_assertions)
                && url.host_str() == Some("localhost")
                && url.port() == Some(1420)
        }
        _ => false,
    }
}

/// Shows or hides Firstlight's Dock icon (macOS) or taskbar button (Windows). The menu-bar or tray icon is
/// always there, so with this off the app is still reachable.
pub(crate) fn apply_dock(app: &AppHandle, show: bool) {
    #[cfg(target_os = "macos")]
    {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            let policy = if show {
                tauri::ActivationPolicy::Regular
            } else {
                tauri::ActivationPolicy::Accessory
            };
            let _ = handle.set_activation_policy(policy);
        });
    }
    #[cfg(not(target_os = "macos"))]
    if let Some(w) = app.get_webview_window(MAIN_WINDOW) {
        let _ = w.set_skip_taskbar(!show);
    }
}

/// Shows the main window, or hides it if it is already in front. Used by the global shortcut.
fn toggle_main(app: &AppHandle) {
    match app.get_webview_window(MAIN_WINDOW) {
        Some(w) if w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false) => {
            let _ = w.hide();
        }
        _ => show_main(app),
    }
}

/// Registers (or clears) the global shortcuts: one shows or hides the briefing, the other joins the next
/// meeting. Returns a plain-language error if the system refuses either.
pub(crate) fn apply_shortcut(
    app: &AppHandle,
    show: Option<&str>,
    join: Option<&str>,
) -> Result<(), String> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
    let gs = app.global_shortcut();
    gs.unregister_all()
        .map_err(|_| "Couldn't clear the old shortcut.".to_string())?;
    let taken = || "That shortcut couldn't be used. Another app may already own it.".to_string();
    if let Some(s) = show {
        gs.on_shortcut(s, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                toggle_main(app);
            }
        })
        .map_err(|_| taken())?;
    }
    if let Some(s) = join {
        gs.on_shortcut(s, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                join_next_meeting(app);
            }
        })
        .map_err(|_| taken())?;
    }
    Ok(())
}

/// Opens the video link of the meeting that is starting soon or already running, or says there is none.
fn join_next_meeting(app: &AppHandle) {
    use tauri_plugin_notification::NotificationExt;
    use tauri_plugin_opener::OpenerExt;
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    use firstlight_core::briefing::{build, day_bounds};
    let (now, tz) = (chrono::Utc::now(), local_tz());
    let url = {
        let Ok(store) = state.store.lock() else {
            return;
        };
        let settings = store.load_settings();
        let today = now.with_timezone(&tz).date_naive();
        let (from, _) = day_bounds(today, tz);
        let (_, to) = day_bounds(today + chrono::Duration::days(1), tz);
        let events = store
            .events_between(from, to)
            .map(firstlight_core::dedupe::dedupe)
            .unwrap_or_default();
        let b = build(&events, now, tz, &settings.briefing);
        firstlight_core::briefing::glance::join_target(
            &b,
            now,
            settings.unlock.up_next_lead_minutes,
        )
        .and_then(|i| i.event.join_url.clone())
    };
    // Only https links are ever opened (PLAN.md section 3), whatever a calendar put in the field.
    match url.filter(|u| u.starts_with("https://")) {
        Some(url) => {
            if app.opener().open_url(url, None::<&str>).is_err() {
                log::warn!("could not open the meeting link");
            }
        }
        None => {
            let _ = app
                .notification()
                .builder()
                .title("Nothing to join")
                .body("No meeting with a video link is starting soon or running.")
                .show();
        }
    }
}

/// Shows the small "up next" panel in the top-right corner, creating it the first time.
fn show_up_next(app: &AppHandle) {
    use tauri::{LogicalPosition, WebviewUrl, WebviewWindowBuilder};
    let (w, h) = (360.0, 148.0);
    let window = match app.get_webview_window(UP_NEXT_WINDOW) {
        Some(win) => win,
        None => {
            let built = WebviewWindowBuilder::new(
                app,
                UP_NEXT_WINDOW,
                WebviewUrl::App("index.html#/upnext".into()),
            )
            .title("Firstlight")
            .inner_size(w, h)
            .resizable(false)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(false)
            .visible(false)
            .shadow(true)
            .build();
            match built {
                Ok(win) => win,
                Err(e) => {
                    log::error!("could not create the up-next panel: {e}");
                    return;
                }
            }
        }
    };
    if let Ok(Some(monitor)) = window.primary_monitor() {
        let scale = monitor.scale_factor();
        let size = monitor.size().to_logical::<f64>(scale);
        let origin = monitor.position().to_logical::<f64>(scale);
        let _ = window.set_position(LogicalPosition::new(
            origin.x + size.width - w - 16.0,
            origin.y + 40.0,
        ));
    }
    let _ = window.show();
    // Tell the page to load the next event now (it may have been hidden for hours).
    let _ = window.emit("up-next-shown", ());
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(UP_NEXT_SECONDS));
        hide_up_next(&handle);
    });
}

pub(crate) fn hide_up_next(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(UP_NEXT_WINDOW) {
        let _ = w.hide();
    }
}

/// Applies the unlock rules and shows whatever they call for.
fn on_unlock(app: &AppHandle) {
    use firstlight_core::briefing::{
        BriefingSettings, build, day_bounds,
        rules::{Action, UnlockRules, decide},
    };
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let now = Utc::now();
    let tz = local_tz();
    let Ok(store) = state.store.lock() else {
        return;
    };
    let today = now.with_timezone(&tz).date_naive();
    let _ = store.log_briefing(today, "unlock", now);

    // The one settings document, never the old scattered keys (those are only read by the migration): reading
    // them here ignored everything the person had configured and fell back to the defaults.
    let all = store.load_settings();
    let rules: UnlockRules = all.unlock;
    let settings: BriefingSettings = all.briefing;
    let (Ok(rule_state), (from, _)) = (store.rule_state(tz), day_bounds(today, tz)) else {
        return;
    };
    let (_, to) = day_bounds(today + chrono::Duration::days(1), tz);
    let events = store
        .events_between(from, to)
        .map(firstlight_core::dedupe::dedupe)
        .unwrap_or_default();
    let next_start = build(&events, now, tz, &settings)
        .next
        .filter(|n| !n.in_progress)
        .map(|n| n.start);

    // Only looked at when the rule is on, so the window list is not read for nothing.
    let covered = rules.skip_when_fullscreen && fullscreen::another_app_is_full_screen();
    let action = decide(now, tz, &rules, &rule_state, next_start, covered);
    // The inputs, so a "nothing appeared" is a log read and not a guess. No event data.
    log::info!(
        "unlock decision: {action:?} (local {}, zone {}, every time {}, days {:?}, quiet {:?}, fullscreen {covered}, last full {:?}, minutes since last shown {:?})",
        now.with_timezone(&tz).format("%a %H:%M"),
        tz.name(),
        rules.open_every_time,
        rules.days,
        rules.quiet_hours,
        rule_state.last_full_date,
        rule_state.last_shown_at.map(|t| (now - t).num_minutes()),
    );
    match action {
        Action::ShowFull => {
            let _ = store.log_briefing(today, "full", now);
            drop(store);
            show_main(app);
        }
        Action::ShowUpNext => {
            let _ = store.log_briefing(today, "upnext", now);
            drop(store);
            show_up_next(app);
        }
        Action::None => {}
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show Briefing", true, None::<&str>)?;
    let at_login = CheckMenuItem::with_id(
        app,
        "autostart",
        "Start at Login",
        true,
        app.autolaunch().is_enabled().unwrap_or(false),
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit Firstlight", true, Some("CmdOrCtrl+Q"))?;
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &at_login,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    // macOS: a black "template" glyph the system recolours for light and dark menu bars. Elsewhere there is no
    // such recolouring (a black glyph disappears on a dark Windows taskbar), so use the coloured app icon.
    #[cfg(target_os = "macos")]
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;
    #[cfg(not(target_os = "macos"))]
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png"))?;
    TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip("Firstlight")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "show" => show_main(app),
            "autostart" => {
                let launcher = app.autolaunch();
                let on = launcher.is_enabled().unwrap_or(false);
                let _ = if on {
                    launcher.disable()
                } else {
                    launcher.enable()
                };
                let _ = at_login.set_checked(launcher.is_enabled().unwrap_or(false));
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Looks for a new version about once a day, if the user allows it and this build can update. Only the app's
/// version and operating system reach the update server. If one is found the page is told; nothing installs
/// without the user choosing to.
fn spawn_update_checks(app: AppHandle) {
    if updates::configured_base().is_none() || !cfg!(feature = "updater") {
        return;
    }
    tauri::async_runtime::spawn(async move {
        tokio_sleep(std::time::Duration::from_secs(90)).await;
        loop {
            let channel = app
                .try_state::<AppState>()
                .and_then(|s| s.store.lock().ok().map(|st| st.load_settings()))
                .filter(|s| s.general.check_for_updates)
                .map(|s| s.general.update_channel);
            if let Some(channel) = channel
                && let updates::UpdateStatus::Available { version } =
                    updates::check(&app, channel).await
            {
                let _ = app.emit("update-available", version);
            }
            tokio_sleep(std::time::Duration::from_secs(24 * 3600)).await;
        }
    });
}

async fn tokio_sleep(d: std::time::Duration) {
    // A plain thread sleep on the blocking pool keeps this free of an extra async-runtime dependency.
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(d)).await;
}

pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        // A second launch just brings the existing window forward.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app)
        }))
        .plugin(
            tauri::plugin::Builder::<tauri::Wry>::new("navigation-guard")
                .on_navigation(|_webview, url| navigation_allowed(url))
                .build(),
        )
        .plugin(tauri_plugin_opener::init());

    // The signed-update plugin exists only in packages built for release (see docs/RELEASE.md).
    #[cfg(feature = "updater")]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());

    builder
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .invoke_handler(tauri::generate_handler![
            commands::get_events,
            commands::get_week_ahead,
            commands::get_conflict_help,
            commands::open_mail_draft,
            commands::find_free_times,
            commands::get_calendars,
            commands::list_accounts,
            commands::add_ics_account,
            commands::add_caldav_account,
            commands::add_oauth_account,
            commands::system_calendar_status,
            commands::add_system_account,
            commands::open_privacy_settings,
            commands::cancel_oauth,
            commands::oauth_available,
            commands::add_demo_account,
            commands::remove_account,
            commands::rename_account,
            commands::set_calendar_visible,
            commands::set_calendar_colour,
            commands::refresh_now,
            commands::get_autostart,
            commands::set_autostart,
            commands::get_unlock_count_today,
            commands::get_briefing,
            commands::get_app_info,
            commands::check_for_update,
            commands::install_update,
            commands::get_last_crash_report,
            commands::clear_crash_reports,
            mail_ai::get_mail_digest,
            mail_ai::enable_apple_mail,
            mail_ai::enable_account_mail,
            mail_ai::ai_keys,
            mail_ai::sign_in_openrouter,
            commands::writable_calendars,
            commands::get_own_event,
            commands::export_own_events,
            commands::skip_event_occurrence,
            commands::get_system_accent,
            commands::invite_senders,
            commands::invite_preview,
            commands::send_invite_mail,
            commands::create_event,
            commands::update_event,
            commands::delete_event,
            mail_ai::add_imap_mailbox,
            mail_ai::remove_imap_mailbox,
            mail_ai::open_mail_message,
            mail_ai::ai_status,
            mail_ai::ai_preview,
            mail_ai::enable_ai,
            mail_ai::disable_ai,
            mail_ai::forget_ai_key,
            mail_ai::get_ai_summary,
            mail_ai::ask_calendar,
            mail_ai::get_meeting_prep,
            commands::get_settings,
            commands::set_settings,
            commands::export_settings,
            commands::save_settings_export,
            commands::import_settings,
            commands::erase_all_data,
            commands::dismiss_up_next,
            commands::open_briefing,
        ])
        .on_window_event(|window, event| {
            // Closing hides to the tray; Quit lives in the tray menu.
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Both windows only hide; Quit lives in the tray menu.
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            // First thing: if anything below panics, a content-free report is saved (when the user allows it).
            if let Ok(dir) = app.path().app_log_dir() {
                crash_reports::install_hook(dir, env!("CARGO_PKG_VERSION"));
            }

            // A normal app (Dock icon) until the saved setting is read below.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Regular);

            // Database key lives only in the OS keychain.
            // An optional profile gives a second, separate copy of the data (own keychain entries and database),
            // so a release build can be tried without touching a real installation.
            let profile = std::env::var("FIRSTLIGHT_PROFILE")
                .ok()
                .and_then(|p| firstlight_core::profile::validate(&p));
            let keychain = keychain::KeychainStore::new(
                firstlight_core::profile::keychain_service(KEYCHAIN_SERVICE, profile.as_deref()),
            );
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;

            // An installation from before the rename (the app was called Daybrief) keeps its data: the database
            // and its secrets are moved under the current names, checked, and only then removed from the old
            // place. If that cannot be done now, nothing old is touched and it is tried again at the next start.
            if let Some(parent) = dir.parent() {
                use firstlight_core::legacy;
                let paths = legacy::Paths {
                    old_dir: parent.join(legacy::OLD_APP_DIR),
                    old_db: legacy::old_db_file_name(profile.as_deref()),
                    new_dir: dir.clone(),
                    new_db: firstlight_core::profile::db_file_name(profile.as_deref()),
                };
                let old_keychain =
                    keychain::KeychainStore::new(firstlight_core::profile::keychain_service(
                        legacy::OLD_KEYCHAIN_BASE,
                        profile.as_deref(),
                    ));
                match legacy::migrate(&paths, &old_keychain, &keychain) {
                    Ok(outcome) => log::info!("previous installation: {outcome:?}"),
                    Err(e) => log::error!("could not move the previous data: {e}"),
                }
            }

            let key = load_or_create_db_key(&keychain).map_err(|e| {
                log::error!("keychain unavailable: {e}");
                e.to_string()
            })?;

            let db_path = dir.join(firstlight_core::profile::db_file_name(profile.as_deref()));
            let store = Store::open(&db_path, &key).map_err(|e| {
                log::error!("cannot open database: {e}");
                e.to_string()
            })?;

            let deps = Deps {
                http: Arc::new(ReqwestClient::new().map_err(|e| e.to_string())?),
                secrets: Arc::new(keychain),
                oauth: providers::OAuthConfigs::from_build(),
                // The computer's own calendars: the Calendar app on a Mac, classic Outlook on Windows.
                #[cfg(windows)]
                system: Arc::new(outlook_win::source()),
                #[cfg(not(windows))]
                system: Arc::new(eventkit::EventKitSource),
                ai_http: Arc::new(
                    ReqwestClient::with_options(
                        std::time::Duration::from_secs(12),
                        Transport::HttpsOnly,
                    )
                    .map_err(|e| e.to_string())?,
                ),
                local_http: Arc::new(
                    ReqwestClient::with_options(
                        std::time::Duration::from_secs(12),
                        Transport::HttpsOrLoopback,
                    )
                    .map_err(|e| e.to_string())?,
                ),
            };
            let sync = scheduler::spawn(
                app.handle().clone(),
                db_path.clone(),
                key.clone(),
                deps.clone(),
            );
            sync.trigger(Trigger::Launch);
            let (db_path_for_notifier, key_for_notifier) = (db_path.clone(), key.clone());
            app.manage(AppState {
                store: Mutex::new(store),
                db_path,
                key,
                deps,
                sync,
                oauth_cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                oauth_lock: Arc::new(Mutex::new(())),
                mail_cache: Mutex::new(None),
                ai_cache: Mutex::new(None),
            });

            let startup_settings = app
                .state::<AppState>()
                .store
                .lock()
                .map(|st| st.load_settings())
                .unwrap_or_default();
            crash_reports::set_enabled(startup_settings.privacy.crash_reports);
            apply_dock(app.handle(), startup_settings.general.show_in_dock);
            if let Err(e) = apply_shortcut(
                app.handle(),
                startup_settings.general.global_shortcut.as_deref(),
                startup_settings.general.join_shortcut.as_deref(),
            ) {
                log::warn!("saved shortcut not registered: {e}");
            }
            // Clicking a notification brings the app forward. Registered before the first one is shown.
            {
                use tauri_plugin_notification::NotificationExt;
                use tauri_plugin_notification::{Action, ActionType};
                let handle = app.handle().clone();
                // Heads-ups carry a "Snooze 10 min" button; pressing it brings the same words back later.
                let _ = app.handle().notification().register_action_types(vec![
                    ActionType::builder(notifier::SNOOZABLE)
                        .actions(vec![
                            Action::builder(notifier::SNOOZE_ACTION, "Snooze 10 min").build(),
                        ])
                        .build(),
                ]);
                let _ = app.handle().notification().on_action(move |performed| {
                    if performed.action_id() == notifier::SNOOZE_ACTION {
                        if let Some(n) = performed.notification() {
                            notifier::snooze(n.title().unwrap_or(""), n.body().unwrap_or(""));
                        }
                    } else {
                        // A click on the notification itself.
                        show_main(&handle);
                    }
                });
            }
            spawn_update_checks(app.handle().clone());
            notifier::spawn(app.handle().clone(), db_path_for_notifier, key_for_notifier);
            build_tray(app.handle())?;

            // Unlock: refresh calendars, then apply the rules (first unlock of the day, up-next, quiet hours...).
            let handle = app.handle().clone();
            unlock::start(move || {
                log::info!("screen unlock detected");
                if let Some(state) = handle.try_state::<AppState>() {
                    // Skipped if a sync just ran; see scheduler::UNLOCK_COOLDOWN.
                    state.sync.trigger(Trigger::Unlock);
                }
                on_unlock(&handle);
                let _ = handle.emit("system-unlock", ());
            });

            // Launched by the login item: start in the tray, but a fresh sign-in is not an unlock, so no unlock
            // event will ever come for it. Ask the same rules once, a few seconds in (the window and the first
            // sync need a moment), so the first sign-in of the day opens the briefing like the first unlock does.
            // Quiet hours, days and full screen still apply. Otherwise show the window.
            if std::env::args().any(|a| a == "--hidden") {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(8));
                    log::info!("started at sign-in: applying the unlock rules once");
                    on_unlock(&handle);
                });
            } else {
                show_main(app.handle());
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Firstlight")
        .run(|app, event| {
            // Clicking the Dock icon brings the window back (it only ever hides when closed).
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                show_main(app);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
}

#[cfg(test)]
mod capability_tests {
    use serde_json::Value;

    fn permissions(file: &str) -> Vec<String> {
        let v: Value = serde_json::from_str(file).expect("capability file is valid JSON");
        v["permissions"]
            .as_array()
            .expect("permissions list")
            .iter()
            .map(|p| {
                p.as_str()
                    .or_else(|| p["identifier"].as_str())
                    .expect("permission id")
                    .to_string()
            })
            .collect()
    }

    /// The panel only ever needs to read, hide itself and open the main window. If it were ever compromised
    /// it must not be able to erase data, change settings, add accounts or touch the keychain.
    #[test]
    fn the_up_next_panel_can_only_read_and_dismiss() {
        let p = permissions(include_str!("../capabilities/upnext.json"));
        assert!(p.len() <= 9, "keep this list short: {p:?}");
        for forbidden in [
            "erase",
            "set-settings",
            "import",
            "add-",
            "remove",
            "rename",
            "set-calendar",
            "oauth",
            "mail",
            "ai-",
            "enable",
            "autostart",
            "save-settings",
            "export",
        ] {
            assert!(
                !p.iter().any(|x| x.contains(forbidden)),
                "up-next window must not have {forbidden}: {p:?}"
            );
        }
        let main_only = [
            "allow-erase-all-data",
            "allow-set-settings",
            "allow-add-ics-account",
            "allow-remove-account",
        ];
        assert!(main_only.iter().all(|m| !p.contains(&m.to_string())));
    }

    /// Command names from `build.rs`, the single list the permission system is generated from.
    fn registered_commands() -> Vec<String> {
        let build = include_str!("../build.rs");
        let list = build
            .split(".commands(&[")
            .nth(1)
            .and_then(|r| r.split("])").next())
            .expect("command list in build.rs");
        list.split('"')
            .skip(1)
            .step_by(2)
            .map(String::from)
            .collect()
    }

    #[test]
    fn every_registered_command_is_granted_to_the_main_window_and_handled() {
        let main = permissions(include_str!("../capabilities/default.json"));
        let lib = include_str!("lib.rs");
        let commands = registered_commands();
        assert!(commands.len() > 30, "found {} commands", commands.len());
        for c in &commands {
            let perm = format!("allow-{}", c.replace('_', "-"));
            assert!(
                main.contains(&perm),
                "{c}: the main window has no permission {perm}"
            );
            assert!(
                lib.contains(&format!("::{c},")) || lib.contains(&format!("::{c}\n")),
                "{c} is not in generate_handler!"
            );
        }
        // And no permission refers to a command that no longer exists.
        for p in main.iter().filter(|p| p.starts_with("allow-")) {
            assert!(
                commands
                    .iter()
                    .any(|c| format!("allow-{}", c.replace('_', "-")) == **p),
                "stale permission {p}"
            );
        }
    }

    #[test]
    fn windows_can_only_ever_show_the_apps_own_pages() {
        let ok = |s: &str| super::navigation_allowed(&s.parse().unwrap());
        for allowed in [
            "tauri://localhost/",
            "tauri://localhost/index.html",
            "http://tauri.localhost/",
            "https://tauri.localhost/#/upnext",
            "about:blank",
        ] {
            assert!(ok(allowed), "{allowed}");
        }
        for blocked in [
            "https://example.com/",
            "http://example.com/",
            "https://tauri.localhost.evil.com/",
            "http://localhost.evil.com:1420/",
            "tauri://evil/",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,<script>1</script>",
            "ftp://localhost/",
            "about:srcdoc",
            "https://accounts.google.com/",
            "message://%3Cx%3E",
            "blob:https://tauri.localhost/x",
        ] {
            assert!(!ok(blocked), "{blocked}");
        }
        // The dev server is only trusted in debug builds, and only on the exact port.
        assert_eq!(ok("http://localhost:1420/"), cfg!(debug_assertions));
        assert!(!ok("http://localhost:9999/"));
    }

    #[test]
    fn the_two_capabilities_target_different_windows() {
        let main: Value =
            serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
        let up: Value = serde_json::from_str(include_str!("../capabilities/upnext.json")).unwrap();
        assert_eq!(main["windows"], serde_json::json!(["main"]));
        assert_eq!(up["windows"], serde_json::json!(["upnext"]));
    }

    /// Every command the app registers must be a permission of the main window, and nothing else is reachable.
    #[test]
    fn the_main_window_holds_every_app_command_and_no_broad_core_permission() {
        let p = permissions(include_str!("../capabilities/default.json"));
        assert!(
            p.iter().all(|x| x.starts_with("allow-")
                || matches!(
                    x.as_str(),
                    "core:event:allow-listen"
                        | "core:event:allow-unlisten"
                        | "core:window:allow-start-dragging"
                        | "opener:allow-open-url"
                )),
            "unexpected permission: {p:?}"
        );
        assert!(
            !p.iter().any(|x| x.ends_with(":default")),
            "no blanket default permission sets"
        );
    }
}
