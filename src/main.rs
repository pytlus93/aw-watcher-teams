use aw_watcher_teams::{bounded_duration, classify, validate_server, Snapshot};
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
}
fn collect(script: &str, timeout: u64) -> Snapshot {
    let result = (|| -> Result<Snapshot, Box<dyn std::error::Error>> {
        let mut command = Command::new("powershell.exe");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        let mut child = command
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
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
                return Err("timeout".into());
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
    let base = PathBuf::from(std::env::var("LOCALAPPDATA")?)
        .join("activitywatch")
        .join("aw-watcher-teams");
    fs::create_dir_all(&base)?;
    let config_path = base.join("config.json");
    if std::env::args().any(|a| a == "--init-config") {
        if !config_path.exists() {
            fs::write(
                &config_path,
                r#"{"server":"http://127.0.0.1:5600/","interval_seconds":5,"collector_timeout_seconds":3}"#,
            )?;
        }
        println!("{}", config_path.display());
        return Ok(());
    }
    let config: Config = serde_json::from_slice(&fs::read(&config_path)?)?;
    if !(2..=60).contains(&config.interval_seconds)
        || !(1..=10).contains(&config.collector_timeout_seconds)
    {
        return Err("invalid timing settings".into());
    }
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
    let bucket = format!("aw-watcher-teams_{}", host);
    let bucket_url = server.join(&format!("api/0/buckets/{bucket}"))?;
    // Only this module's bucket. Repeated creation is allowed by AW.
    loop {
        if agent
            .post(bucket_url.as_str())
            .send_json(serde_json::json!({
            "client":"aw-watcher-teams","type":"call-evidence","hostname":host,"name":"MS Teams"
            }))
            .is_ok()
        {
            break;
        }
        eprintln!("AW unavailable; retrying bucket creation");
        thread::sleep(Duration::from_secs(5));
    }
    let mut previous = None;
    let mut previous_at = chrono::Utc::now();
    loop {
        let observation = classify(&collect(
            include_str!("../scripts/snapshot.ps1"),
            config.collector_timeout_seconds,
        ));
        let now = chrono::Utc::now();
        let elapsed = (now - previous_at).num_milliseconds() as f64 / 1000.0;
        let duration = bounded_duration(
            previous.as_ref(),
            &observation,
            elapsed,
            (config.interval_seconds + config.collector_timeout_seconds + 4) as f64,
        );
        let timestamp = if duration > 0.0 {
            previous_at.to_rfc3339()
        } else {
            now.to_rfc3339()
        };
        // Zero pulse prevents heartbeat bridging across sleeps, hangs and failed deliveries.
        let heartbeat = format!(
            "{}/heartbeat?pulsetime=0",
            bucket_url.as_str().trim_end_matches('/')
        );
        let result = agent.post(&heartbeat).send_json(serde_json::json!({
        "timestamp":timestamp,"duration":duration,"data":observation
        }));
        if result.is_err() {
            eprintln!("AW delivery failed; sample not counted");
            previous = None;
        } else {
            previous = Some(observation);
        }
        previous_at = now;
        thread::sleep(Duration::from_secs(config.interval_seconds));
    }
}

#[cfg(all(test, windows))]
mod collector_tests {
    use super::*;
    #[test]
    fn synthetic_collector_protocol() {
        let s=collect("Write-Output '{\"collector_ok\":true,\"windows\":[{\"leave\":true,\"microphone\":true,\"camera\":true,\"join\":false,\"sharing\":false}]}'", 5);
        assert_eq!(classify(&s).state, "joined_candidate");
    }
    #[test]
    fn collector_timeout_becomes_unknown() {
        let s = collect("Start-Sleep -Seconds 20", 1);
        assert!(!s.collector_ok);
        assert_eq!(classify(&s).state, "unknown");
    }
}
