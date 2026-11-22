//! Keeping the sharing files in the person's own cloud storage, through its API, for computers (and later phones)
//! with no synced folder: Google Drive's hidden app data folder (`drive.appdata`) or OneDrive's app folder
//! (`Files.ReadWrite.AppFolder`). Both are folders only Firstlight can see; neither permission reaches the person's
//! other files. The files are the same encrypted `.flsync` files as in a synced folder, so the storage holds noise.

use super::{RemoteFile, SyncFiles, device_of, file_name};
use crate::http::{HttpClient, HttpRequest, HttpResponse, Url};
use crate::oauth::TokenManager;
use crate::{Error, Result};
use chrono::Utc;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub const GOOGLE_SCOPE: &str = "https://www.googleapis.com/auth/drive.appdata";
pub const MICROSOFT_SCOPE: &str = "Files.ReadWrite.AppFolder";
pub const GOOGLE_API: &str = "https://www.googleapis.com";
pub const GRAPH_API: &str = "https://graph.microsoft.com/v1.0";
/// Biggest file sent in one simple upload to OneDrive; bigger ones go through an upload session.
const ONEDRIVE_SIMPLE_MAX: usize = 4 * 1024 * 1024;
const MAX_PAGES: usize = 10;

fn check(resp: &HttpResponse, service: &str) -> Result<()> {
    match resp.status {
        200..=299 => Ok(()),
        401 => Err(Error::AuthRevoked),
        403 => Err(Error::PermissionNeeded),
        429 | 500..=599 => Err(Error::Offline),
        s => Err(Error::Provider(format!("{service} returned {s}"))),
    }
}

fn json_of(resp: &HttpResponse, service: &str) -> Result<Value> {
    check(resp, service)?;
    serde_json::from_slice(&resp.body)
        .map_err(|_| Error::Provider(format!("{service} sent a response we could not read")))
}

/// `base` + path segments, each percent-encoded, so an id can never change the path.
fn url(base: &str, segments: &[&str]) -> Result<Url> {
    let mut u = Url::parse(base).map_err(|_| Error::Invalid("bad address"))?;
    u.path_segments_mut()
        .map_err(|_| Error::Invalid("bad address"))?
        .extend(segments);
    Ok(u)
}

struct Authed {
    http: Arc<dyn HttpClient>,
    tokens: TokenManager,
    account_id: String,
}

impl Authed {
    fn send(&self, req: HttpRequest) -> Result<HttpResponse> {
        let token = self.tokens.access_token(&self.account_id, Utc::now())?;
        self.http
            .send(&req.header("Authorization", format!("Bearer {token}")))
    }
}

/// File ids seen in the last listing, by device id.
type Ids = Mutex<HashMap<String, String>>;

fn remember(ids: &Ids, device: &str, id: &str) {
    if let Ok(mut m) = ids.lock() {
        m.insert(device.to_string(), id.to_string());
    }
}

fn id_of(ids: &Ids, device: &str) -> Option<String> {
    ids.lock().ok().and_then(|m| m.get(device).cloned())
}

// ---- Google Drive ---------------------------------------------------------------------------

pub struct GoogleDriveFiles {
    auth: Authed,
    api: String,
    ids: Ids,
}

impl GoogleDriveFiles {
    pub fn new(
        http: Arc<dyn HttpClient>,
        tokens: TokenManager,
        account_id: &str,
        api: &str,
    ) -> Self {
        Self {
            auth: Authed {
                http,
                tokens,
                account_id: account_id.into(),
            },
            api: api.trim_end_matches('/').into(),
            ids: Mutex::default(),
        }
    }
}

