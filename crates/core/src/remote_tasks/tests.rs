use super::*;
use crate::http::ReqwestClient;
use crate::http::testserver::{Resp, TestServer};
use crate::oauth::{OAuthConfig, TokenSet, secret_key};
use crate::secrets::{MemorySecretStore, SecretStore, load_or_create_db_key};
use crate::tasks::TaskInput;
use std::sync::Mutex;

const NY: Tz = chrono_tz::America::New_York;

fn t(s: &str) -> DateTime<Utc> {
    s.parse().unwrap()
}

fn store() -> Store {
    let key = load_or_create_db_key(&MemorySecretStore::default()).unwrap();
    Store::open_in_memory(&key).unwrap()
}

/// A task service kept in memory, with a change stamp that moves on every write.
#[derive(Default)]
struct Fake {
    tasks: Mutex<Vec<RemoteTask>>,
    counter: Mutex<u32>,
}

impl Fake {
    fn stamp(&self) -> String {
        let mut c = self.counter.lock().unwrap();
        *c += 1;
        format!("s{c}")
    }
    fn put(&self, id: &str, f: &TaskFields, updated: DateTime<Utc>) -> RemoteTask {
        let r = RemoteTask {
            id: id.into(),
            title: f.title.clone(),
            notes: f.notes.clone(),
            due: f.due,
            done: f.done,
            completed_at: f.done.then_some(updated),
            updated,
            stamp: self.stamp(),
        };
        let mut all = self.tasks.lock().unwrap();
        all.retain(|x| x.id != id);
        all.push(r.clone());
        r
    }
}

impl TaskService for Fake {
    fn list_id(&self) -> Result<String> {
        Ok("@default".into())
    }
    fn list(&self, _: &str) -> Result<Vec<RemoteTask>> {
        Ok(self.tasks.lock().unwrap().clone())
    }
    fn create(&self, _: &str, f: &TaskFields) -> Result<RemoteTask> {
        let id = format!("r{}", self.tasks.lock().unwrap().len() + 1);
        Ok(self.put(&id, f, t("2026-11-18T12:00:00Z")))
    }
    fn update(&self, _: &str, id: &str, f: &TaskFields) -> Result<RemoteTask> {
        Ok(self.put(id, f, t("2026-11-18T12:00:00Z")))
    }
    fn delete(&self, _: &str, id: &str) -> Result<()> {
        self.tasks.lock().unwrap().retain(|x| x.id != id);
        Ok(())
    }
}

fn input(title: &str) -> TaskInput {
    TaskInput {
        title: title.into(),
        notes: None,
        due_date: Some("2026-11-20".into()),
        due_time: Some("15:30".into()),
        minutes: None,
        from_meeting: None,
        ..Default::default()
    }
}

