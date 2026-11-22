//! Sharing on the same network: the sockets and the background thread. What is sent and why it is safe is in
//! `firstlight_core::folder_sync::lan`.
//!
//! While `settings.tasks.lan_sync` is on and a sync code exists, one thread announces this device about every 30
//! seconds (UDP broadcast), listens for the others' announcements, answers their requests on a TCP port of its own,
//! and runs a sharing round when another device has something new (and once a minute, so changes made here are handed
//! out promptly). macOS asks once for Local Network access; Windows may ask to allow it on private networks.

use crate::AppState;
use crate::commands::{CmdResult, internal, lock_store};
use crate::folder_sync_cmd::{FolderSyncOn, computer_name, key_of};
use crate::pairing_cmd;
use crate::providers::Deps;
use chrono::Utc;
use firstlight_core::folder_sync::lan::{Beacon, LanFiles, PORT};
use firstlight_core::folder_sync::{self, KEY_SECRET, Report, pairing};
use firstlight_core::secrets::Redacted;
use firstlight_core::settings::AppSettings;
use firstlight_core::store::Store;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};

const ANNOUNCE_EVERY: Duration = Duration::from_secs(30);
const ROUND_EVERY: Duration = Duration::from_secs(60);

/// The last round on the network, for Settings ("seen on this network").
static LAST: Mutex<Option<(chrono::DateTime<Utc>, Report)>> = Mutex::new(None);

struct Running {
    key: [u8; 32],
    lan: Arc<LanFiles>,
    port: u16,
    stop: Arc<AtomicBool>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Answers requests from the other devices until `stop` is set.
fn listen(lan: Arc<LanFiles>, stop: Arc<AtomicBool>) -> std::io::Result<u16> {
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();
    std::thread::Builder::new()
        .name("lan-answer".into())
        .spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((conn, _)) => {
                        let lan = lan.clone();
                        let _ = conn.set_nonblocking(false);
                        std::thread::spawn(move || {
                            let _ = lan.serve(conn);
                        });
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(200)),
                }
            }
        })?;
    Ok(port)
}

fn round(store: &mut Store, lan: &LanFiles, key: &[u8; 32], app: &AppHandle) {
    match folder_sync::sync(store, lan, key, &computer_name(), Utc::now()) {
        Ok(r) => {
            if r.applied > 0 {
                log::info!("network sharing: {} changes", r.applied);
                let _ = app.emit("tasks-changed", ());
                let _ = app.emit("events-changed", ());
                let _ = app.emit("settings-changed", ());
            }
            if let Ok(mut last) = LAST.lock() {
                *last = Some((Utc::now(), r));
            }
        }
        Err(e) => log::warn!("network sharing failed: {e}"),
    }
}

