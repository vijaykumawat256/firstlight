//! Writes the real scripts out so they can be run by hand on a Windows PC with classic Outlook:
//! `FIRSTLIGHT_SCRIPT_DIR=<folder> cargo test -p firstlight-core live_outlook_scripts -- --ignored`
//! then `powershell -ExecutionPolicy Bypass -File <folder>\calendars.ps1`. For the events script, put a store and
//! entry id from the calendars answer in `FIRSTLIGHT_OUTLOOK_STORE` / `FIRSTLIGHT_OUTLOOK_ENTRY` first.

use super::*;

#[test]
#[ignore = "writes files for a run by hand; needs FIRSTLIGHT_SCRIPT_DIR"]
fn live_outlook_scripts() {
    let dir = std::env::var("FIRSTLIGHT_SCRIPT_DIR").expect("set FIRSTLIGHT_SCRIPT_DIR");
    let dir = std::path::Path::new(&dir);
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("calendars.ps1"), calendars_script()).unwrap();

    let hex = |name: &str| {
        std::env::var(name)
            .ok()
            .filter(|v| is_hex(v))
            .unwrap_or_else(|| "00".into())
    };
    let now = chrono::Utc::now().timestamp();
    let script = events_script(
        &hex("FIRSTLIGHT_OUTLOOK_STORE"),
        &hex("FIRSTLIGHT_OUTLOOK_ENTRY"),
        now - 30 * 86_400,
        now + 62 * 86_400,
    );
    std::fs::write(dir.join("events.ps1"), script).unwrap();
}
