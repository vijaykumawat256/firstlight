//! Keeping the task list in step with Google Tasks or Microsoft To Do: what needs doing on each side.
//!
//! Pure: given this computer's tasks and the service's tasks, it returns the actions to take; `firstlight-core`
//! carries them out. A task is linked to one account (`TaskLink`). A change is told apart by the service's own change
//! stamp (remote) and by the task changing after it was last synced (local). When both sides changed, the newer change
//! wins. A task deleted here leaves a marker (`deleted_at`) so the deletion reaches the service; a task deleted there is
//! deleted here unless it was changed here since. Services keep only a due *day*: a time set here stays here.

use crate::tasks::{Due, Task};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use chrono_tz::Tz;

/// Done tasks on the service older than this are not brought in.
const DONE_DAYS: i64 = 30;

/// A task as the service has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteTask {
    pub id: String,
    pub title: String,
    pub notes: Option<String>,
    pub due: Option<NaiveDate>,
    pub done: bool,
    pub completed_at: Option<DateTime<Utc>>,
    /// When the service says it last changed, and its change stamp exactly as given.
    pub updated: DateTime<Utc>,
    pub stamp: String,
}

/// The fields sent to the service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskFields {
    pub title: String,
    pub notes: Option<String>,
    pub due: Option<NaiveDate>,
    pub done: bool,
}

