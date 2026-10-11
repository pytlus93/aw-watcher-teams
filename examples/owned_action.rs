//! Development-only probe for management-child lifetime under a killed action.
use std::{
    io::{self, Write},
    process::{Command, Stdio},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("PID file required")?;
    let mut command = Command::new("powershell.exe");
    command.env("AW_TEAMS_SYNTHETIC_PID_PATH",path).args(["-NoProfile","-NonInteractive","-Command","[IO.File]::WriteAllText($env:AW_TEAMS_SYNTHETIC_PID_PATH,[string]$PID); Start-Sleep -Seconds 120"]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    let _child = aw_watcher_teams::owned_process::OwnedChild::spawn(&mut command)?;
    println!("OWNED");
    io::stdout().flush()?;
    let mut text = String::new();
    io::stdin().read_line(&mut text)?;
    Ok(())
}