impl SyncFiles for GoogleDriveFiles {
    fn list(&self) -> Result<Vec<RemoteFile>> {
        let mut out = Vec::new();
        let mut page: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut u = url(&self.api, &["drive", "v3", "files"])?;
            u.query_pairs_mut()
                .append_pair("spaces", "appDataFolder")
                .append_pair("q", "trashed = false")
                .append_pair("pageSize", "100")
                .append_pair(
                    "fields",
                    "nextPageToken,files(id,name,size,modifiedTime,md5Checksum)",
                );
            if let Some(p) = &page {
                u.query_pairs_mut().append_pair("pageToken", p);
            }
            let v = json_of(
                &self.auth.send(HttpRequest::get(u.to_string()))?,
                "Google Drive",
            )?;
            for f in v["files"].as_array().into_iter().flatten() {
                let (Some(id), Some(name)) = (f["id"].as_str(), f["name"].as_str()) else {
                    continue;
                };
                let Some(device) = device_of(name) else {
                    continue;
                };
                remember(&self.ids, device, id);
                let size = f["size"]
                    .as_str()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(u64::MAX);
                let stamp = f["md5Checksum"]
                    .as_str()
                    .or(f["modifiedTime"].as_str())
                    .unwrap_or("")
                    .to_string();
                out.push(RemoteFile {
                    device: device.to_string(),
                    size,
                    stamp,
                });
            }
            page = v["nextPageToken"].as_str().map(str::to_string);
            if page.is_none() {
                break;
            }
        }
        Ok(out)
    }

    fn read(&self, device: &str) -> Result<Vec<u8>> {
        let id = id_of(&self.ids, device).ok_or(Error::Invalid("that file isn't there"))?;
        let mut u = url(&self.api, &["drive", "v3", "files", &id])?;
        u.query_pairs_mut().append_pair("alt", "media");
        let resp = self.auth.send(HttpRequest::get(u.to_string()))?;
        check(&resp, "Google Drive")?;
        Ok(resp.body)
    }

    fn write(&self, device: &str, data: &[u8]) -> Result<()> {
        let resp = match id_of(&self.ids, device) {
            Some(id) => {
                let mut u = url(&self.api, &["upload", "drive", "v3", "files", &id])?;
                u.query_pairs_mut().append_pair("uploadType", "media");
                self.auth.send(
                    HttpRequest::new("PATCH", u.to_string())
                        .header("Content-Type", "application/octet-stream")
                        .body(data.to_vec()),
                )?
            }
            None => {
                let mut u = url(&self.api, &["upload", "drive", "v3", "files"])?;
                u.query_pairs_mut()
                    .append_pair("uploadType", "multipart")
                    .append_pair("fields", "id");
                let boundary = "firstlight-flsync-boundary";
                let meta =
                    serde_json::json!({"name": file_name(device), "parents": ["appDataFolder"]});
                let mut body = format!(
                    "--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{meta}\r\n--{boundary}\r\nContent-Type: application/octet-stream\r\n\r\n"
                )
                .into_bytes();
                body.extend_from_slice(data);
                body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
                let resp = self.auth.send(
                    HttpRequest::new("POST", u.to_string())
                        .header(
                            "Content-Type",
                            format!("multipart/related; boundary={boundary}"),
                        )
                        .body(body),
                )?;
                if let Ok(v) = serde_json::from_slice::<Value>(&resp.body)
                    && let Some(id) = v["id"].as_str()
                {
                    remember(&self.ids, device, id);
                }
                resp
            }
        };
        check(&resp, "Google Drive")
    }
}

// ---- OneDrive -------------------------------------------------------------------------------

pub struct OneDriveFiles {
    auth: Authed,
    api: String,
    ids: Ids,
}

impl OneDriveFiles {
    pub fn new(
        http: Arc<dyn HttpClient>,
        tokens: TokenManager,
        account_id: &str,
        api: &str,
    ) -> Self {
        Self {
            auth: Authed {
                http,
                tokens,
                account_id: account_id.into(),
            },
            api: api.trim_end_matches('/').into(),
            ids: Mutex::default(),
        }
    }

