use aw_watcher_teams::{aw::AwWriter, classify, validate_server, Snapshot, Tracker};
use aw_watcher_teams::{
    context::{Command as LaunchCommand, Launch},
    privacy_log::{self, Code},
};
use std::{
    fs,
    io::Read,
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
        command
            .args(["-NoProfile", "-NonInteractive", "-Command", &expression])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = aw_watcher_teams::owned_process::OwnedChild::spawn(&mut command)?;
        let stdout = child
            .child_mut()
            .stdout
            .take()
            .ok_or("Collector stdout unavailable")?;
        let reader = thread::spawn(move || {
            let mut bytes = Vec::new();
            stdout
                .take(1_048_577)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
        });
        let start = Instant::now();
        loop {
            if let Some(status) = child.child_mut().try_wait()? {
                child.wait()?;
                drop(child);
                let output = reader.join().map_err(|_| "Collector reader failed")??;
                if !status.success() || output.len() > 1_048_576 {
                    return Err("collector failed".into());
                }
                return Ok(serde_json::from_slice(&output)?);
            }
            if start.elapsed() > Duration::from_secs(timeout) {
                child.child_mut().kill()?;
                child.wait()?;
                drop(child);
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
    let launch = Launch::current()?;
    match launch.command {
        LaunchCommand::Configure
        | LaunchCommand::Logs
        | LaunchCommand::Status
        | LaunchCommand::About => return aw_watcher_teams::actions::dispatch(&launch),
        LaunchCommand::Version => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        LaunchCommand::Help => {
            println!("aw-watcher-teams [--testing | --profile NAME] [--server URL] [--port PORT] [--config PATH]\nconfigure | --settings | logs-ui | status-ui | about [--no-ui]\n--init-config | --check-config PATH | --version");
            return Ok(());
        }
        LaunchCommand::Init => {
            launch.ensure_config()?;
            println!("{}", launch.config_path.display());
            return Ok(());
        }
        LaunchCommand::Check => {
            launch.config()?;
            return Ok(());
        }
        LaunchCommand::Run => {}
    }
    let config = launch.config()?;
    fs::create_dir_all(&launch.base)?;
    let base = &launch.base;
    let collect_title = config.collect_title();
    let title_format = config.effective_title_format();
    let server = validate_server(&config.server)?;
    let _lock = aw_watcher_teams::lifecycle::acquire_module_lock(base)?;
    let _ = privacy_log::append(base, Code::Started);
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
    let mut delivery_pending = None;
    loop {
        let snapshot = collect(
            include_str!("../scripts/snapshot.ps1"),
            config.collector_timeout_seconds,
            collect_title,
        );
        let activity = classify(&snapshot, collect_title);
        let status = format!("{}:{}", activity.state, activity.evidence);
        if status != last_status {
            eprintln!("Collector state: {status}");
            let _ = privacy_log::append(base, Code::observation(&activity));
            last_status = status;
        }
        if let Some(event) = tracker.observe(&snapshot, chrono::Utc::now()) {
            writer.queue(event)?;
        }
        let pending = writer.flush().is_err();
        if delivery_pending != Some(pending) {
            let _ = privacy_log::append(
                base,
                if pending {
                    Code::DeliveryPending
                } else {
                    Code::DeliveryAvailable
                },
            );
            if pending {
                eprintln!("AW delivery pending; retrying saved sessions");
            }
            delivery_pending = Some(pending);
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
