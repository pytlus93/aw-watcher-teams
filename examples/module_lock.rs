//! Development-only lock/lifecycle probe; never deployed as an independent watcher.
use std::{
    io::{self, Write},
    path::PathBuf,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let base = PathBuf::from(std::env::args().nth(1).ok_or("State directory required")?);
    let _lock = aw_watcher_teams::lifecycle::acquire_module_lock(&base)?;
    println!("LOCKED");
    io::stdout().flush()?;
    let mut command = String::new();
    io::stdin().read_line(&mut command)?;
    Ok(())
}
