use super::*;
use crate::folder_sync::{new_code, sync};
use crate::secrets::{MemorySecretStore, load_or_create_db_key};
use crate::store::Store;
use crate::tasks::TaskInput;
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
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

#[test]
fn a_phone_pushes_its_changes_and_pulls_the_computers_with_connections_it_starts() {
    let (_, key) = new_code().unwrap();
    let (mut phone_db, mut mac_db) = (store(), store());
    let now: chrono::DateTime<chrono::Utc> = "2026-11-22T10:00:00Z".parse().unwrap();
    let phone = Arc::new(LanFiles::new(
        key,
        &crate::folder_sync::device_id(&phone_db).unwrap(),
    ));
    let mac = Arc::new(LanFiles::new(
        key,
        &crate::folder_sync::device_id(&mac_db).unwrap(),
    ));
    // Only the computer answers connections; the phone never does (as when a phone refuses incoming ones).
    let mac_port = serve(mac.clone());
    mac.set_port(mac_port);
    let at = SocketAddr::new(HERE, mac_port);

    crate::todo::add(&phone_db, &task("Made on the phone"), NY, now).unwrap();
    crate::todo::add(&mac_db, &task("Made on the Mac"), NY, now).unwrap();
    sync(&mut phone_db, phone.as_ref(), &key, "iPhone", now).unwrap();
    sync(&mut mac_db, mac.as_ref(), &key, "Mac", now).unwrap();

    // The phone pushes; the computer has its snapshot without connecting to the phone.
    let b = phone.push_to(at).unwrap();
    assert_eq!(b.device, crate::folder_sync::device_id(&mac_db).unwrap());
    let r = sync(&mut mac_db, mac.as_ref(), &key, "Mac", now).unwrap();
    assert_eq!(r.applied, 1);
    assert_eq!(r.peers[0].name, "iPhone");
    // And the phone, having heard the computer through the push, fetches the computer's changes itself.
    let r = sync(&mut phone_db, phone.as_ref(), &key, "iPhone", now).unwrap();
    assert_eq!(r.applied, 1);
    let titles = |db: &Store| {
        let mut t: Vec<String> = crate::todo::all(db)
            .unwrap()
            .into_iter()
            .map(|t| t.title)
            .collect();
        t.sort();
        t
    };
    assert_eq!(titles(&phone_db), ["Made on the Mac", "Made on the phone"]);
    assert_eq!(titles(&mac_db), ["Made on the Mac", "Made on the phone"]);

    // A later change on the phone goes across at the next push; nothing changed means nothing applied.
    crate::todo::add(&phone_db, &task("Second phone task"), NY, now).unwrap();
    sync(&mut phone_db, phone.as_ref(), &key, "iPhone", now).unwrap();
    phone.push_to(at).unwrap();
    assert_eq!(
        sync(&mut mac_db, mac.as_ref(), &key, "Mac", now)
            .unwrap()
            .applied,
        1
    );
    phone.push_to(at).unwrap();
    assert_eq!(
        sync(&mut mac_db, mac.as_ref(), &key, "Mac", now)
            .unwrap()
            .applied,
        0
    );
}

#[test]
fn a_push_from_a_stranger_or_a_recording_is_refused() {
    let (_, key) = new_code().unwrap();
    let (_, other) = new_code().unwrap();
    let mac = Arc::new(LanFiles::new(key, "aa11"));
    let port = serve(mac.clone());
    let at = SocketAddr::new(HERE, port);
    // Another sync code: the computer can't open it, and the phone gets nothing back.
    let stranger = LanFiles::new(other, "bb22");
    assert!(stranger.push_to(at).is_err());
    assert!(mac.list().unwrap().is_empty());
    // A push sealed to an old number (a recording) is refused.
    let mut c = TcpStream::connect(at).unwrap();
    c.write_all(PUSH).unwrap();
    let mut fresh = [0u8; NONCE];
    c.read_exact(&mut fresh).unwrap();
    let mut plain = vec![9u8; NONCE];
    plain.push(4);
    plain.extend_from_slice(b"cc33");
    plain.extend_from_slice(b"old");
    let sealed = seal(&key, &plain).unwrap();
    c.write_all(&(sealed.len() as u32).to_be_bytes()).unwrap();
    c.write_all(&sealed).unwrap();
    let mut buf = [0u8; 4];
    let _ = c.set_read_timeout(Some(Duration::from_secs(2)));
    assert!(matches!(c.read(&mut buf), Ok(0) | Err(_)));
    assert!(mac.list().unwrap().is_empty());
}
