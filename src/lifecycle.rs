use fs2::FileExt;
use std::{
    fs::{File, OpenOptions},
    path::Path,
};
/// The AW-hosted worker and isolated lifecycle harness use this same OS lock.
/// No stale PID file is trusted; the OS releases the lock on unexpected exit.
pub fn acquire_module_lock(base: &Path) -> std::io::Result<File> {
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(base.join("module.lock"))?;
    lock.try_lock_exclusive()?;
    Ok(lock)
}
