//! Apple Reminders as a task list (macOS): your tasks kept in step with the Reminders app (and so with your iPhone,
//! Watch and Siri) through EventKit. It is one more `TaskService`, so the same rules apply as for Google Tasks and
//! Microsoft To Do (`firstlight_logic::tasksync`). Uses the default Reminders list. Needs the person's permission
//! (System Settings, Privacy and Security, Reminders), asked for once when they turn it on.
//!
//! As with the Calendar app, a fresh `EKEventStore` is made per call so no Objective-C object crosses threads.

use firstlight_core::remote_tasks::TaskService;
use firstlight_core::tasksync::{RemoteTask, TaskFields};
use firstlight_core::{Error, Result};

/// The id under which Reminders sits in `settings.tasks.sync_accounts` (there is no account record for it).
pub(crate) const ID: &str = "apple-reminders";
pub(crate) const AVAILABLE: bool = cfg!(target_os = "macos");

/// Whether Reminders access is granted (without asking).
pub(crate) fn granted() -> bool {
    imp::granted()
}

/// Asks for Reminders access (shows the system prompt the first time). True if granted.
pub(crate) fn request() -> Result<bool> {
    imp::request()
}

pub(crate) struct RemindersService;

impl TaskService for RemindersService {
    fn list_id(&self) -> Result<String> {
        imp::list_id()
    }
    fn list(&self, list: &str) -> Result<Vec<RemoteTask>> {
        imp::list(list)
    }
    fn create(&self, list: &str, f: &TaskFields) -> Result<RemoteTask> {
        imp::save(list, None, f)
    }
    fn update(&self, list: &str, id: &str, f: &TaskFields) -> Result<RemoteTask> {
        imp::save(list, Some(id), f)
    }
    fn delete(&self, _list: &str, id: &str) -> Result<()> {
        imp::delete(id)
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use block2::{DynBlock, RcBlock};
    use chrono::{DateTime, NaiveDate, Utc};
    use objc2::rc::{Retained, autoreleasepool};
    use objc2::runtime::{Bool, NSObjectProtocol};
    use objc2_event_kit::{
        EKAuthorizationStatus, EKCalendar, EKEntityType, EKEventStore, EKReminder,
    };
    use objc2_foundation::{NSArray, NSDate, NSDateComponents, NSError, NSString};
    use std::sync::mpsc;
    use std::time::Duration;

    /// Notes longer than this are cut (ours allow 4000).
    const MAX_NOTES: usize = 4000;

    pub(super) fn granted() -> bool {
        let status =
            unsafe { EKEventStore::authorizationStatusForEntityType(EKEntityType::Reminder) };
        status == EKAuthorizationStatus::FullAccess
    }

    pub(super) fn request() -> Result<bool> {
        if granted() {
            return Ok(true);
        }
        let status =
            unsafe { EKEventStore::authorizationStatusForEntityType(EKEntityType::Reminder) };
        if status != EKAuthorizationStatus::NotDetermined {
            return Ok(false);
        }
        let store = unsafe { EKEventStore::new() };
        if !store.respondsToSelector(objc2::sel!(requestFullAccessToRemindersWithCompletion:)) {
            return Err(Error::Provider("Reminders needs macOS 14 or later".into()));
        }
        let (tx, rx) = mpsc::channel::<bool>();
        let block = RcBlock::new(move |ok: Bool, _e: *mut NSError| {
            let _ = tx.send(ok.as_bool());
        });
        unsafe {
            store.requestFullAccessToRemindersWithCompletion(
                RcBlock::as_ptr(&block) as *mut DynBlock<dyn Fn(Bool, *mut NSError)>
            )
        };
        match rx.recv_timeout(Duration::from_secs(180)) {
            Ok(_) => Ok(granted()),
            Err(_) => Err(Error::Provider(
                "the Reminders permission prompt was not answered".into(),
            )),
        }
    }

    fn gone() -> Error {
        Error::Provider("the reminder is no longer in Reminders".into())
    }

    fn need_access() -> Result<()> {
        if granted() {
            Ok(())
        } else {
            Err(Error::PermissionNeeded)
        }
    }

    fn date_of(d: &NSDate) -> Option<DateTime<Utc>> {
        DateTime::from_timestamp(d.timeIntervalSince1970().floor() as i64, 0)
    }

    pub(super) fn day_of(c: &NSDateComponents) -> Option<NaiveDate> {
        let (y, m, d) = (c.year(), c.month(), c.day());
        // NSDateComponents uses a huge "undefined" value for missing parts.
        if !(1..=9999).contains(&y) || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
            return None;
        }
        NaiveDate::from_ymd_opt(y as i32, m as u32, d as u32)
    }

    pub(super) fn components(day: NaiveDate) -> Retained<NSDateComponents> {
        use chrono::Datelike;
        let c = NSDateComponents::new();
        c.setYear(day.year() as isize);
        c.setMonth(day.month() as isize);
        c.setDay(day.day() as isize);
        c
    }

    fn remote_of(r: &EKReminder) -> RemoteTask {
        unsafe {
            let updated = r
                .lastModifiedDate()
                .as_deref()
                .and_then(date_of)
                .unwrap_or_else(Utc::now);
            RemoteTask {
                id: r.calendarItemIdentifier().to_string(),
                title: r.title().to_string(),
                notes: r
                    .notes()
                    .map(|n| n.to_string())
                    .filter(|n| !n.trim().is_empty()),
                due: r.dueDateComponents().as_deref().and_then(day_of),
                done: r.isCompleted(),
                completed_at: r.completionDate().as_deref().and_then(date_of),
                updated,
                stamp: updated.timestamp().to_string(),
            }
        }
    }

