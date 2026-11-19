//! Keeping tasks in step with Google Tasks and Microsoft To Do (step 7 of docs/WORKDAY_PLAN.md).

use firstlight_core::model::Account;
use firstlight_core::settings::AppSettings;

/// The task permissions this account holds (none yet).
pub(crate) fn scopes_held(_settings: &AppSettings, _account: &Account) -> Vec<&'static str> {
    vec![]
}
