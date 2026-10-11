#![cfg(windows)]
use std::{
    fs,
    io::{BufRead, BufReader},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
#[test]
#[ignore = "Set AW_TEAMS_OWNED_ACTION_PROBE to built development-only example"]
fn killed_action_terminates_owned_powershell_child() {
    let path = std::env::current_dir().unwrap().join("local").join(format!(
        "action-child-{}-{}.pid",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap()
    ));
    let mut command = Command::new(std::env::var("AW_TEAMS_OWNED_ACTION_PROBE").unwrap());
    use std::os::windows::process::CommandExt;
    command
        .creation_flags(0x08000000)
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut owner = command.spawn().unwrap();
    let mut ready = String::new();
    BufReader::new(owner.stdout.take().unwrap())
        .read_line(&mut ready)
        .unwrap();
    assert_eq!(ready.trim(), "OWNED");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !path.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(50));
    }
    let pid: u32 = fs::read_to_string(&path).unwrap().parse().unwrap();
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut std::ffi::c_void;
        fn WaitForSingleObject(handle: *mut std::ffi::c_void, milliseconds: u32) -> u32;
        fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
    }
    let handle = unsafe { OpenProcess(0x00100000, 0, pid) };
    assert!(!handle.is_null(), "Owned test helper must be running");
    owner.kill().unwrap();
    owner.wait().unwrap();
    let exit = unsafe { WaitForSingleObject(handle, 5000) };
    unsafe {
        CloseHandle(handle);
    }
    assert_eq!(
        exit, 0,
        "Killing action owner must terminate child through its Windows job"
    );
    fs::remove_file(path).unwrap();
}
