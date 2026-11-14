//! Daybrief desktop shell: windows, tray, autostart, unlock detection, and the
//! small set of commands the UI is allowed to call. All calendar logic lives in
//! `daybrief-core`.

mod commands;
mod eventkit;
mod keychain;
mod providers;
mod scheduler;
mod unlock;

use chrono::Utc;
use chrono_tz::Tz;
use daybrief_core::http::ReqwestClient;
use daybrief_core::secrets::{Redacted, load_or_create_db_key};
use daybrief_core::store::Store;
use providers::Deps;
use scheduler::{SyncHandle, Trigger};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

const KEYCHAIN_SERVICE: &str = "com.daybrief.app";
const MAIN_WINDOW: &str = "main";
const UP_NEXT_WINDOW: &str = "upnext";
/// The small panel hides itself after this long if the user ignores it.
const UP_NEXT_SECONDS: u64 = 14;

pub(crate) struct AppState {
    /// The UI's own connection. Sync work uses separate connections so it never blocks this one.
    pub store: Mutex<Store>,
    pub db_path: PathBuf,
    pub key: Redacted,
    pub tz: Tz,
    pub deps: Deps,
    pub sync: SyncHandle,
    /// Set to abort a sign-in that is waiting for the browser.
    pub oauth_cancel: Arc<std::sync::atomic::AtomicBool>,
    /// Held for the duration of a sign-in so only one runs at a time.
    pub oauth_lock: Arc<Mutex<()>>,
}

// ---- helpers --------------------------------------------------------------

fn local_tz() -> Tz {
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
            .title("Daybrief")
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
    use daybrief_core::briefing::{
        BriefingSettings, build, day_bounds,
        rules::{Action, UnlockRules, decide},
    };
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let now = Utc::now();
    let tz = state.tz;
    let Ok(store) = state.store.lock() else {
        return;
    };
    let today = now.with_timezone(&tz).date_naive();
    let _ = store.log_briefing(today, "unlock", now);

    let saved = |key: &str| store.get_setting(key).ok().flatten();
    let rules: UnlockRules = saved("unlock_rules")
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    let settings: BriefingSettings = saved("briefing")
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    let (Ok(rule_state), (from, _)) = (store.rule_state(tz), day_bounds(today, tz)) else {
        return;
    };
    let (_, to) = day_bounds(today + chrono::Duration::days(1), tz);
    let events = store
        .events_between(from, to)
        .map(daybrief_core::dedupe::dedupe)
        .unwrap_or_default();
    let next_start = build(&events, now, tz, &settings)
        .next
        .filter(|n| !n.in_progress)
        .map(|n| n.start);

    // Detecting another app in full screen is not implemented yet, so this is always false.
    let action = decide(now, tz, &rules, &rule_state, next_start, false);
    log::info!("unlock decision: {action:?}");
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
    let quit = MenuItem::with_id(app, "quit", "Quit Daybrief", true, Some("CmdOrCtrl+Q"))?;
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &at_login,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;
    TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(true)
        .tooltip("Daybrief")
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

pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        // A second launch just brings the existing window forward.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app)
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .invoke_handler(tauri::generate_handler![
            commands::get_events,
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
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // Database key lives only in the OS keychain.
            let keychain = keychain::KeychainStore::new(KEYCHAIN_SERVICE);
            let key = load_or_create_db_key(&keychain).map_err(|e| {
                log::error!("keychain unavailable: {e}");
                e.to_string()
            })?;

            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let db_path = dir.join("daybrief.db");
            let store = Store::open(&db_path, &key).map_err(|e| {
                log::error!("cannot open database: {e}");
                e.to_string()
            })?;

            let tz = local_tz();
            let deps = Deps {
                http: Arc::new(ReqwestClient::new().map_err(|e| e.to_string())?),
                secrets: Arc::new(keychain),
                tz,
                oauth: providers::OAuthConfigs::from_build(),
                system: Arc::new(eventkit::EventKitSource),
            };
            let sync = scheduler::spawn(
                app.handle().clone(),
                db_path.clone(),
                key.clone(),
                deps.clone(),
            );
            sync.trigger(Trigger::Launch);
            app.manage(AppState {
                store: Mutex::new(store),
                db_path,
                key,
                tz,
                deps,
                sync,
                oauth_cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                oauth_lock: Arc::new(Mutex::new(())),
            });

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

            // Launched by the login item: stay in the tray. Otherwise show the window.
            if !std::env::args().any(|a| a == "--hidden") {
                show_main(app.handle());
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Daybrief");
}
