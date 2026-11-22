//! Minimal blocking HTTP abstraction so providers can be tested without the internet.
//!
//! Security rules enforced here, once, for every provider:
//! * only `https` (loopback `http` only when explicitly enabled for tests);
//! * redirects are limited and may never downgrade to plain `http`;
//! * responses are size-capped;
//! * error values never contain the URL, because private calendar links and OAuth
//!   requests embed secrets in them.

use crate::{Error, Result};
use std::io::Read;

/// Re-exported so the shell can parse and check URLs without its own HTTP dependency.
pub use reqwest::Url;
use std::time::Duration;

const MAX_BODY_BYTES: u64 = 20 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Default)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

impl HttpRequest {
    pub fn new(method: &str, url: impl Into<String>) -> Self {
        Self {
            method: method.to_string(),
            url: url.into(),
            ..Default::default()
        }
    }
    pub fn get(url: impl Into<String>) -> Self {
        Self::new("GET", url)
    }
    pub fn header(mut self, k: &str, v: impl Into<String>) -> Self {
        self.headers.push((k.to_string(), v.into()));
        self
    }
    pub fn body(mut self, b: impl Into<Vec<u8>>) -> Self {
        self.body = Some(b.into());
        self
    }
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// Case-insensitive header lookup.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

pub trait HttpClient: Send + Sync {
    fn send(&self, req: &HttpRequest) -> Result<HttpResponse>;
}

/// Turns user-entered calendar addresses into a fetchable https URL: `webcal://` becomes
/// `https://`; anything that is not https afterwards is refused.
pub fn normalise_calendar_url(input: &str) -> Result<String> {
    let t = input.trim();
    let url = if let Some(rest) = t
        .strip_prefix("webcal://")
        .or_else(|| t.strip_prefix("WEBCAL://"))
    {
        format!("https://{rest}")
    } else {
        t.to_string()
    };
    if !url.to_ascii_lowercase().starts_with("https://")
        || url.len() < 12
        || url.contains(char::is_whitespace)
    {
        return Err(Error::Invalid(
            "calendar address must be an https:// or webcal:// link",
        ));
    }
    Ok(url)
}

/// Which addresses a client may talk to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    /// https only. What every remote service uses.
    HttpsOnly,
    /// https anywhere, plus plain http to this computer only (for a model running locally).
    HttpsOrLoopback,
    /// Anything, including plain http. Tests only.
    Anything,
}

fn is_loopback_host(host: Option<&str>) -> bool {
    matches!(host, Some("127.0.0.1" | "localhost" | "[::1]" | "::1"))
}

fn allowed(t: Transport, url: &Url) -> bool {
    match t {
        Transport::Anything => true,
        Transport::HttpsOnly => url.scheme() == "https",
        Transport::HttpsOrLoopback => {
            url.scheme() == "https" || (url.scheme() == "http" && is_loopback_host(url.host_str()))
        }
    }
}

pub struct ReqwestClient {
    client: reqwest::blocking::Client,
    transport: Transport,
}

impl ReqwestClient {
    /// Production client: https only.
    pub fn new() -> Result<Self> {
        Self::with_options(TIMEOUT, Transport::HttpsOnly)
    }

    /// Allows plain `http` (for loopback test servers). Never use outside tests.
    #[doc(hidden)]
    pub fn new_allow_http_for_tests() -> Result<Self> {
        Self::with_options(TIMEOUT, Transport::Anything)
    }

    pub fn with_options(timeout: Duration, transport: Transport) -> Result<Self> {
        let policy = reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= 5 {
                attempt.error("too many redirects")
            } else if !allowed(transport, attempt.url()) {
                attempt.error("redirect to a disallowed address refused")
            } else {
                attempt.follow()
            }
        });
        let builder = reqwest::blocking::Client::builder();
        // Android: the certificate check is set up here (see `tls`); elsewhere reqwest asks the system itself.
        #[cfg(target_os = "android")]
        let builder = builder.tls_backend_preconfigured(crate::tls::client_config()?);
        let client = builder
            .https_only(transport == Transport::HttpsOnly)
            .redirect(policy)
            .timeout(timeout)
            .user_agent(concat!("Firstlight/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| Error::Provider("could not create HTTP client".into()))?;
        Ok(Self { client, transport })
    }
}

fn classify(e: reqwest::Error) -> Error {
    // `without_url` matters: the default Display includes the full URL.
    let e = e.without_url();
    if e.is_timeout() || e.is_connect() {
        Error::Offline
    } else {
        Error::Provider(format!(
            "request failed ({})",
            if e.is_redirect() {
                "redirect refused"
            } else {
                "transport"
            }
        ))
    }
}

