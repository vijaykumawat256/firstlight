//! Sharing directly between your devices on the same network (home Wi-Fi, an office network), with no folder and no
//! cloud: the same encrypted snapshots, the same merge rules, carried over the local network instead.
//!
//! - **Finding each other:** every device that has sharing on the network sends a small announcement (UDP broadcast on
//!   [`PORT`]) about every 30 seconds: a group id derived from the sync code (so devices with another code ignore it,
//!   and the code itself can't be worked out from it), its device id, the port it answers on, and a stamp that changes
//!   when its data does. Nothing about the person's data is in it.
//! - **Exchanging:** a device that hears a new stamp connects to the announcer (at the address the announcement came
//!   from) and asks for its snapshot with a fresh random number; the answer is the snapshot file, sealed with the sync
//!   code together with that number. So nobody without the code can read it or forge it, and an old answer recorded
//!   on the network can't be played back later.
//!
//! [`LanFiles`] is a [`SyncFiles`]: the devices heard recently are its files, so `folder_sync::sync` does the rest.
//! The sockets and threads are in the shell (`src-tauri/src/lan_sync.rs`); everything here is tested over loopback.

use super::{MAX_FILE, RemoteFile, SyncFiles, open, seal, valid_device};
use crate::foldersync::{Snapshot, fingerprint};
use crate::{Error, Result};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The UDP port announcements are sent to and heard on.
pub const PORT: u16 = 47_913;
const MAGIC: &str = "FLLAN1";
const REQUEST: &[u8; 6] = b"FLREQ1";
const NONCE: usize = 16;
/// A device not heard from for this long is no longer listed.
pub const FORGET_AFTER: Duration = Duration::from_secs(150);
const IO_LIMIT: Duration = Duration::from_secs(10);

