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
            accent: "system".into()
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
    let text = AppSettings::default().export().to_ascii_lowercase();
    for word in [
        "token",
        "secret",
        "password",
        "credential",
        "apikey",
        "api_key",
        "bearer",
        "url",
        "http",
    ] {
        assert!(!text.contains(word), "export mentions {word}");
    }
}
