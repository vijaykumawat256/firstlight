//! Google Tasks and Microsoft To Do: reading and writing the person's default task list, and carrying out what
//! `tasksync::reconcile` decided. Opt-in per account (its own permission: `tasks` / `Tasks.ReadWrite`).

use crate::http::{HttpClient, HttpRequest, HttpResponse};
use crate::model::new_id;
use crate::oauth::TokenManager;
use crate::store::Store;
use crate::tasks::{Task, TaskLink};
use crate::tasksync::{Action, RemoteTask, TaskFields, apply_remote, fields_of, reconcile};
use crate::todo;
use crate::{Error, Result};
use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use chrono_tz::Tz;
use reqwest::Url;
use serde_json::{Value, json};
use std::sync::Arc;

pub const GOOGLE_TASKS_SCOPE: &str = "https://www.googleapis.com/auth/tasks";
pub const MS_TASKS_SCOPE: &str = "Tasks.ReadWrite";
pub const GOOGLE_TASKS_API: &str = "https://tasks.googleapis.com/tasks/v1";
pub const GRAPH_API: &str = "https://graph.microsoft.com/v1.0";
/// Most pages read from a service in one sync (100 tasks each).
const MAX_PAGES: usize = 10;

pub trait TaskService {
    /// The person's default list.
    fn list_id(&self) -> Result<String>;
    fn list(&self, list: &str) -> Result<Vec<RemoteTask>>;
    fn create(&self, list: &str, f: &TaskFields) -> Result<RemoteTask>;
    fn update(&self, list: &str, id: &str, f: &TaskFields) -> Result<RemoteTask>;
    /// Already gone counts as done.
    fn delete(&self, list: &str, id: &str) -> Result<()>;
}

fn check(resp: &HttpResponse, service: &str) -> Result<()> {
    match resp.status {
        200..=299 => Ok(()),
        401 => Err(Error::AuthRevoked),
        403 => Err(Error::PermissionNeeded),
        429 | 500..=599 => Err(Error::Offline),
        s => Err(Error::Provider(format!("{service} returned {s}"))),
    }
}

fn json_of(resp: &HttpResponse, service: &str) -> Result<Value> {
    check(resp, service)?;
    serde_json::from_slice(&resp.body)
        .map_err(|_| Error::Provider(format!("{service} sent a response we could not read")))
}

/// `base` + path segments, each percent-encoded, so an id can never change the path.
fn url(base: &str, segments: &[&str]) -> Result<Url> {
    let mut u = Url::parse(base).map_err(|_| Error::Invalid("bad address"))?;
    u.path_segments_mut()
        .map_err(|_| Error::Invalid("bad address"))?
        .extend(segments);
    Ok(u)
}

fn date_of(s: Option<&str>) -> Option<NaiveDate> {
    s.and_then(|s| s.get(..10))
        .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
}

fn instant_of(s: Option<&str>) -> Option<DateTime<Utc>> {
    let s = s?;
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            NaiveDateTime::parse_from_str(s.get(..19)?, "%Y-%m-%dT%H:%M:%S")
                .ok()
                .map(|n| n.and_utc())
        })
}

fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

struct Authed {
    http: Arc<dyn HttpClient>,
    tokens: TokenManager,
    account_id: String,
}

impl Authed {
    fn send(&self, method: &str, u: Url, body: Option<Value>) -> Result<HttpResponse> {
        let token = self.tokens.access_token(&self.account_id, Utc::now())?;
        let mut req = HttpRequest::new(method, u.to_string())
            .header("Authorization", format!("Bearer {token}"))
            .header("Accept", "application/json");
        if let Some(b) = body {
            req = req
                .header("Content-Type", "application/json")
                .body(b.to_string());
        }
        self.http.send(&req)
    }
}

// ---- Google Tasks ---------------------------------------------------------------------------

pub struct GoogleTasks {
    auth: Authed,
    api_base: String,
}

