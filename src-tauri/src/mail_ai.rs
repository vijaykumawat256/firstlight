//! The optional email digest and AI summaries: everything here is off until the user turns it on.
//!
//! Mail is read-only and reduced to sender, subject and a short snippet (see `firstlight_core::mail`).
//! AI sends only the text that `ai_preview` shows, to the provider the user agreed to.

use crate::AppState;
use crate::commands::{CmdResult, browser_sign_in, config_for, internal, lock_store, oauth_error};
use chrono::{Duration, Utc};
use firstlight_core::ai::{
    AnthropicSummariser, LocalSummariser, OpenAiStyleSummariser, Prompt, Summariser, build_prompt,
    summarise_if_enabled,
};
use firstlight_core::briefing::{Briefing, build, day_bounds};
use firstlight_core::dedupe::dedupe;
use firstlight_core::mail::applemail::{self, Outcome};
use firstlight_core::mail::gmail::GmailSource;
use firstlight_core::mail::imap::{self, ImapMailbox, ImapPreset, ImapSource, TlsConnect};
use firstlight_core::mail::microsoft::MicrosoftMailSource;
use firstlight_core::mail::{MailDigest, OpenTarget, SourceBatch, build_digest, gmail, microsoft};
use firstlight_core::model::{Account, ProviderKind};
use firstlight_core::oauth::{Pkce, TokenManager, loopback::Loopback, random_state};
use firstlight_core::secrets::Redacted;
use firstlight_core::settings::{AiProvider, AppSettings};
use firstlight_core::{Error, oauth};
use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::time::{Duration as StdDuration, Instant};
use tauri::{AppHandle, Emitter, State};

const MAIL_CACHE: StdDuration = StdDuration::from_secs(180);
const AI_CACHE: StdDuration = StdDuration::from_secs(300);
const OSASCRIPT_TIMEOUT: StdDuration = StdDuration::from_secs(40);
/// The first permission prompt can sit on screen a while.
const PROBE_TIMEOUT: StdDuration = StdDuration::from_secs(90);
const MAX_ICS_BYTES: u64 = 256 * 1024;

// ---- results the UI receives ----------------------------------------------------------

#[derive(Serialize, Clone)]
pub struct MailProblem {
    /// "apple_mail" or the account's name.
    pub source: String,
    /// "permission" | "not_running" | "offline" | "reconnect" | "failed"
    pub kind: &'static str,
}

#[derive(Serialize, Clone)]
pub struct MailResult {
    pub enabled: bool,
    pub digest: Option<MailDigest>,
    pub problems: Vec<MailProblem>,
}

// ---- running AppleScript -------------------------------------------------------------

enum RunError {
    Permission,
    Timeout,
    Failed,
}

/// Runs `osascript` with a hard time limit. The script text is fixed; only numbers and a directory
/// path are passed as arguments, never anything taken from a message.
fn run_osascript(script: &str, args: &[&str], timeout: StdDuration) -> Result<String, RunError> {
    use std::process::{Command, Stdio};
    let child = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| RunError::Failed)?;
    let pid = child.id();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(out)) if out.status.success() => {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        }
        Ok(Ok(out)) => {
            let err = String::from_utf8_lossy(&out.stderr);
            // -1743: "Not authorized to send Apple events"; the user has not allowed (or has revoked) access.
            if err.contains("-1743") || err.to_ascii_lowercase().contains("not authorized") {
                Err(RunError::Permission)
            } else {
                Err(RunError::Failed)
            }
        }
        Ok(Err(_)) => Err(RunError::Failed),
        Err(_) => {
            let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
            Err(RunError::Timeout)
        }
    }
}

/// A private scratch directory for calendar attachments, removed when dropped.
struct ScratchDir(PathBuf);

impl ScratchDir {
    fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "firstlight-mail-{}",
            firstlight_core::model::new_id()
        ));
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            std::fs::DirBuilder::new().mode(0o700).create(&path)?;
        }
        #[cfg(not(unix))]
        std::fs::create_dir(&path)?;
        Ok(Self(path))
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Reads a saved attachment, but only from inside our own scratch directory and only if it is small.
fn read_attachment(dir: &Path, path: &str) -> Option<String> {
    let p = Path::new(path).canonicalize().ok()?;
    if !p.starts_with(dir.canonicalize().ok()?) {
        return None;
    }
    let file = std::fs::File::open(&p).ok()?;
    let mut text = String::new();
    file.take(MAX_ICS_BYTES).read_to_string(&mut text).ok()?;
    Some(text)
}

enum Apple {
    NotRunning,
    Batch(SourceBatch),
}

fn fetch_apple(days: i64) -> Result<Apple, Error> {
    let scratch = ScratchDir::new()
        .map_err(|_| Error::Provider("could not create a temporary folder".into()))?;
    let dir = scratch.0.to_string_lossy().into_owned();
    let out = run_osascript(
        applemail::SCRIPT,
        &[&days.to_string(), &applemail::MAX_FETCH.to_string(), &dir],
        OSASCRIPT_TIMEOUT,
    )
    .map_err(|e| match e {
        RunError::Permission => Error::PermissionNeeded,
        RunError::Timeout => Error::Offline,
        RunError::Failed => Error::Provider("Mail did not answer".into()),
    })?;
    match applemail::parse_output(&out).map_err(Error::Provider)? {
        Outcome::NotRunning => Ok(Apple::NotRunning),
        Outcome::Messages(parsed) => Ok(Apple::Batch(
            parsed.into_batch(|p| read_attachment(&scratch.0, p)),
        )),
    }
}

/// Classic Outlook's inbox (Windows only; see `firstlight_core::mail::outlookmail`).
fn fetch_outlook(days: i64, now: chrono::DateTime<Utc>) -> Result<SourceBatch, Error> {
    #[cfg(windows)]
    {
        firstlight_core::mail::outlookmail::fetch(
            &crate::outlook_win::PowerShell::default(),
            days,
            now,
            crate::local_tz(),
        )
    }
    #[cfg(not(windows))]
    {
        let _ = (days, now);
        Err(Error::Provider(
            "Outlook mail is read on Windows only".into(),
        ))
    }
}

// ---- collecting the digest -------------------------------------------------------------

fn problem_kind(e: &Error) -> &'static str {
    match e {
        Error::PermissionNeeded => "permission",
        Error::AuthRevoked => "reconnect",
        Error::Offline => "offline",
        _ => "failed",
    }
}

