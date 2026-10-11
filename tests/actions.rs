use aw_watcher_teams::{
    context::{Command as Action, Launch},
    privacy_log,
};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    process::{Command, Output},
    thread,
};
fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}
fn env() -> BTreeMap<String, String> {
    BTreeMap::from([("LOCALAPPDATA".into(), "synthetic-root".into())])
}
fn root(name: &str) -> PathBuf {
    let path = std::env::current_dir().unwrap().join("local").join(format!(
        "actions-{name}-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
fn run(root: &PathBuf, argv: &[&str], vars: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_aw-watcher-teams"));
    for name in [
        "AW_MODULE_ACTION_PROTOCOL",
        "AW_MODULE_ACTION",
        "AW_PROFILE",
        "AW_TESTING",
        "AW_SERVER_URL",
        "AW_TEAMS_CONFIG_PATH",
    ] {
        cmd.env_remove(name);
    }
    cmd.env("LOCALAPPDATA", root).args(argv);
    for (k, v) in vars {
        cmd.env(k, v);
    }
    cmd.output().unwrap()
}
#[test]
fn profiles_testing_and_cli_overrides_share_resolution() {
    let e = env();
    let default = Launch::resolve(&[], &e).unwrap();
    assert!(default.base.ends_with("activitywatch/aw-watcher-teams"));
    assert!(!default.testing);
    let test = Launch::resolve(&args(&["--testing", "--init-config"]), &e).unwrap();
    assert!(test
        .base
        .ends_with("activitywatch-testing/aw-watcher-teams"));
    assert!(test.defaults().server.contains(":5666/"));
    let mut e = e;
    e.insert("AW_PROFILE".into(), "research".into());
    e.insert("AW_TESTING".into(), "0".into());
    e.insert("AW_SERVER_URL".into(), "http://127.0.0.1:7123/".into());
    let named = Launch::resolve(&args(&["configure"]), &e).unwrap();
    assert!(named
        .base
        .ends_with("activitywatch-research/aw-watcher-teams"));
    assert!(named.defaults().server.contains(":7123/"));
    let cli = Launch::resolve(
        &args(&[
            "--testing",
            "--server",
            "http://127.0.0.1:7654/",
            "--config",
            "explicit.json",
            "configure",
        ]),
        &e,
    )
    .unwrap();
    assert!(cli.testing);
    assert_eq!(cli.config_path, PathBuf::from("explicit.json"));
    assert!(cli.defaults().server.contains(":7654/"));
}
#[test]
fn malformed_context_unknown_commands_and_action_identity_fail() {
    for profile in ["../bad", "UPPER", "-bad", "bad/path", ""] {
        let mut e = env();
        e.insert("AW_PROFILE".into(), profile.into());
        if !profile.is_empty() {
            assert!(Launch::resolve(&[], &e).is_err());
        }
    }
    for invalid in ["--unknown", "refresh", "pause"] {
        assert!(Launch::resolve(&args(&[invalid]), &env()).is_err());
    }
    let mut e = env();
    e.insert("AW_MODULE_ACTION_PROTOCOL".into(), "2".into());
    e.insert("AW_MODULE_ACTION".into(), "about".into());
    assert!(Launch::resolve(&args(&["about"]), &e).is_err());
    e.insert("AW_MODULE_ACTION_PROTOCOL".into(), "1".into());
    assert!(Launch::resolve(&[], &e).is_err());
    assert!(Launch::resolve(&args(&["configure"]), &e).is_err());
    assert!(Launch::resolve(&args(&["about"]), &e).is_ok());
    e.insert("AW_TESTING".into(), "yes".into());
    assert!(Launch::resolve(&args(&["about"]), &e).is_err());
    e.insert("AW_TESTING".into(), "1".into());
    e.insert("AW_PROFILE".into(), "research".into());
    assert!(Launch::resolve(&args(&["about"]), &e).is_err());
    e.insert("AW_PROFILE".into(), "testing".into());
    assert!(Launch::resolve(&args(&["about"]), &e).unwrap().testing);
    e.insert(
        "AW_SERVER_URL".into(),
        "https://remote.invalid/secret".into(),
    );
    assert!(Launch::resolve(&args(&["about"]), &e).is_err());
}
#[test]
fn all_manifest_actions_dispatch_with_producer_lock_held_and_server_offline() {
    let root = root("dispatch");
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../aw-watcher-teams.exe.actions.json")).unwrap();
    assert_eq!(manifest["schema_version"], 1);
    assert_eq!(manifest["module"], "aw-watcher-teams");
    assert_eq!(manifest["actions"].as_array().unwrap().len(), 4);
    let base = root.join("activitywatch-research/aw-watcher-teams");
    fs::create_dir_all(&base).unwrap();
    let _lock = aw_watcher_teams::lifecycle::acquire_module_lock(&base).unwrap();
    let offline = TcpListener::bind("127.0.0.1:0").unwrap();
    let server = format!("http://{}/", offline.local_addr().unwrap());
    drop(offline);
    for action in manifest["actions"].as_array().unwrap() {
        let id = action["id"].as_str().unwrap();
        let command = action["args"][0].as_str().unwrap();
        let out = run(
            &root,
            &[command, "--no-ui"],
            &[
                ("AW_MODULE_ACTION_PROTOCOL", "1"),
                ("AW_MODULE_ACTION", id),
                ("AW_PROFILE", "research"),
                ("AW_TESTING", "0"),
                ("AW_SERVER_URL", &server),
            ],
        );
        assert!(
            out.status.success(),
            "{id}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let text = String::from_utf8(out.stdout).unwrap();
        if id == "status" {
            let actual: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert_eq!(actual["reachable"], false);
        }
        assert!(
            !base.join("session-delivery.json").exists(),
            "Action started delivery"
        );
    }
    assert!(base.join("config.json").exists());
    assert!(!root.join("activitywatch/aw-watcher-teams").exists());
    let out = run(
        &root,
        &["--profile", "research"],
        &[("AW_SERVER_URL", &server)],
    );
    assert!(
        !out.status.success(),
        "A producer duplicate must fail before collection"
    );
}
#[test]
fn status_checks_actual_info_response_but_never_claims_producer_health() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server = format!("http://{}/", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let host = thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(pair) => break pair,
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        && std::time::Instant::now() < deadline =>
                {
                    thread::sleep(std::time::Duration::from_millis(20))
                }
                Err(e) => panic!("Status test connection did not arrive: {e}"),
            }
        };
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let mut buffer = [0u8; 2048];
        let amount = stream.read(&mut buffer).unwrap();
        assert!(amount > 0);
        let body = r#"{"version":"synthetic","testing":true}"#;
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
    });
    let actual = aw_watcher_teams::actions::server_status(&server, false);
    host.join().unwrap();
    assert!(actual.reachable);
    assert_eq!(actual.profile_testing_matches, Some(false));
    assert!(actual.explanation.contains("does not prove"));
}
#[test]
fn sanitized_logs_reject_arbitrary_text_and_config_metadata() {
    let root = root("logs");
    let base = root.join("activitywatch/aw-watcher-teams");
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("module-log.jsonl"),b"private topic self@example.invalid\n{\"timestamp\":\"2026-10-11T10:00:00Z\",\"code\":\"private message\"}\n").unwrap();
    privacy_log::append(&base, privacy_log::Code::DeliveryPending).unwrap();
    let out = run(&root, &["logs-ui", "--no-ui"], &[]);
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(!text.contains("private"));
    assert!(!text.contains("self@"));
    assert!(text.contains("delivery_pending"));
    let logs: Vec<privacy_log::Record> = serde_json::from_str(&text).unwrap();
    assert_eq!(logs.len(), 1);
}
#[test]
fn manual_settings_alias_and_testing_init_are_isolated_without_server() {
    let root = root("testing");
    let init = run(&root, &["--testing", "--init-config"], &[]);
    assert!(init.status.success());
    let test = Launch::resolve(
        &args(&["--testing"]),
        &BTreeMap::from([("LOCALAPPDATA".into(), root.to_string_lossy().into_owned())]),
    )
    .unwrap();
    assert!(test.config_path.exists());
    assert!(test.config().unwrap().server.contains(":5666/"));
    let out = run(&root, &["--testing", "--settings", "--no-ui"], &[]);
    assert!(out.status.success());
    assert!(!root.join("activitywatch/aw-watcher-teams").exists());
    let unknown = run(&root, &["unsupported-command"], &[]);
    assert!(!unknown.status.success());
    assert!(!test.base.join("session-delivery.json").exists());
    assert!(!test.base.join("module.lock").exists());
}
#[test]
fn manifest_contract_has_only_known_commands_and_build_install_matching_sidecar() {
    let bytes = include_bytes!("../aw-watcher-teams.exe.actions.json");
    assert!(bytes.len() < 65536);
    assert!(!bytes.starts_with(&[0xef, 0xbb, 0xbf]));
    let manifest: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(manifest.as_object().unwrap().len(), 3);
    let mut ids = std::collections::HashSet::new();
    for action in manifest["actions"].as_array().unwrap() {
        let id = action["id"].as_str().unwrap();
        assert!(ids.insert(id));
        let label = action["label"].as_str().unwrap();
        assert!(label.chars().count() <= 100);
        assert!(!label.contains('&'));
        assert!(!label.chars().any(char::is_control));
        let args: Vec<String> = serde_json::from_value(action["args"].clone()).unwrap();
        assert_eq!(
            Launch::resolve(&args, &env()).unwrap().command.action_id(),
            Some(id)
        );
    }
    assert_eq!(
        Launch::resolve(&args(&["--settings"]), &env())
            .unwrap()
            .command,
        Action::Configure
    );
    assert!(include_str!("../scripts/build.ps1").contains("aw-watcher-teams.exe.actions.json"));
    assert!(include_str!("../scripts/install.ps1").contains(".actions.json"));
}

