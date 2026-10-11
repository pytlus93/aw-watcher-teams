use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::Duration,
};
fn spawn(exe: &str, dir: &PathBuf) -> Child {
    let mut cmd = Command::new(exe);
    cmd.arg(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd.spawn().unwrap()
}
fn ready(child: &mut Child) {
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert_eq!(line.trim(), "LOCKED");
}
#[test]
#[ignore = "Set AW_TEAMS_LOCK_PROBE to built development-only example"]
fn isolated_crash_failed_relaunch_stop_and_duplicate_lock() {
    let exe = std::env::var("AW_TEAMS_LOCK_PROBE").unwrap();
    let dir = std::env::temp_dir().join(format!(
        "teams-lock-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap()
    ));
    fs::create_dir_all(&dir).unwrap();
    let mut first = spawn(&exe, &dir);
    ready(&mut first);
    let mut duplicate = spawn(&exe, &dir);
    assert!(!duplicate.wait().unwrap().success());
    first.kill().unwrap();
    first.wait().unwrap();
    let mut after_crash = spawn(&exe, &dir);
    ready(&mut after_crash);
    after_crash
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"stop\n")
        .unwrap();
    assert!(after_crash.wait().unwrap().success());
    // Relaunch errors remain visible, and successful later launches can reacquire.
    let blocker = dir.join("not-a-directory");
    fs::write(&blocker, b"synthetic").unwrap();
    let mut failed = spawn(&exe, &blocker);
    assert!(!failed.wait().unwrap().success());
    thread::sleep(Duration::from_millis(50));
    let mut manual = spawn(&exe, &dir);
    ready(&mut manual);
    manual.stdin.as_mut().unwrap().write_all(b"stop\n").unwrap();
    assert!(manual.wait().unwrap().success());
    fs::remove_dir_all(dir).unwrap();
    println!("PASS shared worker OS lock: duplicate rejection, crash release, intentional stop, failed/later relaunch");
}