fn collect(
    state_deps: crate::providers::Deps,
    settings: AppSettings,
    accounts: Vec<Account>,
    meeting_people: Vec<String>,
) -> MailResult {
    let now = Utc::now();
    let days = settings.email.days_back;
    let mut batches = Vec::new();
    let mut problems = Vec::new();

    if settings.email.apple_mail {
        match fetch_apple(days) {
            Ok(Apple::Batch(b)) => batches.push(b),
            Ok(Apple::NotRunning) => problems.push(MailProblem {
                source: "apple_mail".into(),
                kind: "not_running",
            }),
            Err(e) => problems.push(MailProblem {
                source: "apple_mail".into(),
                kind: problem_kind(&e),
            }),
        }
    }

    if settings.email.outlook_mail {
        match fetch_outlook(days, now) {
            Ok(b) => batches.push(b),
            Err(e) => problems.push(MailProblem {
                source: "Outlook".into(),
                kind: problem_kind(&e),
            }),
        }
    }

    for account in &accounts {
        let cfg = match account.provider {
            ProviderKind::Google => state_deps.oauth.google(),
            ProviderKind::Microsoft => state_deps.oauth.microsoft(),
            _ => None,
        };
        let Some(cfg) = cfg else { continue };
        let tokens = TokenManager {
            http: state_deps.http.clone(),
            secrets: state_deps.secrets.clone(),
            cfg,
        };
        let result = match account.provider {
            ProviderKind::Google => GmailSource::new(state_deps.http.clone(), tokens).fetch(
                &account.id,
                account.email.as_deref().unwrap_or(""),
                days,
                15,
            ),
            _ => MicrosoftMailSource::new(state_deps.http.clone(), tokens).fetch(
                &account.id,
                days,
                25,
                now,
            ),
        };
        match result {
            Ok(b) => batches.push(b),
            Err(e) => problems.push(MailProblem {
                source: account.display_name.clone(),
                kind: problem_kind(&e),
            }),
        }
    }

    if !settings.email.imap.is_empty() {
        let source = ImapSource::new(Arc::new(TlsConnect), state_deps.secrets.clone());
        for mailbox in &settings.email.imap {
            match source.fetch(mailbox, days, 15, now) {
                Ok(b) => batches.push(b),
                Err(e) => problems.push(MailProblem {
                    // A custom mailbox is named by its address; the label alone ("Email") would say little.
                    source: match mailbox.preset {
                        ImapPreset::Custom => mailbox.address.clone(),
                        p => p.label().into(),
                    },
                    kind: problem_kind(&e),
                }),
            }
        }
    }

    let digest = (!batches.is_empty()).then(|| {
        build_digest(
            &batches,
            &settings.email.important_senders,
            &meeting_people,
            now,
        )
    });
    MailResult {
        enabled: true,
        digest,
        problems,
    }
}

/// Today's meeting organisers: their email counts as important too.
fn organisers_today(state: &AppState, settings: &AppSettings) -> Vec<String> {
    let (now, tz) = (Utc::now(), crate::local_tz());
    let today = now.with_timezone(&tz).date_naive();
    let (from, to) = (day_bounds(today, tz).0, day_bounds(today, tz).1);
    let Ok(store) = lock_store(state) else {
        return vec![];
    };
    let events = store
        .events_between(from, to)
        .map(dedupe)
        .unwrap_or_default();
    build(&events, now, tz, &settings.briefing)
        .remaining
        .into_iter()
        .filter_map(|i| i.event.organizer)
        .map(|o| o.trim().to_string())
        .filter(|o| !o.is_empty())
        .collect()
}

#[tauri::command]
pub async fn get_mail_digest(state: State<'_, AppState>, force: bool) -> CmdResult<MailResult> {
    let settings = lock_store(&state)?.load_settings();
    // Email is off by default, and while it is off no mail code runs at all.
    if !settings.email.enabled {
        return Ok(MailResult {
            enabled: false,
            digest: None,
            problems: vec![],
        });
    }
    let cached = state
        .mail_cache
        .lock()
        .map_err(|_| "state unavailable")?
        .as_ref()
        .filter(|(at, _)| !force && at.elapsed() < MAIL_CACHE)
        .map(|(_, r)| r.clone());
    if let Some(r) = cached {
        return Ok(r);
    }
    let accounts: Vec<Account> = lock_store(&state)?
        .list_accounts()
        .map_err(internal("loading accounts"))?
        .into_iter()
        .filter(|a| settings.email.account_ids.contains(&a.id))
        .collect();
    let people = organisers_today(&state, &settings);
    let deps = state.deps.clone();
    let result =
        tauri::async_runtime::spawn_blocking(move || collect(deps, settings, accounts, people))
            .await
            .map_err(|_| "mail failed".to_string())?;
    *state.mail_cache.lock().map_err(|_| "state unavailable")? =
        Some((Instant::now(), result.clone()));
    Ok(result)
}

fn save_settings(
    app: &AppHandle,
    state: &AppState,
    edit: impl FnOnce(&mut AppSettings),
) -> CmdResult<AppSettings> {
    let store = lock_store(state)?;
    let mut s = store.load_settings();
    edit(&mut s);
    s.sanitize();
    store
        .save_settings(&s)
        .map_err(internal("saving settings"))?;
    drop(store);
    *state.mail_cache.lock().map_err(|_| "state unavailable")? = None;
    let _ = app.emit("settings-changed", ());
    Ok(s)
}