impl HttpClient for ReqwestClient {
    fn send(&self, req: &HttpRequest) -> Result<HttpResponse> {
        // Checked before anything is sent, so a refused address never sees a request.
        let url = Url::parse(&req.url).map_err(|_| Error::Invalid("bad address"))?;
        if !allowed(self.transport, &url) {
            return Err(Error::Invalid("this address is not allowed"));
        }
        let method = reqwest::Method::from_bytes(req.method.as_bytes())
            .map_err(|_| Error::Invalid("bad HTTP method"))?;
        let mut b = self.client.request(method, &req.url);
        for (k, v) in &req.headers {
            b = b.header(k.as_str(), v.as_str());
        }
        if let Some(body) = &req.body {
            b = b.body(body.clone());
        }
        let resp = b.send().map_err(classify)?;
        let status = resp.status().as_u16();
        let headers = resp
            .headers()
            .iter()
            .filter_map(|(k, v)| Some((k.as_str().to_string(), v.to_str().ok()?.to_string())))
            .collect();
        if resp.content_length().is_some_and(|n| n > MAX_BODY_BYTES) {
            return Err(Error::Provider("response too large".into()));
        }
        let mut body = Vec::new();
        resp.take(MAX_BODY_BYTES + 1)
            .read_to_end(&mut body)
            .map_err(|_| Error::Offline)?;
        if body.len() as u64 > MAX_BODY_BYTES {
            return Err(Error::Provider("response too large".into()));
        }
        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}

/// A scripted loopback HTTP server for tests.
#[cfg(test)]
pub mod testserver {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Clone)]
    pub struct Req {
        pub method: String,
        pub path: String,
        pub headers: Vec<(String, String)>,
        pub body: String,
    }

    impl Req {
        pub fn header(&self, name: &str) -> Option<&str> {
            self.headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.as_str())
        }
    }

    pub struct Resp {
        pub status: u16,
        pub headers: Vec<(String, String)>,
        pub body: String,
    }

    impl Resp {
        pub fn ok(body: &str) -> Self {
            Self {
                status: 200,
                headers: vec![],
                body: body.to_string(),
            }
        }
        pub fn status(status: u16) -> Self {
            Self {
                status,
                headers: vec![],
                body: String::new(),
            }
        }
        pub fn with_header(mut self, k: &str, v: &str) -> Self {
            self.headers.push((k.into(), v.into()));
            self
        }
        pub fn with_body(mut self, body: &str) -> Self {
            self.body = body.to_string();
            self
        }
        pub fn json(body: &str) -> Self {
            Self::ok(body).with_header("Content-Type", "application/json")
        }
    }

    pub struct TestServer {
        pub base: String,
        pub requests: Arc<Mutex<Vec<Req>>>,
    }

    impl TestServer {
        pub fn start(handler: impl Fn(&Req) -> Resp + Send + 'static) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let requests = Arc::new(Mutex::new(Vec::new()));
            let log = requests.clone();
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { break };
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        continue;
                    }
                    let mut parts = line.split_whitespace();
                    let (method, path) = (
                        parts.next().unwrap_or("").to_string(),
                        parts.next().unwrap_or("").to_string(),
                    );
                    let mut headers = Vec::new();
                    let mut len = 0usize;
                    loop {
                        let mut h = String::new();
                        if reader.read_line(&mut h).unwrap_or(0) == 0 || h.trim().is_empty() {
                            break;
                        }
                        if let Some((k, v)) = h.split_once(':') {
                            if k.eq_ignore_ascii_case("content-length") {
                                len = v.trim().parse().unwrap_or(0);
                            }
                            headers.push((k.trim().to_string(), v.trim().to_string()));
                        }
                    }
                    let mut body = vec![0u8; len];
                    reader.read_exact(&mut body).ok();
                    let req = Req {
                        method,
                        path,
                        headers,
                        body: String::from_utf8_lossy(&body).into_owned(),
                    };
                    let resp = handler(&req);
                    log.lock().unwrap().push(req);
                    let mut out = format!(
                        "HTTP/1.1 {} X\r\nContent-Length: {}\r\nConnection: close\r\n",
                        resp.status,
                        resp.body.len()
                    );
                    for (k, v) in &resp.headers {
                        out.push_str(&format!("{k}: {v}\r\n"));
                    }
                    out.push_str("\r\n");
                    out.push_str(&resp.body);
                    stream.write_all(out.as_bytes()).ok();
                }
            });
            Self { base, requests }
        }

        pub fn url(&self, path: &str) -> String {
            format!("{}{path}", self.base)
        }

        pub fn count(&self) -> usize {
            self.requests.lock().unwrap().len()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testserver::*;
    use super::*;

    #[test]
    fn webcal_becomes_https_and_junk_is_refused() {
        assert_eq!(
            normalise_calendar_url("webcal://example.com/a.ics").unwrap(),
            "https://example.com/a.ics"
        );
        assert_eq!(
            normalise_calendar_url("  https://example.com/a.ics ").unwrap(),
            "https://example.com/a.ics"
        );
        for bad in [
            "http://example.com/a.ics",
            "ftp://x.test/a",
            "javascript:alert(1)",
            "example.com/a.ics",
            "https://a b",
            "",
            "https://",
        ] {
            assert!(normalise_calendar_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn production_client_refuses_plain_http() {
        let server = TestServer::start(|_| Resp::ok("hi"));
        let c = ReqwestClient::new().unwrap();
        assert!(c.send(&HttpRequest::get(server.url("/"))).is_err());
        assert_eq!(server.count(), 0, "nothing may reach the server over http");
    }

    #[test]
    fn sends_method_headers_and_body_and_reads_response() {
        let server = TestServer::start(|r| {
            Resp::ok(&format!("{} {}", r.method, r.body)).with_header("ETag", "\"v1\"")
        });
        let c = ReqwestClient::new_allow_http_for_tests().unwrap();
        let r = c
            .send(
                &HttpRequest::new("REPORT", server.url("/cal"))
                    .header("Depth", "1")
                    .body("<x/>"),
            )
            .unwrap();
        assert_eq!(r.status, 200);
        assert_eq!(r.text(), "REPORT <x/>");
        assert_eq!(r.header("etag"), Some("\"v1\""));
        let seen = server.requests.lock().unwrap();
        assert_eq!(seen[0].header("depth"), Some("1"));
    }

    #[test]
    fn errors_never_contain_the_url() {
        // Closed port: connection refused.
        let c = ReqwestClient::new_allow_http_for_tests().unwrap();
        let err = c
            .send(&HttpRequest::get(
                "http://127.0.0.1:1/secret-token-abc123/calendar.ics",
            ))
            .unwrap_err();
        let shown = format!("{err} {err:?}");
        assert!(
            !shown.contains("secret-token") && !shown.contains("127.0.0.1"),
            "{shown}"
        );
        assert!(matches!(err, Error::Offline));
    }

    #[test]
    fn the_loopback_client_allows_http_to_this_computer_and_nowhere_else() {
        let server = TestServer::start(|_| Resp::ok("local"));
        let c = ReqwestClient::with_options(Duration::from_secs(5), Transport::HttpsOrLoopback)
            .unwrap();
        assert_eq!(
            c.send(&HttpRequest::get(server.url("/"))).unwrap().text(),
            "local"
        );
        for bad in [
            "http://example.com/",
            "http://192.168.1.10/",
            "http://127.0.0.1.evil.com/",
            "ftp://127.0.0.1/",
            "http://localhost.evil.com/",
        ] {
            let err = c.send(&HttpRequest::get(bad)).unwrap_err();
            assert!(
                matches!(err, Error::Invalid(_)),
                "{bad} must be refused before any connection: {err:?}"
            );
        }
    }

    #[test]
    fn a_loopback_redirect_cannot_escape_to_a_remote_http_address() {
        let server = TestServer::start(|_| {
            Resp::status(302).with_header("Location", "http://example.com/steal")
        });
        let c = ReqwestClient::with_options(Duration::from_secs(5), Transport::HttpsOrLoopback)
            .unwrap();
        assert!(c.send(&HttpRequest::get(server.url("/"))).is_err());
    }

    #[test]
    fn redirects_are_followed_but_capped() {
        let server = TestServer::start(|r| match r.path.as_str() {
            "/start" => Resp::status(302).with_header("Location", "/end"),
            "/end" => Resp::ok("arrived"),
            _ => Resp::status(302).with_header("Location", "/loop"),
        });
        let c = ReqwestClient::new_allow_http_for_tests().unwrap();
        assert_eq!(
            c.send(&HttpRequest::get(server.url("/start")))
                .unwrap()
                .text(),
            "arrived"
        );
        assert!(
            c.send(&HttpRequest::get(server.url("/loop"))).is_err(),
            "redirect loops stop"
        );
    }
}
