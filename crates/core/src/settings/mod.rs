//! Everything the user can configure, as one typed, versioned document.
//!
//! Design rules (PLAN.md, Phase 4):
//! * every field has a default, so nobody has to configure anything;
//! * loading never fails: a damaged or out-of-range value falls back to its default and the
//!   valid fields around it are kept;
//! * it holds no secrets (credentials live in the keychain), so exporting it is always safe;
//! * old shapes are migrated, newer ones are read as far as they are understood.

use crate::briefing::BriefingSettings;
use crate::briefing::notify::NotifySettings;
use crate::briefing::rules::UnlockRules;
use crate::mail::imap::{ImapMailbox, MAX_MAILBOXES};
use crate::model::CALENDAR_COLOURS;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

pub const SETTINGS_KEY: &str = "app_settings";
pub const CURRENT_VERSION: u32 = 1;
/// Settings files larger than this are refused on import.
pub const MAX_IMPORT_BYTES: usize = 64 * 1024;
const EXPORT_FORMAT: &str = "firstlight-settings";
/// The name files were exported under before the app was renamed; still accepted when importing.
const LEGACY_EXPORT_FORMAT: &str = "daybrief-settings";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DefaultView {
    #[default]
    Briefing,
    Day,
    Week,
    Month,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WeekStart {
    /// Follow the system region.
    #[default]
    Auto,
    Monday,
    Sunday,
    Saturday,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TimeFormat {
    /// Follow the system clock setting.
    #[default]
    Auto,
    H12,
    H24,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
    /// Light, on paper tones.
    Warm,
    Mint,
    Sky,
    Lavender,
    Rose,
    /// Dark, on true black.
    Black,
    Midnight,
    Forest,
    Grape,
    Mocha,
    Slate,
}

/// The typeface for the whole app. Only fonts already on the computer; a style the system lacks falls back to the
/// normal one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FontStyle {
    #[default]
    System,
    Rounded,
    Serif,
    Mono,
    Avenir,
    Helvetica,
    Futura,
    Optima,
    Georgia,
    Palatino,
    Baskerville,
    Verdana,
    Trebuchet,
    Typewriter,
}

/// How big text is, relative to the normal size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TextSize {
    Small,
    #[default]
    Default,
    Large,
    Larger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UpdateChannel {
    #[default]
    Stable,
    /// Earlier builds, for people who want new things sooner and can tolerate rough edges.
    Beta,
}

/// A ready-made kind of event: a usual length and an agenda that goes into the meeting's private note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventTemplate {
    pub name: String,
    pub minutes: u32,
    #[serde(default)]
    pub agenda: String,
}

/// Most templates kept, and the longest name and agenda.
pub const MAX_TEMPLATES: usize = 20;
const MAX_TEMPLATE_NAME: usize = 60;
const MAX_TEMPLATE_AGENDA: usize = 2000;

impl EventTemplate {
    pub fn is_valid(&self) -> bool {
        let name = self.name.trim();
        !name.is_empty()
            && name.chars().count() <= MAX_TEMPLATE_NAME
            && !name.chars().any(char::is_control)
            && (5..=480).contains(&self.minutes)
            && self.agenda.chars().count() <= MAX_TEMPLATE_AGENDA
    }
}

/// Keeping the task list in step with Google Tasks or Microsoft To Do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TaskSyncSettings {
    /// Accounts that granted the separate task permission, in order (the first takes tasks made here). Changed only
    /// by `enable_task_sync` / `disable_task_sync`; never by a save or a settings file.
    pub sync_accounts: Vec<String>,
}

