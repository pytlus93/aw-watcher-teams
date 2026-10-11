use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Code {
    Started,
    Unknown,
    Prejoin,
    Ambiguous,
    JoinedCandidate,
    DeliveryPending,
    DeliveryAvailable,
    ManagementAction,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub timestamp: DateTime<Utc>,
    pub code: Code,
}
pub fn append(base: &Path, code: Code) -> std::io::Result<()> {
    fs::create_dir_all(base)?;
    let path = base.join("module-log.jsonl");
    if fs::metadata(&path).is_ok_and(|m| m.len() > 32768) {
        let older = base.join("module-log.1.jsonl");
        if older.exists() {
            fs::remove_file(&older)?;
        }
        fs::rename(&path, older)?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    serde_json::to_writer(
        &mut file,
        &Record {
            timestamp: Utc::now(),
            code,
        },
    )?;
    file.write_all(b"\n")?;
    Ok(())
}
pub fn read(base: &Path) -> std::io::Result<Vec<Record>> {
    let path = base.join("module-log.jsonl");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut data = Vec::new();
    fs::File::open(path)?.take(65537).read_to_end(&mut data)?;
    if data.len() > 65536 {
        return Err(std::io::Error::other("Log size exceeds limit"));
    }
    Ok(data
        .split(|b| *b == b'\n')
        .filter_map(|line| serde_json::from_slice::<Record>(line).ok())
        .collect())
}
