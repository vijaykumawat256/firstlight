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
use crate::model::CALENDAR_COLOURS;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

pub const SETTINGS_KEY: &str = "app_settings";
pub const CURRENT_VERSION: u32 = 1;
/// Settings files larger than this are refused on import.
pub const MAX_IMPORT_BYTES: usize = 64 * 1024;
const EXPORT_FORMAT: &str = "daybrief-settings";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DefaultView {
    #[default]
    Briefing,
    Day,
    Week,
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
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            sync_interval_minutes: 10,
            default_view: DefaultView::Briefing,
            week_start: WeekStart::Auto,
            time_format: TimeFormat::Auto,
            global_shortcut: None,
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
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            accent: "system".into(),
        }
    }
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
        serde_json::to_string_pretty(
            &json!({ "format": EXPORT_FORMAT, "version": CURRENT_VERSION, "settings": self }),
        )
        .expect("settings serialise")
    }

    pub fn import(text: &str) -> Result<Self, String> {
        if text.len() > MAX_IMPORT_BYTES {
            return Err("That file is too large to be a Daybrief settings file.".into());
        }
        let v: Value = serde_json::from_str(text)
            .map_err(|_| "That isn't a valid settings file.".to_string())?;
        if v.get("format").and_then(Value::as_str) != Some(EXPORT_FORMAT) {
            return Err("That isn't a Daybrief settings file.".into());
        }
        let Some(inner) = v.get("settings") else {
            return Err("That settings file is empty.".into());
        };
        Ok(Self::from_value(inner))
    }
}

#[cfg(test)]
mod tests;