/// Turns on reading the Mail app. Triggers macOS's one-time "Firstlight wants to control Mail" prompt.
#[tauri::command]
pub async fn enable_apple_mail(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<AppSettings> {
    let outcome = tauri::async_runtime::spawn_blocking(|| {
        run_osascript(applemail::PROBE_SCRIPT, &[], PROBE_TIMEOUT)
    })
    .await
    .map_err(|_| "mail failed".to_string())?;
    match outcome {
        Ok(_) => save_settings(&app, &state, |s| {
            s.email.apple_mail = true;
            s.email.enabled = true;
        }),
        Err(RunError::Permission) => Err("Firstlight isn't allowed to control Mail. Open System Settings, go to Automation, switch Mail on under Firstlight, then try again.".into()),
        Err(RunError::Timeout) => Err("Mail didn't respond. Open Mail, then try again.".into()),
        Err(RunError::Failed) => Err("Couldn't talk to Mail. Make sure the Mail app is installed and has an account.".into()),
    }
}

/// Turns on reading classic Outlook's inbox (Windows), after one successful read so a problem shows now, not later.
#[tauri::command]
pub async fn enable_outlook_mail(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<AppSettings> {
    let days = lock_store(&state)?.load_settings().email.days_back;
    tauri::async_runtime::spawn_blocking(move || fetch_outlook(days, Utc::now()))
        .await
        .map_err(|_| "mail failed".to_string())?
        .map_err(|e| match e {
            Error::Provider(m) => m,
            _ => "Outlook couldn't be read.".to_string(),
        })?;
    save_settings(&app, &state, |s| {
        s.email.outlook_mail = true;
        s.email.enabled = true;
    })
}

/// Asks the provider for permission to read mail for one connected account, then turns the digest on for it.
#[tauri::command]
pub async fn enable_account_mail(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
) -> CmdResult<AppSettings> {
    let account = lock_store(&state)?
        .get_account(&account_id)
        .map_err(internal("loading the account"))?
        .ok_or("That account no longer exists.")?;
    let scope = match account.provider {
        ProviderKind::Google => gmail::SCOPE,
        ProviderKind::Microsoft => microsoft::SCOPE,
        _ => return Err("Mail can only be added for Google and Microsoft accounts.".into()),
    };
    upgrade_grant(&app, &state, &account, scope).await?;
    save_settings(&app, &state, |s| {
        if !s.email.account_ids.contains(&account.id) {
            s.email.account_ids.push(account.id.clone());
        }
        s.email.enabled = true;
    })
}

/// Connects a mailbox (Yahoo, iCloud, or another provider's server on port 993) with an app password.
/// The login is tested first; the password goes to the keychain and is never returned to the page or
/// written to settings.
#[tauri::command]
pub async fn add_imap_mailbox(
    app: AppHandle,
    state: State<'_, AppState>,
    preset: String,
    address: String,
    password: String,
    host: Option<String>,
) -> CmdResult<AppSettings> {
    let preset = ImapPreset::parse(&preset).ok_or("Choose a mail provider.")?;
    // Only a custom mailbox takes a server name from the page, and it must be a plain host name.
    let host = match preset {
        ImapPreset::Custom => {
            let h = host.unwrap_or_default().trim().to_ascii_lowercase();
            if !imap::valid_host(&h) {
                return Err("Enter the mail server's name, like imap.example.com (not an address or a port).".into());
            }
            Some(h)
        }
        _ => None,
    };
    let server = host
        .clone()
        .or_else(|| preset.host().map(String::from))
        .ok_or("Choose a mail provider.")?;
    let address = address.trim().to_string();
    let password: String = password.split_whitespace().collect(); // app passwords are shown in groups
    if !imap::valid_address(&address) || !address.contains('@') {
        return Err("Enter your full email address.".into());
    }
    if password.is_empty()
        || password.len() > 256
        || !password.bytes().all(|b| (0x21..0x7f).contains(&b))
    {
        return Err("Enter the app-specific password (not your normal password).".into());
    }

    let secrets = state.deps.secrets.clone();
    let (check_addr, check_pw) = (address.clone(), password.clone());
    tauri::async_runtime::spawn_blocking(move || {
        ImapSource::new(Arc::new(TlsConnect), secrets).check_login(&server, &check_addr, &check_pw)
    })
    .await
    .map_err(|_| "mail failed".to_string())?
    .map_err(|e| match e {
        Error::AuthRevoked => format!(
            "{} didn't accept that. Use an app-specific password, and make sure IMAP access is allowed for the account.",
            preset.label()
        ),
        Error::Offline => "Couldn't reach the mail server. Check your connection and try again.".into(),
        _ => format!("Couldn't sign in to {}.", preset.label()),
    })?;

    // Same address again just replaces the password.
    let existing = lock_store(&state)?
        .load_settings()
        .email
        .imap
        .into_iter()
        .find(|m| m.preset == preset && m.host == host && m.address.eq_ignore_ascii_case(&address));
    if existing.is_none()
        && lock_store(&state)?.load_settings().email.imap.len() >= imap::MAX_MAILBOXES
    {
        return Err("That's the most mailboxes Firstlight can read. Remove one first.".into());
    }
    let id = existing
        .as_ref()
        .map_or_else(firstlight_core::model::new_id, |m| m.id.clone());
    let key = imap::secret_key(&id);
    state
        .deps
        .secrets
        .set(&key, &password)
        .map_err(internal("saving the password"))?;
    let saved = save_settings(&app, &state, |s| {
        if existing.is_none() {
            s.email.imap.push(ImapMailbox {
                id: id.clone(),
                preset,
                address: address.clone(),
                host: host.clone(),
            });
        }
        s.email.enabled = true;
    });
    if saved.is_err() && existing.is_none() {
        let _ = state.deps.secrets.delete(&key); // nothing points at it
    }
    saved
}

/// Disconnects a Yahoo or iCloud mailbox. The password is removed first; if the keychain refuses,
/// the mailbox stays so the user can try again.
#[tauri::command]
pub async fn remove_imap_mailbox(
    app: AppHandle,
    state: State<'_, AppState>,
    mailbox_id: String,
) -> CmdResult<AppSettings> {
    state
        .deps
        .secrets
        .delete(&imap::secret_key(&mailbox_id))
        .map_err(internal("removing the password"))?;
    save_settings(&app, &state, |s| {
        s.email.imap.retain(|m| m.id != mailbox_id)
    })
}

/// Hosts that may be opened for a message link.
const MAIL_HOSTS: [&str; 7] = [
    "mail.yahoo.com",
    "www.icloud.com",
    "mail.google.com",
    "outlook.office.com",
    "outlook.office365.com",
    "outlook.live.com",
    "outlook.cloud.microsoft",
];

/// Opens a message in the user's own mail app or webmail. The target is checked again here because it
/// travels through the page: only https links on known mail sites, or a well-formed Apple Mail id.
#[tauri::command]
pub async fn open_mail_message(app: AppHandle, target: OpenTarget) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let url = match target {
        OpenTarget::Url { url } => {
            let parsed = firstlight_core::http::Url::parse(&url)
                .map_err(|_| "That link isn't valid.".to_string())?;
            let host_ok = parsed.host_str().is_some_and(|h| MAIL_HOSTS.contains(&h));
            if parsed.scheme() != "https" || !host_ok {
                return Err("Refusing to open that link.".into());
            }
            parsed.to_string()
        }
        OpenTarget::AppleMail { message_id } => {
            applemail::message_url(&message_id).ok_or("That message can't be opened.")?
        }
        OpenTarget::None => return Err("Open your mail app to read this message.".into()),
    };
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|_| "Couldn't open the message.".to_string())
}

// ---- AI -------------------------------------------------------------------------------

#[derive(Serialize)]
pub struct AiStatus {
    enabled: bool,
    consented: bool,
    provider: AiProvider,
    has_key: bool,
}

/// The saved API key of a provider, if it has one and one is saved.
fn saved_key(state: &AppState, provider: AiProvider) -> Option<String> {
    let name = provider.key_name()?;
    state
        .deps
        .secrets
        .get(name)
        .ok()
        .flatten()
        .filter(|k| !k.is_empty())
}

fn has_key(state: &AppState, provider: AiProvider) -> bool {
    saved_key(state, provider).is_some()
}

/// Which providers have a key saved, so the page can say "a key is already saved" for the one being chosen.
#[tauri::command]
pub async fn ai_keys(state: State<'_, AppState>) -> CmdResult<Vec<AiProvider>> {
    Ok(AiProvider::ALL
        .into_iter()
        .filter(|p| has_key(&state, *p))
        .collect())
}

#[tauri::command]
pub async fn ai_status(state: State<'_, AppState>) -> CmdResult<AiStatus> {
    let ai = lock_store(&state)?.load_settings().ai;
    Ok(AiStatus {
        enabled: ai.enabled,
        consented: ai.consented == Some(ai.provider),
        provider: ai.provider,
        has_key: has_key(&state, ai.provider),
    })
}

fn make_summariser(state: &AppState, settings: &AppSettings) -> Option<Box<dyn Summariser>> {
    let ai = &settings.ai;
    match ai.provider {
        AiProvider::Anthropic => {
            let key = saved_key(state, AiProvider::Anthropic)?;
            Some(Box::new(AnthropicSummariser::new(
                state.deps.ai_http.clone(),
                Redacted::new(key),
                &ai.model,
            )))
        }
        AiProvider::Local => Some(Box::new(LocalSummariser::new(
            state.deps.local_http.clone(),
            &ai.endpoint,
            &ai.model,
        ))),
        // Every other service with a key speaks the same protocol; only the address and key differ.
        p => {
            let key = saved_key(state, p)?;
            let base = match p.base_url() {
                Some(fixed) => fixed.to_string(),
                None => firstlight_core::settings::valid_custom_endpoint(&ai.custom_endpoint)?,
            };
            Some(Box::new(OpenAiStyleSummariser::new(
                state.deps.ai_http.clone(),
                &base,
                Redacted::new(key),
                &ai.model,
            )))
        }
    }
}

