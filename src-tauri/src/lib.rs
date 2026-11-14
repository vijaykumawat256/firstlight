//! Daybrief desktop shell: windows, tray, autostart, unlock detection, and the
//! small set of commands the UI is allowed to call. All calendar logic lives in
//! `daybrief-core`.

mod commands;
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
        ])
        .on_window_event(|window, event| {
            // Closing hides to the tray; Quit lives in the tray menu.
            if let WindowEvent::CloseRequested { api, .. } = event {
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

            // Unlock detection: log it and tell the UI. (Phase 3 turns this into the show/hide rules.)
            let handle = app.handle().clone();
            unlock::start(move || {
                log::info!("screen unlock detected");
                if let Some(state) = handle.try_state::<AppState>() {
                    let now = Utc::now();
                    if let Ok(store) = state.store.lock() {
                        let _ = store.log_briefing(
                            now.with_timezone(&state.tz).date_naive(),
                            "unlock",
                            now,
                        );
                    }
                    // Refresh calendars (skipped if one just ran; see scheduler::UNLOCK_COOLDOWN).
                    state.sync.trigger(Trigger::Unlock);
                }
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
