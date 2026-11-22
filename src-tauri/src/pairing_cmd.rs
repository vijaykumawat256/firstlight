//! Pairing a phone with a computer by a 6-digit code (the protocol is in `firstlight_core::folder_sync::pairing`).
//!
//! Computer: `start_pairing` shows a code and listens; `pairing_status` tells the page when a phone paired (or the code
//! ran out); `stop_pairing` cancels. Phone: `pair_with_code` finds the computer on the home network and pairs.
//! Afterwards each side remembers the other's address (`settings.tasks.lan_peers`) and sharing on the network is on,
//! so they greet each other directly from then on.

use crate::AppState;
use crate::commands::{CmdResult, internal, lock_store};
use crate::folder_sync_cmd::computer_name;
use crate::lan_sync::enable_lan_sync;
use firstlight_core::folder_sync::lan::PORT;
use firstlight_core::folder_sync::{self, KEY_SECRET, pairing};
use firstlight_core::settings::AppSettings;
use firstlight_core::store::Store;
use serde::Serialize;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};

/// What the computer's page shows while pairing.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PairState {
    Off,
    Waiting { pin: String, seconds_left: u64 },
    Paired { name: String },
    Expired,
    TooManyTries,
}

struct Session {
    pin: String,
    expires: Instant,
    port: u16,
    stop: Arc<AtomicBool>,
    outcome: Option<PairState>,
}

static SESSION: Mutex<Option<Session>> = Mutex::new(None);

fn status_now() -> PairState {
    let Ok(guard) = SESSION.lock() else {
        return PairState::Off;
    };
    match guard.as_ref() {
        None => PairState::Off,
        Some(s) => match &s.outcome {
            Some(o) => o.clone(),
            None if Instant::now() >= s.expires => PairState::Expired,
            None => PairState::Waiting {
                pin: s.pin.clone(),
                seconds_left: s
                    .expires
                    .saturating_duration_since(Instant::now())
                    .as_secs(),
            },
        },
    }
}

/// The port a phone should connect to, while this computer is showing a code that still works.
pub(crate) fn here_port() -> Option<u16> {
    matches!(status_now(), PairState::Waiting { .. })
        .then(|| SESSION.lock().ok()?.as_ref().map(|s| s.port))
        .flatten()
}

fn finish(outcome: PairState) {
    if let Ok(mut guard) = SESSION.lock()
        && let Some(s) = guard.as_mut()
    {
        s.stop.store(true, Ordering::Relaxed);
        s.outcome = Some(outcome);
    }
}

/// Adds an address to the other device's remembered ones (newest first).
fn remember(store: &Store, ip: IpAddr) -> Result<AppSettings, String> {
    let mut s = store.load_settings();
    let mut peers = vec![ip.to_string()];
    peers.extend(s.tasks.lan_peers.iter().cloned());
    s.tasks.lan_peers = pairing::clean_peers(&peers);
    s.tasks.lan_sync = true;
    store
        .save_settings(&s)
        .map_err(|_| "Couldn't save the pairing.".to_string())?;
    Ok(s)
}

/// Shows a new 6-digit code on this computer and waits for a phone. Turns on sharing on the network (with the sync code
/// already used here, or a new one), because that is how the phone will reach it.
#[tauri::command]
pub async fn start_pairing(app: AppHandle, state: State<'_, AppState>) -> CmdResult<PairState> {
    let on = enable_lan_sync(app.clone(), state.clone(), None).await?;
    let device = {
        let store = lock_store(&state)?;
        folder_sync::device_id(&store).map_err(internal("reading this device's id"))?
    };
    let offer = pairing::Offer {
        code: on.code.clone(),
        device,
        name: computer_name(),
    };
    let pin = pairing::new_pin().map_err(|e| e.to_string())?;
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0))
        .and_then(|l| l.set_nonblocking(true).map(|_| l))
        .map_err(|_| "Couldn't start listening for your phone.".to_string())?;
    let port = listener
        .local_addr()
        .map_err(|_| "Couldn't start listening for your phone.".to_string())?
        .port();
    let stop = Arc::new(AtomicBool::new(false));
    // A code shown before is replaced.
    if let Ok(mut guard) = SESSION.lock() {
        if let Some(old) = guard.take() {
            old.stop.store(true, Ordering::Relaxed);
        }
        *guard = Some(Session {
            pin: pin.clone(),
            expires: Instant::now() + pairing::PIN_LIFETIME,
            port,
            stop: stop.clone(),
            outcome: None,
        });
    }
    let (db, key) = (state.db_path.clone(), state.key.clone());
    let expires = Instant::now() + pairing::PIN_LIFETIME;
    std::thread::Builder::new()
        .name("pairing".into())
        .spawn(move || {
            let mut tries = 0;
            while !stop.load(Ordering::Relaxed) && Instant::now() < expires {
                let Ok((conn, from)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(200));
                    continue;
                };
                let _ = conn.set_nonblocking(false);
                match pairing::host(conn, &pin, &offer) {
                    Ok(reply) => {
                        log::info!("paired with a phone");
                        if let Ok(store) = Store::open(&db, &key) {
                            let _ = remember(&store, from.ip());
                        }
                        finish(PairState::Paired { name: reply.name });
                        let _ = app.emit("settings-changed", ());
                        let _ = app.emit("pairing-changed", ());
                        return;
                    }
                    Err(_) => {
                        tries += 1;
                        log::warn!("pairing: a wrong code ({tries} of {})", pairing::MAX_TRIES);
                        if tries >= pairing::MAX_TRIES {
                            finish(PairState::TooManyTries);
                            let _ = app.emit("pairing-changed", ());
                            return;
                        }
                    }
                }
            }
        })
        .map_err(|_| "Couldn't start listening for your phone.".to_string())?;
    Ok(status_now())
}

