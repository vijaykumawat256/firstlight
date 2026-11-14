//! Named profiles: a second, fully separate copy of the app's data (its own keychain entries and database).
//! Used for testing release builds safely next to a real installation. Never changes how normal use works.

/// A profile name: 1 to 24 characters of `a-z`, `0-9` and `-`. Anything else is ignored (the default profile is used).
pub fn validate(name: &str) -> Option<String> {
    let n = name.trim();
    (!n.is_empty()
        && n.len() <= 24
        && n.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'))
    .then(|| n.to_string())
}

/// The keychain service name for a profile (`base` for the default).
pub fn keychain_service(base: &str, profile: Option<&str>) -> String {
    match profile {
        Some(p) => format!("{base}.{p}"),
        None => base.to_string(),
    }
}

/// The database file name for a profile.
pub fn db_file_name(profile: Option<&str>) -> String {
    match profile {
        Some(p) => format!("daybrief-{p}.db"),
        None => "daybrief.db".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_simple_names_are_accepted() {
        assert_eq!(validate("test").as_deref(), Some("test"));
        assert_eq!(validate(" qa-2 ").as_deref(), Some("qa-2"));
        for bad in [
            "",
            "  ",
            "Has Caps",
            "has space",
            "../escape",
            "a/b",
            "dot.name",
            "x".repeat(25).as_str(),
            "naïve",
            "semi;colon",
        ] {
            assert_eq!(validate(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn the_default_profile_is_unchanged_and_others_are_separate() {
        assert_eq!(
            keychain_service("com.daybrief.app", None),
            "com.daybrief.app"
        );
        assert_eq!(db_file_name(None), "daybrief.db");
        assert_eq!(
            keychain_service("com.daybrief.app", Some("qa")),
            "com.daybrief.app.qa"
        );
        assert_eq!(db_file_name(Some("qa")), "daybrief-qa.db");
        assert_ne!(db_file_name(Some("a")), db_file_name(Some("b")));
    }
}
