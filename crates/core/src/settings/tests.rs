use super::*;
use crate::briefing::notify::NotifySettings;

fn parse(s: &str) -> AppSettings {
    AppSettings::from_value(&serde_json::from_str(s).unwrap())
}

#[test]
fn defaults_are_sensible_and_round_trip() {
    let d = AppSettings::default();
    assert_eq!(d.general.sync_interval_minutes, 10);
    assert_eq!(
        (
            d.unlock.first_unlock_after_hour,
            d.unlock.min_interval_minutes,
            d.unlock.up_next_lead_minutes
        ),
        (6, 30, 15)
    );
    assert_eq!(
        (
            d.notify.before_minutes,
            d.notify.morning_hour,
            d.notify.evening_hour
        ),
        (10, 8, 18)
    );
    assert!(d.sections.tomorrow && d.sections.free_time && d.sections.attention);
    assert_eq!(
        d.appearance,
        AppearanceSettings {
            theme: Theme::System,
            accent: "system".into(),
            ambient: false,
        }
    );
    assert_eq!(
        AppSettings::from_value(&serde_json::to_value(&d).unwrap()),
        d
    );
}

#[test]
fn nothing_or_nonsense_loads_as_defaults() {
    for junk in [
        "null",
        "42",
        "\"hello\"",
        "[]",
        "{}",
        r#"{"general": 7, "unlock": "x", "notify": [1,2]}"#,
    ] {
        assert_eq!(parse(junk), AppSettings::default(), "{junk}");
    }
}

#[test]
fn one_bad_field_costs_only_that_field() {
    let s = parse(
        r#"{"general":{"sync_interval_minutes":"often","default_view":"week","time_format":"h24"},
            "notify":{"before_minutes":"soon","morning":false,"evening_hour":20},
            "appearance":{"theme":"dark","accent":42}}"#,
    );
    assert_eq!(s.general.sync_interval_minutes, 10, "wrong type: default");
    assert_eq!(
        (s.general.default_view, s.general.time_format),
        (DefaultView::Week, TimeFormat::H24),
        "valid siblings kept"
    );
    assert_eq!(s.notify.before_minutes, 10);
    assert!(!s.notify.morning);
    assert_eq!(s.notify.evening_hour, 20);
    assert_eq!(
        (s.appearance.theme, s.appearance.accent.as_str()),
        (Theme::Dark, "system")
    );
}

#[test]
fn an_unknown_enum_value_falls_back_without_touching_the_rest() {
    let s = parse(
        r#"{"general":{"default_view":"holographic","week_start":"sunday"},"appearance":{"theme":"neon"}}"#,
    );
    assert_eq!(s.general.default_view, DefaultView::Briefing);
    assert_eq!(s.general.week_start, WeekStart::Sunday);
    assert_eq!(s.appearance.theme, Theme::System);
}

#[test]
fn out_of_range_values_are_pulled_into_range() {
    let s = parse(
        r#"{"general":{"sync_interval_minutes":0},
            "briefing":{"min_gap_minutes":1,"work_start_hour":30,"work_end_hour":2},
            "unlock":{"min_interval_minutes":-5,"up_next_lead_minutes":9999,"first_unlock_after_hour":99,"quiet_hours":[25,3]},
            "notify":{"before_minutes":0,"morning_hour":15,"evening_hour":3}}"#,
    );
    assert_eq!(s.general.sync_interval_minutes, 1);
    assert_eq!(s.briefing.min_gap_minutes, 5);
    assert_eq!(
        (s.briefing.work_start_hour, s.briefing.work_end_hour),
        (9, 18),
        "nonsense hours reset together"
    );
    assert_eq!(
        (
            s.unlock.min_interval_minutes,
            s.unlock.up_next_lead_minutes,
            s.unlock.first_unlock_after_hour
        ),
        (0, 120, 23)
    );
    assert_eq!(
        s.unlock.quiet_hours,
        Some((22, 6)),
        "invalid quiet hours reset"
    );
    assert_eq!(
        (
            s.notify.before_minutes,
            s.notify.morning_hour,
            s.notify.evening_hour
        ),
        (1, 11, 12)
    );
}