#[cfg(windows)]
#[test]
fn isolated_installer_copies_sidecar_and_removes_it_on_rollback() {
    let root = std::env::temp_dir().join(format!(
        "teams-actions-install-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap()
    ));
    fs::create_dir_all(&root).unwrap();
    let host = root.join("installed-aw");
    let source = root.join("source");
    fs::create_dir_all(&host).unwrap();
    fs::create_dir_all(&source).unwrap();
    fs::write(host.join("aw-qt.exe"), b"synthetic-not-executed").unwrap();
    let exe = source.join("aw-watcher-teams.exe");
    fs::copy(env!("CARGO_BIN_EXE_aw-watcher-teams"), &exe).unwrap();
    let sidecar = source.join("aw-watcher-teams.exe.actions.json");
    fs::write(
        &sidecar,
        include_bytes!("../aw-watcher-teams.exe.actions.json"),
    )
    .unwrap();
    let install = || {
        let mut cmd = Command::new("powershell.exe");
        for name in [
            "AW_MODULE_ACTION_PROTOCOL",
            "AW_MODULE_ACTION",
            "AW_PROFILE",
            "AW_TESTING",
            "AW_SERVER_URL",
        ] {
            cmd.env_remove(name);
        }
        cmd.env("LOCALAPPDATA", &root)
            .env("AW_PROFILE", "testing")
            .env("AW_TESTING", "1")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-File",
                "scripts/install.ps1",
                "-NoAutostart",
                "-ActivityWatchDirectory",
            ])
            .arg(&host)
            .arg("-Executable")
            .arg(&exe);
        let out = cmd.output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    install();
    let installed = host.join("aw-watcher-teams");
    assert_eq!(
        fs::read(installed.join("aw-watcher-teams.exe.actions.json")).unwrap(),
        include_bytes!("../aw-watcher-teams.exe.actions.json")
    );
    assert!(installed.join("Settings.lnk").exists());
    assert!(root
        .join("activitywatch-testing/aw-watcher-teams/config.json")
        .exists());
    assert!(!root.join("activitywatch/aw-watcher-teams").exists());
    fs::remove_file(sidecar).unwrap();
    install();
    assert!(!installed.join("aw-watcher-teams.exe.actions.json").exists());
    assert!(!installed.join("Settings.lnk").exists());
    assert!(installed
        .join("aw-watcher-teams.exe.actions.json.before-teams")
        .exists());
}
