//! Receives the browser redirect on `127.0.0.1` (RFC 8252, section 7.3).
//!
//! The listener binds a random port on the loopback interface only, accepts only
//! `GET /callback`, and requires the `state` we generated. Stray requests (favicon, port scanners)
//! are answered and ignored without ending the wait.

use crate::{Error, Result};
use reqwest::Url;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub struct Loopback {
    listener: TcpListener,
    pub port: u16,
}

const PAGE: &str = "<!doctype html><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'>\
<title>Daybrief</title><style>body{font:16px -apple-system,Segoe UI,sans-serif;display:grid;place-items:center;height:100vh;margin:0;background:#f5f5f7;color:#1d1d1f}\
div{text-align:center}h1{font-size:22px;margin:0 0 8px}p{color:#6e6e73;margin:0}@media(prefers-color-scheme:dark){body{background:#1c1c1e;color:#f5f5f7}}</style>\
<div><h1>MESSAGE</h1><p>You can close this tab and return to Daybrief.</p></div>";

fn respond(stream: &mut TcpStream, status: &str, message: &str) {
    let body = PAGE.replace("MESSAGE", message);
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\nReferrer-Policy: no-referrer\r\n\r\n{body}",
        body.len()
    );
}

/// Constant-time-ish comparison; the state is not secret from the user but should not leak by timing.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

impl Loopback {
    pub fn bind() -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|_| Error::Provider("could not start the sign-in listener".into()))?;
        listener
            .set_nonblocking(true)
            .map_err(|_| Error::Provider("could not start the sign-in listener".into()))?;
        let port = listener
            .local_addr()
            .map_err(|_| Error::Provider("could not start the sign-in listener".into()))?
            .port();
        Ok(Self { listener, port })
    }

    pub fn redirect_uri(&self, host: &str) -> String {
        format!("http://{host}:{}/callback", self.port)
    }

    /// Waits for the redirect and returns the authorization code.
    pub fn wait_for_code(
        &self,
        expected_state: &str,
        timeout: Duration,
        cancel: &AtomicBool,
    ) -> Result<String> {
        let deadline = Instant::now() + timeout;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(Error::Provider("sign-in was cancelled".into()));
            }
            if Instant::now() >= deadline {
                return Err(Error::Provider("sign-in timed out".into()));
            }
            match self.listener.accept() {
                Ok((mut stream, _)) => {
                    stream.set_nonblocking(false).ok();
                    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
                    if let Some(result) = Self::handle(&mut stream, expected_state) {
                        return result;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(100))
                }
                Err(_) => return Err(Error::Provider("sign-in listener failed".into())),
            }
        }
    }

    /// `Some(result)` ends the wait; `None` means "not the callback, keep waiting".
    fn handle(stream: &mut TcpStream, expected_state: &str) -> Option<Result<String>> {
        let mut line = String::new();
        BufReader::new(stream.try_clone().ok()?)
            .read_line(&mut line)
            .ok()?;
        let mut parts = line.split_whitespace();
        let (method, target) = (parts.next()?, parts.next()?);
        let url = Url::parse(&format!("http://localhost{target}")).ok()?;
        if method != "GET" || url.path() != "/callback" {
            respond(stream, "404 Not Found", "Not found");
            return None;
        }
        let q = |k: &str| {
            url.query_pairs()
                .find(|(n, _)| n == k)
                .map(|(_, v)| v.into_owned())
        };

        // Check the state first: a callback we did not initiate must never complete the flow.
        if !q("state").is_some_and(|s| same(&s, expected_state)) {
            respond(
                stream,
                "400 Bad Request",
                "That sign-in link wasn't started by Daybrief",
            );
            return None;
        }
        if let Some(err) = q("error") {
            respond(stream, "200 OK", "Sign-in was cancelled");
            return Some(Err(Error::Provider(if err == "access_denied" {
                "sign-in was cancelled".into()
            } else {
                "sign-in was refused".into()
            })));
        }
        match q("code").filter(|c| !c.is_empty() && c.len() < 4096) {
            Some(code) => {
                respond(stream, "200 OK", "You're signed in");
                Some(Ok(code))
            }
            None => {
                respond(stream, "400 Bad Request", "Sign-in didn't complete");
                None
            }
        }
    }
}

/// Convenience for callers that don't need cancellation.
pub fn never_cancelled() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    fn get(port: u16, target: &str) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(s, "GET {target} HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        out
    }

    fn wait(lb: Loopback, state: &'static str) -> std::thread::JoinHandle<Result<String>> {
        std::thread::spawn(move || {
            lb.wait_for_code(state, Duration::from_secs(10), &AtomicBool::new(false))
        })
    }

    #[test]
    fn returns_the_code_when_the_state_matches() {
        let lb = Loopback::bind().unwrap();
        let port = lb.port;
        let h = wait(lb, "good-state");
        let page = get(port, "/callback?code=abc123&state=good-state");
        assert!(page.contains("200 OK") && page.contains("signed in"));
        assert_eq!(h.join().unwrap().unwrap(), "abc123");
    }

    #[test]
    fn a_wrong_or_missing_state_is_refused_and_the_wait_continues() {
        let lb = Loopback::bind().unwrap();
        let port = lb.port;
        let h = wait(lb, "good-state");
        assert!(get(port, "/callback?code=evil&state=bad-state").contains("400"));
        assert!(get(port, "/callback?code=evil").contains("400"));
        assert!(get(port, "/favicon.ico").contains("404"));
        assert!(get(port, "/callback?code=real&state=good-state").contains("200"));
        assert_eq!(
            h.join().unwrap().unwrap(),
            "real",
            "forged callbacks never completed the flow"
        );
    }

    #[test]
    fn denial_ends_the_flow_with_a_cancel_error() {
        let lb = Loopback::bind().unwrap();
        let port = lb.port;
        let h = wait(lb, "s");
        get(port, "/callback?error=access_denied&state=s");
        assert!(
            h.join()
                .unwrap()
                .unwrap_err()
                .to_string()
                .contains("cancelled")
        );
    }

    #[test]
    fn a_denial_without_the_right_state_cannot_cancel_the_flow() {
        let lb = Loopback::bind().unwrap();
        let port = lb.port;
        let h = wait(lb, "s");
        get(port, "/callback?error=access_denied&state=forged");
        get(port, "/callback?code=ok&state=s");
        assert_eq!(h.join().unwrap().unwrap(), "ok");
    }

    #[test]
    fn can_be_cancelled_and_times_out() {
        let lb = Loopback::bind().unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let c2 = cancel.clone();
        let h = std::thread::spawn(move || lb.wait_for_code("s", Duration::from_secs(10), &c2));
        std::thread::sleep(Duration::from_millis(250));
        cancel.store(true, Ordering::Relaxed);
        assert!(
            h.join()
                .unwrap()
                .unwrap_err()
                .to_string()
                .contains("cancelled")
        );

        let lb = Loopback::bind().unwrap();
        let err = lb
            .wait_for_code("s", Duration::from_millis(300), &AtomicBool::new(false))
            .unwrap_err();
        assert!(err.to_string().contains("timed out"));
    }

    #[test]
    fn binds_only_to_loopback_on_a_random_port() {
        let a = Loopback::bind().unwrap();
        let b = Loopback::bind().unwrap();
        assert_ne!(a.port, b.port);
        assert!(a.listener.local_addr().unwrap().ip().is_loopback());
        assert_eq!(
            a.redirect_uri("127.0.0.1"),
            format!("http://127.0.0.1:{}/callback", a.port)
        );
    }
}