/// Choices about what Firstlight does on its own and what it keeps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PrivacySettings {
    /// Save a content-free crash report on this computer if the app stops unexpectedly. Nothing is ever sent.
    pub crash_reports: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralSettings {
    pub sync_interval_minutes: i64,
    pub default_view: DefaultView,
    pub week_start: WeekStart,
    pub time_format: TimeFormat,
    /// A global shortcut that shows or hides the briefing, e.g. `CmdOrCtrl+Shift+Space`.
    pub global_shortcut: Option<String>,
    /// Let Firstlight add the events you create to a calendar in the Calendar app (and change or delete the ones
    /// it added). Off by default: Firstlight otherwise only reads your calendars.
    pub write_to_calendar_app: bool,
    /// CalDAV accounts (by id) the user let Firstlight add events to. Empty by default; each is a separate choice,
    /// made on that account, and only events Firstlight created there are ever changed or removed.
    pub writable_accounts: Vec<String>,
    /// Show a Dock icon (macOS) or taskbar button (Windows). Off keeps Firstlight in the menu bar or tray only.
    pub show_in_dock: bool,
    /// A global shortcut that opens the video link of the meeting that is starting or running.
    pub join_shortcut: Option<String>,
    /// Show the next meeting next to the menu-bar icon. Off by default: titles in a menu bar are visible to
    /// anyone looking at the screen, including in a screen share.
    pub tray_next_meeting: bool,
    /// Look for a new version about once a day. Sends only the app's version and operating system.
    pub check_for_updates: bool,
    pub update_channel: UpdateChannel,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            sync_interval_minutes: 10,
            default_view: DefaultView::Briefing,
            week_start: WeekStart::Auto,
            time_format: TimeFormat::Auto,
            global_shortcut: None,
            show_in_dock: true,
            write_to_calendar_app: false,
            writable_accounts: Vec::new(),
            join_shortcut: None,
            tray_next_meeting: false,
            check_for_updates: true,
            update_channel: UpdateChannel::Stable,
        }
    }
}

/// Which optional parts of the briefing screen are shown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SectionSettings {
    pub tomorrow: bool,
    pub free_time: bool,
    pub attention: bool,
}

impl Default for SectionSettings {
    fn default() -> Self {
        Self {
            tomorrow: true,
            free_time: true,
            attention: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceSettings {
    pub theme: Theme,
    /// `"system"` (the default blue) or one of the calendar palette names.
    pub accent: String,
    /// A soft tint behind the app that follows the time of day. Off by default.
    pub ambient: bool,
    pub font: FontStyle,
    pub text_size: TextSize,
    /// Up to two other time zones (IANA names) shown next to your own in the day and week views and Find a time.
    pub extra_zones: Vec<String>,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            accent: "system".into(),
            ambient: false,
            font: FontStyle::System,
            text_size: TextSize::Default,
            extra_zones: vec![],
        }
    }
}

/// The optional email digest. Off by default; nothing mail-related runs until it is turned on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EmailSettings {
    pub enabled: bool,
    /// Read the Mail app on this Mac.
    pub apple_mail: bool,
    /// Connected Google/Microsoft accounts the user has allowed mail access for.
    pub account_ids: Vec<String>,
    /// Yahoo and iCloud mailboxes (IMAP). Only added or removed by their own commands, which keep the
    /// app password in the keychain in step; never exported, imported or changed by a plain save.
    pub imap: Vec<ImapMailbox>,
    /// Let "Send invitation" send the message itself from a Yahoo or iCloud mailbox above, instead of only
    /// opening a draft. Off by default; a settings file can never turn it on. Each message still needs a press
    /// of Send in the invitation window.
    pub send_invitations: bool,
    /// Let "Running late" and meeting recaps be sent from Firstlight with one press of Send, after the exact message
    /// is shown, instead of only opening a draft. Off by default; a settings file can never turn it on.
    pub send_messages: bool,
    /// Google/Microsoft accounts that granted the separate permission to send (`gmail.send` / `Mail.Send`). Changed
    /// only by `enable_account_send` / `disable_account_send`; never by a save or a settings file.
    pub send_accounts: Vec<String>,
    /// Addresses, `@domain`s, domains or names whose mail always shows.
    pub important_senders: Vec<String>,
    pub days_back: i64,
}