pub fn fields_of(t: &Task, tz: Tz) -> TaskFields {
    TaskFields {
        title: t.title.clone(),
        notes: t.notes.clone(),
        due: t.due.map(|d| d.day(tz)),
        done: t.done_at.is_some(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// A task new on the service: add it here.
    CreateLocal(RemoteTask),
    /// The service's version wins: copy it onto the task here.
    UpdateLocal { task_id: String, remote: RemoteTask },
    /// Deleted on the service and not changed here since: delete it here.
    DeleteLocal { task_id: String },
    /// Send a task to the service (new here, or deleted there after being changed here).
    CreateRemote { task_id: String },
    /// The version here wins: send it.
    UpdateRemote { task_id: String, remote_id: String },
    /// Deleted here: delete it on the service, then forget the marker.
    DeleteRemote { task_id: String, remote_id: String },
    /// A deletion marker whose task is already gone from the service.
    ForgetMarker { task_id: String },
}

/// What to do for one account. `primary` is the account new tasks made here go to (one account takes them).
pub fn reconcile(
    account_id: &str,
    primary: bool,
    local: &[Task],
    remote: &[RemoteTask],
    now: DateTime<Utc>,
) -> Vec<Action> {
    let mut out = Vec::new();
    let linked_here = |t: &Task| t.link.as_ref().is_some_and(|l| l.account_id == account_id);
    for t in local {
        let Some(link) = t.link.as_ref().filter(|l| l.account_id == account_id) else {
            // Not linked to any account: send open tasks to the primary account.
            if t.link.is_none() && primary && t.deleted_at.is_none() && t.done_at.is_none() {
                out.push(Action::CreateRemote {
                    task_id: t.id.clone(),
                });
            }
            continue;
        };
        let there = remote.iter().find(|r| r.id == link.remote_id);
        let changed_here = link.synced_at.is_none_or(|s| t.updated_at > s);
        match (t.deleted_at.is_some(), there) {
            (true, Some(r)) => out.push(Action::DeleteRemote {
                task_id: t.id.clone(),
                remote_id: r.id.clone(),
            }),
            (true, None) => out.push(Action::ForgetMarker {
                task_id: t.id.clone(),
            }),
            (false, None) if changed_here => out.push(Action::CreateRemote {
                task_id: t.id.clone(),
            }),
            (false, None) => out.push(Action::DeleteLocal {
                task_id: t.id.clone(),
            }),
            (false, Some(r)) => {
                let changed_there = link.remote_updated.as_deref() != Some(r.stamp.as_str());
                match (changed_here, changed_there) {
                    (false, false) => {}
                    (false, true) => out.push(Action::UpdateLocal {
                        task_id: t.id.clone(),
                        remote: r.clone(),
                    }),
                    (true, false) => out.push(Action::UpdateRemote {
                        task_id: t.id.clone(),
                        remote_id: r.id.clone(),
                    }),
                    (true, true) if r.updated > t.updated_at => out.push(Action::UpdateLocal {
                        task_id: t.id.clone(),
                        remote: r.clone(),
                    }),
                    (true, true) => out.push(Action::UpdateRemote {
                        task_id: t.id.clone(),
                        remote_id: r.id.clone(),
                    }),
                }
            }
        }
    }
    for r in remote {
        let known = local
            .iter()
            .any(|t| linked_here(t) && t.link.as_ref().is_some_and(|l| l.remote_id == r.id));
        let recent =
            !r.done || r.completed_at.unwrap_or(r.updated) >= now - Duration::days(DONE_DAYS);
        if !known && recent && !r.title.trim().is_empty() {
            out.push(Action::CreateLocal(r.clone()));
        }
    }
    out
}

/// The service's version copied onto a task here. A due day that did not change keeps the time set here; being done
/// keeps the moment it was first done.
pub fn apply_remote(t: &mut Task, r: &RemoteTask, tz: Tz, now: DateTime<Utc>) {
    t.title = r
        .title
        .trim()
        .chars()
        .take(crate::tasks::MAX_TITLE)
        .collect();
    t.notes = r.notes.clone().filter(|n| !n.trim().is_empty());
    t.due = match (t.due, r.due) {
        (_, None) => None,
        (Some(local), Some(day)) if local.day(tz) == day => Some(local),
        (_, Some(day)) => Some(Due::Date { date: day }),
    };
    t.done_at = match (t.done_at, r.done) {
        (_, false) => None,
        (Some(d), true) => Some(d),
        (None, true) => Some(r.completed_at.unwrap_or(now)),
    };
    t.updated_at = now;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::TaskLink;

    const NY: Tz = chrono_tz::America::New_York;

    fn t(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn remote(id: &str, stamp: &str, updated: &str) -> RemoteTask {
        RemoteTask {
            id: id.into(),
            title: format!("Remote {id}"),
            notes: None,
            due: None,
            done: false,
            completed_at: None,
            updated: t(updated),
            stamp: stamp.into(),
        }
    }

    fn task(id: &str, updated: &str, link: Option<(&str, &str, &str)>) -> Task {
        Task {
            id: id.into(),
            title: id.into(),
            notes: None,
            due: None,
            minutes: None,
            done_at: None,
            created_at: t("2026-11-01T00:00:00Z"),
            updated_at: t(updated),
            from_meeting: None,
            planned_event: None,
            link: link.map(|(remote_id, stamp, synced)| TaskLink {
                account_id: "g".into(),
                list_id: "@default".into(),
                remote_id: remote_id.into(),
                remote_updated: Some(stamp.into()),
                synced_at: Some(t(synced)),
            }),
            deleted_at: None,
        }
    }

    const NOW: &str = "2026-11-18T12:00:00Z";

    #[test]
    fn each_side_s_changes_go_the_right_way() {
        let local = vec![
            task(
                "same",
                "2026-11-10T00:00:00Z",
                Some(("r-same", "s1", "2026-11-10T00:00:00Z")),
            ),
            task(
                "changed-there",
                "2026-11-10T00:00:00Z",
                Some(("r-there", "s1", "2026-11-10T00:00:00Z")),
            ),
            task(
                "changed-here",
                "2026-11-12T00:00:00Z",
                Some(("r-here", "s1", "2026-11-10T00:00:00Z")),
            ),
            task(
                "both-newer-here",
                "2026-11-13T00:00:00Z",
                Some(("r-both1", "s1", "2026-11-10T00:00:00Z")),
            ),
            task(
                "both-newer-there",
                "2026-11-11T00:00:00Z",
                Some(("r-both2", "s1", "2026-11-10T00:00:00Z")),
            ),
            task("new-here", "2026-11-12T00:00:00Z", None),
        ];
        let remote = vec![
            remote("r-same", "s1", "2026-11-09T00:00:00Z"),
            remote("r-there", "s2", "2026-11-12T00:00:00Z"),
            remote("r-here", "s1", "2026-11-09T00:00:00Z"),
            remote("r-both1", "s2", "2026-11-12T00:00:00Z"),
            remote("r-both2", "s2", "2026-11-12T00:00:00Z"),
            remote("r-new", "s1", "2026-11-12T00:00:00Z"),
        ];
        let a = reconcile("g", true, &local, &remote, t(NOW));
        assert_eq!(
            a,
            vec![
                Action::UpdateLocal {
                    task_id: "changed-there".into(),
                    remote: remote[1].clone()
                },
                Action::UpdateRemote {
                    task_id: "changed-here".into(),
                    remote_id: "r-here".into()
                },
                Action::UpdateRemote {
                    task_id: "both-newer-here".into(),
                    remote_id: "r-both1".into()
                },
                Action::UpdateLocal {
                    task_id: "both-newer-there".into(),
                    remote: remote[4].clone()
                },
                Action::CreateRemote {
                    task_id: "new-here".into()
                },
                Action::CreateLocal(remote[5].clone()),
            ]
        );
        // Only the primary account takes tasks made here.
        assert!(
            !reconcile("g", false, &local, &remote, t(NOW)).contains(&Action::CreateRemote {
                task_id: "new-here".into()
            })
        );
    }

    #[test]
    fn deletions_reach_the_other_side_once() {
        let mut deleted_here = task(
            "gone-here",
            "2026-11-12T00:00:00Z",
            Some(("r1", "s1", "2026-11-10T00:00:00Z")),
        );
        deleted_here.deleted_at = Some(t("2026-11-12T00:00:00Z"));
        let mut marker_only = task(
            "marker",
            "2026-11-12T00:00:00Z",
            Some(("r-missing", "s1", "2026-11-10T00:00:00Z")),
        );
        marker_only.deleted_at = Some(t("2026-11-12T00:00:00Z"));
        let local = vec![
            deleted_here,
            marker_only,
            task(
                "gone-there",
                "2026-11-10T00:00:00Z",
                Some(("r2", "s1", "2026-11-10T00:00:00Z")),
            ),
            task(
                "gone-there-but-edited",
                "2026-11-12T00:00:00Z",
                Some(("r3", "s1", "2026-11-10T00:00:00Z")),
            ),
        ];
        let a = reconcile(
            "g",
            true,
            &local,
            &[remote("r1", "s1", "2026-11-09T00:00:00Z")],
            t(NOW),
        );
        assert_eq!(
            a,
            vec![
                Action::DeleteRemote {
                    task_id: "gone-here".into(),
                    remote_id: "r1".into()
                },
                Action::ForgetMarker {
                    task_id: "marker".into()
                },
                Action::DeleteLocal {
                    task_id: "gone-there".into()
                },
                Action::CreateRemote {
                    task_id: "gone-there-but-edited".into()
                },
            ]
        );
    }

    #[test]
    fn old_done_tasks_and_other_accounts_are_left_alone() {
        let mut old = remote("old", "s", "2026-09-01T00:00:00Z");
        old.done = true;
        old.completed_at = Some(t("2026-09-01T00:00:00Z"));
        let mut other = task(
            "other",
            "2026-11-12T00:00:00Z",
            Some(("x", "s", "2026-11-10T00:00:00Z")),
        );
        other.link.as_mut().unwrap().account_id = "m".into();
        let mut done_here = task("done-here", "2026-11-12T00:00:00Z", None);
        done_here.done_at = Some(t("2026-11-12T00:00:00Z"));
        assert!(reconcile("g", true, &[other, done_here], &[old], t(NOW)).is_empty());
    }

    #[test]
    fn the_service_s_version_keeps_a_time_set_here_when_the_day_is_the_same() {
        let mut local = task("a", "2026-11-10T00:00:00Z", None);
        local.due = Some(Due::At {
            at: t("2026-11-20T20:30:00Z"),
        }); // 15:30 in New York
        let mut r = remote("r", "s2", "2026-11-12T00:00:00Z");
        r.title = "Renamed".into();
        r.due = Some("2026-11-20".parse().unwrap());
        r.done = true;
        apply_remote(&mut local, &r, NY, t(NOW));
        assert_eq!(local.title, "Renamed");
        assert_eq!(
            local.due,
            Some(Due::At {
                at: t("2026-11-20T20:30:00Z")
            })
        );
        assert_eq!(local.done_at, Some(t(NOW)));
        r.due = Some("2026-11-21".parse().unwrap());
        r.done = false;
        apply_remote(&mut local, &r, NY, t(NOW));
        assert_eq!(
            local.due,
            Some(Due::Date {
                date: "2026-11-21".parse().unwrap()
            })
        );
        assert_eq!(local.done_at, None);
    }
}
