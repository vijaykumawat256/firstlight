//! ICS subscription links: a published calendar anyone with the address can read
//! (Google "secret address", Outlook "published calendar", iCloud public calendars, ...).
//!
//! The link itself is a credential, so it lives in the secret store (keychain), never in the
//! database. Changes are detected with `ETag` / `Last-Modified`.

use super::{Capabilities, Provider, SyncResult, Window};
use crate::http::{HttpClient, HttpRequest};
use crate::ics::{ParseOptions, events_from_ics};
use crate::model::*;
use crate::secrets::SecretStore;
use crate::{Error, Result};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub struct IcsProvider {
    http: Arc<dyn HttpClient>,
    secrets: Arc<dyn SecretStore>,
    default_tz: Tz,
}

#[derive(Serialize, Deserialize, Default)]
struct State {
    etag: Option<String>,
    last_modified: Option<String>,
    /// The sync window start (a date). The window slides daily, so a cached 304 is only
    /// valid for the same window.
    window_from: String,
}

impl IcsProvider {
    pub fn new(http: Arc<dyn HttpClient>, secrets: Arc<dyn SecretStore>, default_tz: Tz) -> Self {
        Self {
            http,
            secrets,
            default_tz,
        }
    }

    /// Secret-store key for an account's link.
    pub fn secret_key(account_id: &str) -> String {
        format!("ics-url:{account_id}")
    }
}