impl Default for EmailSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            apple_mail: false,
            account_ids: vec![],
            imap: vec![],
            send_invitations: false,
            send_messages: false,
            send_accounts: vec![],
            important_senders: vec![],
            days_back: 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AiProvider {
    /// Anthropic's API with the user's own key.
    #[default]
    Anthropic,
    /// A model running on this computer (Ollama, LM Studio, llama.cpp server, ...).
    Local,
    /// OpenAI (ChatGPT's models) with the user's own API key.
    Openai,
    /// xAI (Grok) with the user's own API key.
    Xai,
    /// Google Gemini with the user's own API key.
    Gemini,
    /// Mistral with the user's own API key.
    Mistral,
    /// Groq with the user's own API key.
    Groq,
    /// OpenRouter: one account for models from many companies. Can be signed in to instead of pasting a key.
    Openrouter,
    /// Any other service that speaks the same "chat completions" protocol, at an https address the user
    /// types. The key is sent to that address, so consent is tied to it.
    Custom,
}

impl AiProvider {
    /// Services that use the OpenAI-style protocol and an API key.
    pub fn is_openai_style(self) -> bool {
        !matches!(self, Self::Anthropic | Self::Local)
    }

    /// What the user sees.
    pub fn label(self) -> &'static str {
        match self {
            Self::Anthropic => "Anthropic (Claude)",
            Self::Local => "A model on this computer",
            Self::Openai => "OpenAI",
            Self::Xai => "xAI (Grok)",
            Self::Gemini => "Google Gemini",
            Self::Mistral => "Mistral",
            Self::Groq => "Groq",
            Self::Openrouter => "OpenRouter",
            Self::Custom => "Another service",
        }
    }

    /// A reasonable model to start from; the user can type any other. These names change over time, so the
    /// test request made when AI is turned on says clearly if the name is no longer valid.
    pub fn default_model(self) -> &'static str {
        match self {
            Self::Anthropic => DEFAULT_ANTHROPIC_MODEL,
            Self::Local => DEFAULT_LOCAL_MODEL,
            Self::Openai => "gpt-4o-mini",
            Self::Xai => "grok-3-mini",
            Self::Gemini => "gemini-2.0-flash",
            Self::Mistral => "mistral-small-latest",
            Self::Groq => "llama-3.3-70b-versatile",
            Self::Openrouter => "openai/gpt-4o-mini",
            Self::Custom => "gpt-4o-mini",
        }
    }

    /// The service's fixed address, for the providers that have one.
    pub fn base_url(self) -> Option<&'static str> {
        match self {
            Self::Openai => Some("https://api.openai.com/v1"),
            Self::Xai => Some("https://api.x.ai/v1"),
            Self::Gemini => Some("https://generativelanguage.googleapis.com/v1beta/openai"),
            Self::Mistral => Some("https://api.mistral.ai/v1"),
            Self::Groq => Some("https://api.groq.com/openai/v1"),
            Self::Openrouter => Some("https://openrouter.ai/api/v1"),
            Self::Anthropic | Self::Local | Self::Custom => None,
        }
    }

    /// Where this provider's API key lives in the keychain. `None` for a model on this computer.
    /// Anthropic keeps the name it has always had, so existing keys keep working.
    pub fn key_name(self) -> Option<&'static str> {
        match self {
            Self::Anthropic => Some("ai-key-anthropic"),
            Self::Local => None,
            Self::Openai => Some("ai-key-openai"),
            Self::Xai => Some("ai-key-xai"),
            Self::Gemini => Some("ai-key-gemini"),
            Self::Mistral => Some("ai-key-mistral"),
            Self::Groq => Some("ai-key-groq"),
            Self::Openrouter => Some("ai-key-openrouter"),
            Self::Custom => Some("ai-key-custom"),
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "anthropic" => Self::Anthropic,
            "local" => Self::Local,
            "openai" => Self::Openai,
            "xai" => Self::Xai,
            "gemini" => Self::Gemini,
            "mistral" => Self::Mistral,
            "groq" => Self::Groq,
            "openrouter" => Self::Openrouter,
            "custom" => Self::Custom,
            _ => return None,
        })
    }

    /// Every provider, for tests and for listing which keys are saved.
    pub const ALL: [Self; 9] = [
        Self::Anthropic,
        Self::Local,
        Self::Openai,
        Self::Xai,
        Self::Gemini,
        Self::Mistral,
        Self::Groq,
        Self::Openrouter,
        Self::Custom,
    ];
}

