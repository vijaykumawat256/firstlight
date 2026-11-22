//! Maps an account's provider kind to a working [`Provider`].

use chrono::Utc;
use chrono_tz::Tz;
use firstlight_core::http::HttpClient;
use firstlight_core::model::ProviderKind;
use firstlight_core::oauth::OAuthConfig;
use firstlight_core::providers::{
    Provider,
    caldav::CaldavProvider,
    fixture::FixtureProvider,
    google,
    google::GoogleProvider,
    ics::IcsProvider,
    microsoft,
    microsoft::MicrosoftProvider,
    system::{SystemProvider, SystemSource},
};
use firstlight_core::secrets::SecretStore;
use std::sync::{Arc, RwLock};

/// Everything providers need from the outside world. Cheap to clone.
#[derive(Clone)]
pub struct Deps {
    pub http: Arc<dyn HttpClient>,
    pub secrets: Arc<dyn SecretStore>,
    pub oauth: OAuthConfigs,
    /// The operating system's own calendars (macOS Calendar app).
    pub system: Arc<dyn SystemSource>,
    /// For the AI service: https only, short timeout.
    pub ai_http: Arc<dyn HttpClient>,
    /// For a model on this computer: https, or plain http to loopback only.
    pub local_http: Arc<dyn HttpClient>,
}

impl Deps {
    /// The current system time zone (never cached: see `local_tz`).
    pub fn tz(&self) -> Tz {
        crate::local_tz()
    }
}

/// Sign-in configuration: your own app from Settings when set (`firstlight_core::oauth::apps`), else the IDs built
/// into this package; `None` when neither exists. Cheap to clone, and every clone sees a change made in Settings.
#[derive(Clone, Default)]
pub struct OAuthConfigs {
    built_google: Option<OAuthConfig>,
    built_microsoft: Option<OAuthConfig>,
    own: Arc<RwLock<Own>>,
}

#[derive(Default)]
struct Own {
    google: Option<OAuthConfig>,
    microsoft: Option<OAuthConfig>,
}

impl OAuthConfigs {
    /// Reads the IDs embedded at build time (see `build.rs`).
    pub fn from_build() -> Self {
        let set = |v: Option<&'static str>| v.filter(|s| !s.is_empty());
        Self {
            built_google: set(option_env!("GOOGLE_OAUTH_CLIENT_ID"))
                .map(|id| google::oauth_config(id, set(option_env!("GOOGLE_OAUTH_CLIENT_SECRET")))),
            built_microsoft: set(option_env!("MICROSOFT_OAUTH_CLIENT_ID"))
                .map(microsoft::oauth_config),
            own: Arc::default(),
        }
    }

    fn own(&self) -> std::sync::RwLockReadGuard<'_, Own> {
        self.own.read().unwrap_or_else(|e| e.into_inner())
    }

    pub fn google(&self) -> Option<OAuthConfig> {
        self.own()
            .google
            .clone()
            .or_else(|| self.built_google.clone())
    }

    pub fn microsoft(&self) -> Option<OAuthConfig> {
        self.own()
            .microsoft
            .clone()
            .or_else(|| self.built_microsoft.clone())
    }

    /// Whether this package has its own IDs built in.
    pub fn built_in(&self) -> (bool, bool) {
        (self.built_google.is_some(), self.built_microsoft.is_some())
    }

    /// Uses your own app (from Settings) instead of the built-in one; `None` goes back to the built-in.
    pub fn set_own(&self, google: Option<(String, Option<String>)>, microsoft: Option<String>) {
        let mut own = self.own.write().unwrap_or_else(|e| e.into_inner());
        own.google = google.map(|(id, secret)| google::oauth_config(&id, secret.as_deref()));
        own.microsoft = microsoft.map(|id| microsoft::oauth_config(&id));
    }
}

/// `None` for kinds that are not implemented yet; callers skip those accounts.
pub fn provider_for(kind: ProviderKind, deps: &Deps) -> Option<Box<dyn Provider>> {
    match kind {
        ProviderKind::Fixture => {
            let today = Utc::now().with_timezone(&deps.tz()).date_naive();
            Some(Box::new(FixtureProvider::new(today, deps.tz())))
        }
        ProviderKind::Ics => Some(Box::new(IcsProvider::new(
            deps.http.clone(),
            deps.secrets.clone(),
            deps.tz(),
        ))),
        ProviderKind::Caldav => Some(Box::new(CaldavProvider::new(
            deps.http.clone(),
            deps.secrets.clone(),
            deps.tz(),
        ))),
        ProviderKind::Google => deps.oauth.google().map(|cfg| {
            Box::new(GoogleProvider::new(
                deps.http.clone(),
                deps.secrets.clone(),
                cfg,
            )) as Box<dyn Provider>
        }),
        ProviderKind::Microsoft => deps.oauth.microsoft().map(|cfg| {
            Box::new(MicrosoftProvider::new(
                deps.http.clone(),
                deps.secrets.clone(),
                cfg,
            )) as Box<dyn Provider>
        }),
        // Events the user typed in live in the database already; there is nothing to fetch.
        ProviderKind::Local => None,
        ProviderKind::System => Some(Box::new(SystemProvider::new(
            deps.system.clone(),
            deps.tz(),
        ))),
    }
}
