use crate::{
    context::{Command as Action, Launch},
    owned_process::OwnedChild,
    privacy_log,
};
use serde::Serialize;
use std::{
    process::{Command, Stdio},
    time::Duration,
};
#[derive(Debug, Serialize)]
pub struct ServerStatus {
    pub reachable: bool,
    pub profile_testing_matches: Option<bool>,
    pub explanation: &'static str,
}
pub fn server_status(server: &str, testing: bool) -> ServerStatus {
    let result = (|| -> Result<Option<bool>, Box<dyn std::error::Error>> {
        let server = crate::validate_server(server)?;
        let reply = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(2))
            .redirects(0)
            .build()
            .get(server.join("api/0/info")?.as_str())
            .call()?;
        if reply.status() != 200 {
            return Err("Unexpected server status".into());
        }
        let info: serde_json::Value = reply.into_json()?;
        if !info["version"].is_string() {
            return Err("Invalid AW server info".into());
        }
        Ok(info["testing"].as_bool().map(|value| value == testing))
    })();
    match result {
  Ok(matches)=>ServerStatus{reachable:true,profile_testing_matches:matches,explanation:"AW server responded. This does not prove the producer is running, delivering events, or attending a call."},
  Err(_)=>ServerStatus{reachable:false,profile_testing_matches:None,explanation:"AW server is unavailable or returned invalid information. No connection success is claimed."},
 }
}
#[cfg(windows)]
fn dialog(title: &str, message: &str) -> Result<(), Box<dyn std::error::Error>> {
    #[link(name = "user32")]
    extern "system" {
        fn MessageBoxW(
            window: *mut std::ffi::c_void,
            text: *const u16,
            caption: *const u16,
            kind: u32,
        ) -> i32;
    }
    let title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
    let message: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
    if unsafe { MessageBoxW(std::ptr::null_mut(), message.as_ptr(), title.as_ptr(), 0x40) } == 0 {
        return Err("Action dialog failed".into());
    }
    Ok(())
}
#[cfg(not(windows))]
fn dialog(_title: &str, _message: &str) -> Result<(), Box<dyn std::error::Error>> {
    Err("Management UI requires Windows".into())
}
pub fn dispatch(context: &Launch) -> Result<(), Box<dyn std::error::Error>> {
    match context.command {
        Action::Configure => {
            context.ensure_config()?;
            context.config()?;
            if context.no_ui {
                println!(
                    "{}",
                    serde_json::json!({"action":"configure","profile":context.profile,"testing":context.testing,"config_available":true,"server_contacted":false})
                );
            } else {
                let mut command = Command::new("powershell.exe");
                command
                    .env("AW_TEAMS_SETTINGS_EXE", std::env::current_exe()?)
                    .env("AW_TEAMS_CONFIG_PATH", &context.config_path)
                    .env("AW_TEAMS_SETTINGS_PROFILE", &context.profile)
                    .args([
                        "-NoProfile",
                        "-STA",
                        "-Command",
                        include_str!("../scripts/settings.ps1"),
                    ])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                if !OwnedChild::spawn(&mut command)?.wait()?.success() {
                    return Err("Settings UI failed".into());
                }
            }
        }
        Action::Logs => {
            let records = privacy_log::read(&context.base)?;
            if context.no_ui {
                println!("{}", serde_json::to_string(&records)?);
            } else {
                let text = if records.is_empty() {
                    "No module operational log entries are available.".to_owned()
                } else {
                    records
                        .iter()
                        .rev()
                        .take(50)
                        .rev()
                        .map(|r| format!("{} {:?}\r\n", r.timestamp, r.code))
                        .collect()
                };
                dialog("Teams module operational log", &text)?;
            }
        }
        Action::Status => {
            // Status is read-only: resolve defaults when not initialized, without creating files.
            let config = if context.config_path.exists() {
                context.config()?
            } else {
                context.defaults()
            };
            let status = server_status(&config.server, context.testing);
            if context.no_ui {
                println!("{}", serde_json::to_string(&status)?);
            } else {
                let text=format!("Profile: {}\r\nServer reachable: {}\r\nTesting profile matches: {:?}\r\n\r\n{}",context.profile,status.reachable,status.profile_testing_matches,status.explanation);
                dialog("Teams module server reachability", &text)?;
            }
        }
        Action::About => {
            let text=format!("MS Teams ActivityWatch module {}\r\nTray actions protocol 1.\r\nLocal UI-inferred call evidence; no provider attendance or foreground-work claim.\r\nAW owns producer startup/stop. Actions do not start a producer.",env!("CARGO_PKG_VERSION"));
            if context.no_ui {
                println!("{text}");
            } else {
                dialog("About Teams module", &text)?;
            }
        }
        _ => return Err("Unsupported management command".into()),
    }
    Ok(())
}