#[test]
fn tasks_go_both_ways_and_deletions_follow() {
    let s = store();
    let svc = Fake::default();
    let now = t("2026-11-18T12:00:00Z");
    let here = todo::add(&s, &input("Call the bank"), NY, now).unwrap();
    svc.put(
        "phone-1",
        &TaskFields {
            title: "Buy milk".into(),
            notes: Some("2 litres".into()),
            due: None,
            done: false,
        },
        now,
    );

    let first = sync_account(&s, &svc, "g", true, NY, now).unwrap();
    assert_eq!(
        first,
        Done {
            changed_here: 1,
            changed_there: 1
        }
    );
    // Sent with its day only; the time stays here.
    let sent = svc
        .list("@default")
        .unwrap()
        .into_iter()
        .find(|r| r.title == "Call the bank")
        .unwrap();
    assert_eq!(sent.due, Some("2026-11-20".parse().unwrap()));
    let linked = todo::get(&s, &here.id).unwrap();
    assert_eq!(linked.link.as_ref().unwrap().remote_id, sent.id);
    let milk = todo::all(&s)
        .unwrap()
        .into_iter()
        .find(|t| t.title == "Buy milk")
        .unwrap();
    assert_eq!(milk.notes.as_deref(), Some("2 litres"));

    // A second sync with nothing changed does nothing.
    assert_eq!(
        sync_account(&s, &svc, "g", true, NY, now).unwrap(),
        Done::default()
    );

    // Ticked off on the phone: done here too, and the time set here survives.
    svc.put(
        "phone-1",
        &TaskFields {
            title: "Buy milk".into(),
            notes: None,
            due: None,
            done: true,
        },
        t("2026-11-18T13:00:00Z"),
    );
    let later = t("2026-11-18T14:00:00Z");
    sync_account(&s, &svc, "g", true, NY, later).unwrap();
    assert!(todo::get(&s, &milk.id).unwrap().done_at.is_some());

    // Changed here: sent there.
    todo::update(
        &s,
        &here.id,
        &input("Call the bank about fees"),
        NY,
        t("2026-11-18T15:00:00Z"),
    )
    .unwrap();
    sync_account(&s, &svc, "g", true, NY, t("2026-11-18T16:00:00Z")).unwrap();
    assert!(
        svc.list("@default")
            .unwrap()
            .iter()
            .any(|r| r.title == "Call the bank about fees")
    );
    assert_eq!(
        todo::get(&s, &here.id).unwrap().due,
        Some(crate::tasks::Due::At {
            at: t("2026-11-20T20:30:00Z")
        })
    );

    // Deleted here: a marker until the deletion has gone there, then nothing.
    todo::delete(&s, &here.id).unwrap();
    assert!(todo::all(&s).unwrap().iter().all(|t| t.id != here.id));
    assert!(
        todo::all_records(&s)
            .unwrap()
            .iter()
            .any(|t| t.id == here.id)
    );
    sync_account(&s, &svc, "g", true, NY, t("2026-11-18T17:00:00Z")).unwrap();
    assert!(
        svc.list("@default")
            .unwrap()
            .iter()
            .all(|r| r.title != "Call the bank about fees")
    );
    assert!(
        todo::all_records(&s)
            .unwrap()
            .iter()
            .all(|t| t.id != here.id)
    );

    // Deleted on the phone: deleted here.
    svc.delete("@default", "phone-1").unwrap();
    sync_account(&s, &svc, "g", true, NY, t("2026-11-18T18:00:00Z")).unwrap();
    assert!(todo::all(&s).unwrap().is_empty());
}

#[test]
fn stopping_sync_keeps_the_tasks_as_ordinary_ones() {
    let s = store();
    let svc = Fake::default();
    let now = t("2026-11-18T12:00:00Z");
    let a = todo::add(&s, &input("Keep me"), NY, now).unwrap();
    let b = todo::add(&s, &input("Deleted"), NY, now).unwrap();
    sync_account(&s, &svc, "g", true, NY, now).unwrap();
    todo::delete(&s, &b.id).unwrap();
    unlink_account(&s, "g").unwrap();
    let left = todo::all_records(&s).unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].id, a.id);
    assert!(left[0].link.is_none());
}

fn tokens(server: &TestServer) -> (Arc<dyn HttpClient>, TokenManager) {
    let secrets = Arc::new(MemorySecretStore::default());
    let tk = TokenSet {
        access_token: "AT".into(),
        refresh_token: "RT".into(),
        expires_at: Utc::now() + chrono::Duration::hours(1),
    };
    secrets
        .set(&secret_key("acct"), &serde_json::to_string(&tk).unwrap())
        .unwrap();
    let cfg = OAuthConfig {
        client_id: "c".into(),
        client_secret: None,
        auth_url: "https://a.example/auth".into(),
        token_url: server.url("/token"),
        scopes: vec![],
    };
    let http: Arc<dyn HttpClient> = Arc::new(ReqwestClient::new_allow_http_for_tests().unwrap());
    (http.clone(), TokenManager { http, secrets, cfg })
}

