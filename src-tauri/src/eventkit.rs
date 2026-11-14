//! The macOS half of the system-calendar provider: reads the Calendar app's data through EventKit.
//!
//! Calendar access is a per-app permission the user grants once (System Settings, then Privacy and Security,
//! then Calendars). Firstlight only ever reads. A fresh `EKEventStore` is created per call, so no
//! non-thread-safe Objective-C object is shared between threads.

use chrono::{DateTime, Utc};
use daybrief_core::providers::system::{
    Access, SysCalendar, SysCalendarKind, SysEvent, SysResponse, SysStatus, SystemSource,
};
use daybrief_core::{Error, Result};

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use block2::{DynBlock, RcBlock};
    use objc2::rc::autoreleasepool;
    use objc2::runtime::{Bool, NSObjectProtocol};
    use objc2_event_kit::{
        EKAuthorizationStatus, EKCalendarType, EKEntityType, EKEvent, EKEventStatus, EKEventStore,
        EKParticipantStatus,
    };
    use objc2_foundation::{NSArray, NSDate, NSError, NSString};
    use std::sync::mpsc;
    use std::time::Duration;

    /// How long the permission prompt may stay on screen before we give up waiting.
    const PROMPT_TIMEOUT: Duration = Duration::from_secs(180);

    pub struct EventKitSource;

    fn status() -> Access {
        let s = unsafe { EKEventStore::authorizationStatusForEntityType(EKEntityType::Event) };
        if s == EKAuthorizationStatus::FullAccess {
            Access::Granted
        } else if s == EKAuthorizationStatus::NotDetermined {
            Access::NotDetermined
        } else if s == EKAuthorizationStatus::Restricted {
            Access::Restricted
        } else {
            // Denied, or write-only (not enough to read events).
            Access::Denied
        }
    }

    fn secs(d: &NSDate) -> Option<DateTime<Utc>> {
        DateTime::from_timestamp(d.timeIntervalSince1970().floor() as i64, 0)
    }

    fn kind_of(t: EKCalendarType) -> SysCalendarKind {
        if t == EKCalendarType::CalDAV {
            SysCalendarKind::CalDav
        } else if t == EKCalendarType::Exchange {
            SysCalendarKind::Exchange
        } else if t == EKCalendarType::Subscription {
            SysCalendarKind::Subscription
        } else if t == EKCalendarType::Birthday {
            SysCalendarKind::Birthday
        } else {
            SysCalendarKind::Local
        }
    }

    fn response_of(ev: &EKEvent) -> SysResponse {
        let Some(attendees) = (unsafe { ev.attendees() }) else {
            return SysResponse::NotInvited;
        };
        for a in attendees.iter() {
            if unsafe { a.isCurrentUser() } {
                let s = unsafe { a.participantStatus() };
                return if s == EKParticipantStatus::Pending {
                    SysResponse::Pending
                } else if s == EKParticipantStatus::Accepted {
                    SysResponse::Accepted
                } else if s == EKParticipantStatus::Declined {
                    SysResponse::Declined
                } else if s == EKParticipantStatus::Tentative {
                    SysResponse::Tentative
                } else {
                    SysResponse::Other
                };
            }
        }
        SysResponse::Other
    }

    fn map_event(ev: &EKEvent) -> Option<SysEvent> {
        unsafe {
            let start = secs(&ev.startDate())?;
            let end = secs(&ev.endDate())?;
            let id = ev
                .eventIdentifier()
                .map(|s| s.to_string())
                .or_else(|| ev.calendarItemExternalIdentifier().map(|s| s.to_string()))?;
            let st = ev.status();
            Some(SysEvent {
                id,
                uid: ev.calendarItemExternalIdentifier().map(|s| s.to_string()),
                title: ev.title().to_string(),
                start,
                end,
                all_day: ev.isAllDay(),
                location: ev.location().map(|s| s.to_string()),
                notes: ev.notes().map(|s| s.to_string()),
                url: ev
                    .URL()
                    .and_then(|u| u.absoluteString())
                    .map(|s| s.to_string()),
                status: if st == EKEventStatus::Canceled {
                    SysStatus::Canceled
                } else if st == EKEventStatus::Tentative {
                    SysStatus::Tentative
                } else if st == EKEventStatus::Confirmed {
                    SysStatus::Confirmed
                } else {
                    SysStatus::None
                },
                response: response_of(ev),
                organizer: ev.organizer().and_then(|o| o.name()).map(|s| s.to_string()),
                // EventKit only reports an occurrence date for instances of a repeating event.
                is_recurring: ev.occurrenceDate().is_some(),
                tzid: ev.timeZone().map(|z| z.name().to_string()),
                modified: ev
                    .lastModifiedDate()
                    .and_then(|d| secs(&d))
                    .map(|d| d.timestamp()),
            })
        }
    }

    impl SystemSource for EventKitSource {
        fn access(&self) -> Access {
            status()
        }

        fn request_access(&self) -> Result<Access> {
            match status() {
                Access::NotDetermined => {}
                other => return Ok(other),
            }
            let store = unsafe { EKEventStore::new() };
            // The full-access request only exists on macOS 14 and later.
            if !store.respondsToSelector(objc2::sel!(requestFullAccessToEventsWithCompletion:)) {
                return Ok(Access::Unsupported);
            }
            let (tx, rx) = mpsc::channel::<bool>();
            let block = RcBlock::new(move |granted: Bool, _error: *mut NSError| {
                let _ = tx.send(granted.as_bool());
            });
            let handler = RcBlock::as_ptr(&block) as *mut DynBlock<dyn Fn(Bool, *mut NSError)>;
            unsafe { store.requestFullAccessToEventsWithCompletion(handler) };
            match rx.recv_timeout(PROMPT_TIMEOUT) {
                Ok(_) => Ok(status()),
                Err(_) => Err(Error::Provider(
                    "the calendar permission prompt was not answered".into(),
                )),
            }
        }

        fn calendars(&self) -> Result<Vec<SysCalendar>> {
            autoreleasepool(|_| {
                let store = unsafe { EKEventStore::new() };
                let cals = unsafe { store.calendarsForEntityType(EKEntityType::Event) };
                Ok(cals
                    .iter()
                    .map(|c| unsafe {
                        SysCalendar {
                            id: c.calendarIdentifier().to_string(),
                            title: c.title().to_string(),
                            source: c
                                .source()
                                .map(|s| s.title().to_string())
                                .unwrap_or_default(),
                            kind: kind_of(c.r#type()),
                            read_only: !c.allowsContentModifications(),
                        }
                    })
                    .collect())
            })
        }

        fn events(
            &self,
            calendar_id: &str,
            from: DateTime<Utc>,
            to: DateTime<Utc>,
        ) -> Result<Vec<SysEvent>> {
            autoreleasepool(|_| {
                let store = unsafe { EKEventStore::new() };
                let Some(cal) =
                    (unsafe { store.calendarWithIdentifier(&NSString::from_str(calendar_id)) })
                else {
                    return Err(Error::Provider("calendar not found".into()));
                };
                let start = NSDate::dateWithTimeIntervalSince1970(from.timestamp() as f64);
                let end = NSDate::dateWithTimeIntervalSince1970(to.timestamp() as f64);
                let cals = NSArray::from_retained_slice(&[cal]);
                let predicate = unsafe {
                    store.predicateForEventsWithStartDate_endDate_calendars(
                        &start,
                        &end,
                        Some(&cals),
                    )
                };
                let events = unsafe { store.eventsMatchingPredicate(&predicate) };
                Ok(events.iter().filter_map(|e| map_event(&e)).collect())
            })
        }
    }
}

#[cfg(target_os = "macos")]
pub use mac::EventKitSource;

/// Platforms without a system-calendar integration.
#[cfg(not(target_os = "macos"))]
pub struct EventKitSource;

#[cfg(not(target_os = "macos"))]
impl SystemSource for EventKitSource {
    fn access(&self) -> Access {
        Access::Unsupported
    }
    fn request_access(&self) -> Result<Access> {
        Ok(Access::Unsupported)
    }
    fn calendars(&self) -> Result<Vec<SysCalendar>> {
        Err(Error::Provider(
            "system calendars are not available on this platform".into(),
        ))
    }
    fn events(&self, _: &str, _: DateTime<Utc>, _: DateTime<Utc>) -> Result<Vec<SysEvent>> {
        Err(Error::Provider(
            "system calendars are not available on this platform".into(),
        ))
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    /// Reads the real permission state. It never shows a prompt, so it is safe to run anywhere;
    /// it proves the EventKit bindings link and the OS answers.
    #[test]
    fn reads_the_real_authorization_state_without_prompting() {
        let a = EventKitSource.access();
        println!("calendar access on this machine: {a:?}");
        assert!(matches!(
            a,
            Access::Granted | Access::NotDetermined | Access::Denied | Access::Restricted
        ));
    }
}
