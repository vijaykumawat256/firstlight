//! Do Not Disturb during meetings (macOS, opt-in: `settings.day.dnd_meetings`).
//!
//! macOS gives apps no switch for Focus, but the person's own Shortcuts can set it. The person makes two one-step
//! shortcuts ("Set Focus: Do Not Disturb, On" and "…, Off") with the names below; Firstlight runs them with the
//! system's `shortcuts` tool when a meeting, a focus block or the focus timer starts and ends.
//!
//! It only turns Do Not Disturb off if it turned it on, and "on" counts only when the On shortcut succeeded. macOS
//! keeps the current Focus in a protected file, so Firstlight cannot see a Focus the person set themselves unless
//! they also make the optional third shortcut ("Get Current Focus"); with it, a Focus that is already on is left
//! alone. If the app quits (or stops) while it has Do Not Disturb on, it turns it off on the way out or at its next
//! start (a marker file in the app's data folder). Windows has no supported way for an app to do this, so the setting
//! is not offered there.

use crate::commands::CmdResult;
use serde::Serialize;
use std::process::Command;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub(crate) const AVAILABLE: bool = cfg!(target_os = "macos");
pub(crate) const ON_NAME: &str = "Firstlight Focus On";
pub(crate) const OFF_NAME: &str = "Firstlight Focus Off";
/// Optional: one action, "Get Current Focus". Tells whether a Focus is already on.
pub(crate) const STATUS_NAME: &str = "Firstlight Focus Status";
const TOOL: &str = "/usr/bin/shortcuts";

/// Whether Firstlight turned Do Not Disturb on (so only Firstlight turns it off again).
static ON_BY_US: AtomicBool = AtomicBool::new(false);
/// Whether the meeting (or focus time) going on now has been dealt with: turned on, or left alone.
static HANDLED: AtomicBool = AtomicBool::new(false);
/// The marker file that says Firstlight has Do Not Disturb on (survives a crash).
static MARKER: OnceLock<std::path::PathBuf> = OnceLock::new();

/// Runs the tool with a time limit; its output if it finished successfully.
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

fn mark(on: bool) {
    if let Some(p) = MARKER.get() {
        let _ = if on {
            std::fs::write(p, b"on")
        } else {
            std::fs::remove_file(p)
        };
    }
}

/// At start: if the last run left Do Not Disturb on, turn it off.
pub(crate) fn init(data_dir: &std::path::Path) {
    let path = data_dir.join("dnd-on");
    let left_on = path.exists();
    let _ = MARKER.set(path);
    if AVAILABLE && left_on {
        std::thread::spawn(|| {
            if set(false) {
                log::info!("do not disturb left on by the last run: turned off");
            }
            mark(false);
        });
    }
}

/// On the way out: turn off what we turned on.
pub(crate) fn on_exit() {
    if AVAILABLE && ON_BY_US.swap(false, Ordering::SeqCst) && set(false) {
        mark(false);
    }
}

/// Which of the three shortcuts exist (on, off, status).
pub(crate) fn shortcuts_found() -> (bool, bool, bool) {
    let Some(list) = run(&["list"], Duration::from_secs(10)) else {
        return (false, false, false);
    };
    let has = |name: &str| list.lines().any(|l| l.trim() == name);
    (has(ON_NAME), has(OFF_NAME), has(STATUS_NAME))
}

fn set(on: bool) -> bool {
    run(
        &["run", if on { ON_NAME } else { OFF_NAME }],
        Duration::from_secs(20),
    )
    .is_some()
}

/// Whether a Focus is on now, from the optional status shortcut; `None` without it.
fn focus_is_on() -> Option<bool> {
    let out = std::env::temp_dir().join(format!("firstlight-focus-{}.txt", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let path = out.to_str()?;
    run(
        &[
            "run",
            STATUS_NAME,
            "--output-path",
            path,
            "--output-type",
            "public.plain-text",
        ],
        Duration::from_secs(15),
    )?;
    let text = std::fs::read_to_string(&out).unwrap_or_default();
    let _ = std::fs::remove_file(&out);
    Some(!text.trim().is_empty())
}

/// What to do this tick: `Some(true)` = something started (decide about turning on), `Some(false)` = it ended.
pub(crate) fn decide(want: bool, handled: bool) -> Option<bool> {
    match (want, handled) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

/// Called every notifier tick: when something you are in starts, turn Do Not Disturb on (unless a Focus is already
/// on); when it ends, turn it off only if we turned it on. The shortcuts run on their own thread, so a slow Shortcuts
/// never holds up the notifier, and each start or end is acted on once.
pub(crate) fn follow(enabled: bool, busy: bool) {
    if !AVAILABLE {
        return;
    }
    match decide(enabled && busy, HANDLED.load(Ordering::SeqCst)) {
        Some(true) => {
            HANDLED.store(true, Ordering::SeqCst);
            std::thread::spawn(|| {
                if focus_is_on() == Some(true) {
                    log::info!("a focus is already on: left alone");
                    return;
                }
                if set(true) {
                    ON_BY_US.store(true, Ordering::SeqCst);
                    mark(true);
                    log::info!("do not disturb turned on");
                } else {
                    log::warn!("do not disturb shortcut could not run");
                }
            });
        }
        Some(false) => {
            HANDLED.store(false, Ordering::SeqCst);
            if ON_BY_US.swap(false, Ordering::SeqCst) {
                std::thread::spawn(|| {
                    if set(false) {
                        log::info!("do not disturb turned off");
                    }
                    mark(false);
                });
            }
        }
        None => {}
    }
}

#[derive(Serialize)]
pub struct DndStatus {
    available: bool,
    on_found: bool,
    off_found: bool,
    status_found: bool,
    on_name: &'static str,
    off_name: &'static str,
    status_name: &'static str,
}

/// Whether Do Not Disturb can be followed here, and which of the shortcuts exist.
#[tauri::command]
pub async fn dnd_status() -> CmdResult<DndStatus> {
    let (on_found, off_found, status_found) = if AVAILABLE {
        shortcuts_found()
    } else {
        (false, false, false)
    };
    Ok(DndStatus {
        available: AVAILABLE,
        on_found,
        off_found,
        status_found,
        on_name: ON_NAME,
        off_name: OFF_NAME,
        status_name: STATUS_NAME,
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

/// Opens the Shortcuts app so the shortcuts can be made.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_start_and_end_is_acted_on_once() {
        assert_eq!(decide(true, false), Some(true));
        assert_eq!(
            decide(true, true),
            None,
            "still in the meeting: nothing more"
        );
        assert_eq!(decide(false, true), Some(false));
        assert_eq!(decide(false, false), None);
    }

    #[test]
    fn a_marker_left_by_the_last_run_is_cleared_at_start() {
        let dir = std::env::temp_dir().join(format!("firstlight-dnd-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("dnd-on"), b"on").unwrap();
        init(&dir);
        // The Off shortcut runs on its own thread (and is missing here); the marker goes either way.
        for _ in 0..100 {
            if !dir.join("dnd-on").exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(!AVAILABLE || !dir.join("dnd-on").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
