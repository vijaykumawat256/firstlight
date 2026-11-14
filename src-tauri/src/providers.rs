//! Maps an account's provider kind to a working [`Provider`].

use chrono::Utc;
use chrono_tz::Tz;
use daybrief_core::http::HttpClient;
use daybrief_core::model::ProviderKind;
use daybrief_core::providers::{
    Provider, caldav::CaldavProvider, fixture::FixtureProvider, ics::IcsProvider,
};
use daybrief_core::secrets::SecretStore;
use std::sync::Arc;

/// Everything providers need from the outside world. Cheap to clone.
#[derive(Clone)]
pub struct Deps {
    pub http: Arc<dyn HttpClient>,
    pub secrets: Arc<dyn SecretStore>,
    pub tz: Tz,
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
        ProviderKind::Google | ProviderKind::Microsoft | ProviderKind::System => None,
    }
}
