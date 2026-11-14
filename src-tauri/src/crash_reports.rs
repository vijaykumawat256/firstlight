//! Opt-in, local, content-free crash reports (see `daybrief_core::crash` for what a report contains).
//!
//! The hook is installed once at startup and does nothing unless the user turned reports on. A report is
//! written to the app's log folder; nothing is ever sent anywhere.

use chrono::Utc;
use daybrief_core::crash::report_text;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(false);
const KEEP: usize = 5;

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

fn reports_dir(log_dir: &Path) -> PathBuf {
    log_dir.join("crash-reports")
}

/// Replaces the panic hook. In release builds the default hook is not called: it prints the panic message,
/// which could contain content from the data being processed.
pub fn install_hook(log_dir: PathBuf, version: &'static str) {
    #[cfg(debug_assertions)]
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if ENABLED.load(Ordering::Relaxed) {
            let thread = std::thread::current();
            let text = report_text(
                version,
                std::env::consts::OS,
                std::env::consts::ARCH,
                thread.name().unwrap_or("unnamed"),
                info.location().map(|l| (l.file(), l.line())),
                Utc::now(),
            );
            write(&reports_dir(&log_dir), &text);
        }
        #[cfg(debug_assertions)]
        previous(info);
    }));
}

fn write(dir: &Path, text: &str) {
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let _ = std::fs::write(
        dir.join(format!("crash-{}.txt", Utc::now().timestamp_millis())),
        text,
    );
    // Keep only the newest few.
    if let Ok(rd) = std::fs::read_dir(dir) {
        let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        files.sort();
        let extra = files.len().saturating_sub(KEEP);
        for old in files.into_iter().take(extra) {
            let _ = std::fs::remove_file(old);
        }
    }
}

/// The most recent report, if any.
pub fn latest(log_dir: &Path) -> Option<String> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(reports_dir(log_dir))
        .ok()?
        .flatten()
        .map(|e| e.path())
        .collect();
    files.sort();
    std::fs::read_to_string(files.pop()?).ok()
}

pub fn clear(log_dir: &Path) {
    let _ = std::fs::remove_dir_all(reports_dir(log_dir));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "daybrief-crash-test-{}",
            daybrief_core::model::new_id()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn reports_are_kept_listed_and_cleared_and_old_ones_are_dropped() {
        let log = temp();
        let dir = reports_dir(&log);
        assert_eq!(latest(&log), None);
        for i in 0..8 {
            write(&dir, &format!("report {i}"));
            std::thread::sleep(std::time::Duration::from_millis(3));
        }
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), KEEP);
        assert_eq!(latest(&log).as_deref(), Some("report 7"));
        clear(&log);
        assert_eq!(latest(&log), None);
        let _ = std::fs::remove_dir_all(log);
    }
}