/// Today's briefing for the prompt (the same data the briefing screen uses).
fn current_briefing(state: &AppState, settings: &AppSettings) -> CmdResult<Briefing> {
    let (now, tz) = (Utc::now(), crate::local_tz());
    let today = now.with_timezone(&tz).date_naive();
    let (from, _) = day_bounds(today, tz);
    let (_, to) = day_bounds(today + Duration::days(1), tz);
    let events = dedupe(
        lock_store(state)?
            .events_between(from, to)
            .map_err(internal("loading events"))?,
    );
    Ok(build(&events, now, tz, &settings.briefing))
}

/// The email digest for the prompt, only if the user opted in and we already have one (no fresh fetch).
fn cached_digest(state: &AppState, settings: &AppSettings) -> Option<MailDigest> {
    if !(settings.ai.include_email && settings.email.enabled) {
        return None;
    }
    state
        .mail_cache
        .lock()
        .ok()?
        .as_ref()
        .and_then(|(_, r)| r.digest.clone())
}

fn current_prompt(state: &AppState, settings: &AppSettings) -> CmdResult<Prompt> {
    let b = current_briefing(state, settings)?;
    let digest = cached_digest(state, settings);
    let use_24h = settings
        .notify_for_engine(crate::notifier::system_uses_24h())
        .time_24h;
    Ok(build_prompt(
        &b,
        digest.as_ref(),
        crate::local_tz(),
        use_24h,
    ))
}

/// The exact text that would be sent, for the user to read before agreeing.
#[tauri::command]
pub async fn ai_preview(state: State<'_, AppState>) -> CmdResult<String> {
    let settings = lock_store(&state)?.load_settings();
    Ok(current_prompt(&state, &settings)?.disclosure())
}

fn valid_key(k: &str) -> bool {
    (10..=300).contains(&k.len()) && k.bytes().all(|b| b.is_ascii_graphic())
}

/// Turns AI summaries on. The caller must have shown the preview and obtained the user's agreement
/// (`confirm`). A tiny test request checks the key or model first, so a typo never leaves it half on.
#[tauri::command]
pub async fn enable_ai(
    app: AppHandle,
    state: State<'_, AppState>,
    provider: String,
    key: Option<String>,
    confirm: bool,
) -> CmdResult<AppSettings> {
    if !confirm {
        return Err("Please confirm what will be shared first.".into());
    }
    let provider = AiProvider::parse(&provider).ok_or("unknown provider")?;
    if let Some(name) = provider.key_name() {
        if let Some(k) = key.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
            if !valid_key(k) {
                return Err("That doesn't look like an API key.".into());
            }
            state
                .deps
                .secrets
                .set(name, k)
                .map_err(internal("saving the key"))?;
        } else if !has_key(&state, provider) {
            return Err("Enter your API key.".into());
        }
    }
    let mut settings = lock_store(&state)?.load_settings();
    settings.ai.provider = provider;
    settings.sanitize(); // a bad model or endpoint becomes the default before we use it
    if provider == AiProvider::Custom && settings.ai.custom_endpoint.is_empty() {
        return Err("Enter the service's address, starting with https://".into());
    }
    // The host the user is agreeing to send data to (only meaningful for a custom service).
    let agreed_host = (provider == AiProvider::Custom)
        .then(|| firstlight_core::settings::endpoint_host(&settings.ai.custom_endpoint))
        .flatten();
    let summariser = make_summariser(&state, &settings).ok_or("Enter your API key.")?;
    let probe = Prompt {
        system: "Reply with the single word OK.".into(),
        user: "Ping".into(),
    };
    tauri::async_runtime::spawn_blocking(move || summariser.summarise(&probe))
        .await
        .map_err(|_| "test failed".to_string())?
        .map_err(|e| match e {
            Error::AuthRevoked => {
                "The service rejected the key. Check it and try again.".to_string()
            }
            Error::Provider(m) if m.contains("credit") => {
                "The service says the account has no credit left. Add credit and try again."
                    .to_string()
            }
            Error::Provider(m) if m.contains("model") => {
                "That model name wasn't found. Check the name and try again.".to_string()
            }
            Error::Offline => {
                "Couldn't reach the service. Check your connection and try again.".to_string()
            }
            _ => "The test request failed. Check the model name and try again.".to_string(),
        })?;
    save_settings(&app, &state, |s| {
        s.ai.provider = provider;
        s.ai.consented = Some(provider);
        s.ai.consented_host = agreed_host.clone();
        s.ai.enabled = true;
    })
}

/// How long to wait for the browser sign-in before giving up.
const OPENROUTER_WAIT: StdDuration = StdDuration::from_secs(300);

/// Signs in to OpenRouter in the browser and keeps the API key it issues for the user's own account. This
/// only gets the key; turning AI on is still the separate step that shows exactly what would be sent.
#[tauri::command]
pub async fn sign_in_openrouter(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    use firstlight_core::ai::openrouter;
    use tauri_plugin_opener::OpenerExt;
    let (deps, cancel, lock) = (
        state.deps.clone(),
        state.oauth_cancel.clone(),
        state.oauth_lock.clone(),
    );
    let opener_app = app.clone();
    let key = tauri::async_runtime::spawn_blocking(move || -> CmdResult<Redacted> {
        let _one_at_a_time = lock
            .try_lock()
            .map_err(|_| "A sign-in is already in progress.".to_string())?;
        cancel.store(false, std::sync::atomic::Ordering::Relaxed);
        let listener = Loopback::bind().map_err(oauth_error)?;
        // OpenRouter documents "localhost" callbacks on any port.
        let redirect = listener.redirect_uri("localhost");
        let (state_token, pkce) = (random_state(), Pkce::generate());
        let url = openrouter::authorization_url(&redirect, &pkce.challenge, &state_token);
        let parsed = firstlight_core::http::Url::parse(&url)
            .map_err(|_| "bad sign-in address".to_string())?;
        if parsed.scheme() != "https" || parsed.host_str() != Some(openrouter::HOST) {
            return Err("Refusing to open an unexpected sign-in address.".into());
        }
        opener_app
            .opener()
            .open_url(url, None::<&str>)
            .map_err(|_| "Couldn't open your browser.".to_string())?;
        let code = listener
            .wait_for_code(&state_token, OPENROUTER_WAIT, &cancel)
            .map_err(oauth_error)?;
        openrouter::exchange(
            deps.ai_http.as_ref(),
            openrouter::KEY_URL,
            &code,
            &pkce.verifier,
        )
        .map_err(|e| match e {
            Error::Offline => {
                "Couldn't reach OpenRouter. Check your connection and try again.".to_string()
            }
            Error::AuthRevoked => "OpenRouter didn't accept that sign-in. Try again.".to_string(),
            _ => "Signing in to OpenRouter didn't work. Try again.".to_string(),
        })
    })
    .await
    .map_err(|_| "sign-in failed".to_string())??;
    state
        .deps
        .secrets
        .set(
            AiProvider::Openrouter
                .key_name()
                .unwrap_or("ai-key-openrouter"),
            key.expose(),
        )
        .map_err(internal("saving the key"))?;
    Ok(())
}