impl GoogleTasks {
    pub fn new(
        http: Arc<dyn HttpClient>,
        tokens: TokenManager,
        account_id: &str,
        api_base: &str,
    ) -> Self {
        Self {
            auth: Authed {
                http,
                tokens,
                account_id: account_id.into(),
            },
            api_base: api_base.trim_end_matches('/').into(),
        }
    }

    fn parse(v: &Value) -> Option<RemoteTask> {
        if v.get("deleted").and_then(Value::as_bool) == Some(true) {
            return None;
        }
        let updated = instant_of(v.get("updated").and_then(Value::as_str))?;
        Some(RemoteTask {
            id: text(v, "id")?,
            title: text(v, "title").unwrap_or_default(),
            notes: text(v, "notes"),
            due: date_of(v.get("due").and_then(Value::as_str)),
            done: v.get("status").and_then(Value::as_str) == Some("completed"),
            completed_at: instant_of(v.get("completed").and_then(Value::as_str)),
            updated,
            stamp: text(v, "etag").or_else(|| text(v, "updated"))?,
        })
    }

    fn body(f: &TaskFields) -> Value {
        let mut b = json!({
            "title": f.title,
            "notes": f.notes,
            "status": if f.done { "completed" } else { "needsAction" },
            "due": f.due.map(|d| format!("{}T00:00:00.000Z", d.format("%Y-%m-%d"))),
        });
        if !f.done {
            // Reopening a task: Google keeps the old completion time unless it is cleared.
            b["completed"] = Value::Null;
        }
        b
    }
}

impl TaskService for GoogleTasks {
    fn list_id(&self) -> Result<String> {
        Ok("@default".into())
    }

    fn list(&self, list: &str) -> Result<Vec<RemoteTask>> {
        let mut out = Vec::new();
        let mut page: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut u = url(&self.api_base, &["lists", list, "tasks"])?;
            u.query_pairs_mut()
                .append_pair("showCompleted", "true")
                .append_pair("showHidden", "true")
                .append_pair("maxResults", "100");
            if let Some(p) = &page {
                u.query_pairs_mut().append_pair("pageToken", p);
            }
            let v = json_of(&self.auth.send("GET", u, None)?, "Google Tasks")?;
            out.extend(
                v.get("items")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Self::parse),
            );
            page = text(&v, "nextPageToken");
            if page.is_none() {
                break;
            }
        }
        Ok(out)
    }

    fn create(&self, list: &str, f: &TaskFields) -> Result<RemoteTask> {
        let v = json_of(
            &self.auth.send(
                "POST",
                url(&self.api_base, &["lists", list, "tasks"])?,
                Some(Self::body(f)),
            )?,
            "Google Tasks",
        )?;
        Self::parse(&v).ok_or(Error::Provider(
            "Google Tasks sent a task we could not read".into(),
        ))
    }

    fn update(&self, list: &str, id: &str, f: &TaskFields) -> Result<RemoteTask> {
        let v = json_of(
            &self.auth.send(
                "PATCH",
                url(&self.api_base, &["lists", list, "tasks", id])?,
                Some(Self::body(f)),
            )?,
            "Google Tasks",
        )?;
        Self::parse(&v).ok_or(Error::Provider(
            "Google Tasks sent a task we could not read".into(),
        ))
    }

    fn delete(&self, list: &str, id: &str) -> Result<()> {
        let resp = self.auth.send(
            "DELETE",
            url(&self.api_base, &["lists", list, "tasks", id])?,
            None,
        )?;
        if resp.status == 404 || resp.status == 410 {
            return Ok(());
        }
        check(&resp, "Google Tasks")
    }
}

// ---- Microsoft To Do ------------------------------------------------------------------------

pub struct MicrosoftTodo {
    auth: Authed,
    api_base: String,
}

impl MicrosoftTodo {
    pub fn new(
        http: Arc<dyn HttpClient>,
        tokens: TokenManager,
        account_id: &str,
        api_base: &str,
    ) -> Self {
        Self {
            auth: Authed {
                http,
                tokens,
                account_id: account_id.into(),
            },
            api_base: api_base.trim_end_matches('/').into(),
        }
    }

