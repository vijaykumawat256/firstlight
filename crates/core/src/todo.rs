//! The to-do list, stored on this computer (one JSON record per task in the encrypted database).
//!
//! The rules (checking, grouping, carry-over, reminders) are in `firstlight_logic::tasks`; this file reads and writes
//! the records and links a task to the calendar event made to do it.

use crate::local::{self, EventInput};
use crate::model::{When, new_id};
use crate::store::Store;
use crate::tasks::{self, Task, TaskInput, TaskLists};
use crate::{Error, Result};
use chrono::{DateTime, Utc};
use chrono_tz::Tz;

/// Every task. A record that cannot be read is skipped, never allowed to hide the others.
pub fn all(store: &Store) -> Result<Vec<Task>> {
    Ok(store
        .all_tasks()?
        .into_iter()
        .filter_map(|(_, json)| serde_json::from_str::<Task>(&json).ok())
        .collect())
}

pub fn lists(store: &Store, now: DateTime<Utc>, tz: Tz) -> Result<TaskLists> {
    Ok(tasks::lists(&all(store)?, now, tz))
}

pub fn get(store: &Store, id: &str) -> Result<Task> {
    store
        .get_task(id)?
        .and_then(|j| serde_json::from_str::<Task>(&j).ok())
        .ok_or(Error::Invalid("That task is no longer on your list."))
}

pub fn put(store: &Store, task: &Task) -> Result<()> {
    let json =
        serde_json::to_string(task).map_err(|_| Error::Invalid("That task couldn't be saved."))?;
    store.put_task(&task.id, &json)
}

pub fn add(store: &Store, input: &TaskInput, tz: Tz, now: DateTime<Utc>) -> Result<Task> {
    let c = tasks::check(input, tz)?;
    if store.all_tasks()?.len() >= tasks::MAX_TASKS {
        return Err(Error::Invalid(
            "Your list is full. Delete some done tasks first.",
        ));
    }
    let task = Task {
        id: new_id(),
        title: c.title,
        notes: c.notes,
        due: c.due,
        minutes: c.minutes,
        done_at: None,
        created_at: now,
        updated_at: now,
        from_meeting: c.from_meeting,
        planned_event: None,
        link: None,
    };
    put(store, &task)?;
    Ok(task)
}

/// Changes what was typed; whether it is done, where it came from and its links stay.
pub fn update(
    store: &Store,
    id: &str,
    input: &TaskInput,
    tz: Tz,
    now: DateTime<Utc>,
) -> Result<Task> {
    let mut task = get(store, id)?;
    let c = tasks::check(input, tz)?;
    task.title = c.title;
    task.notes = c.notes;
    task.due = c.due;
    task.minutes = c.minutes;
    if c.from_meeting.is_some() {
        task.from_meeting = c.from_meeting;
    }
    task.updated_at = now;
    put(store, &task)?;
    Ok(task)
}

pub fn set_done(store: &Store, id: &str, done: bool, now: DateTime<Utc>) -> Result<Task> {
    let mut task = get(store, id)?;
    task.done_at = done.then_some(task.done_at.unwrap_or(now));
    task.updated_at = now;
    put(store, &task)?;
    Ok(task)
}

/// Deletes the task. An event planned for it stays in the calendar (the person may still want that time).
pub fn delete(store: &Store, id: &str) -> Result<()> {
    get(store, id)?;
    store.delete_task(id)
}

/// Sets time aside for a task: adds an event of your own (kept on this computer) from `start` to `end` with the
/// task's title, and remembers it on the task.
pub fn plan(
    store: &mut Store,
    id: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    tz: Tz,
    now: DateTime<Utc>,
) -> Result<Task> {
    let mut task = get(store, id)?;
    if task.done_at.is_some() {
        return Err(Error::Invalid("That task is already done."));
    }
    let title: String = task.title.chars().take(200).collect();
    let event = local::save(
        store,
        None,
        &EventInput {
            title,
            all_day: false,
            start: start.to_rfc3339(),
            end: end.to_rfc3339(),
            location: None,
            join_url: None,
            notes: Some("Time set aside for a task on your list.".into()),
            repeat: None,
            reminder_minutes: None,
            invitees: vec![],
            focus: false,
        },
        tz,
        now,
    )?;
    debug_assert!(matches!(event.when, When::Timed { .. }));
    task.planned_event = Some(event.id);
    task.updated_at = now;
    put(store, &task)?;
    Ok(task)
}