#[tauri::command]
pub async fn pairing_status() -> CmdResult<PairState> {
    Ok(status_now())
}

#[tauri::command]
pub async fn stop_pairing() -> CmdResult<()> {
    if let Ok(mut guard) = SESSION.lock()
        && let Some(s) = guard.take()
    {
        s.stop.store(true, Ordering::Relaxed);
    }
    Ok(())
}

/// This device's IPv4 address on the home network: the one its default route leaves from. Connecting a UDP socket
/// sends nothing; it only makes the system choose the interface.
fn own_ipv4() -> Option<Ipv4Addr> {
    let s = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    s.connect(SocketAddr::from(([192, 0, 2, 1], 9))).ok()?;
    match s.local_addr().ok()?.ip() {
        IpAddr::V4(v4) if pairing::local_network_addr(&IpAddr::V4(v4)) => Some(v4),
        _ => None,
    }
}

const NOT_FOUND: &str = "Couldn't find your computer. Check that it shows a pairing code, that both are on the same Wi-Fi, and that Firstlight may use the local network (on iPhone: Settings, Privacy & Security, Local Network).";

/// Looks for a computer showing a code, then pairs with it. Returns the computer's address and what it handed over.
fn find_and_pair(
    pin: &str,
    known: &[IpAddr],
    me: pairing::Reply,
) -> Result<(IpAddr, pairing::Offer), String> {
    let mut targets: Vec<IpAddr> = known.to_vec();
    if let Some(own) = own_ipv4() {
        targets.extend(pairing::neighbours(own).into_iter().map(IpAddr::V4));
    }
    if targets.is_empty() {
        return Err("This phone isn't on a Wi-Fi network. Join the same Wi-Fi as your computer and try again.".into());
    }
    let udp = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).map_err(|_| NOT_FOUND.to_string())?;
    let _ = udp.set_read_timeout(Some(Duration::from_millis(250)));
    // Three rounds: the first messages can be lost while iOS asks for the local-network permission.
    for _ in 0..3 {
        for ip in &targets {
            let _ = udp.send_to(pairing::WHO, SocketAddr::new(*ip, PORT));
        }
        let until = Instant::now() + Duration::from_millis(1500);
        let mut buf = [0u8; 64];
        while Instant::now() < until {
            let Ok((n, from)) = udp.recv_from(&mut buf) else {
                continue;
            };
            let Some(port) = pairing::parse_here(&buf[..n]) else {
                continue;
            };
            if !pairing::local_network_addr(&from.ip()) {
                continue;
            }
            let addr = SocketAddr::new(from.ip(), port);
            let Ok(stream) = TcpStream::connect_timeout(&addr, Duration::from_secs(4)) else {
                continue;
            };
            return pairing::join(stream, pin, &me)
                .map(|offer| (from.ip(), offer))
                .map_err(|e| e.to_string());
        }
    }
    Err(NOT_FOUND.into())
}

/// What the phone's page gets after pairing.
#[derive(Serialize)]
pub struct Paired {
    pub settings: AppSettings,
    /// The computer's name, for "Paired with …".
    pub name: String,
}

/// Pairs this device (a phone) with a computer showing `code`. Takes the computer's sync code (replacing another one
/// used here before), remembers where it is, and turns on sharing on the network.
#[tauri::command]
pub async fn pair_with_code(
    app: AppHandle,
    state: State<'_, AppState>,
    code: String,
) -> CmdResult<Paired> {
    let pin = pairing::clean_pin(&code).ok_or("Type the 6 digits your computer shows.")?;
    let (known, device) = {
        let store = lock_store(&state)?;
        let known: Vec<IpAddr> = store
            .load_settings()
            .tasks
            .lan_peers
            .iter()
            .filter_map(|a| a.parse().ok())
            .collect();
        let device =
            folder_sync::device_id(&store).map_err(internal("reading this device's id"))?;
        (known, device)
    };
    let me = pairing::Reply {
        device,
        name: computer_name(),
    };
    let (ip, offer) = tauri::async_runtime::spawn_blocking(move || find_and_pair(&pin, &known, me))
        .await
        .map_err(|_| NOT_FOUND.to_string())??;
    state
        .deps
        .secrets
        .set(KEY_SECRET, &offer.code)
        .map_err(internal("saving the sync code"))?;
    let settings = {
        let store = lock_store(&state)?;
        let mut s = store.load_settings();
        s.tasks.lan_peers = vec![];
        store
            .save_settings(&s)
            .map_err(internal("saving settings"))?;
        remember(&store, ip)?
    };
    let _ = app.emit("settings-changed", ());
    Ok(Paired {
        settings,
        name: offer.name,
    })
}
