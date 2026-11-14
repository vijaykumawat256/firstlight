//! Daybrief desktop shell: windows, tray, autostart, unlock detection, and the
//! small set of commands the UI is allowed to call. All calendar logic lives in
//! `daybrief-core`.

mod keychain;
mod unlock;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use chrono_tz::Tz;
use daybrief_core::model::{Calendar, Event};
use daybrief_core::providers::{Provider, Window, fixture::FixtureProvider};
use daybrief_core::secrets::load_or_create_db_key;
use daybrief_core::store::Store;
use std::sync::Mutex;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

const KEYCHAIN_SERVICE: &str = "com.daybrief.app";
const MAIN_WINDOW: &str = "main";
/// Longest range the UI may ask for in one call.
const MAX_RANGE_DAYS: i64 = 62;

struct AppState {
    store: Mutex<Store>,
    tz: Tz,
}

type CmdResult<T> = Result<T, String>;

fn internal<E: std::fmt::Debug>(what: &'static str) -> impl Fn(E) -> String {
    // Log the detail (never content), return a generic message to the UI.
    move |e| {
        log::error!("{what}: {e:?}");
        format!("{what} failed")
    }
}

// ---- commands (inputs are validated here, never trusted from the UI) ----------

#[tauri::command]
fn get_events(state: State<AppState>, start: String, end: String) -> CmdResult<Vec<Event>> {
    let start = DateTime::parse_from_rfc3339(&start)
        .map_err(|_| "invalid start")?
        .with_timezone(&Utc);
    let end = DateTime::parse_from_rfc3339(&end)
        .map_err(|_| "invalid end")?
        .with_timezone(&Utc);
    if end <= start {
        return Err("end must be after start".into());
    }
    if end - start > Duration::days(MAX_RANGE_DAYS) {
        return Err("range too large".into());
    }
    let store = state.store.lock().map_err(|_| "state unavailable")?;
    store
        .events_between(start, end)
        .map_err(internal("loading events"))
}

#[tauri::command]
fn get_calendars(state: State<AppState>) -> CmdResult<Vec<Calendar>> {
    let store = state.store.lock().map_err(|_| "state unavailable")?;
    store
        .list_calendars()
        .map_err(internal("loading calendars"))
}

#[tauri::command]
fn get_autostart(app: AppHandle) -> CmdResult<bool> {
    app.autolaunch()
        .is_enabled()
        .map_err(internal("reading autostart"))
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> CmdResult<()> {
    let launcher = app.autolaunch();
    if enabled {
        launcher.enable()
    } else {
        launcher.disable()
    }
    .map_err(internal("changing autostart"))
}

#[tauri::command]
fn get_unlock_count_today(state: State<AppState>) -> CmdResult<i64> {
    let store = state.store.lock().map_err(|_| "state unavailable")?;
    let today = Utc::now().with_timezone(&state.tz).date_naive();
    store
        .count_briefing_log(today, "unlock")
        .map_err(internal("reading unlock log"))
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

/// Phase 1 only: load the fixture calendars into the encrypted store so the UI has data.
fn seed_fixture(store: &mut Store, today: NaiveDate, tz: Tz) -> daybrief_core::Result<()> {
    let provider = FixtureProvider::new(today, tz);
    let account = FixtureProvider::account(Utc::now());
    store.upsert_account(&account)?;
    let from = Utc::now() - Duration::days(30);
    let window = Window {
        from,
        to: from + Duration::days(120),
    };
    for cal in provider.list_calendars(&account)? {
        store.upsert_calendar(&cal)?;
        let result = provider.sync_events(&cal, None, &window)?;
        store.replace_events(&cal.id, &result.changed)?;
        store.set_sync_state(&cal.id, result.next_state.as_deref(), None)?;
    }
    Ok(())
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
            get_events,
            get_calendars,
            get_autostart,
            set_autostart,
            get_unlock_count_today
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
            let mut store = Store::open(&dir.join("daybrief.db"), &key).map_err(|e| {
                log::error!("cannot open database: {e}");
                e.to_string()
            })?;

            let tz = local_tz();
            let today = Utc::now().with_timezone(&tz).date_naive();
            seed_fixture(&mut store, today, tz).map_err(|e| e.to_string())?;
            app.manage(AppState {
                store: Mutex::new(store),
                tz,
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