/// Adds several tasks at once (lines picked from a meeting note), all marked as coming from that meeting.
pub fn add_many(
    store: &Store,
    titles: &[String],
    from_meeting: Option<&str>,
    tz: Tz,
    now: DateTime<Utc>,
) -> Result<Vec<Task>> {
    if titles.is_empty() || titles.len() > 30 {
        return Err(Error::Invalid("Pick between 1 and 30 lines."));
    }
    // Check them all first, so a bad one adds none.
    let inputs: Vec<TaskInput> = titles
        .iter()
        .map(|t| TaskInput {
            title: t.clone(),
            notes: None,
            due_date: None,
            due_time: None,
            minutes: None,
            from_meeting: from_meeting.map(str::to_string),
        })
        .collect();
    for i in &inputs {
        tasks::check(i, tz)?;
    }
    inputs.iter().map(|i| add(store, i, tz, now)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::{MemorySecretStore, load_or_create_db_key};

    const NY: Tz = chrono_tz::America::New_York;

    fn store() -> Store {
        let key = load_or_create_db_key(&MemorySecretStore::default()).unwrap();
        Store::open_in_memory(&key).unwrap()
    }

    fn t(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn input(title: &str, day: Option<&str>) -> TaskInput {
        TaskInput {
            title: title.into(),
            notes: None,
            due_date: day.map(str::to_string),
            due_time: None,
            minutes: None,
            from_meeting: None,
        }
    }

    #[test]
    fn tasks_are_added_changed_done_and_deleted() {
        let s = store();
        let now = t("2026-11-18T15:00:00Z");
        let a = add(&s, &input("Call the bank", Some("2026-11-17")), NY, now).unwrap();
        let b = add(&s, &input("Someday thing", None), NY, now).unwrap();
        let l = lists(&s, now, NY).unwrap();
        assert_eq!(l.today.len(), 1);
        assert_eq!(l.today[0].days_late, 1);
        assert_eq!(l.someday[0].task.id, b.id);

        let changed = update(
            &s,
            &a.id,
            &input("Call the bank about fees", Some("2026-11-20")),
            NY,
            now,
        )
        .unwrap();
        assert_eq!(changed.created_at, a.created_at);
        assert_eq!(lists(&s, now, NY).unwrap().upcoming.len(), 1);

        let done = set_done(&s, &a.id, true, now).unwrap();
        assert_eq!(done.done_at, Some(now));
        // Ticking it again keeps the first time it was done.
        let again = set_done(&s, &a.id, true, now + chrono::Duration::hours(1)).unwrap();
        assert_eq!(again.done_at, Some(now));
        assert_eq!(lists(&s, now, NY).unwrap().done_today, 1);
        assert_eq!(set_done(&s, &a.id, false, now).unwrap().done_at, None);

        delete(&s, &a.id).unwrap();
        assert!(get(&s, &a.id).is_err());
        assert!(delete(&s, &a.id).is_err());
        assert_eq!(all(&s).unwrap().len(), 1);
    }

    #[test]
    fn planning_a_task_adds_an_event_of_your_own_and_remembers_it() {
        let mut s = store();
        let now = t("2026-11-18T15:00:00Z");
        let a = add(&s, &input("Write the report", None), NY, now).unwrap();
        let planned = plan(
            &mut s,
            &a.id,
            t("2026-11-19T14:00:00Z"),
            t("2026-11-19T15:00:00Z"),
            NY,
            now,
        )
        .unwrap();
        let event_id = planned.planned_event.clone().unwrap();
        let ev = s.get_event(&event_id).unwrap().unwrap();
        assert_eq!(ev.title, "Write the report");
        assert_eq!(ev.calendar_id, local::CALENDAR_ID);
        set_done(&s, &a.id, true, now).unwrap();
        assert!(
            plan(
                &mut s,
                &a.id,
                t("2026-11-19T14:00:00Z"),
                t("2026-11-19T15:00:00Z"),
                NY,
                now
            )
            .is_err()
        );
    }

    #[test]
    fn lines_from_a_meeting_note_become_tasks_all_or_none() {
        let s = store();
        let now = t("2026-11-18T15:00:00Z");
        let made = add_many(
            &s,
            &["Send slides".into(), "Book room".into()],
            Some("Design review"),
            NY,
            now,
        )
        .unwrap();
        assert_eq!(made.len(), 2);
        assert!(
            made.iter()
                .all(|t| t.from_meeting.as_deref() == Some("Design review"))
        );
        assert!(add_many(&s, &["ok".into(), " ".into()], None, NY, now).is_err());
        assert_eq!(all(&s).unwrap().len(), 2);
    }

    #[test]
    fn an_unreadable_record_does_not_hide_the_rest() {
        let s = store();
        let now = t("2026-11-18T15:00:00Z");
        add(&s, &input("Fine", None), NY, now).unwrap();
        s.put_task("broken", "{not json").unwrap();
        assert_eq!(all(&s).unwrap().len(), 1);
    }
}
