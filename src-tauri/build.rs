use std::collections::HashMap;

/// OAuth client IDs are baked into the build. They are not secrets for desktop apps (PKCE protects the
/// flow), but they are kept out of git: put them in `../.env` (see `.env.example`) or the environment.
const OAUTH_VARS: [&str; 3] = [
    "GOOGLE_OAUTH_CLIENT_ID",
    "GOOGLE_OAUTH_CLIENT_SECRET",
    "MICROSOFT_OAUTH_CLIENT_ID",
];

fn read_dotenv() -> HashMap<String, String> {
    let mut out = HashMap::new();
    if let Ok(text) = std::fs::read_to_string("../.env") {
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if let Some((k, v)) = line.split_once('=') {
                out.insert(k.trim().to_string(), v.trim().trim_matches('"').to_string());
            }
        }
    }
    out
}

fn main() {
    println!("cargo:rerun-if-changed=../.env");
    let dotenv = read_dotenv();
    for var in OAUTH_VARS {
        println!("cargo:rerun-if-env-changed={var}");
        let value = std::env::var(var)
            .ok()
            .or_else(|| dotenv.get(var).cloned())
            .unwrap_or_default();
        if !value.is_empty() {
            println!("cargo:rustc-env={var}={value}");
        }
    }

    // Only these commands exist as permissions. The UI can call nothing else (PLAN.md, section 4.5).
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "get_events",
            "get_calendars",
            "list_accounts",
            "add_ics_account",
            "add_caldav_account",
            "add_oauth_account",
            "cancel_oauth",
            "oauth_available",
            "add_demo_account",
            "remove_account",
            "rename_account",
            "set_calendar_visible",
            "set_calendar_colour",
            "refresh_now",
            "get_autostart",
            "set_autostart",
            "get_unlock_count_today",
        ]),
    ))
    .expect("failed to run tauri-build");
}
