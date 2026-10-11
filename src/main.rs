use aw_watcher_teams::config::Config;
use aw_watcher_teams::{aw::AwWriter, classify, validate_server, Snapshot, Tracker};
use std::{
    fs,
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
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
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdout = child.stdout.take().ok_or("Collector stdout unavailable")?;
        let reader = thread::spawn(move || {
            let mut bytes = Vec::new();
            stdout
                .take(1_048_577)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
        });
        let start = Instant::now();
        loop {
            if let Some(status) = child.try_wait()? {
                child.wait()?;
                let output = reader.join().map_err(|_| "Collector reader failed")??;
                if !status.success() || output.len() > 1_048_576 {
                    return Err("collector failed".into());
                }
                return Ok(serde_json::from_slice(&output)?);
            }
            if start.elapsed() > Duration::from_secs(timeout) {
                child.kill()?;
                child.wait()?;
                let _ = reader.join();
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
    let args: Vec<String> = std::env::args().collect();
    if let Some(index) = args.iter().position(|a| a == "--check-config") {
        let path = args.get(index + 1).ok_or("Config path required")?;
        let config: Config = serde_json::from_slice(&fs::read(path)?)?;
        config.validate()?;
        return Ok(());
    }
    let base = PathBuf::from(std::env::var("LOCALAPPDATA")?)
        .join("activitywatch")
        .join("aw-watcher-teams");
    fs::create_dir_all(&base)?;
    let config_path = base.join("config.json");
    if std::env::args().any(|a| a == "--init-config") {
        if !config_path.exists() {
            fs::write(&config_path, serde_json::to_vec_pretty(&Config::default())?)?;
        }
        println!("{}", config_path.display());
        return Ok(());
    }
    if std::env::args().any(|a| a == "--settings") {
        let status = Command::new("powershell.exe")
            .env("AW_TEAMS_SETTINGS_EXE", std::env::current_exe()?)
            .args([
                "-NoProfile",
                "-STA",
                "-Command",
                include_str!("../scripts/settings.ps1"),
            ])
            .status()?;
        if !status.success() {
            return Err("Settings UI failed".into());
        }
        return Ok(());
    }
    let config: Config = serde_json::from_slice(&fs::read(&config_path)?)?;
    config.validate()?;
    let collect_title = config.collect_title();
    let title_format = config.effective_title_format();
    let server = validate_server(&config.server)?;
    let _lock = aw_watcher_teams::lifecycle::acquire_module_lock(&base)?;
    let host = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "local".into());
    let bucket = format!("aw-watcher-teams_{host}");
    let bucket_url = server.join(&format!("api/0/buckets/{bucket}"))?;
    // Collection starts immediately, even before AW is available.
    // AwWriter owns idempotent bucket initialization and recovery.
    let mut writer = AwWriter::new(bucket_url.to_string(), base.join("session-delivery.json"))?;
    writer.configure_payload(config.payload.clone(), collect_title, title_format.clone())?;
    let namespace = format!(
        "{}-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        std::process::id()
    );
    let mut tracker = Tracker::new(
        namespace,
        (config.interval_seconds + config.collector_timeout_seconds + 5) as f64,
        collect_title,
    )
    .with_title_format(title_format.clone());
    let mut last_status = String::new();
    loop {
        let snapshot = collect(
            include_str!("../scripts/snapshot.ps1"),
            config.collector_timeout_seconds,
            collect_title,
        );
        let status = classify(&snapshot, collect_title).state;
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
    fn large_snapshot_does_not_block_on_full_stdout_pipe() {
        let s=collect("@{collector_ok=$true;windows=@(1..500|ForEach-Object{@{key=('synthetic-'+$_);leave=$false;microphone=$false}})}|ConvertTo-Json -Depth 4 -Compress", 5, false);
        assert!(s.collector_ok);
        assert_eq!(s.windows.len(), 500);
    }
    #[test]
    fn collector_timeout_becomes_unknown() {
        assert!(!collect("Start-Sleep -Seconds 20", 1, false).collector_ok);
    }
}