impl Provider for IcsProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Ics
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            incremental_sync: true,
            email: false,
            needs_polling: true,
        }
    }

    fn list_calendars(&self, account: &Account) -> Result<Vec<Calendar>> {
        Ok(vec![Calendar {
            id: format!("{}:ics", account.id),
            account_id: account.id.clone(),
            remote_id: "ics".into(),
            name: account.display_name.clone(),
            colour: default_colour_for(&account.id).into(),
            visible: true,
            read_only: true,
        }])
    }

    fn sync_events(
        &self,
        calendar: &Calendar,
        state: Option<&str>,
        window: &Window,
    ) -> Result<SyncResult> {
        // The account is needed for the URL; ICS calendars are 1:1 with accounts, so the id encodes it.
        let account_id = calendar.account_id.as_str();
        let url = self
            .secrets
            .get(&Self::secret_key(account_id))?
            .ok_or(Error::AuthRevoked)?;

        let window_from = window.from.date_naive().to_string();
        let prev: State = state
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();
        let usable = prev.window_from == window_from;

        let mut req = HttpRequest::get(url).header("Accept", "text/calendar, */*;q=0.1");
        if usable {
            if let Some(e) = &prev.etag {
                req = req.header("If-None-Match", e.clone());
            }
            if let Some(m) = &prev.last_modified {
                req = req.header("If-Modified-Since", m.clone());
            }
        }
        let resp = self.http.send(&req)?;

        match resp.status {
            304 if usable => {
                return Ok(SyncResult {
                    changed: vec![],
                    deleted: vec![],
                    next_state: state.map(String::from),
                    full: false,
                });
            }
            401 | 403 => return Err(Error::AuthRevoked),
            404 | 410 => {
                return Err(Error::Provider(
                    "calendar not found; the link may have been reset".into(),
                ));
            }
            s if !(200..300).contains(&s) => {
                return Err(Error::Provider(format!("server returned {s}")));
            }
            _ => {}
        }

        let text = resp.text();
        let events = events_from_ics(
            &text,
            &ParseOptions {
                calendar_id: &calendar.id,
                default_tz: self.default_tz,
                window: *window,
                my_email: None,
            },
        )?;
        let next = State {
            etag: resp.header("etag").map(String::from),
            last_modified: resp.header("last-modified").map(String::from),
            window_from,
        };
        Ok(SyncResult {
            changed: events,
            deleted: vec![],
            next_state: serde_json::to_string(&next).ok(),
            full: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::ReqwestClient;
    use crate::http::testserver::{Resp, TestServer};
    use crate::secrets::MemorySecretStore;
    use chrono::{TimeZone, Utc};

    const FEED: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:a\r\nDTSTART:20260310T090000Z\r\nDTEND:20260310T100000Z\r\nSUMMARY:Hello\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    fn window() -> Window {
        Window {
            from: Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap(),
            to: Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap(),
        }
    }

    fn setup(server: &TestServer, path: &str) -> (IcsProvider, Account, Calendar) {
        let secrets = Arc::new(MemorySecretStore::default());
        let account = Account {
            id: "acct".into(),
            provider: ProviderKind::Ics,
            display_name: "Holidays".into(),
            email: None,
            status: AccountStatus::Ok,
            secret_ref: Some(IcsProvider::secret_key("acct")),
            created_at: Utc::now(),
        };
        secrets
            .set(&IcsProvider::secret_key("acct"), &server.url(path))
            .unwrap();
        let p = IcsProvider::new(
            Arc::new(ReqwestClient::new_allow_http_for_tests().unwrap()),
            secrets,
            chrono_tz::UTC,
        );
        let cal = p.list_calendars(&account).unwrap().remove(0);
        (p, account, cal)
    }

    #[test]
    fn fetches_parses_and_remembers_the_etag() {
        let server = TestServer::start(|_| Resp::ok(FEED).with_header("ETag", "\"v1\""));
        let (p, _, cal) = setup(&server, "/private-token/basic.ics");
        let r = p.sync_events(&cal, None, &window()).unwrap();
        assert!(r.full);
        assert_eq!(r.changed.len(), 1);
        assert_eq!(r.changed[0].title, "Hello");
        assert_eq!(r.changed[0].calendar_id, cal.id);
        assert!(r.next_state.unwrap().contains("v1"));
    }

    #[test]
    fn unchanged_feed_returns_304_and_no_changes() {
        let server = TestServer::start(|req| {
            if req.header("if-none-match") == Some("\"v1\"") {
                Resp::status(304)
            } else {
                Resp::ok(FEED).with_header("ETag", "\"v1\"")
            }
        });
        let (p, _, cal) = setup(&server, "/feed.ics");
        let first = p.sync_events(&cal, None, &window()).unwrap();
        let second = p
            .sync_events(&cal, first.next_state.as_deref(), &window())
            .unwrap();
        assert!(!second.full && second.changed.is_empty());
        assert_eq!(second.next_state, first.next_state);
    }

    #[test]
    fn a_cached_etag_is_ignored_once_the_window_has_moved() {
        let server = TestServer::start(|req| {
            if req.header("if-none-match").is_some() {
                Resp::status(304)
            } else {
                Resp::ok(FEED).with_header("ETag", "\"v1\"")
            }
        });
        let (p, _, cal) = setup(&server, "/feed.ics");
        let first = p.sync_events(&cal, None, &window()).unwrap();
        let tomorrow = Window {
            from: window().from + chrono::Duration::days(1),
            to: window().to + chrono::Duration::days(1),
        };
        let again = p
            .sync_events(&cal, first.next_state.as_deref(), &tomorrow)
            .unwrap();
        assert!(
            again.full,
            "must refetch so recurrences are re-expanded for the new window"
        );
    }

    #[test]
    fn auth_failures_and_missing_calendars_are_distinguished() {
        for (status, want_auth) in [(401u16, true), (403, true), (404, false), (500, false)] {
            let server = TestServer::start(move |_| Resp::status(status));
            let (p, _, cal) = setup(&server, "/x.ics");
            let err = p.sync_events(&cal, None, &window()).unwrap_err();
            assert_eq!(
                matches!(err, Error::AuthRevoked),
                want_auth,
                "status {status}: {err}"
            );
        }
    }

    #[test]
    fn html_error_pages_are_rejected_not_parsed_as_empty() {
        let server = TestServer::start(|_| Resp::ok("<html>Please sign in</html>"));
        let (p, _, cal) = setup(&server, "/x.ics");
        assert!(p.sync_events(&cal, None, &window()).is_err());
    }

    #[test]
    fn errors_do_not_leak_the_secret_link() {
        let server = TestServer::start(|_| Resp::status(500));
        let (p, _, cal) = setup(&server, "/SECRET-TOKEN-9f8e7d/basic.ics");
        let err = p.sync_events(&cal, None, &window()).unwrap_err();
        assert!(!format!("{err} {err:?}").contains("SECRET-TOKEN"));
    }

    #[test]
    fn missing_secret_means_reconnect() {
        let server = TestServer::start(|_| Resp::ok(FEED));
        let (p, _, cal) = setup(&server, "/x.ics");
        p.secrets.delete(&IcsProvider::secret_key("acct")).unwrap();
        assert!(matches!(
            p.sync_events(&cal, None, &window()),
            Err(Error::AuthRevoked)
        ));
    }
}

/// Live check against a real public feed. Ignored by default (needs the internet):
/// `cargo test -p firstlight-core live_ -- --ignored --nocapture`
#[cfg(test)]
mod live {
    use super::*;
    use crate::http::ReqwestClient;
    use crate::secrets::MemorySecretStore;
    use chrono::{Duration, Utc};

    #[test]
    #[ignore = "network"]
    fn live_public_holiday_feed_parses_with_the_production_client() {
        let url = "https://calendar.google.com/calendar/ical/en.usa%23holiday%40group.v.calendar.google.com/public/basic.ics";
        let secrets = Arc::new(MemorySecretStore::default());
        secrets.set(&IcsProvider::secret_key("live"), url).unwrap();
        let p = IcsProvider::new(
            Arc::new(ReqwestClient::new().unwrap()),
            secrets,
            chrono_tz::UTC,
        );
        let acct = Account {
            id: "live".into(),
            provider: ProviderKind::Ics,
            display_name: "US holidays".into(),
            email: None,
            status: AccountStatus::Ok,
            secret_ref: Some(IcsProvider::secret_key("live")),
            created_at: Utc::now(),
        };
        let cal = p.list_calendars(&acct).unwrap().remove(0);
        let now = Utc::now();
        let w = Window {
            from: now - Duration::days(30),
            to: now + Duration::days(365),
        };

        let first = p.sync_events(&cal, None, &w).unwrap();
        println!("fetched {} events in the next year", first.changed.len());
        for e in first.changed.iter().take(6) {
            println!("  {:?}  {}", e.when, e.title);
        }
        assert!(
            first.changed.len() > 5,
            "a year of US holidays should have many events"
        );
        assert!(
            first
                .changed
                .iter()
                .all(|e| matches!(e.when, When::AllDay { .. })),
            "holidays are all-day"
        );

        // Second request with the stored state should be a cheap 304 when the server supports it.
        let second = p
            .sync_events(&cal, first.next_state.as_deref(), &w)
            .unwrap();
        println!(
            "second sync: full={} changed={}",
            second.full,
            second.changed.len()
        );
    }

    /// Every country offered in the add-calendar sheet (`src/features/accounts/holidays.ts`) must be a feed
    /// the production client can fetch and the calendar reader can parse. Keep the ids in step with that list.
    #[test]
    #[ignore = "network"]
    fn live_every_offered_holiday_calendar_parses() {
        let ids = [
            "en.australian",
            "pt_br.brazilian",
            "en.canadian",
            "fr.french",
            "de.german",
            "en.indian",
            "en.irish",
            "it.italian",
            "ja.japanese",
            "en.malaysia",
            "nl.dutch",
            "en.new_zealand",
            "en.philippines",
            "en.singapore",
            "es.spain",
            "en.uk",
            "en.usa",
        ];
        let now = Utc::now();
        let w = Window {
            from: now - Duration::days(30),
            to: now + Duration::days(365),
        };
        for id in ids {
            let url = format!(
                "https://calendar.google.com/calendar/ical/{id}%23holiday%40group.v.calendar.google.com/public/basic.ics"
            );
            let key = IcsProvider::secret_key(id);
            let secrets = Arc::new(MemorySecretStore::default());
            secrets.set(&key, &url).unwrap();
            let p = IcsProvider::new(
                Arc::new(ReqwestClient::new().unwrap()),
                secrets,
                chrono_tz::UTC,
            );
            let acct = Account {
                id: id.into(),
                provider: ProviderKind::Ics,
                display_name: id.into(),
                email: None,
                status: AccountStatus::Ok,
                secret_ref: Some(key),
                created_at: now,
            };
            let cal = p.list_calendars(&acct).unwrap().remove(0);
            let n = p.sync_events(&cal, None, &w).unwrap().changed.len();
            println!("{id}: {n} events in the next year");
            assert!(n > 5, "{id} returned only {n} events");
        }
    }
}
