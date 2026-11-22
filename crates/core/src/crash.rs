//! Crash reports that contain no content.
//!
//! A report says which version of Firstlight stopped, on what system, and *where in the code* (a source file and
//! line). It deliberately cannot say why in words: the panic message is not an input, because it could contain
//! text from a calendar event or an email. Paths are scrubbed so a username can never appear. Reports are
//! saved on this computer only, and only if the user turned them on.

use chrono::{DateTime, Utc};

/// Keeps only the part of a source path that identifies our (or a library's) code, never a user's folders.
pub fn scrub_path(path: &str) -> String {
    let p = path.replace('\\', "/");
    // Library code: ".../registry/src/index.crates.io-xxxx/chrono-0.4.45/src/lib.rs" -> "chrono-0.4.45/src/lib.rs"
    if let Some(i) = p.find("/registry/src/") {
        let rest = &p[i + "/registry/src/".len()..];
        return rest
            .split_once('/')
            .map_or_else(|| "library".to_string(), |(_, after)| after.to_string());
    }
    // Our code: "<anything>/crates/core/src/x.rs" or "<anything>/src-tauri/src/x.rs"
    for marker in ["crates/", "src-tauri/", "src/"] {
        if let Some(i) = p.find(marker) {
            return p[i..].to_string();
        }
    }
    // Standard library and anything else: the file name only.
    p.rsplit('/').next().unwrap_or("unknown").to_string()
}

/// The text of a report. Note what is *not* here: the panic message.
pub fn report_text(
    version: &str,
    os: &str,
    arch: &str,
    thread: &str,
    location: Option<(&str, u32)>,
    at: DateTime<Utc>,
) -> String {
    let place = location.map_or_else(
        || "unknown".to_string(),
        |(file, line)| format!("{}:{line}", scrub_path(file)),
    );
    let thread: String = thread
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(32)
        .collect();
    format!(
        "Firstlight crash report\nTime: {}\nVersion: {version}\nSystem: {os} {arch}\nThread: {thread}\nStopped at: {place}\n\nThis report contains no calendar, email or account information.\n",
        at.format("%Y-%m-%d %H:%M:%S UTC")
    )
}

/// Most warning and error lines a problem report carries.
pub const REPORT_LOG_LINES: usize = 60;

/// One log line made safe to share: the home folder becomes "~", anything shaped like an email address becomes
/// "[address]", and it is cut at 300 characters. (Log lines never carry event or mail text by rule; this is a second
/// guard against a name in a path or an address in an error.)
fn scrub_line(line: &str, home: Option<&str>) -> String {
    let mut l = line.to_string();
    if let Some(h) = home.filter(|h| h.len() > 1) {
        l = l.replace(h, "~");
        l = l.replace(&h.replace('\\', "/"), "~");
    }
    let words: Vec<String> = l
        .split(' ')
        .map(|w| {
            let at = w.find('@');
            match at {
                Some(i) if i > 0 && w[i + 1..].contains('.') => "[address]".to_string(),
                _ => w.to_string(),
            }
        })
        .collect();
    words.join(" ").chars().take(300).collect()
}