/// The group id for a sync code: the first 8 bytes of SHA-256 over a fixed label and the key, as hex.
pub fn group_of(key: &[u8; 32]) -> String {
    let mut h = Sha256::new();
    h.update(b"firstlight-lan-v1");
    h.update(key);
    h.finalize()[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Beacon {
    pub group: String,
    pub device: String,
    pub port: u16,
    pub stamp: String,
}

impl Beacon {
    pub fn text(&self) -> String {
        format!(
            "{MAGIC} {} {} {} {}",
            self.group, self.device, self.port, self.stamp
        )
    }

    /// Reads an announcement; anything that isn't exactly one of ours is `None`.
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.trim().split(' ');
        if parts.next()? != MAGIC {
            return None;
        }
        let (group, device, port, stamp) =
            (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
        let hex = |s: &str, max: usize| {
            !s.is_empty() && s.len() <= max && s.bytes().all(|b| b.is_ascii_hexdigit())
        };
        if parts.next().is_some()
            || group.len() != 16
            || !hex(group, 16)
            || !valid_device(device)
            || !hex(stamp, 32)
        {
            return None;
        }
        let port: u16 = port.parse().ok().filter(|p| *p > 0)?;
        Some(Self {
            group: group.into(),
            device: device.into(),
            port,
            stamp: stamp.into(),
        })
    }
}

struct Peer {
    addr: IpAddr,
    port: u16,
    stamp: String,
    heard: Instant,
}

#[derive(Default)]
struct Served {
    /// This device's snapshot file (sealed), as the next request gets it.
    data: Vec<u8>,
    /// Changes when `data`'s records change (not when only the time it was written does).
    stamp: String,
    records_print: String,
}

/// The devices on this network that share the same sync code, and this device's own snapshot to hand out.
pub struct LanFiles {
    key: [u8; 32],
    group: String,
    me: String,
    peers: Mutex<HashMap<String, Peer>>,
    served: Mutex<Served>,
}

impl LanFiles {
    pub fn new(key: [u8; 32], me: &str) -> Self {
        Self {
            group: group_of(&key),
            key,
            me: me.into(),
            peers: Mutex::default(),
            served: Mutex::default(),
        }
    }

    /// This device's announcement, for the port it answers on.
    pub fn beacon(&self, port: u16) -> Beacon {
        let stamp = self
            .served
            .lock()
            .map(|s| s.stamp.clone())
            .unwrap_or_default();
        Beacon {
            group: self.group.clone(),
            device: self.me.clone(),
            port,
            stamp: if stamp.is_empty() { "0".into() } else { stamp },
        }
    }

    /// Takes in an announcement heard from `from`. True when it is a device of ours that is new or has new data,
    /// so a round is worth running now.
    pub fn hear(&self, b: &Beacon, from: IpAddr, now: Instant) -> bool {
        if b.group != self.group || b.device == self.me {
            return false;
        }
        let Ok(mut peers) = self.peers.lock() else {
            return false;
        };
        let changed = peers.get(&b.device).is_none_or(|p| p.stamp != b.stamp);
        peers.insert(
            b.device.clone(),
            Peer {
                addr: from,
                port: b.port,
                stamp: b.stamp.clone(),
                heard: now,
            },
        );
        changed
    }

    /// Forgets devices not heard from recently. Returns how many are left.
    pub fn prune(&self, now: Instant) -> usize {
        let Ok(mut peers) = self.peers.lock() else {
            return 0;
        };
        peers.retain(|_, p| now.duration_since(p.heard) < FORGET_AFTER);
        peers.len()
    }

    /// Answers one connection from another device: a request with a fresh number gets this device's snapshot,
    /// sealed together with that number. Anything else is dropped.
    pub fn serve(&self, mut stream: TcpStream) -> Result<()> {
        let io = |_| Error::Provider("network".into());
        stream.set_read_timeout(Some(IO_LIMIT)).map_err(io)?;
        stream.set_write_timeout(Some(IO_LIMIT)).map_err(io)?;
        let mut req = [0u8; REQUEST.len() + NONCE];
        stream.read_exact(&mut req).map_err(io)?;
        if &req[..REQUEST.len()] != REQUEST {
            return Err(Error::Invalid("not a request"));
        }
        let data = self
            .served
            .lock()
            .map(|s| s.data.clone())
            .unwrap_or_default();
        if data.is_empty() {
            return Err(Error::Invalid("nothing to share yet"));
        }
        let mut plain = req[REQUEST.len()..].to_vec();
        plain.extend_from_slice(&data);
        let answer = seal(&self.key, &plain)?;
        stream
            .write_all(&(answer.len() as u32).to_be_bytes())
            .and_then(|_| stream.write_all(&answer))
            .map_err(io)
    }

    fn peer(&self, device: &str) -> Option<SocketAddr> {
        let peers = self.peers.lock().ok()?;
        peers.get(device).map(|p| SocketAddr::new(p.addr, p.port))
    }
}

impl SyncFiles for LanFiles {
    fn kind(&self) -> &'static str {
        "lan"
    }

    fn list(&self) -> Result<Vec<RemoteFile>> {
        let Ok(peers) = self.peers.lock() else {
            return Ok(vec![]);
        };
        let now = Instant::now();
        Ok(peers
            .iter()
            .filter(|(_, p)| now.duration_since(p.heard) < FORGET_AFTER)
            .map(|(d, p)| RemoteFile {
                device: d.clone(),
                size: 0,
                stamp: p.stamp.clone(),
            })
            .collect())
    }

    fn read(&self, device: &str) -> Result<Vec<u8>> {
        let addr = self
            .peer(device)
            .ok_or(Error::Invalid("that device isn't here"))?;
        let io = |_| Error::Offline;
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).map_err(io)?;
        stream.set_read_timeout(Some(IO_LIMIT)).map_err(io)?;
        stream.set_write_timeout(Some(IO_LIMIT)).map_err(io)?;
        let mut nonce = [0u8; NONCE];
        getrandom::fill(&mut nonce)
            .map_err(|_| Error::Invalid("Couldn't make a random number."))?;
        let mut req = REQUEST.to_vec();
        req.extend_from_slice(&nonce);
        stream.write_all(&req).map_err(io)?;
        let mut len = [0u8; 4];
        stream.read_exact(&mut len).map_err(io)?;
        let len = u32::from_be_bytes(len) as u64;
        if len > MAX_FILE + 1024 {
            return Err(Error::Invalid("too big"));
        }
        let mut answer = vec![0u8; len as usize];
        stream.read_exact(&mut answer).map_err(io)?;
        let plain = open(&self.key, &answer)?;
        if plain.len() < NONCE || plain[..NONCE] != nonce {
            return Err(Error::Invalid("an answer to another request"));
        }
        Ok(plain[NONCE..].to_vec())
    }

    fn write(&self, device: &str, data: &[u8]) -> Result<()> {
        if device != self.me {
            return Err(Error::Invalid(
                "only this device's own snapshot is handed out",
            ));
        }
        // The stamp follows the records, so re-sealing the same data (a new nonce, a new time) doesn't make every
        // other device download it again.
        let print = open(&self.key, data)
            .ok()
            .and_then(|p| serde_json::from_slice::<Snapshot>(&p).ok())
            .and_then(|s| serde_json::to_string(&s.records).ok())
            .map(|r| fingerprint(&r))
            .unwrap_or_default();
        let mut served = self.served.lock().map_err(|_| Error::Invalid("busy"))?;
        if served.records_print != print || served.data.is_empty() {
            let digest = Sha256::digest(data);
            served.stamp = digest[..8].iter().map(|b| format!("{b:02x}")).collect();
            served.data = data.to_vec();
            served.records_print = print;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