#[test]
fn google_tasks_are_read_page_by_page_and_written_as_google_expects() {
    let seen = Arc::new(Mutex::new(Vec::<(String, String, String)>::new()));
    let s2 = seen.clone();
    let server = TestServer::start(move |r| {
        s2.lock()
            .unwrap()
            .push((r.method.clone(), r.path.clone(), r.body.clone()));
        if r.header("authorization") != Some("Bearer AT") {
            return Resp::status(401);
        }
        if r.method == "GET" && r.path.contains("pageToken=p2") {
            Resp::json(
                r#"{"items":[{"id":"b","title":"Second","status":"completed","completed":"2026-11-17T10:00:00.000Z","updated":"2026-11-17T10:00:00.000Z","etag":"\"e2\""}]}"#,
            )
        } else if r.method == "GET" {
            Resp::json(
                r#"{"nextPageToken":"p2","items":[{"id":"a","title":"First","notes":"n","due":"2026-11-20T00:00:00.000Z","status":"needsAction","updated":"2026-11-16T10:00:00.000Z","etag":"\"e1\""},{"id":"gone","title":"x","deleted":true,"updated":"2026-11-16T10:00:00.000Z"}]}"#,
            )
        } else if r.method == "POST" {
            Resp::json(
                r#"{"id":"new","title":"Made","status":"needsAction","updated":"2026-11-18T10:00:00.000Z","etag":"\"e3\""}"#,
            )
        } else if r.method == "DELETE" {
            Resp::status(404)
        } else {
            Resp::status(400)
        }
    });
    let (http, tm) = tokens(&server);
    let g = GoogleTasks::new(http, tm, "acct", &server.url("/tasks/v1"));
    let all = g.list("@default").unwrap();
    assert_eq!(
        all.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert_eq!(all[0].due, Some("2026-11-20".parse().unwrap()));
    assert!(all[1].done && all[1].completed_at.is_some());
    let made = g
        .create(
            "@default",
            &TaskFields {
                title: "Made".into(),
                notes: None,
                due: Some("2026-11-21".parse().unwrap()),
                done: false,
            },
        )
        .unwrap();
    assert_eq!(made.stamp, "\"e3\"");
    g.delete("@default", "a/../../x").unwrap(); // 404 counts as gone
    let calls = seen.lock().unwrap().clone();
    let post = calls.iter().find(|c| c.0 == "POST").unwrap();
    assert_eq!(post.1, "/tasks/v1/lists/@default/tasks");
    let body: Value = serde_json::from_str(&post.2).unwrap();
    assert_eq!(body["due"], "2026-11-21T00:00:00.000Z");
    assert_eq!(body["status"], "needsAction");
    // An id is one path segment, however odd.
    let del = calls.iter().find(|c| c.0 == "DELETE").unwrap();
    assert_eq!(del.1, "/tasks/v1/lists/@default/tasks/a%2F..%2F..%2Fx");
}

#[test]
fn microsoft_uses_the_default_list_and_follows_its_own_next_links_only() {
    let base = Arc::new(Mutex::new(String::new()));
    let b2 = base.clone();
    let server = TestServer::start(move |r| {
        if r.path == "/v1.0/me/todo/lists" {
            return Resp::json(
                r#"{"value":[{"id":"other","wellknownListName":"none"},{"id":"L1","wellknownListName":"defaultList"}]}"#,
            );
        }
        if r.path.starts_with("/v1.0/me/todo/lists/L1/tasks?$top=100")
            || r.path
                .starts_with("/v1.0/me/todo/lists/L1/tasks?%24top=100")
        {
            let next = format!("{}/v1.0/me/todo/lists/L1/tasks?page=2", b2.lock().unwrap());
            return Resp::json(&format!(
                r#"{{"@odata.nextLink":"{next}","value":[{{"id":"m1","title":"Report","status":"notStarted","body":{{"content":"draft","contentType":"text"}},"dueDateTime":{{"dateTime":"2026-11-20T00:00:00.0000000","timeZone":"UTC"}},"lastModifiedDateTime":"2026-11-16T10:00:00Z","@odata.etag":"W/1"}}]}}"#
            ));
        }
        if r.path == "/v1.0/me/todo/lists/L1/tasks?page=2" {
            return Resp::json(
                r#"{"@odata.nextLink":"https://evil.example/steal","value":[{"id":"m2","title":"Html note","status":"completed","body":{"content":"<p>x</p>","contentType":"html"},"completedDateTime":{"dateTime":"2026-11-17T09:00:00.0000000","timeZone":"UTC"},"lastModifiedDateTime":"2026-11-17T09:00:00Z"}]}"#,
            );
        }
        Resp::status(404)
    });
    *base.lock().unwrap() = server.url("");
    let (http, tm) = tokens(&server);
    let m = MicrosoftTodo::new(http, tm, "acct", &server.url("/v1.0"));
    assert_eq!(m.list_id().unwrap(), "L1");
    let all = m.list("L1").unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(
        (all[0].notes.as_deref(), all[0].due),
        (Some("draft"), Some("2026-11-20".parse().unwrap()))
    );
    assert_eq!(all[1].notes, None); // HTML notes are not brought in
    assert_eq!(all[1].completed_at, Some(t("2026-11-17T09:00:00Z")));
}