/// The text of a problem report the person can read in full, save and email: version and system, the last crash
/// report, and the recent warnings and errors from the log (not the ordinary lines).
pub fn problem_report(
    version: &str,
    os: &str,
    arch: &str,
    crash: Option<&str>,
    log: &str,
    home: Option<&str>,
    at: DateTime<Utc>,
) -> String {
    let lines: Vec<String> = log
        .lines()
        .filter(|l| l.contains("[WARN]") || l.contains("[ERROR]"))
        .map(|l| scrub_line(l, home))
        .collect();
    let recent = &lines[lines.len().saturating_sub(REPORT_LOG_LINES)..];
    let mut out = format!(
        "Firstlight problem report\nMade: {}\nVersion: {version}\nSystem: {os} {arch}\n\nWhat happened (write it here):\n\n\n",
        at.format("%Y-%m-%d %H:%M UTC")
    );
    out.push_str("--- Last crash report ---\n");
    out.push_str(crash.unwrap_or("None saved.\n"));
    out.push_str("\n--- Recent warnings and errors ---\n");
    if recent.is_empty() {
        out.push_str("None.\n");
    }
    for l in recent {
        out.push_str(l);
        out.push('\n');
    }
    out.push_str("\nThis report contains no calendar, email or account content.\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn a_problem_report_keeps_only_warnings_and_errors_and_hides_names_and_addresses() {
        let log = "\
[2026-11-21][09:00:00][firstlight_lib][INFO] sync done
[2026-11-21][09:01:00][firstlight_lib][WARN] folder sync failed at /Users/jane/Library/CloudStorage/x
[2026-11-21][09:02:00][firstlight_lib][ERROR] sign-in for jane@example.com failed
";
        let at = Utc.with_ymd_and_hms(2026, 11, 21, 10, 0, 0).unwrap();
        let r = problem_report(
            "0.2.0",
            "macos",
            "aarch64",
            None,
            log,
            Some("/Users/jane"),
            at,
        );
        assert!(r.contains("Version: 0.2.0"));
        assert!(!r.contains("sync done"), "ordinary lines are left out");
        assert!(r.contains("folder sync failed at ~/Library/CloudStorage/x"));
        assert!(r.contains("sign-in for [address] failed"));
        assert!(!r.contains("jane"));
        assert!(r.contains("None saved."));
        // Only the newest lines.
        let many: String = (0..100).map(|i| format!("[x][ERROR] line {i}\n")).collect();
        let r = problem_report("1", "w", "x", Some("crash!\n"), &many, None, at);
        assert!(r.contains("line 99") && !r.contains("line 39\n") && r.contains("line 40\n"));
        assert!(r.contains("crash!"));
    }

    #[test]
    fn paths_never_reveal_a_username_or_folder_layout() {
        assert_eq!(
            scrub_path("/Users/jane.doe/Desktop/calendar/crates/core/src/ics/mod.rs"),
            "crates/core/src/ics/mod.rs"
        );
        assert_eq!(
            scrub_path("C:\\Users\\jane\\code\\firstlight\\src-tauri\\src\\lib.rs"),
            "src-tauri/src/lib.rs"
        );
        assert_eq!(
            scrub_path(
                "/Users/jane/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/chrono-0.4.45/src/lib.rs"
            ),
            "chrono-0.4.45/src/lib.rs"
        );
        assert_eq!(
            scrub_path("/rustc/abc123/library/core/src/option.rs"),
            "src/option.rs"
        );
        assert_eq!(scrub_path("/home/bob/weird/place/thing.rs"), "thing.rs");
        for p in [
            "/Users/jane/x/crates/core/src/a.rs",
            "/Users/jane/.cargo/registry/src/i/c-1/src/l.rs",
            "/home/bob/q/thing.rs",
        ] {
            assert!(
                !scrub_path(p).contains("jane") && !scrub_path(p).contains("bob"),
                "{p}"
            );
        }
    }

    #[test]
    fn a_report_has_version_system_and_place_and_no_message() {
        let at = Utc.with_ymd_and_hms(2026, 3, 10, 9, 30, 0).unwrap();
        let r = report_text(
            "0.1.0",
            "macos",
            "aarch64",
            "sync",
            Some((
                "/Users/jane/Desktop/calendar/crates/core/src/ics/mod.rs",
                123,
            )),
            at,
        );
        assert!(
            r.contains("Version: 0.1.0")
                && r.contains("macos aarch64")
                && r.contains("Thread: sync")
        );
        assert!(r.contains("Stopped at: crates/core/src/ics/mod.rs:123"));
        assert!(r.contains("2026-03-10 09:30:00 UTC"));
        assert!(!r.contains("jane") && !r.contains("/Users"));
    }

    #[test]
    fn a_missing_location_and_odd_thread_names_are_handled() {
        let at = Utc.with_ymd_and_hms(2026, 3, 10, 9, 30, 0).unwrap();
        let r = report_text(
            "0.1.0",
            "windows",
            "x86_64",
            "worker <script>alert(1)</script>\nSECRET",
            None,
            at,
        );
        assert!(r.contains("Stopped at: unknown"));
        let thread_lines: Vec<&str> = r.lines().filter(|l| l.starts_with("Thread:")).collect();
        assert_eq!(
            thread_lines.len(),
            1,
            "a newline in the name cannot inject extra lines: {r}"
        );
        assert!(
            thread_lines[0]["Thread: ".len()..]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "{}",
            thread_lines[0]
        );
        assert!(!r.contains('<') && !r.contains('>'));
    }

    /// The function has no parameter for a message: nothing a panic says can ever reach a report. This test
    /// makes that explicit by panicking with private text and building the report the way the hook does.
    #[test]
    fn private_text_in_a_panic_message_cannot_reach_the_report() {
        let secret = "Dinner with the lawyer about the divorce";
        let caught = std::panic::catch_unwind(|| -> () { panic!("{secret}") });
        let payload = caught.unwrap_err();
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .unwrap_or_default();
        assert!(
            message.contains("divorce"),
            "the panic really carried the text"
        );
        let r = report_text(
            "0.1.0",
            "macos",
            "aarch64",
            "main",
            Some(("crates/core/src/x.rs", 1)),
            Utc::now(),
        );
        assert!(!r.contains("lawyer") && !r.contains("divorce"));
    }
}