/// The address of a custom service: https only, a plain public-style host name (the same rule as a custom mail
/// server: no IP addresses, no `localhost`, no private-network names), no credentials, query or fragment.
/// Returns it tidied (no trailing slash), or `None` if it is not acceptable.
pub fn valid_custom_endpoint(s: &str) -> Option<String> {
    let u = reqwest::Url::parse(s.trim()).ok()?;
    let ok = u.scheme() == "https"
        && u.username().is_empty()
        && u.password().is_none()
        && u.query().is_none()
        && u.fragment().is_none()
        && u.port().is_none_or(|p| p == 443)
        && u.host_str().is_some_and(crate::mail::imap::valid_host)
        && u.path().len() <= 200
        && u.path()
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(&b));
    ok.then(|| s.trim().trim_end_matches('/').to_string())
}

/// The host of a custom endpoint, for tying consent to where the data goes.
pub fn endpoint_host(s: &str) -> Option<String> {
    reqwest::Url::parse(s).ok()?.host_str().map(str::to_string)
}

pub const DEFAULT_LOCAL_ENDPOINT: &str = "http://127.0.0.1:11434";
pub const DEFAULT_ANTHROPIC_MODEL: &str = "claude-haiku-5-5";
pub const DEFAULT_LOCAL_MODEL: &str = "llama3.2";

/// Optional AI summaries. Off by default; nothing is sent anywhere until the user has read exactly what
/// would be sent and agreed on this computer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiSettings {
    pub enabled: bool,
    pub provider: AiProvider,
    pub model: String,
    /// Only used for the local provider, and only loopback addresses are allowed.
    pub endpoint: String,
    /// Only used for `AiProvider::Custom`: an https address (see [`valid_custom_endpoint`]).
    pub custom_endpoint: String,
    /// Also share sender names and subjects of important mail (never snippets or bodies).
    pub include_email: bool,
    /// The provider the user agreed to. Set only by the explicit enable action; never by import or save.
    pub consented: Option<AiProvider>,
    /// For a custom service, the host the user agreed to send data to. If the address changes to another host,
    /// the agreement no longer applies and AI turns off until it is given again.
    pub consented_host: Option<String>,
    /// The provider the user agreed to send task titles to, for "Plan my day". A separate agreement, set only by the
    /// explicit enable action; it lapses whenever the main agreement does.
    pub plan_consented: Option<AiProvider>,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: AiProvider::Anthropic,
            model: DEFAULT_ANTHROPIC_MODEL.into(),
            endpoint: DEFAULT_LOCAL_ENDPOINT.into(),
            custom_endpoint: String::new(),
            include_email: false,
            consented: None,
            consented_host: None,
            plan_consented: None,
        }
    }
}

/// A local-model address: http or https, loopback host only, no credentials.
pub fn valid_local_endpoint(s: &str) -> bool {
    let Ok(u) = reqwest::Url::parse(s) else {
        return false;
    };
    matches!(u.scheme(), "http" | "https")
        && u.username().is_empty()
        && u.password().is_none()
        && matches!(
            u.host_str(),
            Some("127.0.0.1" | "localhost" | "[::1]" | "::1")
        )
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub version: u32,
    pub general: GeneralSettings,
    pub briefing: BriefingSettings,
    pub sections: SectionSettings,
    pub unlock: UnlockRules,
    pub notify: NotifySettings,
    pub appearance: AppearanceSettings,
    pub email: EmailSettings,
    pub ai: AiSettings,
    pub privacy: PrivacySettings,
    pub day: crate::day::DaySettings,
    /// Your own event templates (the built-in ones live in the page).
    pub templates: Vec<EventTemplate>,
    pub tasks: TaskSyncSettings,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            general: Default::default(),
            briefing: Default::default(),
            sections: Default::default(),
            unlock: Default::default(),
            notify: Default::default(),
            appearance: Default::default(),
            email: Default::default(),
            ai: Default::default(),
            privacy: Default::default(),
            day: Default::default(),
            templates: vec![],
            tasks: Default::default(),
        }
    }
}

