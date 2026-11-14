fn main() {
    // Only these commands exist as permissions. The UI can call nothing else (PLAN.md, section 4.5).
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "get_events",
            "get_calendars",
            "get_autostart",
            "set_autostart",
            "get_unlock_count_today",
        ]),
    ))
    .expect("failed to run tauri-build");
}