#[test]
fn quiet_hours_can_be_switched_off_and_days_changed() {
    let s =
        parse(r#"{"unlock":{"quiet_hours":null,"days":[true,true,true,true,true,false,false]}}"#);
    assert_eq!(s.unlock.quiet_hours, None);
    assert_eq!(s.unlock.days, [true, true, true, true, true, false, false]);
    let bad_days = parse(r#"{"unlock":{"days":[true,false]}}"#);
    assert_eq!(
        bad_days.unlock.days, [true; 7],
        "a malformed list falls back"
    );
}

#[test]
fn accents_must_be_system_or_a_palette_colour() {
    assert_eq!(
        parse(r#"{"appearance":{"accent":"purple"}}"#)
            .appearance
            .accent,
        "purple"
    );
    assert_eq!(
        parse(r##"{"appearance":{"accent":"#ff0000"}}"##)
            .appearance
            .accent,
        "system"
    );
    assert_eq!(
        parse(r#"{"appearance":{"accent":"javascript:alert(1)"}}"#)
            .appearance
            .accent,
        "system"
    );
}

#[test]
fn shortcut_validation() {
    for ok in [
        "CmdOrCtrl+Shift+Space",
        "Alt+K",
        "ctrl+alt+9",
        "Cmd+F5",
        "CommandOrControl+Shift+Right",
        "Option+Shift+B",
    ] {
        assert!(valid_shortcut(ok), "{ok}");
    }
    for bad in [
        "",
        "K",
        "Space",
        "Shift+K",
        "Cmd+",
        "Cmd+Cmd+K",
        "Cmd+Shift+Banana",
        "Cmd+;",
        "Hyper+K",
        "Cmd+F99",
        "+++",
        "Cmd + K + L",
    ] {
        assert!(!valid_shortcut(bad), "{bad}");
    }
}

#[test]
fn an_invalid_shortcut_is_dropped_on_load_and_reported_on_save() {
    let loaded = parse(r#"{"general":{"global_shortcut":"Shift+K"}}"#);
    assert_eq!(loaded.general.global_shortcut, None);
    let mut s = AppSettings::default();
    s.general.global_shortcut = Some("K".into());
    assert!(s.validate().unwrap_err().contains("shortcut"));
    s.general.global_shortcut = Some("CmdOrCtrl+Shift+Space".into());
    assert!(s.validate().is_ok());
    s.briefing.work_end_hour = s.briefing.work_start_hour;
    assert!(s.validate().unwrap_err().contains("Working hours"));
}

#[test]
fn time_format_resolves_for_the_notification_engine() {
    let mut s = AppSettings::default();
    assert!(
        s.notify_for_engine(true).time_24h && !s.notify_for_engine(false).time_24h,
        "auto follows the system"
    );
    s.general.time_format = TimeFormat::H12;
    assert!(!s.notify_for_engine(true).time_24h);
    s.general.time_format = TimeFormat::H24;
    assert!(s.notify_for_engine(false).time_24h);
}

#[test]
fn newer_or_older_versions_still_load() {
    let future = parse(
        r#"{"version":99,"general":{"default_view":"day","some_future_option":true},"holograms":{"on":true}}"#,
    );
    assert_eq!(future.general.default_view, DefaultView::Day);
    assert_eq!(future.version, CURRENT_VERSION);
    assert_eq!(
        parse(r#"{"general":{"default_view":"day"}}"#)
            .general
            .default_view,
        DefaultView::Day,
        "no version at all"
    );
}

#[test]
fn export_and_import_round_trip() {
    let mut s = AppSettings::default();
    s.general.default_view = DefaultView::Week;
    s.general.global_shortcut = Some("CmdOrCtrl+Shift+Space".into());
    s.notify = NotifySettings {
        before_minutes: 15,
        morning: false,
        ..Default::default()
    };
    s.appearance.accent = "teal".into();
    s.unlock.days[6] = false;
    let text = s.export();
    assert_eq!(AppSettings::import(&text).unwrap(), s);
}

#[test]
fn import_refuses_foreign_oversized_and_broken_files_without_changing_anything() {
    assert!(AppSettings::import("not json").is_err());
    assert!(AppSettings::import(r#"{"format":"something-else","settings":{}}"#).is_err());
    assert!(AppSettings::import(r#"{"format":"daybrief-settings"}"#).is_err());
    assert!(
        AppSettings::import(&format!(
            r#"{{"format":"daybrief-settings","settings":{{"pad":"{}"}}}}"#,
            "x".repeat(MAX_IMPORT_BYTES)
        ))
        .is_err()
    );
    // An imported file with damaged values still loads, sanitised.
    let s = AppSettings::import(r#"{"format":"daybrief-settings","settings":{"general":{"sync_interval_minutes":99999},"appearance":{"accent":"nope"}}}"#).unwrap();
    assert_eq!(
        (
            s.general.sync_interval_minutes,
            s.appearance.accent.as_str()
        ),
        (1440, "system")
    );
}

#[test]
fn settings_never_contain_anything_secret() {
    let mut s = AppSettings::default();
    s.email.important_senders = vec!["boss@acme.com".into()];
    let text = s.export().to_ascii_lowercase();
    for word in [
        "token",
        "secret",
        "password",
        "credential",
        "apikey",
        "api_key",
        "bearer",
        "sk-ant",
    ] {
        assert!(!text.contains(word), "export mentions {word}");
    }
    // The only address in there is the local-model default on this computer.
    for part in text.split("http").skip(1) {
        assert!(
            part.starts_with("://127.0.0.1") || part.starts_with("://localhost"),
            "export contains a remote address: http{part:.30}"
        );
    }
}

#[test]
fn email_and_ai_are_off_by_default() {
    let d = AppSettings::default();
    assert!(
        !d.email.enabled
            && !d.email.apple_mail
            && d.email.account_ids.is_empty()
            && d.email.important_senders.is_empty()
    );
    assert!(!d.ai.enabled && d.ai.consented.is_none() && !d.ai.include_email);
}

#[test]
fn ai_cannot_be_on_without_consent_for_that_provider() {
    let on = parse(r#"{"ai":{"enabled":true,"provider":"anthropic"}}"#);
    assert!(!on.ai.enabled, "no consent recorded");
    let mismatch = parse(r#"{"ai":{"enabled":true,"provider":"local","consented":"anthropic"}}"#);
    assert!(!mismatch.ai.enabled, "consent was for a different provider");
    let ok = parse(r#"{"ai":{"enabled":true,"provider":"anthropic","consented":"anthropic"}}"#);
    assert!(ok.ai.enabled);
}

#[test]
fn an_imported_file_can_never_grant_consent_or_switch_ai_on() {
    let file = r#"{"format":"daybrief-settings","settings":{"ai":{"enabled":true,"provider":"anthropic","consented":"anthropic","model":"claude-haiku-5-5"}}}"#;
    let s = AppSettings::import(file).unwrap();
    assert_eq!((s.ai.enabled, s.ai.consented), (false, None));
    assert_eq!(
        s.ai.model, "claude-haiku-5-5",
        "harmless preferences still import"
    );
}

#[test]
fn the_local_model_endpoint_must_be_this_computer() {
    for ok in [
        "http://127.0.0.1:11434",
        "http://localhost:1234",
        "https://localhost:8443/v1",
        "http://[::1]:8080",
    ] {
        assert!(valid_local_endpoint(ok), "{ok}");
    }
    for bad in [
        "http://example.com",
        "http://192.168.1.5:11434",
        "http://127.0.0.1.evil.com",
        "http://user:pw@127.0.0.1",
        "ftp://127.0.0.1",
        "javascript:1",
        "",
        "localhost:11434",
        "http://localhost.evil.com",
    ] {
        assert!(!valid_local_endpoint(bad), "{bad}");
    }
    let s = parse(r#"{"ai":{"endpoint":"http://evil.example.com"}}"#);
    assert_eq!(
        s.ai.endpoint, DEFAULT_LOCAL_ENDPOINT,
        "a remote address is replaced, not trusted"
    );
}

#[test]
fn model_names_are_restricted_and_default_per_provider() {
    assert_eq!(
        parse(r#"{"ai":{"model":"claude-haiku-5-5"}}"#).ai.model,
        "claude-haiku-5-5"
    );
    assert_eq!(
        parse(r#"{"ai":{"model":"llama3.2:3b"}}"#).ai.model,
        "llama3.2:3b"
    );
    assert_eq!(
        parse(r#"{"ai":{"model":"bad model; rm -rf"}}"#).ai.model,
        DEFAULT_ANTHROPIC_MODEL
    );
    assert_eq!(
        parse(r#"{"ai":{"provider":"local","model":""}}"#).ai.model,
        DEFAULT_LOCAL_MODEL
    );
}

#[test]
fn important_senders_are_cleaned_deduplicated_and_capped() {
    let many: Vec<String> = (0..80).map(|i| format!("p{i}@x.com")).collect();
    let json = serde_json::json!({"email": {"important_senders": [" boss@acme.com ", "BOSS@acme.com", "", "  ", "@clients.org", "bad\u{7}entry", "x".repeat(300)]}});
    let s = AppSettings::from_value(&json);
    assert_eq!(s.email.important_senders, ["boss@acme.com", "@clients.org"]);
    let capped =
        AppSettings::from_value(&serde_json::json!({"email": {"important_senders": many}}));
    assert_eq!(capped.email.important_senders.len(), 50);
}

#[test]
fn email_account_ids_and_days_are_sanitised() {
    let s = AppSettings::from_value(
        &serde_json::json!({"email": {"account_ids": ["abc123", "abc123", "has space", "../etc", "ok-id_1"], "days_back": 400}}),
    );
    assert_eq!(s.email.account_ids, ["abc123", "ok-id_1"]);
    assert_eq!(s.email.days_back, 14);
    assert_eq!(
        AppSettings::from_value(&serde_json::json!({"email": {"days_back": 0}}))
            .email
            .days_back,
        1
    );
}

#[test]
fn updates_are_checked_by_default_on_the_stable_channel_and_crash_reports_are_off() {
    let d = AppSettings::default();
    assert!(d.general.check_for_updates);
    assert_eq!(d.general.update_channel, UpdateChannel::Stable);
    assert!(!d.privacy.crash_reports, "crash reports are opt-in");
    let s = parse(
        r#"{"general":{"update_channel":"beta","check_for_updates":false},"privacy":{"crash_reports":true}}"#,
    );
    assert_eq!(
        (
            s.general.update_channel,
            s.general.check_for_updates,
            s.privacy.crash_reports
        ),
        (UpdateChannel::Beta, false, true)
    );
    assert_eq!(
        parse(r#"{"general":{"update_channel":"nightly-with-bugs"}}"#)
            .general
            .update_channel,
        UpdateChannel::Stable,
        "unknown channel falls back"
    );
}

fn mailbox(id: &str, address: &str) -> crate::mail::imap::ImapMailbox {
    crate::mail::imap::ImapMailbox {
        id: id.into(),
        preset: crate::mail::imap::ImapPreset::Yahoo,
        address: address.into(),
    }
}

#[test]
fn imap_mailboxes_are_cleaned_and_never_travel_in_files() {
    let mut s = AppSettings::default();
    s.email.imap = vec![
        mailbox("a1", "me@yahoo.com"),
        mailbox("a1", "dup@yahoo.com"),
        mailbox("bad id", "x@y.com"),
        mailbox("b2", "has space@y.com"),
        mailbox("c3", "ok@icloud.com"),
        mailbox("d4", "d@y.com"),
        mailbox("e5", "e@y.com"),
        mailbox("f6", "f@y.com"),
    ];
    s.sanitize();
    let ids: Vec<_> = s.email.imap.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(
        ids,
        ["a1", "c3", "d4", "e5"],
        "invalid and duplicate dropped, capped at four"
    );

    assert!(
        !s.export().contains("yahoo.com"),
        "addresses stay out of exports"
    );
    let back = AppSettings::import(&s.export()).unwrap();
    assert!(back.email.imap.is_empty());
    let hostile = r#"{"format":"daybrief-settings","settings":{"email":{"imap":[{"id":"z9","preset":"yahoo","address":"a@b.c"}]}}}"#;
    assert!(AppSettings::import(hostile).unwrap().email.imap.is_empty());
}

#[test]
fn a_bad_preset_cannot_name_another_server_and_costs_nothing_else() {
    let s = parse(
        r#"{"email":{"days_back":5,"imap":[{"id":"a1","preset":"imap.evil.example","address":"a@b.c"}]}}"#,
    );
    assert!(s.email.imap.is_empty());
    assert_eq!(s.email.days_back, 5);
}