#[tauri::command]
pub async fn disable_ai(app: AppHandle, state: State<'_, AppState>) -> CmdResult<AppSettings> {
    *state.ai_cache.lock().map_err(|_| "state unavailable")? = None;
    save_settings(&app, &state, |s| s.ai.enabled = false)
}

/// Deletes the saved API key and withdraws consent.
#[tauri::command]
pub async fn forget_ai_key(app: AppHandle, state: State<'_, AppState>) -> CmdResult<AppSettings> {
    // The key of the provider in use; a model on this computer has none.
    let provider = lock_store(&state)?.load_settings().ai.provider;
    if let Some(name) = provider.key_name() {
        state
            .deps
            .secrets
            .delete(name)
            .map_err(internal("removing the key"))?;
    }
    *state.ai_cache.lock().map_err(|_| "state unavailable")? = None;
    save_settings(&app, &state, |s| {
        s.ai.enabled = false;
        s.ai.consented = None;
    })
}

// ---- meeting prep ------------------------------------------------------------------------

#[derive(Serialize)]
pub struct PrepReply {
    #[serde(flatten)]
    prep: firstlight_core::prep::Prep,
    /// Unread messages that look to be about the meeting, from the digest already fetched.
    mail: Vec<firstlight_core::mail::MailMessage>,
}

/// What is worth knowing before a meeting: earlier times and their notes, what comes right before and after, overlaps
/// and related unread mail. Local only and instant: it reads stored events and the cached mail digest, and uses no AI
/// and no network.
#[tauri::command]
pub async fn get_meeting_prep(state: State<'_, AppState>, id: String) -> CmdResult<PrepReply> {
    let (now, tz) = (Utc::now(), crate::local_tz());
    let (settings, events) = {
        let store = lock_store(&state)?;
        let events = store
            .events_between(now - Duration::days(60), now + Duration::days(120))
            .map_err(internal("loading events"))?;
        (store.load_settings(), dedupe(events))
    };
    let Some(target) = events.iter().find(|e| e.id == id) else {
        return Ok(PrepReply {
            prep: Default::default(),
            mail: vec![],
        });
    };
    let prep = firstlight_core::prep::prepare(target, &events, tz, &settings.briefing);
    let mail = if settings.email.enabled {
        state
            .mail_cache
            .lock()
            .map_err(|_| "state unavailable")?
            .as_ref()
            .and_then(|(_, r)| r.digest.as_ref())
            .map(|d| firstlight_core::mail::related::related(d, target))
            .unwrap_or_default()
    } else {
        vec![]
    };
    Ok(PrepReply { prep, mail })
}

// ---- ask your calendar -----------------------------------------------------------------

const ASK_LIMIT_PER_HOUR: usize = 30;
static ASKED: std::sync::Mutex<Vec<Instant>> = std::sync::Mutex::new(Vec::new());

fn within_ask_limit() -> bool {
    let Ok(mut asked) = ASKED.lock() else {
        return false;
    };
    asked.retain(|t| t.elapsed() < StdDuration::from_secs(3600));
    if asked.len() >= ASK_LIMIT_PER_HOUR {
        return false;
    }
    asked.push(Instant::now());
    true
}

#[derive(Serialize)]
pub struct AskReply {
    /// AI is off, or the person has not agreed for this provider.
    enabled: bool,
    /// The question was about the calendar and the AI's plan was usable.
    understood: bool,
    /// "limit" | "ai" when something went wrong; nothing else is ever said about why.
    problem: Option<&'static str>,
    answer: Option<firstlight_core::ask::Answer>,
}

fn reply(enabled: bool, understood: bool, problem: Option<&'static str>) -> AskReply {
    AskReply {
        enabled,
        understood,
        problem,
        answer: None,
    }
}

/// Answers a question about the calendar. Only the question (with today's date and the time zone) goes to the AI,
/// which answers with a small plan; the plan is checked and run here against the stored events, so no event ever
/// leaves the computer. Nothing is remembered.
#[tauri::command]
pub async fn ask_calendar(state: State<'_, AppState>, question: String) -> CmdResult<AskReply> {
    use firstlight_core::ai::{ask_if_enabled, build_ask_prompt};
    use firstlight_core::ask::{Understood, answer, parse_plan};
    let settings = lock_store(&state)?.load_settings();
    if !settings.ai.enabled || settings.ai.consented != Some(settings.ai.provider) {
        return Ok(reply(false, false, None));
    }
    let question = question.trim();
    if question.is_empty() {
        return Ok(reply(true, false, None));
    }
    if !within_ask_limit() {
        return Ok(reply(true, false, Some("limit")));
    }
    let (now, tz) = (Utc::now(), crate::local_tz());
    let today = now.with_timezone(&tz).date_naive();
    let prompt = build_ask_prompt(question, today, tz);
    let summariser = make_summariser(&state, &settings);
    let ai = settings.ai.clone();
    let raw =
        tauri::async_runtime::spawn_blocking(move || ask_if_enabled(&ai, || prompt, || summariser))
            .await
            .ok()
            .flatten();
    let Some(raw) = raw else {
        return Ok(reply(true, false, Some("ai")));
    };
    let plan = match parse_plan(&raw, today) {
        Some(Understood::Plan(p)) => p,
        _ => return Ok(reply(true, false, None)),
    };
    let (from, _) = day_bounds(plan.from, tz);
    let (_, to) = day_bounds(plan.to, tz);
    let events = dedupe(
        lock_store(&state)?
            .events_between(from, to)
            .map_err(internal("loading events"))?,
    );
    let mut out = reply(true, true, None);
    out.answer = Some(answer(&plan, &events, now, tz, &settings.briefing));
    Ok(out)
}

/// A short AI summary of the day, or `None` (AI off, or anything went wrong). Never an error: the
/// briefing already has its own summary and carries on without this.
#[tauri::command]
pub async fn get_ai_summary(state: State<'_, AppState>) -> CmdResult<Option<String>> {
    let settings = lock_store(&state)?.load_settings();
    // Mirror the core gate so that when AI is off we don't even gather the data.
    if !settings.ai.enabled || settings.ai.consented != Some(settings.ai.provider) {
        return Ok(None);
    }
    let Ok(prompt) = current_prompt(&state, &settings) else {
        return Ok(None);
    };
    // The first line is only the date and clock time; leaving it out means an unchanged day is
    // summarised once per cache period instead of being re-sent every minute.
    let facts = prompt.user.split_once('\n').map_or("", |(_, rest)| rest);
    let signature = format!("{:?}|{}", settings.ai.provider, facts);
    let cached = state
        .ai_cache
        .lock()
        .map_err(|_| "state unavailable")?
        .as_ref()
        .filter(|(at, sig, _)| sig == &signature && at.elapsed() < AI_CACHE)
        .map(|(_, _, text)| text.clone());
    if let Some(text) = cached {
        return Ok(text);
    }
    let summariser = make_summariser(&state, &settings);
    let ai = settings.ai.clone();
    let text = tauri::async_runtime::spawn_blocking(move || {
        summarise_if_enabled(&ai, || prompt, || summariser)
    })
    .await
    .ok()
    .flatten();
    *state.ai_cache.lock().map_err(|_| "state unavailable")? =
        Some((Instant::now(), signature, text.clone()));
    Ok(text)
}

