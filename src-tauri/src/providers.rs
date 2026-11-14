//! Maps an account's provider kind to a working [`Provider`].

use chrono::Utc;
use chrono_tz::Tz;
use daybrief_core::http::HttpClient;
use daybrief_core::model::ProviderKind;
use daybrief_core::oauth::OAuthConfig;
use daybrief_core::providers::{
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
use daybrief_core::secrets::SecretStore;
use std::sync::Arc;

/// Everything providers need from the outside world. Cheap to clone.
#[derive(Clone)]
pub struct Deps {
    pub http: Arc<dyn HttpClient>,
    pub secrets: Arc<dyn SecretStore>,
    pub tz: Tz,
    pub oauth: OAuthConfigs,
    /// The operating system's own calendars (macOS Calendar app).
    pub system: Arc<dyn SystemSource>,
    /// For the AI service: https only, short timeout.
    pub ai_http: Arc<dyn HttpClient>,
    /// For a model on this computer: https, or plain http to loopback only.
    pub local_http: Arc<dyn HttpClient>,
}

/// Sign-in configuration baked into this build; `None` when the build has no client ID.
#[derive(Clone, Default)]
pub struct OAuthConfigs {
    pub google: Option<OAuthConfig>,
    pub microsoft: Option<OAuthConfig>,
}

impl OAuthConfigs {
    /// Reads the IDs embedded at build time (see `build.rs`).
    pub fn from_build() -> Self {
        let set = |v: Option<&'static str>| v.filter(|s| !s.is_empty());
        Self {
            google: set(option_env!("GOOGLE_OAUTH_CLIENT_ID"))
                .map(|id| google::oauth_config(id, set(option_env!("GOOGLE_OAUTH_CLIENT_SECRET")))),
            microsoft: set(option_env!("MICROSOFT_OAUTH_CLIENT_ID")).map(microsoft::oauth_config),
        }
    }
}

/// `None` for kinds that are not implemented yet; callers skip those accounts.
pub fn provider_for(kind: ProviderKind, deps: &Deps) -> Option<Box<dyn Provider>> {
    match kind {
        ProviderKind::Fixture => {
            let today = Utc::now().with_timezone(&deps.tz).date_naive();
            Some(Box::new(FixtureProvider::new(today, deps.tz)))
        }
        ProviderKind::Ics => Some(Box::new(IcsProvider::new(
            deps.http.clone(),
            deps.secrets.clone(),
            deps.tz,
        ))),
        ProviderKind::Caldav => Some(Box::new(CaldavProvider::new(
            deps.http.clone(),
            deps.secrets.clone(),
            deps.tz,
        ))),
        ProviderKind::Google => deps.oauth.google.clone().map(|cfg| {
            Box::new(GoogleProvider::new(
                deps.http.clone(),
                deps.secrets.clone(),
                cfg,
            )) as Box<dyn Provider>
        }),
        ProviderKind::Microsoft => deps.oauth.microsoft.clone().map(|cfg| {
            Box::new(MicrosoftProvider::new(
                deps.http.clone(),
                deps.secrets.clone(),
                cfg,
            )) as Box<dyn Provider>
        }),
        ProviderKind::System => Some(Box::new(SystemProvider::new(deps.system.clone(), deps.tz))),
    }
}
