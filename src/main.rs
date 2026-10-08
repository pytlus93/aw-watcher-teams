use aw_watcher_teams::title::TitleFormat;
use aw_watcher_teams::{aw::AwWriter, classify, validate_server, Snapshot, Tracker};
use fs2::FileExt;
use serde::Deserialize;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    server: String,
    interval_seconds: u64,
    collector_timeout_seconds: u64,
    #[serde(default)]
    collect_meeting_title: bool,
    #[serde(default)]
    title_format: TitleFormat,
}
fn collect(script: &str, timeout: u64, collect_title: bool) -> Snapshot {
    let result = (|| -> Result<Snapshot, Box<dyn std::error::Error>> {
        let mut command = Command::new("powershell.exe");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let expression = if collect_title {
            format!("& {{\n{script}\n}} -CollectMeetingTitle")
        } else {
            script.to_owned()
        };
        let mut child = command
            .args(["-NoProfile", "-NonInteractive", "-Command", &expression])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let start = Instant::now();
        loop {
            if let Some(status) = child.try_wait()? {
                let output = child.wait_with_output()?;
                if !status.success() {
                    return Err("collector failed".into());
                }
                return Ok(serde_json::from_slice(&output.stdout)?);
            }
            if start.elapsed() > Duration::from_secs(timeout) {
                child.kill()?;
                child.wait()?;
                return Err("collector timeout".into());
            }
            thread::sleep(Duration::from_millis(50));
        }
    })();
    result.unwrap_or(Snapshot {
        collector_ok: false,
        windows: vec![],
    })
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let profile = std::env::var("AW_PROFILE").unwrap_or_default();
    if !profile.is_empty() && profile != "default" {
        return Err("Custom AW profiles unsupported".into());
    }
    let base = PathBuf::from(std::env::var("LOCALAPPDATA")?)
        .join("activitywatch")
        .join("aw-watcher-teams");
    fs::create_dir_all(&base)?;
    let config_path = base.join("config.json");
    if std::env::args().any(|a| a == "--init-config") {
        if !config_path.exists() {
            fs::write(
                &config_path,
                r#"{"server":"http://127.0.0.1:5600/","interval_seconds":5,"collector_timeout_seconds":8,"collect_meeting_title":false,"title_format":{"include_organization":true,"include_account":false,"organization_map":{},"separator":" | "}}"#,
            )?;
        }
        println!("{}", config_path.display());
        return Ok(());
    }
    let config: Config = serde_json::from_slice(&fs::read(&config_path)?)?;
    if !(2..=60).contains(&config.interval_seconds)
        || !(1..=15).contains(&config.collector_timeout_seconds)
    {
        return Err("Invalid timing settings".into());
    }
    config.title_format.validate()?;
    let server = validate_server(&config.server)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(base.join("module.lock"))?;
    lock.try_lock_exclusive()?;
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(3))
        .redirects(0)
        .build();
    let host = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "local".into());
    let bucket = format!("aw-watcher-teams_{host}");
    let bucket_url = server.join(&format!("api/0/buckets/{bucket}"))?;
    loop {
        match agent
            .post(bucket_url.as_str())
            .send_json(serde_json::json!({
                "client":"aw-watcher-teams","type":"call-evidence","hostname":host
            })) {
            Ok(_) => break,
            Err(ureq::Error::Status(304, _)) => break,
            Err(_) => {
                eprintln!("AW unavailable; retrying bucket creation");
                thread::sleep(Duration::from_secs(5));
            }
        }
    }
    let mut writer = AwWriter::new(bucket_url.to_string(), base.join("session-delivery.json"))?;
    writer.reformat_pending_titles(config.collect_meeting_title, &config.title_format)?;
    let namespace = format!(
        "{}-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        std::process::id()
    );
    let mut tracker = Tracker::new(
        namespace,
        (config.interval_seconds + config.collector_timeout_seconds + 5) as f64,
        config.collect_meeting_title,
    )
    .with_title_format(config.title_format.clone());
    let mut last_status = String::new();
    loop {
        let snapshot = collect(
            include_str!("../scripts/snapshot.ps1"),
            config.collector_timeout_seconds,
            config.collect_meeting_title,
        );
        let status = classify(&snapshot, config.collect_meeting_title).state;
        if status != last_status {
            eprintln!("Collector state: {status}");
            last_status = status;
        }
        if let Some(event) = tracker.observe(&snapshot, chrono::Utc::now()) {
            writer.queue(event)?;
        }
        if writer.flush().is_err() {
            eprintln!("AW delivery pending; retrying saved sessions");
        }
        thread::sleep(Duration::from_secs(config.interval_seconds));
    }
}
#[cfg(all(test, windows))]
mod collector_tests {
    use super::*;
    #[test]
    fn synthetic_collector_protocol() {
        let s=collect("Write-Output '{\"collector_ok\":true,\"windows\":[{\"key\":\"test-window\",\"leave\":true,\"microphone\":true,\"camera\":true}]}'",5,false);
        assert_eq!(classify(&s, true).state, "joined_candidate");
    }
    #[test]
    fn title_switch_protocol() {
        let s=collect("param([switch]$CollectMeetingTitle) @{collector_ok=$true;windows=@(@{key='test';leave=$true;microphone=$true;title=$(if($CollectMeetingTitle){'Synthetic review'}else{$null})})}|ConvertTo-Json -Depth 4 -Compress",5,true);
        assert_eq!(classify(&s, true).title, "Synthetic review");
    }
    #[test]
    fn collector_timeout_becomes_unknown() {
        assert!(!collect("Start-Sleep -Seconds 20", 1, false).collector_ok);
    }
}
