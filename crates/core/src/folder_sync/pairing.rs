//! Pairing a phone with a computer by scanning one QR code.
//!
//! The computer shows a short text (as a QR code, and as text to copy): the sync code, its device id and its addresses
//! on the home or office network. A phone that reads it gets the code (so its data is sealed the same way) and knows
//! where the computer is, so it can greet it directly. That matters because a phone often can't use the network-wide
//! announcements computers use: iOS only lets an app send or hear those with a special permission from Apple, and some
//! Android phones filter them. A direct greeting needs only the ordinary local-network permission.
//!
//! The text holds the sync code, so it is as private as the code: it is shown only on the computer's own screen.

use super::{code_of, parse_code, valid_device};
use crate::{Error, Result};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

const PREFIX: &str = "firstlight:pair?v=1";
/// At most this many addresses are carried (a computer with Wi-Fi and a cable has two).
pub const MAX_ADDRS: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pairing {
    pub key: [u8; 32],
    pub device: String,
    pub addrs: Vec<IpAddr>,
}

/// Only addresses on a home or office network: a pairing can never point a phone at the internet or at itself.
pub fn local_network_addr(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || is_shared_v4(v4),
        // Unique local addresses (fc00::/7), the IPv6 version of a home network's 192.168.x.x.
        IpAddr::V6(v6) => (v6.segments()[0] & 0xfe00) == 0xfc00 && !is_v4_mapped(v6),
    }
}

/// 100.64.0.0/10, used by some home routers and carriers behind a shared address.
fn is_shared_v4(ip: &Ipv4Addr) -> bool {
    let o = ip.octets();
    o[0] == 100 && (o[1] & 0xc0) == 64
}

fn is_v4_mapped(ip: &Ipv6Addr) -> bool {
    ip.to_ipv4_mapped().is_some()
}

impl Pairing {
    pub fn text(&self) -> String {
        let addrs: Vec<String> = self.addrs.iter().map(|a| a.to_string()).collect();
        format!(
            "{PREFIX}&code={}&device={}&at={}",
            code_of(&self.key),
            self.device,
            addrs.join(",")
        )
    }

    /// Reads a pairing text. Anything that isn't exactly one is refused with a plain message.
    pub fn parse(text: &str) -> Result<Self> {
        let bad = || Error::Invalid("That isn't a Firstlight pairing code.");
        let rest = text.trim().strip_prefix(PREFIX).ok_or_else(bad)?;
        let (mut code, mut device, mut at) = (None, None, None);
        for part in rest.split('&').filter(|p| !p.is_empty()) {
            let (k, v) = part.split_once('=').ok_or_else(bad)?;
            let slot = match k {
                "code" => &mut code,
                "device" => &mut device,
                "at" => &mut at,
                _ => return Err(bad()),
            };
            if slot.replace(v).is_some() {
                return Err(bad());
            }
        }
        let key = parse_code(code.ok_or_else(bad)?)?;
        let device = device.filter(|d| valid_device(d)).ok_or_else(bad)?;
        let mut addrs = Vec::new();
        for a in at.unwrap_or("").split(',').filter(|a| !a.is_empty()) {
            let ip: IpAddr = a.parse().map_err(|_| bad())?;
            if !local_network_addr(&ip) {
                return Err(Error::Invalid(
                    "That pairing code points outside your home or office network.",
                ));
            }
            if !addrs.contains(&ip) {
                addrs.push(ip);
            }
        }
        if addrs.len() > MAX_ADDRS {
            return Err(bad());
        }
        Ok(Self {
            key,
            device: device.to_string(),
            addrs,
        })
    }
}

/// The addresses of a peer as kept in settings: valid local-network ones, each once, at most [`MAX_ADDRS`].
pub fn clean_peers(addrs: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for a in addrs {
        if let Ok(ip) = a.trim().parse::<IpAddr>()
            && local_network_addr(&ip)
            && !out.contains(&ip.to_string())
        {
            out.push(ip.to_string());
        }
    }
    out.truncate(MAX_ADDRS);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::folder_sync::new_code;

    fn sample() -> Pairing {
        let (_, key) = new_code().unwrap();
        Pairing {
            key,
            device: "a1b2c3d4".into(),
            addrs: vec!["192.168.1.20".parse().unwrap(), "fd12::5".parse().unwrap()],
        }
    }

    #[test]
    fn a_pairing_reads_back_exactly() {
        let p = sample();
        assert_eq!(Pairing::parse(&p.text()).unwrap(), p);
        assert_eq!(Pairing::parse(&format!("  {}\n", p.text())).unwrap(), p);
        // No addresses is allowed (the phone then relies on the network announcements or a cloud place).
        let none = Pairing {
            addrs: vec![],
            ..sample()
        };
        assert!(Pairing::parse(&none.text()).unwrap().addrs.is_empty());
    }

    #[test]
    fn a_pairing_can_only_point_at_the_local_network() {
        let p = sample();
        for outside in [
            "8.8.8.8",
            "127.0.0.1",
            "0.0.0.0",
            "169.254.1.1",
            "2001:db8::1",
            "::1",
            "::ffff:192.168.1.2",
            "fe80::1",
        ] {
            let t = p.text().replace("192.168.1.20", outside);
            assert!(Pairing::parse(&t).is_err(), "{outside}");
        }
        for inside in ["10.0.0.136", "172.16.4.2", "100.64.1.1", "fc00::1"] {
            let t = p.text().replace("192.168.1.20", inside);
            assert!(Pairing::parse(&t).is_ok(), "{inside}");
        }
    }

    #[test]
    fn anything_else_is_refused() {
        let p = sample();
        let good = p.text();
        for bad in [
            String::new(),
            "FL1-AAAA".into(),
            good.replace("v=1", "v=2"),
            good.replace("device=a1b2c3d4", "device=../x"),
            good.replace("&code=", "&cod="),
            format!("{good}&code=FL1-AAAA"),
            format!("{good}&extra=1"),
            good.replace("192.168.1.20", "192.168.1.300"),
            format!("{good},10.0.0.1,10.0.0.2,10.0.0.3"),
            "https://example.com/?firstlight:pair?v=1".into(),
        ] {
            assert!(Pairing::parse(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn kept_peers_are_cleaned() {
        let kept = clean_peers(&[
            "192.168.1.20".into(),
            " 192.168.1.20 ".into(),
            "8.8.8.8".into(),
            "nonsense".into(),
            "10.0.0.1".into(),
            "10.0.0.2".into(),
            "10.0.0.3".into(),
            "10.0.0.4".into(),
        ]);
        assert_eq!(kept, ["192.168.1.20", "10.0.0.1", "10.0.0.2", "10.0.0.3"]);
    }
}