    fn parse(v: &Value) -> Option<RemoteTask> {
        let updated = instant_of(v.get("lastModifiedDateTime").and_then(Value::as_str))?;
        let body = v.get("body");
        let notes = body
            .filter(|b| b.get("contentType").and_then(Value::as_str) != Some("html"))
            .and_then(|b| text(b, "content"))
            .filter(|n| !n.trim().is_empty());
        Some(RemoteTask {
            id: text(v, "id")?,
            title: text(v, "title").unwrap_or_default(),
            notes,
            due: date_of(
                v.get("dueDateTime")
                    .and_then(|d| d.get("dateTime"))
                    .and_then(Value::as_str),
            ),
            done: v.get("status").and_then(Value::as_str) == Some("completed"),
            completed_at: instant_of(
                v.get("completedDateTime")
                    .and_then(|d| d.get("dateTime"))
                    .and_then(Value::as_str),
            ),
            updated,
            stamp: text(v, "@odata.etag").or_else(|| text(v, "lastModifiedDateTime"))?,
        })
    }

    fn body(f: &TaskFields) -> Value {
        json!({
            "title": f.title,
            "body": { "content": f.notes.clone().unwrap_or_default(), "contentType": "text" },
            "status": if f.done { "completed" } else { "notStarted" },
            "dueDateTime": f.due.map(|d| json!({ "dateTime": format!("{}T00:00:00", d.format("%Y-%m-%d")), "timeZone": "UTC" })),
        })
    }

    /// The next page, only if it is on the same service over https.
    fn same_service(&self, next: &str) -> Option<Url> {
        let (u, base) = (Url::parse(next).ok()?, Url::parse(&self.api_base).ok()?);
        (u.scheme() == base.scheme() && u.host_str() == base.host_str() && u.port() == base.port())
            .then_some(u)
    }
}

impl TaskService for MicrosoftTodo {
    fn list_id(&self) -> Result<String> {
        let v = json_of(
            &self
                .auth
                .send("GET", url(&self.api_base, &["me", "todo", "lists"])?, None)?,
            "Microsoft To Do",
        )?;
        v.get("value")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|l| l.get("wellknownListName").and_then(Value::as_str) == Some("defaultList"))
            .and_then(|l| text(l, "id"))
            .ok_or(Error::Provider(
                "Microsoft To Do has no default task list".into(),
            ))
    }

    fn list(&self, list: &str) -> Result<Vec<RemoteTask>> {
        let mut out = Vec::new();
        let mut u = url(&self.api_base, &["me", "todo", "lists", list, "tasks"])?;
        u.query_pairs_mut().append_pair("$top", "100");
        let mut next = Some(u);
        for _ in 0..MAX_PAGES {
            let Some(u) = next.take() else { break };
            let v = json_of(&self.auth.send("GET", u, None)?, "Microsoft To Do")?;
            out.extend(
                v.get("value")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Self::parse),
            );
            next = v
                .get("@odata.nextLink")
                .and_then(Value::as_str)
                .and_then(|n| self.same_service(n));
        }
        Ok(out)
    }

    fn create(&self, list: &str, f: &TaskFields) -> Result<RemoteTask> {
        let v = json_of(
            &self.auth.send(
                "POST",
                url(&self.api_base, &["me", "todo", "lists", list, "tasks"])?,
                Some(Self::body(f)),
            )?,
            "Microsoft To Do",
        )?;
        Self::parse(&v).ok_or(Error::Provider(
            "Microsoft To Do sent a task we could not read".into(),
        ))
    }

    fn update(&self, list: &str, id: &str, f: &TaskFields) -> Result<RemoteTask> {
        let v = json_of(
            &self.auth.send(
                "PATCH",
                url(&self.api_base, &["me", "todo", "lists", list, "tasks", id])?,
                Some(Self::body(f)),
            )?,
            "Microsoft To Do",
        )?;
        Self::parse(&v).ok_or(Error::Provider(
            "Microsoft To Do sent a task we could not read".into(),
        ))
    }

    fn delete(&self, list: &str, id: &str) -> Result<()> {
        let resp = self.auth.send(
            "DELETE",
            url(&self.api_base, &["me", "todo", "lists", list, "tasks", id])?,
            None,
        )?;
        if resp.status == 404 || resp.status == 410 {
            return Ok(());
        }
        check(&resp, "Microsoft To Do")
    }
}

