//! Windows half of the classic-Outlook reader: starts PowerShell hidden and waits for it, with a limit.
//! Everything that decides what is asked and what the answer means is in `firstlight_core::providers::system::outlook`
//! (tested on any computer); this file only runs the script. It has been type-checked for Windows but never run.

use firstlight_core::providers::system::outlook::{OutlookSource, ScriptRunner, encode_command};
use firstlight_core::{Error, Result};
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// No console window flashes up.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// Starting Outlook for the first time can take a while.
const LIMIT: Duration = Duration::from_secs(60);
/// Whatever the script prints is read up to this much.
const MAX_OUTPUT: u64 = 16 * 1024 * 1024;

#[derive(Default)]
pub struct PowerShell {
    installed: Mutex<Option<(Instant, bool)>>,
}

pub fn source() -> OutlookSource<PowerShell> {
    OutlookSource::new(PowerShell::default())
}

fn fail(m: &str) -> Error {
    Error::Provider(m.into())
}

impl ScriptRunner for PowerShell {
    fn installed(&self) -> bool {
        let mut cached = self.installed.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((at, yes)) = *cached
            && at.elapsed() < Duration::from_secs(60)
        {
            return yes;
        }
        // Classic Outlook registers this name; the new, web-based Outlook does not.
        let yes = Command::new("reg")
            .args(["query", r"HKCR\Outlook.Application\CLSID"])
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        *cached = Some((Instant::now(), yes));
        yes
    }

    fn run(&self, script: &str) -> Result<String> {
        // The script goes in as one encoded command: no file, nothing for a shell to reinterpret.
        let mut child = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-InputFormat",
                "None",
                "-OutputFormat",
                "Text",
                "-EncodedCommand",
                &encode_command(script),
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| fail("PowerShell couldn't be started on this computer."))?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| fail("PowerShell gave no output"))?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| fail("PowerShell gave no output"))?;
        let out_reader = std::thread::spawn(move || {
            let mut out = Vec::new();
            let _ = (&mut stdout).take(MAX_OUTPUT).read_to_end(&mut out);
            out
        });
        let err_reader = std::thread::spawn(move || {
            let mut out = Vec::new();
            let _ = (&mut stderr).take(4096).read_to_end(&mut out);
            out
        });
        let start = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break s,
                Ok(None) if start.elapsed() > LIMIT => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(fail(
                        "Outlook didn't answer within a minute. Open Outlook yourself, close any window it shows (such as choosing a profile), then try again.",
                    ));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(100)),
                Err(_) => return Err(fail("PowerShell stopped unexpectedly")),
            }
        };
        let out = out_reader
            .join()
            .map_err(|_| fail("PowerShell output was lost"))?;
        let out = String::from_utf8_lossy(&out).into_owned();
        // A script always ends by printing its answer or its failure, whatever its exit code.
        if out.contains("FLJSON:") || out.contains("FLERR:") {
            return Ok(out);
        }
        let err = err_reader.join().unwrap_or_default();
        let said = String::from_utf8_lossy(&err)
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty() && !l.starts_with("#< CLIXML"))
            .map(|l| {
                l.chars()
                    .filter(|c| !c.is_control())
                    .take(200)
                    .collect::<String>()
            })
            .unwrap_or_default();
        Err(fail(&format!(
            "PowerShell stopped without an answer (exit code {}){}",
            status.code().map_or("?".to_string(), |c| c.to_string()),
            if said.is_empty() {
                String::new()
            } else {
                format!(": {said}")
            }
        )))
    }
}