// ---- plan my day -------------------------------------------------------------------------

const PLAN_LIMIT_PER_HOUR: usize = 20;
static PLANNED: std::sync::Mutex<Vec<Instant>> = std::sync::Mutex::new(Vec::new());

fn within_plan_limit() -> bool {
    let Ok(mut done) = PLANNED.lock() else {
        return false;
    };
    done.retain(|t| t.elapsed() < StdDuration::from_secs(3600));
    if done.len() >= PLAN_LIMIT_PER_HOUR {
        return false;
    }
    done.push(Instant::now());
    true
}

/// The tasks worth planning and today's free stretches (from now, in working hours).
fn plan_inputs(
    state: &AppState,
) -> CmdResult<(
    AppSettings,
    Vec<firstlight_core::dayplan::PlanTask>,
    Vec<firstlight_core::briefing::slots::Slot>,
)> {
    use firstlight_core::briefing::slots::find_free_slots;
    let (now, tz) = (Utc::now(), crate::local_tz());
    let store = lock_store(state)?;
    let settings = store.load_settings();
    let lists = firstlight_core::todo::lists(&store, now, tz).map_err(internal("loading tasks"))?;
    let today = now.with_timezone(&tz).date_naive();
    let (from, to) = day_bounds(today, tz);
    let events = dedupe(
        store
            .events_between(from, to)
            .map_err(internal("loading events"))?,
    );
    let slots = find_free_slots(&events, today, 1, 15, now, tz, &settings.briefing);
    Ok((
        settings,
        firstlight_core::dayplan::candidates(&lists),
        slots,
    ))
}

#[derive(Serialize)]
pub struct DayPlanReply {
    /// AI summaries are on and agreed to, so planning with AI could be offered.
    ai_available: bool,
    /// The person also agreed to send task titles for planning.
    ai_allowed: bool,
    /// These suggestions came from the AI service (otherwise they were worked out on this computer).
    by_ai: bool,
    /// "limit" | "ai" when AI was asked for and could not be used; the plan from this computer is shown instead.
    problem: Option<&'static str>,
    /// How many tasks were looked at, and how much free time there is today (minutes).
    tasks: usize,
    free_minutes: i64,
    suggestions: Vec<firstlight_core::dayplan::Suggestion>,
}

/// Suggests when today's tasks could be done, in today's free stretches. Nothing is changed: the page asks for the
/// time to be set aside only when the person accepts. With `use_ai` (and both agreements), the task titles, their
/// lengths and the free times are sent to the person's AI service; its answer is checked line by line here.
#[tauri::command]
pub async fn plan_my_day(state: State<'_, AppState>, use_ai: bool) -> CmdResult<DayPlanReply> {
    use firstlight_core::ai::{build_day_plan_prompt, plan_if_allowed};
    use firstlight_core::dayplan::{local_plan, parse_ai_plan};
    let (settings, tasks, slots) = plan_inputs(&state)?;
    let ai_available = settings.ai.enabled && settings.ai.consented == Some(settings.ai.provider);
    let ai_allowed = ai_available && settings.ai.plan_consented == Some(settings.ai.provider);
    let mut reply = DayPlanReply {
        ai_available,
        ai_allowed,
        by_ai: false,
        problem: None,
        tasks: tasks.len(),
        free_minutes: slots.iter().map(|s| s.minutes).sum(),
        suggestions: local_plan(&tasks, &slots),
    };
    if !use_ai || !ai_allowed || tasks.is_empty() || slots.is_empty() {
        return Ok(reply);
    }
    if !within_plan_limit() {
        reply.problem = Some("limit");
        return Ok(reply);
    }
    let (now, tz) = (Utc::now(), crate::local_tz());
    let prompt = build_day_plan_prompt(&tasks, &slots, now, tz);
    let summariser = make_summariser(&state, &settings);
    let ai = settings.ai.clone();
    let raw = tauri::async_runtime::spawn_blocking(move || {
        plan_if_allowed(&ai, || prompt, || summariser)
    })
    .await
    .ok()
    .flatten();
    let today = now.with_timezone(&tz).date_naive();
    match raw.and_then(|r| parse_ai_plan(&r, &tasks, &slots, today, tz)) {
        Some(s) => {
            reply.by_ai = true;
            reply.suggestions = s;
        }
        None => reply.problem = Some("ai"),
    }
    Ok(reply)
}

/// Exactly what "Plan my day" would send right now, shown before the person agrees.
#[tauri::command]
pub async fn day_plan_preview(state: State<'_, AppState>) -> CmdResult<String> {
    let (_, tasks, slots) = plan_inputs(&state)?;
    Ok(
        firstlight_core::ai::build_day_plan_prompt(&tasks, &slots, Utc::now(), crate::local_tz())
            .disclosure(),
    )
}

/// Records the agreement to send task titles for planning. Needs `confirm` (the person read the preview) and AI on.
#[tauri::command]
pub async fn enable_ai_planning(
    app: AppHandle,
    state: State<'_, AppState>,
    confirm: bool,
) -> CmdResult<AppSettings> {
    if !confirm {
        return Err("Please confirm what will be shared first.".into());
    }
    let settings = lock_store(&state)?.load_settings();
    if !settings.ai.enabled || settings.ai.consented != Some(settings.ai.provider) {
        return Err("Turn on AI in Settings first.".into());
    }
    let provider = settings.ai.provider;
    save_settings(&app, &state, |s| s.ai.plan_consented = Some(provider))
}

/// Withdraws the agreement to send task titles. AI summaries (if on) stay as they are.
#[tauri::command]
pub async fn disable_ai_planning(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<AppSettings> {
    save_settings(&app, &state, |s| s.ai.plan_consented = None)
}

// ---- permissions on a connected account ------------------------------------------------

/// Every extra permission this Google or Microsoft account has been given, so asking again for one more never drops
/// the others (a new sign-in replaces the stored grant).
pub(crate) fn scopes_held(settings: &AppSettings, account: &Account) -> Vec<&'static str> {
    use firstlight_core::mail::send::{GMAIL_SEND_SCOPE, GRAPH_SEND_SCOPE};
    let has = |list: &[String]| list.iter().any(|id| id == &account.id);
    let (read, send) = match account.provider {
        ProviderKind::Google => (gmail::SCOPE, GMAIL_SEND_SCOPE),
        ProviderKind::Microsoft => (microsoft::SCOPE, GRAPH_SEND_SCOPE),
        _ => return vec![],
    };
    let mut out = Vec::new();
    if has(&settings.email.account_ids) {
        out.push(read);
    }
    if has(&settings.email.send_accounts) {
        out.push(send);
    }
    if has(&settings.email.autoreply_accounts) {
        out.push(match account.provider {
            ProviderKind::Google => firstlight_core::mail::autoreply::GMAIL_SETTINGS_SCOPE,
            _ => firstlight_core::mail::autoreply::GRAPH_MAILBOX_SCOPE,
        });
    }
    out.extend(crate::task_sync::scopes_held(settings, account));
    if settings.tasks.cloud_sync.as_deref() == Some(account.id.as_str()) {
        out.push(match account.provider {
            ProviderKind::Google => firstlight_core::folder_sync::cloud::GOOGLE_SCOPE,
            _ => firstlight_core::folder_sync::cloud::MICROSOFT_SCOPE,
        });
    }
    out
}

