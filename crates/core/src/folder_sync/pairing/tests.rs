use super::*;
use crate::folder_sync::{code_of, new_code};
use std::net::TcpListener;

fn offer() -> Offer {
    let (_, key) = new_code().unwrap();
    Offer {
        code: code_of(&key),
        device: "c0ffee01".into(),
        name: "Vijay's MacBook".into(),
    }
}

fn reply() -> Reply {
    Reply {
        device: "beef0002".into(),
        name: "iPhone".into(),
    }
}

/// Runs the computer's side on a loopback port, returns its result and the phone's.
fn attempt(host_pin: &str, phone_pin: &str) -> (Result<Reply>, Result<Offer>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let o = offer();
    let h = std::thread::spawn({
        let pin = host_pin.to_string();
        move || host(listener.accept().unwrap().0, &pin, &o)
    });
    let phone = join(
        TcpStream::connect(("127.0.0.1", port)).unwrap(),
        phone_pin,
        &reply(),
    );
    (h.join().unwrap(), phone)
}

#[test]
fn the_same_code_pairs_both_sides() {
    let (computer, phone) = attempt("482913", "482913");
    assert_eq!(computer.unwrap(), reply());
    let got = phone.unwrap();
    assert_eq!(got.device, "c0ffee01");
    assert_eq!(got.name, "Vijay's MacBook");
    assert!(crate::folder_sync::parse_code(&got.code).is_ok());
}

#[test]
fn a_wrong_code_pairs_neither_side_and_says_so() {
    let (computer, phone) = attempt("482913", "482914");
    assert!(computer.is_err());
    assert_eq!(phone.unwrap_err().to_string(), wrong().to_string());
}

#[test]
fn something_that_isnt_a_phone_gets_nothing() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let h = std::thread::spawn(move || host(listener.accept().unwrap().0, "111111", &offer()));
    let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
    c.write_all(b"GET / HTTP/1.1\r\n\r\n").unwrap();
    assert!(h.join().unwrap().is_err());
    let mut buf = [0u8; 16];
    let _ = c.set_read_timeout(Some(Duration::from_secs(2)));
    assert!(matches!(c.read(&mut buf), Ok(0) | Err(_)));
}

#[test]
fn codes_are_six_digits_and_typing_is_forgiving() {
    for _ in 0..50 {
        let p = new_pin().unwrap();
        assert_eq!(p.len(), 6);
        assert!(p.bytes().all(|b| b.is_ascii_digit()));
    }
    assert_eq!(clean_pin(" 482 913 ").as_deref(), Some("482913"));
    assert_eq!(clean_pin("482-913").as_deref(), Some("482913"));
    for bad in ["", "48291", "4829134", "48291a", "４８２９１３"] {
        assert_eq!(clean_pin(bad), None, "{bad}");
    }
}

#[test]
fn the_answer_to_who_is_only_a_port() {
    assert_eq!(parse_here(here_text(5123).as_bytes()), Some(5123));
    for bad in [
        "",
        "FLHERE1",
        "FLHERE1 0",
        "FLHERE1 70000",
        "FLHERE2 5000",
        "FLHERE15000",
    ] {
        assert_eq!(parse_here(bad.as_bytes()), None, "{bad}");
    }
}

#[test]
fn the_phone_only_looks_on_its_own_home_network() {
    let n = neighbours("192.168.1.20".parse().unwrap());
    assert_eq!(n.len(), 253);
    assert!(!n.contains(&"192.168.1.20".parse().unwrap()));
    assert!(n.iter().all(|a| a.octets()[..3] == [192, 168, 1]));
    assert!(neighbours("8.8.8.8".parse().unwrap()).is_empty());
    assert!(neighbours("127.0.0.1".parse().unwrap()).is_empty());
}

#[test]
fn only_local_network_addresses_are_kept() {
    let kept = clean_peers(&[
        "192.168.1.20".into(),
        " 192.168.1.20 ".into(),
        "8.8.8.8".into(),
        "nonsense".into(),
        "10.0.0.1".into(),
        "fd12::5".into(),
        "::ffff:10.0.0.9".into(),
        "100.64.1.1".into(),
        "172.16.0.2".into(),
    ]);
    assert_eq!(kept, ["192.168.1.20", "10.0.0.1", "fd12::5", "100.64.1.1"]);
}

#[test]
fn names_are_tidied() {
    assert_eq!(clean_name("  Riya's\u{0007} Pixel "), "Riya's Pixel");
    assert_eq!(clean_name(""), "Your device");
    assert_eq!(clean_name(&"x".repeat(200)).len(), 80);
}
