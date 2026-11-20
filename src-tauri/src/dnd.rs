//! Do Not Disturb during meetings (macOS, opt-in: `settings.day.dnd_meetings`).
//!
//! macOS gives apps no switch for Focus, but the person's own Shortcuts can set it. The person makes two one-step
//! shortcuts ("Set Focus: Do Not Disturb, On" and "…, Off") with the names below; Firstlight runs them with the
//! system's `shortcuts` tool when a meeting, a focus block or the focus timer starts and ends. It only turns Do Not
//! Disturb off if it turned it on, so a Focus the person set themselves is left alone. Windows has no supported way
//! for an app to do this, so the setting is not offered there.

use crate::commands::CmdResult;
use serde::Serialize;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub(crate) const AVAILABLE: bool = cfg!(target_os = "macos");
pub(crate) const ON_NAME: &str = "Firstlight Focus On";
pub(crate) const OFF_NAME: &str = "Firstlight Focus Off";
const TOOL: &str = "/usr/bin/shortcuts";

/// Whether Firstlight turned Do Not Disturb on (so only Firstlight turns it off again).
static ON_BY_US: AtomicBool = AtomicBool::new(false);

/// Runs the tool with a time limit; true if it finished successfully.
fn run(args: &[&str], limit: Duration) -> Option<String> {
    if !AVAILABLE {
        return None;
    }
    let mut child = Command::new(TOOL)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut out = String::new();
                if let Some(so) = child.stdout.take() {
                    use std::io::Read;
                    let _ = so.take(64 * 1024).read_to_string(&mut out);
                }
                return status.success().then_some(out);
            }
            Ok(None) if started.elapsed() < limit => std::thread::sleep(Duration::from_millis(100)),
            _ => {
                let _ = child.kill();
                return None;
            }
        }
    }
}

/// Which of the two shortcuts exist.
pub(crate) fn shortcuts_found() -> (bool, bool) {
    let Some(list) = run(&["list"], Duration::from_secs(10)) else {
        return (false, false);
    };
    let has = |name: &str| list.lines().any(|l| l.trim() == name);
    (has(ON_NAME), has(OFF_NAME))
}

fn set(on: bool) -> bool {
    run(
        &["run", if on { ON_NAME } else { OFF_NAME }],
        Duration::from_secs(20),
    )
    .is_some()
}

/// Called every notifier tick: on when something you are in starts, off (only if we turned it on) when it ends.
pub(crate) fn follow(enabled: bool, busy: bool) {
    if !AVAILABLE {
        return;
    }
    let ours = ON_BY_US.load(Ordering::SeqCst);
    let want = enabled && busy;
    // The flag changes first and the shortcut runs on its own thread, so a slow Shortcuts never holds up the
    // notifier, and a failing one is not retried every tick.
    if want != ours {
        ON_BY_US.store(want, Ordering::SeqCst);
        std::thread::spawn(move || {
            if set(want) {
                log::info!("do not disturb turned {}", if want { "on" } else { "off" });
            } else {
                log::warn!("do not disturb shortcut could not run");
            }
        });
    }
}

#[derive(Serialize)]
pub struct DndStatus {
    available: bool,
    on_found: bool,
    off_found: bool,
    on_name: &'static str,
    off_name: &'static str,
}

/// Whether Do Not Disturb can be followed here, and which of the two shortcuts exist.
#[tauri::command]
pub async fn dnd_status() -> CmdResult<DndStatus> {
    let (on_found, off_found) = if AVAILABLE {
        shortcuts_found()
    } else {
        (false, false)
    };
    Ok(DndStatus {
        available: AVAILABLE,
        on_found,
        off_found,
        on_name: ON_NAME,
        off_name: OFF_NAME,
    })
}

/// Runs one of the shortcuts now, to try the setup.
#[tauri::command]
pub async fn try_dnd(on: bool) -> CmdResult<()> {
    if !AVAILABLE {
        return Err("Do Not Disturb can only be set on a Mac.".into());
    }
    if set(on) {
        Ok(())
    } else {
        Err(format!(
            "Couldn't run the “{}” shortcut. Check it exists in the Shortcuts app.",
            if on { ON_NAME } else { OFF_NAME }
        ))
    }
}

/// Opens the Shortcuts app so the two shortcuts can be made.
#[tauri::command]
pub async fn open_shortcuts_app() -> CmdResult<()> {
    if !AVAILABLE {
        return Err("The Shortcuts app is on Macs only.".into());
    }
    Command::new("/usr/bin/open")
        .args(["-a", "Shortcuts"])
        .spawn()
        .map(|_| ())
        .map_err(|_| "Couldn't open Shortcuts.".to_string())
}