/// Signs in to the same account again asking for one more permission (plus everything it already has), checks it
/// is the same account, and stores the new grant in place of the old one.
pub(crate) async fn upgrade_grant(
    app: &AppHandle,
    state: &AppState,
    account: &Account,
    extra: &str,
) -> CmdResult<()> {
    let kind = match account.provider {
        ProviderKind::Google | ProviderKind::Microsoft => account.provider,
        _ => return Err("Only Google and Microsoft accounts can be given that permission.".into()),
    };
    let mut cfg = config_for(state, kind)?;
    let held = scopes_held(&lock_store(state)?.load_settings(), account);
    for scope in held.into_iter().chain([extra]) {
        if !cfg.scopes.iter().any(|s| s == scope) {
            cfg.scopes.push(scope.to_string());
        }
    }
    let (tokens, email) = browser_sign_in(app, state, kind, cfg).await?;
    if !account
        .email
        .as_deref()
        .is_some_and(|e| e.eq_ignore_ascii_case(&email))
    {
        return Err(
            "That's a different account from the one you chose. Sign in with the same account."
                .into(),
        );
    }
    // The new grant replaces the old one under the same keychain entry, so the account keeps working.
    let secret_ref = account
        .secret_ref
        .clone()
        .unwrap_or_else(|| oauth::secret_key(&account.id));
    state
        .deps
        .secrets
        .set(
            &secret_ref,
            &serde_json::to_string(&tokens).map_err(internal("saving the sign-in"))?,
        )
        .map_err(internal("saving the sign-in"))
}

// ---- sending a note (running late, a recap) ----------------------------------------------

#[derive(Serialize)]
pub struct MessageSender {
    /// `imap:<mailbox id>` or `account:<account id>`.
    id: String,
    address: String,
    label: String,
}

/// Where a note can be sent from: nothing unless sending is turned on; then Yahoo and iCloud mailboxes, and Google or
/// Microsoft accounts that granted the separate send permission.
#[tauri::command]
pub async fn message_senders(state: State<'_, AppState>) -> CmdResult<Vec<MessageSender>> {
    let store = lock_store(&state)?;
    let settings = store.load_settings();
    if !settings.email.send_messages {
        return Ok(vec![]);
    }
    let mut out: Vec<MessageSender> = settings
        .email
        .imap
        .iter()
        .filter(|m| firstlight_core::mail::smtp::server_for(m.preset).is_some())
        .map(|m| MessageSender {
            id: format!("imap:{}", m.id),
            address: m.address.clone(),
            label: m.preset.label().to_string(),
        })
        .collect();
    for id in &settings.email.send_accounts {
        if let Ok(Some(a)) = store.get_account(id)
            && let Some(email) = a.email.clone()
        {
            out.push(MessageSender {
                id: format!("account:{}", a.id),
                address: email,
                label: if a.provider == ProviderKind::Google {
                    "Gmail".into()
                } else {
                    "Outlook".into()
                },
            });
        }
    }
    Ok(out)
}

/// Sends a short plain note the person has just read in full and chosen to send. Only on their press of Send.
#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    sender_id: String,
    to: String,
    subject: String,
    body: String,
) -> CmdResult<()> {
    use firstlight_core::invite;
    use firstlight_core::mail::{send, smtp};
    let to = invite::parse_list(&to)
        .map_err(|_| "One of those email addresses doesn't look right.".to_string())?;
    if to.is_empty() {
        return Err("Add at least one email address.".into());
    }
    let clip = |s: &str, max: usize| -> String {
        s.chars()
            .filter(|c| !c.is_control() || *c == '\n')
            .take(max)
            .collect()
    };
    let (subject, body) = (clip(&subject, 200).replace('\n', " "), clip(&body, 4000));
    if subject.trim().is_empty() || body.trim().is_empty() {
        return Err("The message is empty.".into());
    }
    let settings = lock_store(&state)?.load_settings();
    if !settings.email.send_messages {
        return Err("Sending from Firstlight is turned off in Settings.".into());
    }
    if !crate::commands::within_send_limit() {
        return Err("That's a lot of messages for one hour. Try again later.".into());
    }
    let now = Utc::now();
    let explain = |e: Error| match e {
        Error::Invalid(m) => m.to_string(),
        Error::AuthRevoked => {
            "The mail service didn't accept the saved sign-in. Allow sending again in Settings."
                .to_string()
        }
        Error::PermissionNeeded => {
            "Sending isn't allowed for that account (it may be blocked by your organisation)."
                .to_string()
        }
        Error::Offline => "Couldn't reach the mail service. Nothing was sent.".to_string(),
        Error::Provider(m) => format!("The mail service didn't send it ({m})."),
        _ => "The message couldn't be sent.".to_string(),
    };
    if let Some(mailbox_id) = sender_id.strip_prefix("imap:") {
        let mailbox = settings
            .email
            .imap
            .iter()
            .find(|m| m.id == mailbox_id)
            .cloned()
            .ok_or("That mailbox is no longer connected.")?;
        let server =
            smtp::server_for(mailbox.preset).ok_or("Sending isn't available for that mailbox.")?;
        let password =
            smtp::password_for(state.deps.secrets.as_ref(), &mailbox.id).map_err(|_| {
                "Firstlight can't find that mailbox's password. Add the mailbox again.".to_string()
            })?;
        return tauri::async_runtime::spawn_blocking(move || {
            smtp::send(
                &smtp::TlsTransport::default(),
                &server,
                &mailbox.address,
                &password,
                &smtp::Outgoing {
                    from: &mailbox.address,
                    to: &to,
                    subject: &subject,
                    text: &body,
                    ics: None,
                    method: "",
                    now,
                },
            )
        })
        .await
        .map_err(|_| "Sending was interrupted.".to_string())?
        .map_err(explain);
    }
    let account_id = sender_id
        .strip_prefix("account:")
        .ok_or("Choose who it is from.")?;
    if !settings.email.send_accounts.iter().any(|a| a == account_id) {
        return Err("Allow sending for that account in Settings first.".into());
    }
    let account = lock_store(&state)?
        .get_account(account_id)
        .map_err(internal("loading the account"))?
        .ok_or("That account no longer exists.")?;
    let cfg = match account.provider {
        ProviderKind::Google => state.deps.oauth.google(),
        ProviderKind::Microsoft => state.deps.oauth.microsoft(),
        _ => None,
    }
    .ok_or("Sending isn't available for that account.")?;
    let http = state.deps.http.clone();
    let tokens = TokenManager {
        http: http.clone(),
        secrets: state.deps.secrets.clone(),
        cfg,
    };
    let from = account.email.clone().unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        let note = send::Note {
            from: &from,
            to: &to,
            subject: &subject,
            text: &body,
            now,
        };
        match account.provider {
            ProviderKind::Google => {
                send::send_gmail(http.as_ref(), &tokens, &account.id, &note, send::GMAIL_API)
            }
            _ => send::send_graph(http.as_ref(), &tokens, &account.id, &note, send::GRAPH_API),
        }
    })
    .await
    .map_err(|_| "Sending was interrupted.".to_string())?
    .map_err(explain)
}

