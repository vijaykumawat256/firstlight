//! The macOS half of the system-calendar provider: reads the Calendar app's data through EventKit.
//!
//! Calendar access is a per-app permission the user grants once (System Settings, then Privacy and Security,
//! then Calendars). Firstlight only ever reads. A fresh `EKEventStore` is created per call, so no
//! non-thread-safe Objective-C object is shared between threads.

use chrono::{DateTime, Utc};
use firstlight_core::providers::system::{Access, SysCalendar, SysEvent, SystemSource};
use firstlight_core::{Error, Result};

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use block2::{DynBlock, RcBlock};
    use firstlight_core::model::{Attendee, Response, add_guest};
    use firstlight_core::providers::system::{SysCalendarKind, SysResponse, SysStatus};
    use firstlight_core::providers::system::{SysFreq, SysNewEvent, SysWhen};
    use objc2::AnyThread;
    use objc2::rc::autoreleasepool;
    use objc2::runtime::{Bool, NSObjectProtocol};
    use objc2_event_kit::{
        EKAlarm, EKAuthorizationStatus, EKCalendarType, EKEntityType, EKEvent, EKEventStatus,
        EKEventStore, EKParticipantStatus, EKParticipantType, EKRecurrenceEnd,
        EKRecurrenceFrequency, EKRecurrenceRule, EKSpan,
    };
    use objc2_foundation::{NSArray, NSDate, NSError, NSString, NSTimeZone, NSURL};
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

    /// The address of a participant: EventKit gives it as a `mailto:` link.
    fn email_of(p: &objc2_event_kit::EKParticipant) -> Option<String> {
        unsafe { p.URL().absoluteString().map(|s| s.to_string()) }
    }

    /// The guest list: the organiser, then each person (rooms and equipment are left out).
    fn guests_of(ev: &EKEvent) -> Vec<Attendee> {
        let mut out = Vec::new();
        unsafe {
            if let Some(o) = ev.organizer()
                && let Some(email) = email_of(&o)
            {
                add_guest(
                    &mut out,
                    &email,
                    o.name().map(|n| n.to_string()).as_deref(),
                    Response::None,
                    true,
                );
            }
            let Some(list) = ev.attendees() else {
                return out;
            };
            for a in list.iter() {
                let kind = a.participantType();
                if kind == EKParticipantType::Room || kind == EKParticipantType::Resource {
                    continue;
                }
                let s = a.participantStatus();
                let response = if s == EKParticipantStatus::Pending {
                    Response::NeedsAction
                } else if s == EKParticipantStatus::Accepted {
                    Response::Accepted
                } else if s == EKParticipantStatus::Declined {
                    Response::Declined
                } else if s == EKParticipantStatus::Tentative {
                    Response::Tentative
                } else {
                    Response::None
                };
                if let Some(email) = email_of(&a) {
                    add_guest(
                        &mut out,
                        &email,
                        a.name().map(|n| n.to_string()).as_deref(),
                        response,
                        false,
                    );
                }
            }
        }
        out
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
                attendees: guests_of(ev),
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

        /// Creates or replaces an event Firstlight added. Needs the same full access that reading needs, and
        /// the calendar must accept changes. A repeating event is replaced as a whole (this and all later days,
        /// from its first day).
        fn save_event(
            &self,
            calendar_id: &str,
            existing: Option<&str>,
            e: &SysNewEvent,
            tz: chrono_tz::Tz,
        ) -> Result<String> {
            if status() != Access::Granted {
                return Err(Error::PermissionNeeded);
            }
            autoreleasepool(|_| {
                let store = unsafe { EKEventStore::new() };
                let Some(cal) =
                    (unsafe { store.calendarWithIdentifier(&NSString::from_str(calendar_id)) })
                else {
                    return Err(Error::Provider("calendar not found".into()));
                };
                if !unsafe { cal.allowsContentModifications() } {
                    return Err(Error::Invalid("That calendar can't be added to."));
                }
                let ev = match existing {
                    Some(id) => unsafe { store.eventWithIdentifier(&NSString::from_str(id)) }
                        .ok_or_else(|| {
                            Error::Provider("the event is no longer in the Calendar app".into())
                        })?,
                    None => unsafe { EKEvent::eventWithEventStore(&store) },
                };
                let ns = |s: &str| NSString::from_str(s);
                let date =
                    |d: DateTime<Utc>| NSDate::dateWithTimeIntervalSince1970(d.timestamp() as f64);
                unsafe {
                    ev.setCalendar(Some(&cal));
                    ev.setTitle(Some(&ns(&e.title)));
                    match &e.when {
                        SysWhen::Timed { start, end } => {
                            ev.setAllDay(false);
                            ev.setTimeZone(NSTimeZone::timeZoneWithName(&ns(tz.name())).as_deref());
                            ev.setStartDate(Some(&date(*start)));
                            ev.setEndDate(Some(&date(*end)));
                        }
                        SysWhen::AllDay { first, last } => {
                            ev.setAllDay(true);
                            ev.setTimeZone(None);
                            let at = |d: chrono::NaiveDate, h, m, s| {
                                firstlight_core::tz::local_to_utc(
                                    tz,
                                    d.and_hms_opt(h, m, s).unwrap_or_default(),
                                )
                            };
                            ev.setStartDate(Some(&date(at(*first, 0, 0, 0))));
                            ev.setEndDate(Some(&date(at(*last, 23, 59, 59))));
                        }
                    }
                    ev.setLocation(e.location.as_deref().map(ns).as_deref());
                    ev.setNotes(e.notes.as_deref().map(ns).as_deref());
                    ev.setURL(
                        e.url
                            .as_deref()
                            .and_then(|u| NSURL::URLWithString(&ns(u)))
                            .as_deref(),
                    );
                    // Repeat: replaced as a whole, so an edit can change or remove it.
                    ev.setRecurrenceRules(None);
                    if let Some(r) = &e.repeat {
                        let freq = match r.freq {
                            SysFreq::Daily => EKRecurrenceFrequency::Daily,
                            SysFreq::Weekly => EKRecurrenceFrequency::Weekly,
                            SysFreq::Monthly => EKRecurrenceFrequency::Monthly,
                            SysFreq::Yearly => EKRecurrenceFrequency::Yearly,
                        };
                        let end = r.until.map(|d| {
                            let last_moment = firstlight_core::tz::local_to_utc(
                                tz,
                                d.and_hms_opt(23, 59, 59).unwrap_or_default(),
                            );
                            EKRecurrenceEnd::recurrenceEndWithEndDate(&date(last_moment))
                        });
                        let rule = EKRecurrenceRule::initRecurrenceWithFrequency_interval_end(
                            EKRecurrenceRule::alloc(),
                            freq,
                            r.interval as isize,
                            end.as_deref(),
                        );
                        ev.setRecurrenceRules(Some(&NSArray::from_retained_slice(&[rule])));
                    }
                    ev.setAlarms(None);
                    if let Some(m) = e.alarm_minutes {
                        let alarm = EKAlarm::alarmWithRelativeOffset(-(f64::from(m)) * 60.0);
                        ev.setAlarms(Some(&NSArray::from_retained_slice(&[alarm])));
                    }
                }
                let span = if existing.is_some() {
                    EKSpan::FutureEvents
                } else {
                    EKSpan::ThisEvent
                };
                unsafe { store.saveEvent_span_error(&ev, span) }.map_err(|_| {
                    Error::Provider("the Calendar app could not save the event".into())
                })?;
                unsafe { ev.eventIdentifier() }
                    .map(|s| s.to_string())
                    .ok_or_else(|| {
                        Error::Provider("the Calendar app did not name the event".into())
                    })
            })
        }

        /// Removes an event Firstlight added, with every repeat. Already gone counts as done.
        fn delete_event(&self, event_id: &str) -> Result<()> {
            if status() != Access::Granted {
                return Err(Error::PermissionNeeded);
            }
            autoreleasepool(|_| {
                let store = unsafe { EKEventStore::new() };
                let Some(ev) =
                    (unsafe { store.eventWithIdentifier(&NSString::from_str(event_id)) })
                else {
                    return Ok(());
                };
                unsafe { store.removeEvent_span_error(&ev, EKSpan::FutureEvents) }.map_err(|_| {
                    Error::Provider("the Calendar app could not remove the event".into())
                })
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
