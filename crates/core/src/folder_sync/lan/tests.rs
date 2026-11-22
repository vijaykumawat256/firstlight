use super::*;
use crate::folder_sync::{new_code, sync};
use crate::secrets::{MemorySecretStore, load_or_create_db_key};
use crate::store::Store;
use crate::tasks::TaskInput;
use std::net::{Ipv4Addr, TcpListener};
use std::sync::Arc;

const NY: chrono_tz::Tz = chrono_tz::America::New_York;

fn store() -> Store {
    let key = load_or_create_db_key(&MemorySecretStore::default()).unwrap();
    Store::open_in_memory(&key).unwrap()
}

fn task(title: &str) -> TaskInput {
    TaskInput {
        title: title.into(),
        ..Default::default()
    }
}

/// Serves `lan` on a loopback port (every connection answered in turn) and returns the port.
fn serve(lan: Arc<LanFiles>) -> u16 {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let _ = lan.serve(conn);
        }
    });
    port
}

const HERE: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

#[test]
fn announcements_carry_nothing_private_and_strangers_are_refused() {
    let (_, key) = new_code().unwrap();
    let lan = LanFiles::new(key, "aa11");
    let b = lan.beacon(5000);
    assert_eq!(Beacon::parse(&b.text()), Some(b.clone()));
    assert_eq!(b.group.len(), 16);
    assert!(!b.text().contains(&crate::folder_sync::code_of(&key)));
    for bad in [
        "",
        "FLLAN1",
        "FLLAN2 0123456789abcdef aa11 5000 0",
        "FLLAN1 0123456789abcdef aa11 0 0",
        "FLLAN1 0123456789abcdef ../x 5000 0",
        "FLLAN1 0123456789abcdef aa11 5000 0 extra",
        "FLLAN1 0123456789abcdeg aa11 5000 0",
    ] {
        assert_eq!(Beacon::parse(bad), None, "{bad}");
    }
    // Another sync code is another group: ignored. Our own announcement is ignored too.
    let (_, other) = new_code().unwrap();
    let stranger = LanFiles::new(other, "bb22").beacon(5001);
    let now = Instant::now();
    assert!(!lan.hear(&stranger, HERE, now));
    assert!(!lan.hear(&lan.beacon(5000), HERE, now));
    assert!(lan.list().unwrap().is_empty());
    // A device of ours is heard once per new stamp, and forgotten when it goes quiet.
    let friend = LanFiles::new(key, "cc33").beacon(5002);
    assert!(lan.hear(&friend, HERE, now));
    assert!(!lan.hear(&friend, HERE, now), "same stamp: nothing new");
    assert_eq!(lan.prune(now + FORGET_AFTER + Duration::from_secs(1)), 0);
}

#[test]
fn two_devices_on_the_same_network_share_directly() {
    let (_, key) = new_code().unwrap();
    let (mut laptop, mut desktop) = (store(), store());
    let now: chrono::DateTime<chrono::Utc> = "2026-11-22T10:00:00Z".parse().unwrap();
    let a = Arc::new(LanFiles::new(
        key,
        &crate::folder_sync::device_id(&laptop).unwrap(),
    ));
    let b = Arc::new(LanFiles::new(
        key,
        &crate::folder_sync::device_id(&desktop).unwrap(),
    ));
    let (pa, pb) = (serve(a.clone()), serve(b.clone()));

    crate::todo::add(&laptop, &task("Renew passport"), NY, now).unwrap();
    // Each publishes (nobody heard yet), then they hear each other's announcements.
    sync(&mut laptop, a.as_ref(), &key, "Laptop", now).unwrap();
    sync(&mut desktop, b.as_ref(), &key, "Desktop", now).unwrap();
    let t = Instant::now();
    assert!(b.hear(&a.beacon(pa), HERE, t));
    assert!(a.hear(&b.beacon(pb), HERE, t));

    let r = sync(&mut desktop, b.as_ref(), &key, "Desktop", now).unwrap();
    assert_eq!(r.applied, 1);
    assert_eq!(r.peers[0].name, "Laptop");
    assert_eq!(
        crate::todo::all(&desktop).unwrap()[0].title,
        "Renew passport"
    );

    // A change on the desktop: its stamp changes, the laptop hears it and takes the change.
    crate::todo::add(&desktop, &task("Water plants"), NY, now).unwrap();
    sync(&mut desktop, b.as_ref(), &key, "Desktop", now).unwrap();
    assert!(a.hear(&b.beacon(pb), HERE, t), "new data, new stamp");
    let r = sync(&mut laptop, a.as_ref(), &key, "Laptop", now).unwrap();
    assert_eq!(r.applied, 1);
    assert_eq!(crate::todo::all(&laptop).unwrap().len(), 2);

    // Nothing changed: publishing again keeps the same stamp, so nobody downloads it again.
    let before = a.beacon(pa).stamp;
    sync(&mut laptop, a.as_ref(), &key, "Laptop", now).unwrap();
    assert_eq!(a.beacon(pa).stamp, before);
}

#[test]
fn an_answer_must_match_the_question_and_the_code() {
    let (_, key) = new_code().unwrap();
    let (_, wrong) = new_code().unwrap();
    // A fake device that answers with a recorded (or forged) reply, not one sealed with our fresh number.
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let replay = seal(&key, &[[7u8; NONCE].as_slice(), b"old snapshot"].concat()).unwrap();
    let forged = seal(&wrong, &[0u8; 40]).unwrap();
    std::thread::spawn(move || {
        for (i, conn) in listener.incoming().flatten().enumerate() {
            let mut c = conn;
            let mut req = [0u8; REQUEST.len() + NONCE];
            let _ = c.read_exact(&mut req);
            let answer = if i == 0 { &replay } else { &forged };
            let _ = c.write_all(&(answer.len() as u32).to_be_bytes());
            let _ = c.write_all(answer);
        }
    });
    let lan = LanFiles::new(key, "aa11");
    let fake = Beacon {
        group: group_of(&key),
        device: "dd44".into(),
        port,
        stamp: "1".into(),
    };
    lan.hear(&fake, HERE, Instant::now());
    assert!(lan.read("dd44").is_err(), "a replayed answer is refused");
    assert!(
        lan.read("dd44").is_err(),
        "an answer sealed with another code is refused"
    );
    // Something that isn't a request gets nothing.
    let quiet = Arc::new(LanFiles::new(key, "ee55"));
    let p = serve(quiet);
    let mut c = TcpStream::connect((Ipv4Addr::LOCALHOST, p)).unwrap();
    c.write_all(b"GET / HTTP/1.1\r\n\r\n").unwrap();
    c.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let mut buf = [0u8; 8];
    assert!(matches!(c.read(&mut buf), Ok(0) | Err(_)));
}
