//! Where to look for updates. The update check itself (and signature verification) is done by Tauri's updater;
//! this only decides the address, so it can be tested without a server.

use crate::settings::UpdateChannel;
use reqwest::Url;

/// `https://host/path` + channel -> `https://host/path/<channel>/latest.json`. `None` unless the base is a
/// plain https address with no credentials, query or fragment.
pub fn manifest_url(base: &str, channel: UpdateChannel) -> Option<String> {
    let url = Url::parse(base.trim()).ok()?;
    if url.scheme() != "https"
        || url.host_str().is_none_or(str::is_empty)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let name = match channel {
        UpdateChannel::Stable => "stable",
        UpdateChannel::Beta => "beta",
    };
    Some(format!(
        "{}/{name}/latest.json",
        url.as_str().trim_end_matches('/')
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_channel_specific_https_address() {
        assert_eq!(
            manifest_url(
                "https://updates.example.com/daybrief",
                UpdateChannel::Stable
            )
            .as_deref(),
            Some("https://updates.example.com/daybrief/stable/latest.json")
        );
        assert_eq!(
            manifest_url("https://updates.example.com/daybrief/", UpdateChannel::Beta).as_deref(),
            Some("https://updates.example.com/daybrief/beta/latest.json")
        );
        assert_eq!(
            manifest_url("https://updates.example.com", UpdateChannel::Stable).as_deref(),
            Some("https://updates.example.com/stable/latest.json")
        );
    }

    #[test]
    fn refuses_anything_that_is_not_a_plain_https_address() {
        for bad in [
            "",
            "http://updates.example.com",
            "ftp://x.test",
            "updates.example.com",
            "https://user:pw@x.test",
            "https://x.test/?a=1",
            "https://x.test/#frag",
            "javascript:1",
            "https://",
        ] {
            assert_eq!(manifest_url(bad, UpdateChannel::Stable), None, "{bad:?}");
        }
    }
}
