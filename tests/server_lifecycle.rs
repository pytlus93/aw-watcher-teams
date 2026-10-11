use aw_watcher_teams::{
    aw::{event_data, AwWriter},
    classify, SessionEvent, Snapshot, Window,
};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
struct Host {
    child: Option<Child>,
    root: PathBuf,
    port: u16,
    exe: String,
    kind: String,
}
impl Host {
    fn start(&mut self) {
        let mut cmd = Command::new(&self.exe);
        cmd.args([
            "--testing",
            "--host",
            "127.0.0.1",
            "--port",
            &self.port.to_string(),
        ]);
        if self.kind == "python" {
            cmd.args(["--storage", "memory"]);
        } else {
            cmd.arg("--dbpath")
                .arg(self.root.join("server.db"))
                .arg("--no-legacy-import");
        }
        cmd.env("LOCALAPPDATA", self.root.join("appdata"))
            .env("APPDATA", self.root.join("appdata"))
            .stdout(Stdio::from(
                fs::File::create(self.root.join("server.out")).unwrap(),
            ))
            .stderr(Stdio::from(
                fs::File::create(self.root.join("server.err")).unwrap(),
            ));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        self.child = Some(cmd.spawn().unwrap());
        let until = Instant::now() + Duration::from_secs(25);
        while Instant::now() < until {
            if ureq::get(&format!("http://127.0.0.1:{}/api/0/info", self.port))
                .timeout(Duration::from_millis(250))
                .call()
                .is_ok()
            {
                return;
            }
            assert!(
                self.child.as_mut().unwrap().try_wait().unwrap().is_none(),
                "Isolated server exited; inspect local test logs"
            );
            thread::sleep(Duration::from_millis(100));
        }
        panic!("Isolated server startup timeout");
    }
    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    fn url(&self) -> String {
        format!(
            "http://127.0.0.1:{}/api/0/buckets/synthetic-teams",
            self.port
        )
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        self.stop();
    }
}
fn records(url: &str) -> Vec<Value> {
    ureq::get(&format!("{url}/events?limit=-1"))
        .call()
        .unwrap()
        .into_json()
        .unwrap()
}
fn event(session: &str, start: i64, seconds: f64) -> SessionEvent {
    SessionEvent {
        timestamp: DateTime::from_timestamp(start, 123_456_000).unwrap(),
        duration: seconds,
        data: event_data(
            classify(
                &Snapshot {
                    collector_ok: true,
                    windows: vec![Window {
                        key: "synthetic".into(),
                        leave: true,
                        microphone: true,
                        title: Some("Synthetic review | Microsoft Teams".into()),
                        ..Default::default()
                    }],
                },
                true,
            ),
            session.into(),
        ),
    }
}
#[test]
#[ignore = "Explicit AW_TEST_SERVER_EXE and AW_TEST_SERVER_KIND; starts/stops only owned isolated server"]
fn isolated_server_lifecycle() {
    let kind = std::env::var("AW_TEST_SERVER_KIND").unwrap();
    assert!(kind == "python" || kind == "rust");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let root = PathBuf::from("local").join(format!(
        "real-{kind}-{}-{}",
        std::process::id(),
        Utc::now().timestamp_millis()
    ));
    fs::create_dir_all(root.join("appdata")).unwrap();
    let mut host = Host {
        child: None,
        root: root.clone(),
        port,
        exe: std::env::var("AW_TEST_SERVER_EXE").unwrap(),
        kind,
    };
    let url = host.url();
    let checkpoint = root.join("queue.json");
    let mut writer = AwWriter::new(url.clone(), checkpoint.clone()).unwrap();
    let mut one = event("one", 1_700_000_000, 5.0);
    writer.queue(one.clone()).unwrap();
    assert!(writer.flush().is_err());
    host.start();
    writer.flush().unwrap();
    let first = records(&url);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0]["duration"], 5.0);
    let id = first[0]["id"].clone();
    assert_eq!(
        ureq::post(&url)
            .send_json(json!({"client":"synthetic","type":"call-evidence","hostname":"synthetic"}))
            .unwrap()
            .status(),
        304
    );
    one.duration = 15.0;
    writer.queue(one.clone()).unwrap();
    writer.flush().unwrap();
    let second = records(&url);
    assert_eq!(
        second.len(),
        1,
        "Server must update one interval rather than duplicate it"
    );
    assert_eq!(second[0]["id"], id);
    assert_eq!(second[0]["duration"], 15.0);
    drop(writer);
    let mut writer = AwWriter::new(url.clone(), checkpoint.clone()).unwrap();
    one.duration = 20.0;
    writer.queue(one.clone()).unwrap();
    writer.flush().unwrap();
    assert_eq!(records(&url)[0]["duration"], 20.0);
    host.stop();
    one.duration = 25.0;
    writer.queue(one.clone()).unwrap();
    assert!(writer.flush().is_err());
    host.start();
    writer.flush().unwrap();
    let resumed = records(&url);
    assert_eq!(resumed.len(), 1);
    assert_eq!(resumed[0]["duration"], 25.0);
    let actual: DateTime<Utc> = resumed[0]["timestamp"].as_str().unwrap().parse().unwrap();
    assert_eq!(actual.timestamp_millis(), one.timestamp.timestamp_millis());
    // A collector gap is represented by a second interval, never an extension across it.
    writer.queue(event("two", 1_700_000_100, 5.0)).unwrap();
    writer.flush().unwrap();
    let records = records(&url);
    assert_eq!(records.len(), 2);
    let sum: f64 = records
        .iter()
        .map(|e| e["duration"].as_f64().unwrap())
        .sum();
    assert_eq!(sum, 30.0);
    let export: Value = ureq::get(&format!("{url}/export"))
        .call()
        .unwrap()
        .into_json()
        .unwrap();
    assert_eq!(
        export["buckets"]["synthetic-teams"]["events"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    if host.kind == "python" {
        let output = Command::new("cargo")
            .env(
                "AW_PROTOCOL_SERVER",
                format!("http://127.0.0.1:{}/", host.port),
            )
            .args([
                "test",
                "--offline",
                "--locked",
                "--lib",
                "installed_aw_updates_one_session_and_recovers_lost_ack",
                "--",
                "--ignored",
                "--nocapture",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "Legacy compatibility fixture failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        println!(
            "PASS existing opt-in protocol/privacy migration fixture on isolated Python server"
        );
    }
    // Verify actual stored/API/export properties, not only request serialization.
    let mut payload = aw_watcher_teams::payload::PayloadConfig::extended();
    payload.fields.account = true;
    let mut format = aw_watcher_teams::title::TitleFormat {
        include_account: true,
        ..Default::default()
    };
    format
        .organization_map
        .insert("Example Ltd.".into(), "Example".into());
    writer
        .configure_payload(payload.clone(), true, format.clone())
        .unwrap();
    let mut private = event("two", 1_700_000_100, 5.0);
    private.data.activity = aw_watcher_teams::classify_with_format(
        &Snapshot {
            collector_ok: true,
            windows: vec![Window {
                key: "synthetic".into(),
                leave: true,
                microphone: true,
                title: Some(
                    "Review | Example Ltd. | self@example.invalid | Microsoft Teams".into(),
                ),
                ..Default::default()
            }],
        },
        true,
        &format,
    );
    writer.queue(private).unwrap();
    writer.flush().unwrap();
    let response = ureq::get(&format!("{url}/events?limit=-1"))
        .call()
        .unwrap()
        .into_string()
        .unwrap();
    let records: Vec<Value> = serde_json::from_str(&response).unwrap();
    let data = records
        .iter()
        .find(|e| e["data"]["meeting_title"] == "Review")
        .unwrap()["data"]
        .clone();
    assert_eq!(data["title"], "Review | Example | self@example.invalid");
    assert_eq!(data["organization"], "Example Ltd.");
    assert_eq!(data["account"], "self@example.invalid");
    let title_first =
        response.find("\"title\"").unwrap() < response.find("\"schema_version\"").unwrap();
    println!(
        "API serialized title before schema_version: {title_first} (not a GUI ordering guarantee)"
    );
    payload.fields.organization = false;
    payload.fields.account = false;
    writer.configure_payload(payload, true, format).unwrap();
    writer.flush().unwrap();
    let export: Value = ureq::get(&format!("{url}/export"))
        .call()
        .unwrap()
        .into_json()
        .unwrap();
    let latest = export["buckets"]["synthetic-teams"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["data"]["meeting_title"] == "Review")
        .unwrap();
    assert_eq!(latest["data"]["title"], "Review");
    assert!(latest["data"].get("organization").is_none());
    assert!(latest["data"].get("account").is_none());
    assert!(!export.to_string().contains("self@"));
    assert!(!export.to_string().contains("Example"));
    println!("PASS isolated {} server: late start, creation/304, same-ID updates, producer restart, outage/restart, timestamps/gaps/export",host.kind);
}