/// Asks the account (in the browser) for the separate permission to send mail, then lets notes be sent from it.
#[tauri::command]
pub async fn enable_account_send(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
) -> CmdResult<AppSettings> {
    use firstlight_core::mail::send::{GMAIL_SEND_SCOPE, GRAPH_SEND_SCOPE};
    let account = lock_store(&state)?
        .get_account(&account_id)
        .map_err(internal("loading the account"))?
        .ok_or("That account no longer exists.")?;
    let scope = match account.provider {
        ProviderKind::Google => GMAIL_SEND_SCOPE,
        ProviderKind::Microsoft => GRAPH_SEND_SCOPE,
        _ => return Err("Only Google and Microsoft accounts can send.".into()),
    };
    upgrade_grant(&app, &state, &account, scope).await?;
    save_settings(&app, &state, |s| {
        if !s.email.send_accounts.contains(&account.id) {
            s.email.send_accounts.push(account.id.clone());
        }
    })
}

/// Stops sending from that account. (The permission stays granted at Google or Microsoft until removed there.)
#[tauri::command]
pub async fn disable_account_send(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
) -> CmdResult<AppSettings> {
    save_settings(&app, &state, |s| {
        s.email.send_accounts.retain(|a| a != &account_id)
    })
}

// ---- Automatic out-of-office reply (Time off) ------------------------------------------

/// Asks the account (in the browser) for the separate permission to set its automatic reply.
#[tauri::command]
pub async fn enable_account_autoreply(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
) -> CmdResult<AppSettings> {
    use firstlight_core::mail::autoreply::{GMAIL_SETTINGS_SCOPE, GRAPH_MAILBOX_SCOPE};
    let account = lock_store(&state)?
        .get_account(&account_id)
        .map_err(internal("loading the account"))?
        .ok_or("That account no longer exists.")?;
    let scope =
        match account.provider {
            ProviderKind::Google => GMAIL_SETTINGS_SCOPE,
            ProviderKind::Microsoft => GRAPH_MAILBOX_SCOPE,
            _ => return Err(
                "Only Google and Microsoft accounts have an automatic reply Firstlight can set."
                    .into(),
            ),
        };
    upgrade_grant(&app, &state, &account, scope).await?;
    save_settings(&app, &state, |s| {
        if !s.email.autoreply_accounts.contains(&account.id) {
            s.email.autoreply_accounts.push(account.id.clone());
        }
    })
}

/// Stops offering the automatic reply for that account. (The permission stays granted until removed there.)
#[tauri::command]
pub async fn disable_account_autoreply(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
) -> CmdResult<AppSettings> {
    save_settings(&app, &state, |s| {
        s.email.autoreply_accounts.retain(|a| a != &account_id)
    })
}

/// Sets the account's automatic reply for days off `from`..=`to` (`YYYY-MM-DD`, local): on at the start of the first
/// day, off at the start of the day after the last. `text` is what the page showed; it is sent as it is.
#[tauri::command]
pub async fn set_auto_reply(
    state: State<'_, AppState>,
    account_id: String,
    from: String,
    to: String,
    text: String,
) -> CmdResult<()> {
    let (google, account, tokens, http) = autoreply_account(&state, &account_id)?;
    let tz = crate::local_tz();
    let day = |s: &str| {
        chrono::NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
            .map_err(|_| "Choose valid days.".to_string())
    };
    let (from, to) = (day(&from)?, day(&to)?);
    if to < from {
        return Err("The last day off is before the first.".into());
    }
    let (start, _) = firstlight_core::briefing::day_bounds(from, tz);
    let (_, end) = firstlight_core::briefing::day_bounds(to, tz);
    let now = Utc::now();
    if end <= now {
        return Err("Those days are already over.".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        use firstlight_core::mail::{autoreply, send};
        let reply = autoreply::AutoReply {
            from: start.max(now),
            to: end,
            subject: "Out of office",
            text: &text,
        };
        if google {
            autoreply::set_gmail(
                http.as_ref(),
                &tokens,
                &account.id,
                &reply,
                send::GMAIL_API,
                now,
            )
        } else {
            autoreply::set_graph(
                http.as_ref(),
                &tokens,
                &account.id,
                &reply,
                send::GRAPH_API,
                now,
            )
        }
    })
    .await
    .map_err(|_| "Setting the reply was interrupted.".to_string())?
    .map_err(explain_autoreply)
}

/// Turns the account's automatic reply off now.
#[tauri::command]
pub async fn clear_auto_reply(state: State<'_, AppState>, account_id: String) -> CmdResult<()> {
    let (google, account, tokens, http) = autoreply_account(&state, &account_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        use firstlight_core::mail::{autoreply, send};
        let base = if google {
            send::GMAIL_API
        } else {
            send::GRAPH_API
        };
        autoreply::clear(
            http.as_ref(),
            &tokens,
            &account.id,
            google,
            base,
            Utc::now(),
        )
    })
    .await
    .map_err(|_| "Turning the reply off was interrupted.".to_string())?
    .map_err(explain_autoreply)
}

type AutoReplyAccount = (
    bool,
    Account,
    TokenManager,
    std::sync::Arc<dyn firstlight_core::http::HttpClient>,
);

fn autoreply_account(state: &AppState, account_id: &str) -> CmdResult<AutoReplyAccount> {
    let settings = lock_store(state)?.load_settings();
    if !settings
        .email
        .autoreply_accounts
        .iter()
        .any(|a| a == account_id)
    {
        return Err("Allow Firstlight to set that account's automatic reply first.".into());
    }
    let account = lock_store(state)?
        .get_account(account_id)
        .map_err(internal("loading the account"))?
        .ok_or("That account no longer exists.")?;
    let cfg = match account.provider {
        ProviderKind::Google => state.deps.oauth.google(),
        ProviderKind::Microsoft => state.deps.oauth.microsoft(),
        _ => None,
    }
    .ok_or("That account has no automatic reply Firstlight can set.")?;
    let http = state.deps.http.clone();
    let tokens = TokenManager {
        http: http.clone(),
        secrets: state.deps.secrets.clone(),
        cfg,
    };
    Ok((
        account.provider == ProviderKind::Google,
        account,
        tokens,
        http,
    ))
}

fn explain_autoreply(e: Error) -> String {
    match e {
        Error::Invalid(m) => m.to_string(),
        Error::AuthRevoked => "The mail service didn't accept the saved sign-in. Allow the automatic reply again in Time off.".into(),
        Error::PermissionNeeded => "That account doesn't allow its automatic reply to be set (your organisation may block it).".into(),
        Error::Offline => "Couldn't reach the mail service. Nothing was changed.".into(),
        Error::Provider(m) => format!("The mail service didn't take it ({m})."),
        _ => "The automatic reply couldn't be set.".into(),
    }
}