// ---- carrying out a sync ----------------------------------------------------------------------

/// What one sync of one account did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Done {
    pub changed_here: usize,
    pub changed_there: usize,
}

/// Brings one account's default list and this computer's tasks in step. Each step is saved as soon as it is done, so
/// an interruption leaves nothing half-done; the next sync carries on.
pub fn sync_account(
    store: &Store,
    svc: &dyn TaskService,
    account_id: &str,
    primary: bool,
    tz: Tz,
    now: DateTime<Utc>,
) -> Result<Done> {
    let list = svc.list_id()?;
    let remote = svc.list(&list)?;
    let local = todo::all_records(store)?;
    let by_id = |id: &str| local.iter().find(|t| t.id == id).cloned();
    let link = |remote_id: &str, stamp: &str| TaskLink {
        account_id: account_id.into(),
        list_id: list.clone(),
        remote_id: remote_id.into(),
        remote_updated: Some(stamp.into()),
        synced_at: Some(now),
    };
    let mut done = Done::default();
    for action in reconcile(account_id, primary, &local, &remote, now) {
        match action {
            Action::CreateLocal(r) => {
                let mut t = Task {
                    id: new_id(),
                    title: String::new(),
                    notes: None,
                    due: None,
                    minutes: None,
                    done_at: None,
                    created_at: now,
                    updated_at: now,
                    from_meeting: None,
                    planned_event: None,
                    link: Some(link(&r.id, &r.stamp)),
                    deleted_at: None,
                    ..Default::default()
                };
                apply_remote(&mut t, &r, tz, now);
                todo::put(store, &t)?;
                done.changed_here += 1;
            }
            Action::UpdateLocal { task_id, remote: r } => {
                let Some(mut t) = by_id(&task_id) else {
                    continue;
                };
                apply_remote(&mut t, &r, tz, now);
                t.link = Some(link(&r.id, &r.stamp));
                todo::put(store, &t)?;
                done.changed_here += 1;
            }
            Action::DeleteLocal { task_id } | Action::ForgetMarker { task_id } => {
                store.delete_task(&task_id)?;
                done.changed_here += 1;
            }
            Action::CreateRemote { task_id } => {
                let Some(mut t) = by_id(&task_id) else {
                    continue;
                };
                let r = svc.create(&list, &fields_of(&t, tz))?;
                t.link = Some(link(&r.id, &r.stamp));
                todo::put(store, &t)?;
                done.changed_there += 1;
            }
            Action::UpdateRemote { task_id, remote_id } => {
                let Some(mut t) = by_id(&task_id) else {
                    continue;
                };
                let r = svc.update(&list, &remote_id, &fields_of(&t, tz))?;
                t.link = Some(link(&r.id, &r.stamp));
                todo::put(store, &t)?;
                done.changed_there += 1;
            }
            Action::DeleteRemote { task_id, remote_id } => {
                svc.delete(&list, &remote_id)?;
                store.delete_task(&task_id)?;
                done.changed_there += 1;
            }
        }
    }
    Ok(done)
}

/// Stops keeping tasks in step with an account: its tasks stay here as ordinary tasks, and markers for tasks deleted
/// here are dropped (nothing more is sent to it).
pub fn unlink_account(store: &Store, account_id: &str) -> Result<()> {
    for mut t in todo::all_records(store)? {
        if t.link.as_ref().is_some_and(|l| l.account_id == account_id) {
            if t.deleted_at.is_some() {
                store.delete_task(&t.id)?;
            } else {
                t.link = None;
                todo::put(store, &t)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