/// Overlays `incoming` onto the defaults one field at a time, keeping each field only if the result
/// still deserializes. A wrong type in one field costs only that field.
fn merge_valid<T: Serialize + DeserializeOwned + Clone>(default: &T, incoming: &Value) -> T {
    let Ok(Value::Object(mut base)) = serde_json::to_value(default) else {
        return default.clone();
    };
    if let Some(inc) = incoming.as_object() {
        for (key, value) in inc {
            let previous = base.insert(key.clone(), value.clone());
            if serde_json::from_value::<T>(Value::Object(base.clone())).is_err() {
                match previous {
                    Some(p) => base.insert(key.clone(), p),
                    None => base.remove(key),
                };
            }
        }
    }
    serde_json::from_value(Value::Object(base)).unwrap_or_else(|_| default.clone())
}

/// Modifier and key names the global-shortcut layer understands.
const MODIFIERS: [&str; 10] = [
    "cmdorctrl",
    "commandorcontrol",
    "cmd",
    "command",
    "ctrl",
    "control",
    "alt",
    "option",
    "shift",
    "super",
];

/// A shortcut needs at least one real modifier (Shift alone would hijack ordinary typing) and a key.
pub fn valid_shortcut(s: &str) -> bool {
    let parts: Vec<&str> = s.split('+').map(str::trim).collect();
    let Some((key, mods)) = parts.split_last() else {
        return false;
    };
    if mods.is_empty()
        || mods
            .iter()
            .any(|m| !MODIFIERS.contains(&m.to_ascii_lowercase().as_str()))
    {
        return false;
    }
    let mut seen: Vec<String> = mods.iter().map(|m| m.to_ascii_lowercase()).collect();
    seen.sort();
    seen.dedup();
    if seen.len() != mods.len() || seen.iter().all(|m| m == "shift") {
        return false;
    }
    let k = key.to_ascii_lowercase();
    let named = [
        "space",
        "enter",
        "tab",
        "up",
        "down",
        "left",
        "right",
        "home",
        "end",
        "pageup",
        "pagedown",
        "backspace",
        "delete",
        "escape",
    ];
    let function = k
        .strip_prefix('f')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=24).contains(&n));
    let single = k.len() == 1 && k.bytes().all(|b| b.is_ascii_alphanumeric());
    named.contains(&k.as_str()) || function || single
}