    fn calendar(store: &EKEventStore, id: &str) -> Result<Retained<EKCalendar>> {
        unsafe { store.calendarWithIdentifier(&NSString::from_str(id)) }
            .ok_or_else(|| Error::Provider("the Reminders list is gone".into()))
    }

    pub(super) fn list_id() -> Result<String> {
        need_access()?;
        autoreleasepool(|_| {
            let store = unsafe { EKEventStore::new() };
            unsafe { store.defaultCalendarForNewReminders() }
                .map(|c| unsafe { c.calendarIdentifier() }.to_string())
                .ok_or_else(|| Error::Provider("there is no default Reminders list".into()))
        })
    }

    pub(super) fn list(list: &str) -> Result<Vec<RemoteTask>> {
        need_access()?;
        let store = unsafe { EKEventStore::new() };
        let cal = calendar(&store, list)?;
        let cals = NSArray::from_retained_slice(&[cal]);
        let predicate = unsafe { store.predicateForRemindersInCalendars(Some(&cals)) };
        let (tx, rx) = mpsc::channel::<Vec<RemoteTask>>();
        // The answer arrives on a queue of EventKit's own; the reminders are read there, inside the block.
        let block = RcBlock::new(move |found: *mut NSArray<EKReminder>| {
            let out = unsafe { found.as_ref() }
                .map(|a| a.iter().map(|r| remote_of(&r)).collect())
                .unwrap_or_default();
            let _ = tx.send(out);
        });
        let _fetch =
            unsafe { store.fetchRemindersMatchingPredicate_completion(&predicate, &block) };
        rx.recv_timeout(Duration::from_secs(30))
            .map_err(|_| Error::Provider("Reminders did not answer".into()))
    }

    pub(super) fn save(list: &str, existing: Option<&str>, f: &TaskFields) -> Result<RemoteTask> {
        need_access()?;
        autoreleasepool(|_| {
            let store = unsafe { EKEventStore::new() };
            let reminder: Retained<EKReminder> = match existing {
                Some(id) => {
                    let item = unsafe { store.calendarItemWithIdentifier(&NSString::from_str(id)) }
                        .ok_or_else(gone)?;
                    item.downcast::<EKReminder>().map_err(|_| gone())?
                }
                None => {
                    let r = unsafe { EKReminder::reminderWithEventStore(&store) };
                    let cal = calendar(&store, list)?;
                    unsafe { r.setCalendar(Some(&cal)) };
                    r
                }
            };
            let notes: Option<String> = f
                .notes
                .as_ref()
                .map(|n| n.chars().take(MAX_NOTES).collect());
            unsafe {
                reminder.setTitle(Some(&NSString::from_str(&f.title)));
                reminder.setNotes(notes.as_deref().map(NSString::from_str).as_deref());
                reminder.setDueDateComponents(f.due.map(components).as_deref());
                if reminder.isCompleted() != f.done {
                    reminder.setCompleted(f.done);
                }
                store
                    .saveReminder_commit_error(&reminder, true)
                    .map_err(|_| Error::Provider("Reminders did not save it".into()))?;
            }
            Ok(remote_of(&reminder))
        })
    }

    pub(super) fn delete(id: &str) -> Result<()> {
        need_access()?;
        autoreleasepool(|_| {
            let store = unsafe { EKEventStore::new() };
            let Some(item) = (unsafe { store.calendarItemWithIdentifier(&NSString::from_str(id)) })
            else {
                return Ok(()); // already gone
            };
            let Ok(reminder) = item.downcast::<EKReminder>() else {
                return Ok(());
            };
            unsafe { store.removeReminder_commit_error(&reminder, true) }
                .map_err(|_| Error::Provider("Reminders did not delete it".into()))
        })
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::*;
    pub(super) fn granted() -> bool {
        false
    }
    pub(super) fn request() -> Result<bool> {
        Ok(false)
    }
    fn none<T>() -> Result<T> {
        Err(Error::Provider("Reminders is on Macs only".into()))
    }
    pub(super) fn list_id() -> Result<String> {
        none()
    }
    pub(super) fn list(_: &str) -> Result<Vec<RemoteTask>> {
        none()
    }
    pub(super) fn save(_: &str, _: Option<&str>, _: &TaskFields) -> Result<RemoteTask> {
        none()
    }
    pub(super) fn delete(_: &str) -> Result<()> {
        none()
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    /// Reads the real permission state without asking (it must never show a prompt from a test).
    #[test]
    fn reads_the_reminders_permission_without_prompting() {
        let _ = granted();
    }

    #[test]
    fn a_due_day_goes_into_a_reminder_and_back_unchanged() {
        use chrono::NaiveDate;
        for d in ["2026-11-30", "2027-02-28", "2028-02-29", "2026-01-01"] {
            let day: NaiveDate = d.parse().unwrap();
            assert_eq!(imp::day_of(&imp::components(day)), Some(day));
        }
        // A reminder without a day: every part "undefined".
        let empty = objc2_foundation::NSDateComponents::new();
        assert_eq!(imp::day_of(&empty), None);
    }

    /// Run by hand after allowing Reminders for the test binary: lists the default list.
    #[test]
    #[ignore]
    fn live_reminders_list() {
        let svc = RemindersService;
        let list = svc.list_id().expect("default list");
        let all = svc.list(&list).expect("reminders");
        println!("{} reminders in the default list", all.len());
    }
}
