//! Trip times from Apple Maps (Mac only, opt-in: `settings.day.travel_maps`). Sends the two addresses to Apple and
//! nothing else. Answers are kept for a few hours, so each trip is asked about rarely; the notifier asks, the page only
//! reads what is already known.

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use firstlight_core::briefing::BriefingSettings;
use firstlight_core::day::DaySettings;
use firstlight_core::model::Event;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A found trip time is kept this long; a failed lookup is not retried for half an hour.
const KEEP_FOUND: Duration = Duration::from_secs(3 * 3600);
const KEEP_FAILED: Duration = Duration::from_secs(30 * 60);

/// (from, to) to when it was asked and the answer.
type Answers = HashMap<(String, String), (Instant, Option<i64>)>;
static CACHE: Mutex<Option<Answers>> = Mutex::new(None);

pub(crate) const AVAILABLE: bool = cfg!(target_os = "macos");

fn cached(from: &str, to: &str) -> Option<Option<i64>> {
    let cache = CACHE.lock().ok()?;
    let (at, minutes) = cache.as_ref()?.get(&(from.to_string(), to.to_string()))?;
    let keep = if minutes.is_some() {
        KEEP_FOUND
    } else {
        KEEP_FAILED
    };
    (at.elapsed() < keep).then_some(*minutes)
}

/// Trip times for today's in-person meetings (meeting id to minutes, without the margin). With `look_up` false only
/// answers already known are used (the page must never wait on the network).
pub(crate) fn trip_times(
    events: &[Event],
    now: DateTime<Utc>,
    tz: Tz,
    ds: &DaySettings,
    bs: &BriefingSettings,
    look_up: bool,
) -> HashMap<String, i64> {
    if !AVAILABLE || !ds.travel_maps {
        return HashMap::new();
    }
    let mut out = HashMap::new();
    for (id, from, to) in firstlight_core::day::trips(events, now, tz, ds, bs) {
        let minutes = match cached(&from, &to) {
            Some(m) => m,
            None if look_up => {
                let m = eta_minutes(&from, &to);
                if let Ok(mut cache) = CACHE.lock() {
                    cache
                        .get_or_insert_with(HashMap::new)
                        .insert((from.clone(), to.clone()), (Instant::now(), m));
                }
                log::info!("trip time looked up: found {}", m.is_some());
                m
            }
            None => None,
        };
        if let Some(m) = minutes {
            out.insert(id, m);
        }
    }
    out
}

/// Driving time between two places (addresses or place names such as "Café Nero, Soho") from Apple Maps, in whole
/// minutes (rounded up), or `None` if either place could not be found or Apple did not answer within 15 seconds. Must
/// not run on the main thread: the answers arrive there.
#[cfg(target_os = "macos")]
pub(crate) fn eta_minutes(from: &str, to: &str) -> Option<i64> {
    use block2::{DynBlock, RcBlock};
    use objc2::AnyThread;
    use objc2::rc::Retained;
    use objc2_foundation::{NSError, NSString};
    use objc2_map_kit::{
        MKDirections, MKDirectionsRequest, MKDirectionsTransportType, MKETAResponse, MKLocalSearch,
        MKLocalSearchRequest, MKLocalSearchResponse, MKMapItem,
    };
    use std::cell::RefCell;
    use std::sync::mpsc;

    thread_local! {
        // Work in flight is kept alive here (on the main thread, where the answers arrive) until the last answer.
        static SEARCHES: RefCell<Vec<Retained<MKLocalSearch>>> = const { RefCell::new(Vec::new()) };
        static DIRECTIONS: RefCell<Vec<Retained<MKDirections>>> = const { RefCell::new(Vec::new()) };
    }
    type SearchBlock = DynBlock<dyn Fn(*mut MKLocalSearchResponse, *mut NSError)>;
    fn search(place: &str) -> Retained<MKLocalSearch> {
        unsafe {
            let request = MKLocalSearchRequest::new();
            request.setNaturalLanguageQuery(Some(&NSString::from_str(place)));
            MKLocalSearch::initWithRequest(MKLocalSearch::alloc(), &request)
        }
    }
    fn first_item(response: *mut MKLocalSearchResponse) -> Option<Retained<MKMapItem>> {
        unsafe { response.as_ref() }.and_then(|r| unsafe { r.mapItems() }.firstObject())
    }

    let (tx, rx) = mpsc::channel::<Option<f64>>();
    let first = search(from);
    let destination = to.to_string();
    let tx_first = tx.clone();
    let after_first = RcBlock::new(
        move |found: *mut MKLocalSearchResponse, _error: *mut NSError| {
            let Some(origin) = first_item(found) else {
                let _ = tx_first.send(None);
                return;
            };
            let tx_second = tx_first.clone();
            let after_second = RcBlock::new(
                move |found: *mut MKLocalSearchResponse, _error: *mut NSError| {
                    let Some(target) = first_item(found) else {
                        let _ = tx_second.send(None);
                        SEARCHES.with(|s| s.borrow_mut().clear());
                        return;
                    };
                    unsafe {
                        let request = MKDirectionsRequest::new();
                        request.setSource(Some(&origin));
                        request.setDestination(Some(&target));
                        request.setTransportType(MKDirectionsTransportType::Automobile);
                        let directions =
                            MKDirections::initWithRequest(MKDirections::alloc(), &request);
                        let tx_eta = tx_second.clone();
                        let answered = RcBlock::new(
                            move |response: *mut MKETAResponse, _error: *mut NSError| {
                                let _ =
                                    tx_eta.send(response.as_ref().map(|r| r.expectedTravelTime()));
                                DIRECTIONS.with(|d| d.borrow_mut().clear());
                                SEARCHES.with(|s| s.borrow_mut().clear());
                            },
                        );
                        directions.calculateETAWithCompletionHandler(RcBlock::as_ptr(&answered)
                            as *mut DynBlock<dyn Fn(*mut MKETAResponse, *mut NSError)>);
                        DIRECTIONS.with(|d| d.borrow_mut().push(directions));
                    }
                },
            );
            let second = search(&destination);
            unsafe {
                second
                    .startWithCompletionHandler(RcBlock::as_ptr(&after_second) as *mut SearchBlock)
            };
            SEARCHES.with(|s| s.borrow_mut().push(second));
        },
    );
    unsafe { first.startWithCompletionHandler(RcBlock::as_ptr(&after_first) as *mut SearchBlock) };
    let seconds = rx.recv_timeout(Duration::from_secs(15)).ok().flatten();
    drop(first);
    seconds
        .filter(|s| s.is_finite() && *s > 0.0)
        .map(|s| ((s / 60.0).ceil() as i64).clamp(1, 600))
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn eta_minutes(_from: &str, _to: &str) -> Option<i64> {
    None
}
