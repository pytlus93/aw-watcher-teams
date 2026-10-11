use crate::{payload::PayloadConfig, title::TitleFormat};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub server: String,
    pub interval_seconds: u64,
    pub collector_timeout_seconds: u64,
    #[serde(default)]
    pub collect_meeting_title: bool,
    #[serde(default)]
    pub title_format: TitleFormat,
    #[serde(default)]
    pub payload: PayloadConfig,
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        crate::validate_server(&self.server)?;
        if !(2..=60).contains(&self.interval_seconds)
            || !(1..=15).contains(&self.collector_timeout_seconds)
        {
            return Err("Invalid timing settings".into());
        }
        self.title_format.validate()
    }
    pub fn effective_title_format(&self) -> TitleFormat {
        self.payload.title_format(&self.title_format)
    }
    pub fn collect_title(&self) -> bool {
        self.payload.collect_title(self.collect_meeting_title)
    }
}
impl Default for Config {
    fn default() -> Self {
        Self {
            server: "http://127.0.0.1:5600/".into(),
            interval_seconds: 5,
            collector_timeout_seconds: 8,
            collect_meeting_title: false,
            title_format: TitleFormat::default(),
            payload: PayloadConfig::default(),
        }
    }
}