impl AppSettings {
    /// Reads settings from stored or imported JSON without ever failing.
    pub fn from_value(v: &Value) -> Self {
        let d = Self::default();
        let obj = v.as_object();
        let section = |key: &str| obj.and_then(|o| o.get(key)).cloned().unwrap_or(Value::Null);
        let mut s = Self {
            version: CURRENT_VERSION,
            general: merge_valid(&d.general, &section("general")),
            briefing: merge_valid(&d.briefing, &section("briefing")),
            sections: merge_valid(&d.sections, &section("sections")),
            unlock: merge_valid(&d.unlock, &section("unlock")),
            notify: merge_valid(&d.notify, &section("notify")),
            appearance: merge_valid(&d.appearance, &section("appearance")),
            email: merge_valid(&d.email, &section("email")),
            ai: merge_valid(&d.ai, &section("ai")),
            privacy: merge_valid(&d.privacy, &section("privacy")),
            day: merge_valid(&d.day, &section("day")),
            tasks: merge_valid(&d.tasks, &section("tasks")),
            // One bad template costs only itself.
            templates: section("templates")
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|t| serde_json::from_value::<EventTemplate>(t.clone()).ok())
                        .collect()
                })
                .unwrap_or_default(),
        };
        s.sanitize();
        s
    }

    /// Pulls every value into its allowed range; anything unusable becomes the default.
    pub fn sanitize(&mut self) {
        let d = Self::default();
        self.version = CURRENT_VERSION;
        self.general.sync_interval_minutes = self.general.sync_interval_minutes.clamp(1, 1440);
        if self
            .general
            .global_shortcut
            .as_deref()
            .is_some_and(|s| !valid_shortcut(s))
        {
            self.general.global_shortcut = None;
        }
        if self
            .general
            .join_shortcut
            .as_deref()
            .is_some_and(|s| !valid_shortcut(s))
            || self.general.join_shortcut == self.general.global_shortcut
        {
            // Not valid, or the same key as the other shortcut: one key cannot do two things.
            self.general.join_shortcut = None;
        }

        let mut seen = std::collections::HashSet::new();
        self.general
            .writable_accounts
            .retain(|a| !a.is_empty() && a.len() <= 100 && seen.insert(a.clone()));
        self.general.writable_accounts.truncate(50);

        let b = &mut self.briefing;
        b.min_gap_minutes = b.min_gap_minutes.clamp(5, 240);
        b.early_hour = b.early_hour.min(23);
        b.work_start_hour = b.work_start_hour.min(23);
        b.work_end_hour = b.work_end_hour.min(24);
        if b.work_end_hour <= b.work_start_hour {
            (b.work_start_hour, b.work_end_hour) =
                (d.briefing.work_start_hour, d.briefing.work_end_hour);
        }

        let u = &mut self.unlock;
        u.first_unlock_after_hour = u.first_unlock_after_hour.min(23);
        u.min_interval_minutes = u.min_interval_minutes.clamp(0, 720);
        u.up_next_lead_minutes = u.up_next_lead_minutes.clamp(1, 120);
        if u.quiet_hours.is_some_and(|(a, b)| a > 23 || b > 23) {
            u.quiet_hours = d.unlock.quiet_hours;
        }

        let n = &mut self.notify;
        n.before_minutes = n.before_minutes.clamp(1, 120);
        n.morning_hour = n.morning_hour.min(11); // the morning briefing is due until noon
        n.evening_hour = n.evening_hour.clamp(12, 22); // and the evening one until 23:00

        let e = &mut self.email;
        e.days_back = e.days_back.clamp(1, 14);
        let mut seen = std::collections::HashSet::new();
        e.important_senders = std::mem::take(&mut e.important_senders)
            .into_iter()
            .map(|p| p.trim().to_string())
            .filter(|p| {
                !p.is_empty()
                    && p.chars().count() <= 254
                    && !p.chars().any(char::is_control)
                    && seen.insert(p.to_ascii_lowercase())
            })
            .take(50)
            .collect();
        let mut ids = std::collections::HashSet::new();
        e.account_ids = std::mem::take(&mut e.account_ids)
            .into_iter()
            .filter(|id| {
                !id.is_empty()
                    && id.len() <= 64
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_:".contains(&b))
                    && ids.insert(id.clone())
            })
            .take(20)
            .collect();

        let mut mailbox_ids = std::collections::HashSet::new();
        e.imap = std::mem::take(&mut e.imap)
            .into_iter()
            .filter(|m| m.is_valid() && mailbox_ids.insert(m.id.clone()))
            .take(MAX_MAILBOXES)
            .collect();

        let ai = &mut self.ai;
        ai.model = ai.model.trim().to_string();
        let model_ok = !ai.model.is_empty()
            && ai.model.len() <= 100
            && ai
                .model
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-._:/".contains(&b));
        if !model_ok {
            ai.model = ai.provider.default_model().into();
        }
        if !valid_local_endpoint(&ai.endpoint) {
            ai.endpoint = DEFAULT_LOCAL_ENDPOINT.into();
        }
        // A custom address that is not acceptable is dropped, never kept to be used later.
        ai.custom_endpoint = valid_custom_endpoint(&ai.custom_endpoint).unwrap_or_default();
        // Agreement to send data to a custom service is for the host the user saw. A different host needs a
        // fresh agreement, so an edited address (or a settings change from anywhere) can never redirect it.
        if ai.consented == Some(AiProvider::Custom)
            && (ai.custom_endpoint.is_empty()
                || ai.consented_host.as_deref() != endpoint_host(&ai.custom_endpoint).as_deref())
        {
            ai.consented = None;
        }
        if ai.consented != Some(AiProvider::Custom) {
            ai.consented_host = None;
        }
        // Planning with AI rests on the main agreement: it lapses with it, or when the provider changes.
        if ai.plan_consented.is_some() && ai.plan_consented != ai.consented {
            ai.plan_consented = None;
        }
        // AI can only be on for a provider the user explicitly agreed to on this computer.
        if ai.consented != Some(ai.provider) {
            ai.enabled = false;
        }

        self.day.sanitize();
        // Only real zone names, each once, at most two.
        let mut seen = std::collections::HashSet::new();
        self.appearance
            .extra_zones
            .retain(|z| z.parse::<chrono_tz::Tz>().is_ok() && seen.insert(z.clone()));
        self.appearance.extra_zones.truncate(2);
        self.templates.retain(EventTemplate::is_valid);
        for t in &mut self.templates {
            t.name = t.name.trim().to_string();
        }
        let mut seen = std::collections::HashSet::new();
        self.templates
            .retain(|t| seen.insert(t.name.to_lowercase()));
        self.templates.truncate(MAX_TEMPLATES);

        let accent = &self.appearance.accent;
        if accent != "system" && !CALENDAR_COLOURS.contains(&accent.as_str()) {
            self.appearance.accent = "system".into();
        }
    }

    /// Problems worth telling the user about instead of silently correcting.
    pub fn validate(&self) -> Result<(), String> {
        if self
            .general
            .global_shortcut
            .as_deref()
            .is_some_and(|s| !valid_shortcut(s))
        {
            return Err("That shortcut isn't valid. Use at least one modifier key (such as Cmd or Ctrl) plus a letter, number or function key.".into());
        }
        if self
            .general
            .join_shortcut
            .as_deref()
            .is_some_and(|s| !valid_shortcut(s))
        {
            return Err("That shortcut isn't valid. Use at least one modifier key (such as Cmd or Ctrl) plus a letter, number or function key.".into());
        }
        if self.general.join_shortcut.is_some()
            && self.general.join_shortcut == self.general.global_shortcut
        {
            return Err("Choose a different key for each shortcut.".into());
        }
        if self.briefing.work_end_hour <= self.briefing.work_start_hour {
            return Err("Working hours must end after they start.".into());
        }
        Ok(())
    }

    /// The notification settings with the clock format resolved (`system_24h` is what the OS reports).
    pub fn notify_for_engine(&self, system_24h: bool) -> NotifySettings {
        let mut n = self.notify.clone();
        n.time_24h = match self.general.time_format {
            TimeFormat::Auto => system_24h,
            TimeFormat::H12 => false,
            TimeFormat::H24 => true,
        };
        n
    }

    /// A portable copy: no secrets, no accounts, safe to share or keep.
    pub fn export(&self) -> String {
        // Mailbox addresses are personal and their passwords stay behind in the keychain.
        let mut portable = self.clone();
        portable.email.imap.clear();
        serde_json::to_string_pretty(
            &json!({ "format": EXPORT_FORMAT, "version": CURRENT_VERSION, "settings": portable }),
        )
        .expect("settings serialise")
    }

    pub fn import(text: &str) -> Result<Self, String> {
        if text.len() > MAX_IMPORT_BYTES {
            return Err("That file is too large to be a Firstlight settings file.".into());
        }
        let v: Value = serde_json::from_str(text)
            .map_err(|_| "That isn't a valid settings file.".to_string())?;
        if !matches!(
            v.get("format").and_then(Value::as_str),
            Some(EXPORT_FORMAT | LEGACY_EXPORT_FORMAT)
        ) {
            return Err("That isn't a Firstlight settings file.".into());
        }
        let Some(inner) = v.get("settings") else {
            return Err("That settings file is empty.".into());
        };
        let mut imported = Self::from_value(inner);
        // Agreement to send data to an AI service must be given on this computer, never inherited from a file.
        // Writing to a calendar is never switched on by a file.
        imported.general.write_to_calendar_app = false;
        imported.general.writable_accounts.clear();
        imported.email.send_invitations = false;
        imported.email.send_messages = false;
        imported.email.send_accounts.clear();
        imported.tasks.sync_accounts.clear();
        imported.ai.consented = None;
        imported.ai.consented_host = None;
        imported.ai.plan_consented = None;
        imported.ai.enabled = false;
        // Passwords are not in the file, so a mailbox from it could never sign in.
        imported.email.imap.clear();
        imported.sanitize();
        Ok(imported)
    }
}

#[cfg(test)]
mod tests;
