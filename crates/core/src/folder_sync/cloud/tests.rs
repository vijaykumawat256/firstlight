use super::*;
use crate::http::ReqwestClient;
use crate::http::testserver::{Resp, TestServer};
use crate::oauth::{OAuthConfig, TokenSet, secret_key};
use crate::secrets::{MemorySecretStore, SecretStore};

fn tokens(server: &TestServer) -> (Arc<dyn HttpClient>, TokenManager) {
    let secrets = Arc::new(MemorySecretStore::default());
    let tk = TokenSet {
        access_token: "AT".into(),
        refresh_token: "RT".into(),
        expires_at: Utc::now() + chrono::Duration::hours(1),
    };
    secrets
        .set(&secret_key("acct"), &serde_json::to_string(&tk).unwrap())
        .unwrap();
    let cfg = OAuthConfig {
        client_id: "c".into(),
        client_secret: None,
        auth_url: "https://a.example/auth".into(),
        token_url: server.url("/token"),
        scopes: vec![],
    };
    let http: Arc<dyn HttpClient> = Arc::new(ReqwestClient::new_allow_http_for_tests().unwrap());
    (http.clone(), TokenManager { http, secrets, cfg })
}

type Log = Arc<Mutex<Vec<(String, String, String, Option<String>)>>>;

fn logged(log: &Log, r: &crate::http::testserver::Req) {
    log.lock().unwrap().push((
        r.method.clone(),
        r.path.clone(),
        r.body.clone(),
        r.header("authorization").map(str::to_string),
    ));
}

#[test]
fn google_drive_keeps_the_files_in_the_hidden_app_folder() {
    let log: Log = Arc::default();
    let l2 = log.clone();
    let server = TestServer::start(move |r| {
        logged(&l2, r);
        if r.header("authorization") != Some("Bearer AT") {
            return Resp::status(401);
        }
        match (r.method.as_str(), r.path.split('?').next().unwrap()) {
            ("GET", "/drive/v3/files") => Resp::json(
                r#"{"files":[
                    {"id":"F1","name":"aa11.flsync","size":"5","md5Checksum":"m1"},
                    {"id":"F2","name":"notes.txt","size":"9"},
                    {"id":"F3","name":"../x.flsync","size":"1"}
                ]}"#,
            ),
            ("GET", "/drive/v3/files/F1") => Resp::ok("hello"),
            ("POST", "/upload/drive/v3/files") => Resp::json(r#"{"id":"NEW"}"#),
            ("PATCH", "/upload/drive/v3/files/NEW") => Resp::json("{}"),
            _ => Resp::status(404),
        }
    });
    let (http, tk) = tokens(&server);
    let drive = GoogleDriveFiles::new(http, tk, "acct", &server.url(""));
    let list = drive.list().unwrap();
    assert_eq!(
        list,
        vec![RemoteFile {
            device: "aa11".into(),
            size: 5,
            stamp: "m1".into()
        }],
        "only our own files, with safe names"
    );
    assert_eq!(drive.read("aa11").unwrap(), b"hello");
    // First write creates the file in appDataFolder; the next one replaces it by id.
    drive.write("bb22", b"sealed-bytes").unwrap();
    drive.write("bb22", b"sealed-again").unwrap();

    let log = log.lock().unwrap();
    let listing = &log[0];
    assert!(listing.1.contains("spaces=appDataFolder"), "{}", listing.1);
    let create = log.iter().find(|e| e.0 == "POST").unwrap();
    assert!(create.1.contains("uploadType=multipart"));
    assert!(create.2.contains(r#""parents":["appDataFolder"]"#));
    assert!(create.2.contains(r#""name":"bb22.flsync""#));
    assert!(create.2.contains("sealed-bytes"));
    let update = log.iter().find(|e| e.0 == "PATCH").unwrap();
    assert!(
        update.1.starts_with("/upload/drive/v3/files/NEW") && update.1.contains("uploadType=media")
    );
    assert_eq!(update.2, "sealed-again");
}

#[test]
fn onedrive_uses_the_app_folder_and_a_missing_folder_is_empty() {
    let log: Log = Arc::default();
    let l2 = log.clone();
    let first = Arc::new(Mutex::new(true));
    let server = TestServer::start(move |r| {
        logged(&l2, r);
        let path = r.path.split('?').next().unwrap().to_string();
        match (r.method.as_str(), path.as_str()) {
            ("GET", "/me/drive/special/approot/children") => {
                let mut f = first.lock().unwrap();
                if *f {
                    *f = false;
                    return Resp::status(404);
                }
                Resp::json(
                    r#"{"value":[{"id":"I1","name":"cc33.flsync","size":7,"eTag":"e1"}],"@odata.nextLink":"https://evil.example/next"}"#,
                )
            }
            ("GET", "/me/drive/items/I1/content") => Resp::ok("payload"),
            ("PUT", "/me/drive/special/approot:/dd44.flsync:/content") => Resp::json("{}"),
            _ => Resp::status(404),
        }
    });
    let (http, tk) = tokens(&server);
    let od = OneDriveFiles::new(http, tk, "acct", &server.url(""));
    assert!(od.list().unwrap().is_empty(), "no app folder yet");
    let list = od.list().unwrap();
    assert_eq!(list.len(), 1, "a next page on another host is not followed");
    assert_eq!(
        (list[0].device.as_str(), list[0].stamp.as_str()),
        ("cc33", "e1")
    );
    assert_eq!(od.read("cc33").unwrap(), b"payload");
    od.write("dd44", b"sealed").unwrap();
    let log = log.lock().unwrap();
    assert!(log.iter().all(|e| e.3.as_deref() == Some("Bearer AT")));
    assert!(!log.iter().any(|e| e.1.contains("evil")));
    let put = log.iter().find(|e| e.0 == "PUT").unwrap();
    assert_eq!(put.2, "sealed");
}

#[test]
fn a_missing_permission_and_a_revoked_sign_in_are_told_apart() {
    let server = TestServer::start(|r| {
        if r.path.contains("approot") {
            Resp::status(403)
        } else {
            Resp::status(401)
        }
    });
    let (http, tk) = tokens(&server);
    let od = OneDriveFiles::new(http, tk, "acct", &server.url(""));
    assert!(matches!(od.list(), Err(Error::PermissionNeeded)));
    let (http, tk) = tokens(&server);
    let drive = GoogleDriveFiles::new(http, tk, "acct", &server.url(""));
    assert!(matches!(drive.list(), Err(Error::AuthRevoked)));
}
