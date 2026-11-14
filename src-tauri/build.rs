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
            "system_calendar_status",
            "add_system_account",
            "open_privacy_settings",
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
            "get_briefing",
            "get_app_info",
            "check_for_update",
            "install_update",
            "get_last_crash_report",
            "clear_crash_reports",
            "get_mail_digest",
            "enable_apple_mail",
            "enable_account_mail",
            "get_own_event",
            "skip_event_occurrence",
            "get_system_accent",
            "prepare_invite",
            "create_event",
            "update_event",
            "delete_event",
            "add_imap_mailbox",
            "remove_imap_mailbox",
            "open_mail_message",
            "ai_status",
            "ai_preview",
            "enable_ai",
            "disable_ai",
            "forget_ai_key",
            "get_ai_summary",
            "get_settings",
            "set_settings",
            "export_settings",
            "save_settings_export",
            "import_settings",
            "erase_all_data",
            "dismiss_up_next",
            "open_briefing",
        ]),
    ))
    .expect("failed to run tauri-build");
}