    fn same_host(&self, link: &str) -> bool {
        let (Ok(a), Ok(b)) = (Url::parse(&self.api), Url::parse(link)) else {
            return false;
        };
        a.scheme() == b.scheme() && a.host_str() == b.host_str() && a.port() == b.port()
    }
}

impl SyncFiles for OneDriveFiles {
    fn list(&self) -> Result<Vec<RemoteFile>> {
        let mut out = Vec::new();
        let mut u = url(
            &self.api,
            &["me", "drive", "special", "approot", "children"],
        )?;
        u.query_pairs_mut()
            .append_pair("$select", "id,name,size,eTag")
            .append_pair("$top", "200");
        let mut next = Some(u.to_string());
        for _ in 0..MAX_PAGES {
            let Some(link) = next.take() else { break };
            let resp = self.auth.send(HttpRequest::get(link))?;
            if resp.status == 404 {
                return Ok(out); // the app folder doesn't exist yet
            }
            let v = json_of(&resp, "OneDrive")?;
            for f in v["value"].as_array().into_iter().flatten() {
                let (Some(id), Some(name)) = (f["id"].as_str(), f["name"].as_str()) else {
                    continue;
                };
                let Some(device) = device_of(name) else {
                    continue;
                };
                remember(&self.ids, device, id);
                out.push(RemoteFile {
                    device: device.to_string(),
                    size: f["size"].as_u64().unwrap_or(u64::MAX),
                    stamp: f["eTag"].as_str().unwrap_or("").to_string(),
                });
            }
            // Only follow a next page on the same service.
            next = v["@odata.nextLink"]
                .as_str()
                .filter(|l| self.same_host(l))
                .map(str::to_string);
        }
        Ok(out)
    }

    fn read(&self, device: &str) -> Result<Vec<u8>> {
        let id = id_of(&self.ids, device).ok_or(Error::Invalid("that file isn't there"))?;
        // OneDrive answers with a redirect to a download address on another host; the HTTP client drops the
        // Authorization header when a redirect leaves the host.
        let u = url(&self.api, &["me", "drive", "items", &id, "content"])?;
        let resp = self.auth.send(HttpRequest::get(u.to_string()))?;
        check(&resp, "OneDrive")?;
        Ok(resp.body)
    }

    fn write(&self, device: &str, data: &[u8]) -> Result<()> {
        // OneDrive's path syntax needs literal slashes; the name is a checked hex device id plus ".flsync", so it
        // can't change the path.
        if !super::valid_device(device) {
            return Err(Error::Invalid("bad device id"));
        }
        let item = format!(
            "{}/me/drive/special/approot:/{}:",
            self.api,
            file_name(device)
        );
        if data.len() <= ONEDRIVE_SIMPLE_MAX {
            let u = format!("{item}/content");
            let resp = self.auth.send(
                HttpRequest::new("PUT", u)
                    .header("Content-Type", "application/octet-stream")
                    .body(data.to_vec()),
            )?;
            return check(&resp, "OneDrive");
        }
        // Bigger: an upload session, sent as one piece (OneDrive accepts up to 60 MB per piece; ours stay under 32).
        let u = format!("{item}/createUploadSession");
        let v = json_of(
            &self.auth.send(
                HttpRequest::new("POST", u)
                    .header("Content-Type", "application/json")
                    .body(r#"{"item":{"@microsoft.graph.conflictBehavior":"replace"}}"#),
            )?,
            "OneDrive",
        )?;
        let upload = v["uploadUrl"]
            .as_str()
            .filter(|l| l.starts_with("https://"))
            .ok_or_else(|| Error::Provider("OneDrive gave no upload address".into()))?;
        // The upload address carries its own authorisation: the Bearer token is not sent there.
        let resp = self.auth.http.send(
            &HttpRequest::new("PUT", upload)
                .header(
                    "Content-Range",
                    format!("bytes 0-{}/{}", data.len() - 1, data.len()),
                )
                .body(data.to_vec()),
        )?;
        check(&resp, "OneDrive")
    }
}

#[cfg(test)]
mod tests;