pub(crate) fn spawn(app: AppHandle, db: PathBuf, db_key: Redacted, deps: Deps) {
    let _ = std::thread::Builder::new()
        .name("lan-sync".into())
        .spawn(move || {
            let mut store = match Store::open(&db, &db_key) {
                Ok(s) => s,
                Err(e) => {
                    log::error!("network sharing cannot open the database: {e}");
                    return;
                }
            };
            let udp = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, PORT))
                .and_then(|s| {
                    s.set_broadcast(true)?;
                    s.set_read_timeout(Some(Duration::from_secs(1)))?;
                    Ok(s)
                })
                .map_err(|e| log::warn!("network sharing can't listen for other devices: {e}"))
                .ok();
            let mut running: Option<Running> = None;
            let (mut announced, mut last_round) = (
                Instant::now() - ANNOUNCE_EVERY,
                Instant::now() - ROUND_EVERY,
            );
            let mut checked = Instant::now() - Duration::from_secs(10);
            // The paired computer's addresses (phones): greeted directly as well as by broadcast.
            let mut paired: Vec<IpAddr> = Vec::new();
            loop {
                // Settings and the code are looked at every few seconds, so turning it on or off applies at once.
                if checked.elapsed() >= Duration::from_secs(5) {
                    checked = Instant::now();
                    let tasks = store.load_settings().tasks;
                    let on = tasks.lan_sync;
                    paired = tasks
                        .lan_peers
                        .iter()
                        .filter_map(|a| a.parse().ok())
                        .collect();
                    let key = on.then(|| key_of(&deps)).flatten();
                    match key {
                        None => running = None,
                        Some(k) if running.as_ref().is_none_or(|r| r.key != k) => {
                            let me = match folder_sync::device_id(&store) {
                                Ok(m) => m,
                                Err(_) => {
                                    std::thread::sleep(Duration::from_secs(5));
                                    continue;
                                }
                            };
                            let lan = Arc::new(LanFiles::new(k, &me));
                            let stop = Arc::new(AtomicBool::new(false));
                            match listen(lan.clone(), stop.clone()) {
                                Ok(port) => {
                                    // Publish first, so a device that hears us can fetch right away.
                                    round(&mut store, &lan, &k, &app);
                                    last_round = Instant::now();
                                    announced = Instant::now() - ANNOUNCE_EVERY;
                                    running = Some(Running {
                                        key: k,
                                        lan,
                                        port,
                                        stop,
                                    });
                                }
                                Err(e) => {
                                    log::warn!("network sharing can't answer other devices: {e}")
                                }
                            }
                        }
                        Some(_) => {}
                    }
                }
                let Some(r) = running.as_ref() else {
                    std::thread::sleep(Duration::from_secs(2));
                    continue;
                };
                let mut due = last_round.elapsed() >= ROUND_EVERY;
                if announced.elapsed() >= ANNOUNCE_EVERY {
                    announced = Instant::now();
                    if let Some(s) = &udp {
                        let text = r.lan.beacon(r.port).text();
                        // Everyone on the network, then directly to the paired computer and to every device heard
                        // lately: an iPhone can't hear broadcasts without a permission from Apple, and some Android
                        // phones filter them, but a direct message always arrives.
                        let _ = s.send_to(
                            text.as_bytes(),
                            SocketAddr::from((Ipv4Addr::BROADCAST, PORT)),
                        );
                        let mut direct = paired.clone();
                        direct.extend(r.lan.peer_addrs());
                        direct.sort();
                        direct.dedup();
                        for ip in direct {
                            let _ = s.send_to(text.as_bytes(), SocketAddr::new(ip, PORT));
                        }
                    }
                    r.lan.prune(Instant::now());
                }
                match &udp {
                    Some(s) => {
                        let mut buf = [0u8; 256];
                        // A phone asking who is pairing: answered only while this computer shows a pairing code.
                        if let Ok((n, from)) = s.peek_from(&mut buf)
                            && &buf[..n] == pairing::WHO
                        {
                            let _ = s.recv_from(&mut buf);
                            if let Some(port) = pairing_cmd::here_port() {
                                let _ = s.send_to(pairing::here_text(port).as_bytes(), from);
                            }
                            continue;
                        }
                        if let Ok((n, from)) = s.recv_from(&mut buf)
                            && let Some(b) =
                                std::str::from_utf8(&buf[..n]).ok().and_then(Beacon::parse)
                            && r.lan.hear(&b, from.ip(), Instant::now())
                        {
                            due = true;
                            // Answer the newcomer directly at once (it may be a phone that never hears broadcasts).
                            let text = r.lan.beacon(r.port).text();
                            let _ = s.send_to(text.as_bytes(), SocketAddr::new(from.ip(), PORT));
                            // Answer at once, so a device that just appeared hears us without waiting 30 s.
                            announced = Instant::now() - ANNOUNCE_EVERY;
                        }
                    }
                    None => std::thread::sleep(Duration::from_secs(1)),
                }
                if due {
                    let (lan, key) = (r.lan.clone(), r.key);
                    round(&mut store, &lan, &key, &app);
                    last_round = Instant::now();
                }
            }
        });
}

/// Turns on sharing on the same network. `code` as for `enable_folder_sync`: empty on the first device (one is made,
/// or the one already used for a folder or cloud storage is kept).
#[tauri::command]
pub async fn enable_lan_sync(
    app: AppHandle,
    state: State<'_, AppState>,
    code: Option<String>,
) -> CmdResult<FolderSyncOn> {
    let typed = code.map(|c| c.trim().to_string()).filter(|c| !c.is_empty());
    let (code, _) = match typed {
        Some(c) => {
            let key = folder_sync::parse_code(&c).map_err(|e| e.to_string())?;
            (folder_sync::code_of(&key), key)
        }
        None => match key_of(&state.deps) {
            Some(k) => (folder_sync::code_of(&k), k),
            None => folder_sync::new_code().map_err(|e| e.to_string())?,
        },
    };
    state
        .deps
        .secrets
        .set(KEY_SECRET, &code)
        .map_err(internal("saving the sync code"))?;
    let settings = {
        let store = lock_store(&state)?;
        let mut s = store.load_settings();
        s.tasks.lan_sync = true;
        store
            .save_settings(&s)
            .map_err(internal("saving settings"))?;
        s
    };
    let _ = app.emit("settings-changed", ());
    Ok(FolderSyncOn::new(settings, code, Report::default()))
}

#[tauri::command]
pub async fn disable_lan_sync(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<AppSettings> {
    let store = lock_store(&state)?;
    let mut s = store.load_settings();
    s.tasks.lan_sync = false;
    store
        .save_settings(&s)
        .map_err(internal("saving settings"))?;
    drop(store);
    if let Ok(mut last) = LAST.lock() {
        *last = None;
    }
    let _ = app.emit("settings-changed", ());
    Ok(s)
}

/// The devices seen on this network in the last round (empty until one is found).
#[tauri::command]
pub async fn lan_sync_status() -> CmdResult<Report> {
    Ok(LAST
        .lock()
        .ok()
        .and_then(|l| l.as_ref().map(|(_, r)| r.clone()))
        .unwrap_or_default())
}
