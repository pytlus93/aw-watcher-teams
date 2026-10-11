use aw_watcher_teams::{
    aw::AwWriter,
    classify_with_format,
    config::Config,
    payload::{Mode, PayloadConfig},
    SessionEvent, Snapshot, Window,
};
use chrono::{DateTime, Utc};
use serde_json::Value;
fn event(config: &Config) -> SessionEvent {
    let activity = classify_with_format(
        &Snapshot {
            collector_ok: true,
            windows: vec![Window {
                key: "synthetic".into(),
                leave: true,
                microphone: true,
                meeting_info: true,
                title: Some(
                    "Review | Example Ltd. | self@example.invalid | Microsoft Teams".into(),
                ),
                ..Default::default()
            }],
        },
        config.collect_title(),
        &config.effective_title_format(),
    );
    SessionEvent {
        timestamp: DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
        duration: 5.0,
        data: aw_watcher_teams::aw::event_data(activity, "local-test-1".into()),
    }
}
fn configured() -> Config {
    let mut config = Config {
        collect_meeting_title: true,
        ..Default::default()
    };
    config
        .title_format
        .organization_map
        .insert("Example Ltd.".into(), "Example".into());
    config
}
fn wire(config: &Config) -> Value {
    serde_json::to_value(config.payload.wire(&event(config), None)).unwrap()
}
#[test]
fn legacy_configuration_migrates_to_strict_without_new_private_fields() {
    let c:Config=serde_json::from_str(r#"{"server":"http://127.0.0.1:5600/","interval_seconds":5,"collector_timeout_seconds":8,"collect_meeting_title":true}"#).unwrap();
    assert_eq!(c.payload.mode, Mode::Strict);
    assert!(!c.payload.fields.account);
    let data = wire(&c)["data"].clone();
    assert_eq!(data.as_object().unwrap().len(), 1);
    assert!(data.get("title").is_some());
    assert!(!data.to_string().contains("self@"));
}
#[test]
fn every_extension_is_individually_omitted() {
    let mut c = configured();
    c.payload = PayloadConfig::extended();
    c.payload.fields.account = true;
    c.title_format.include_account = true;
    let fields = [
        "schema_version",
        "app",
        "meeting_title",
        "organization",
        "account",
        "call_kind",
        "state",
        "assessment",
        "evidence",
        "title_source",
        "session_id",
        "identity_source",
    ];
    let enabled = wire(&c);
    for field in fields {
        assert!(enabled["data"].get(field).is_some(), "{field}");
        let mut value = serde_json::to_value(&c).unwrap();
        value["payload"]["fields"][field] = false.into();
        let disabled: Config = serde_json::from_value(value).unwrap();
        let actual = wire(&disabled);
        assert!(actual["data"].get(field).is_none(), "{field}");
        assert!(actual["data"]["title"].is_string());
        if field == "organization" {
            assert!(!actual.to_string().contains("Example"));
        }
        if field == "account" {
            assert!(!actual.to_string().contains("self@"));
        }
        if field == "meeting_title" {
            assert!(!actual.to_string().contains("Review"));
            assert!(!actual.to_string().contains("Example"));
            assert!(!actual.to_string().contains("self@"));
        }
    }
}
#[test]
fn request_serialization_puts_required_title_before_extensions() {
    let mut c = configured();
    c.payload.mode = Mode::Extended;
    let e = event(&c);
    let json = serde_json::to_string(&[c.payload.wire(&e, None)]).unwrap();
    assert!(json.contains("\"data\":{\"title\":"));
    assert!(json.find("\"title\":").unwrap() < json.find("\"schema_version\":").unwrap());
    assert_eq!(wire(&configured())["data"]["title"], "Review | Example");
}
#[test]
fn invalid_payload_settings_are_rejected() {
    for bad in [
        r#"{"mode":"window"}"#,
        r#"{"mode":"strict","fields":{"unexpected":true}}"#,
        r#"{"fields":{"account":"yes"}}"#,
    ] {
        assert!(serde_json::from_str::<PayloadConfig>(bad).is_err());
    }
}
#[test]
fn disabled_private_context_is_removed_from_checkpoint_on_replay() {
    let dir = std::env::temp_dir().join(format!(
        "teams-private-{}-{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("queue.json");
    let mut c = configured();
    c.payload.mode = Mode::Extended;
    c.payload.fields.account = true;
    c.title_format.include_account = true;
    let mut w = AwWriter::new(
        "http://127.0.0.1:59999/api/0/buckets/synthetic".into(),
        path.clone(),
    )
    .unwrap();
    w.configure_payload(
        c.payload.clone(),
        c.collect_title(),
        c.effective_title_format(),
    )
    .unwrap();
    w.queue(event(&c)).unwrap();
    assert!(std::fs::read_to_string(&path).unwrap().contains("self@"));
    drop(w);
    c.payload.fields.meeting_title = false;
    let mut w = AwWriter::new(
        "http://127.0.0.1:59999/api/0/buckets/synthetic".into(),
        path.clone(),
    )
    .unwrap();
    w.configure_payload(
        c.payload.clone(),
        c.collect_title(),
        c.effective_title_format(),
    )
    .unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    for private in ["Review", "Example", "self@"] {
        assert!(!saved.contains(private));
    }
    drop(w);
    std::fs::remove_dir_all(dir).unwrap();
}
#[cfg(windows)]
#[test]
fn settings_defaults_match_rust_without_opening_dialog() {
    let dir = std::env::temp_dir().join(format!("teams-settings-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.json");
    std::fs::write(
        &path,
        r#"{"server":"http://127.0.0.1:5600/","interval_seconds":5,"collector_timeout_seconds":8}"#,
    )
    .unwrap();
    let output = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-File",
            "scripts/settings.ps1",
            "-ValidateOnly",
            "-ConfigPath",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: Config = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        serde_json::to_value(actual.payload).unwrap(),
        serde_json::to_value(PayloadConfig::default()).unwrap()
    );
    std::fs::remove_dir_all(dir).unwrap();
}
#[cfg(windows)]
#[test]
fn collector_lock_desktop_interop_compiles_without_collecting() {
    let output = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-File",
            "scripts/snapshot.ps1",
            "-ValidateInterop",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Interop compiled"));
}
