//! Finding the video-call link in an event. Only `https` links are ever returned.

use regex::Regex;
use std::sync::OnceLock;

const KNOWN_HOSTS: &[&str] = &[
    "zoom.us",
    "zoom.com",
    "meet.google.com",
    "teams.microsoft.com",
    "teams.live.com",
    "webex.com",
    "whereby.com",
    "gotomeeting.com",
    "meet.jit.si",
];

fn url_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"https://[^\s<>"')\]]+"#).unwrap())
}

fn host_of(url: &str) -> Option<&str> {
    let rest = url.strip_prefix("https://")?;
    let host = rest.split(['/', '?', '#']).next()?;
    // Drop credentials and port: "user@host:443" -> "host".
    let host = host.rsplit('@').next()?;
    host.split(':').next()
}

fn is_known(url: &str) -> bool {
    host_of(url).is_some_and(|h| {
        let h = h.to_ascii_lowercase();
        KNOWN_HOSTS
            .iter()
            .any(|k| h == *k || h.ends_with(&format!(".{k}")))
    })
}

/// A link the provider explicitly labelled as the conference/join link. Any https URL is accepted.
pub fn from_conference_field(value: &str) -> Option<String> {
    let v = value.trim();
    (v.starts_with("https://") && host_of(v).is_some_and(|h| !h.is_empty())).then(|| v.to_string())
}

/// Searches free text (location, description) for a known video-call link.
pub fn find_in_text(text: &str) -> Option<String> {
    url_re()
        .find_iter(text)
        .map(|m| m.as_str().trim_end_matches(['.', ',', ';']).to_string())
        .find(|u| is_known(u))
}

/// Provider field first, then location, then description.
pub fn extract(
    conference: Option<&str>,
    location: Option<&str>,
    description: Option<&str>,
) -> Option<String> {
    conference
        .and_then(from_conference_field)
        .or_else(|| location.and_then(find_in_text))
        .or_else(|| description.and_then(find_in_text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_known_providers() {
        for u in [
            "https://zoom.us/j/123456789?pwd=abc",
            "https://acme.zoom.us/j/1",
            "https://meet.google.com/abc-defg-hij",
            "https://teams.microsoft.com/l/meetup-join/19%3ameeting_x",
            "https://acme.webex.com/meet/jo",
        ] {
            assert_eq!(
                find_in_text(&format!("Join here: {u}.")),
                Some(u.to_string()),
                "{u}"
            );
        }
    }

    #[test]
    fn never_returns_non_https() {
        assert_eq!(find_in_text("http://zoom.us/j/1"), None);
        assert_eq!(find_in_text("javascript:alert(1) zoom.us"), None);
        assert_eq!(from_conference_field("http://x.test/a"), None);
        assert_eq!(from_conference_field("file:///etc/passwd"), None);
    }

    #[test]
    fn ignores_lookalike_hosts() {
        assert_eq!(find_in_text("https://zoom.us.evil.com/j/1"), None);
        assert_eq!(find_in_text("https://notzoom.us/j/1"), None);
        assert_eq!(find_in_text("https://zoom.us@evil.com/j/1"), None);
    }

    #[test]
    fn conference_field_beats_text_and_location_beats_description() {
        let c = extract(
            Some("https://custom.example.org/room"),
            Some("https://zoom.us/j/1"),
            None,
        );
        assert_eq!(c.as_deref(), Some("https://custom.example.org/room"));
        let l = extract(
            None,
            Some("https://zoom.us/j/1"),
            Some("https://meet.google.com/a-b-c"),
        );
        assert_eq!(l.as_deref(), Some("https://zoom.us/j/1"));
        assert_eq!(extract(None, Some("Room 4"), Some("no link")), None);
    }
}
