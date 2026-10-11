use crate::config::Config;
use std::{collections::BTreeMap, fs, path::PathBuf};
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    Run,
    Configure,
    Logs,
    Status,
    About,
    Version,
    Help,
    Init,
    Check,
}
impl Command {
    pub fn action_id(self) -> Option<&'static str> {
        match self {
            Self::Configure => Some("configure"),
            Self::Logs => Some("logs"),
            Self::Status => Some("status"),
            Self::About => Some("about"),
            _ => None,
        }
    }
}
#[derive(Debug)]
pub struct Launch {
    pub command: Command,
    pub no_ui: bool,
    pub profile: String,
    pub testing: bool,
    pub base: PathBuf,
    pub config_path: PathBuf,
    pub server_override: Option<String>,
}
impl Launch {
    pub fn resolve(args: &[String], env: &BTreeMap<String, String>) -> Result<Self, String> {
        let mut command = Command::Run;
        let mut no_ui = false;
        let mut cli_testing = false;
        let mut profile = None;
        let mut server = None;
        let mut config = None;
        let mut i = 0;
        while i < args.len() {
            let token = &args[i];
            i += 1;
            match token.as_str() {
                "--testing" => cli_testing = true,
                "--no-ui" => no_ui = true,
                "--profile" | "--server" | "--config" => {
                    let value = args.get(i).ok_or("Missing option value")?.clone();
                    i += 1;
                    match token.as_str() {
                        "--profile" => profile = Some(value),
                        "--server" => server = Some(value),
                        _ => config = Some(PathBuf::from(value)),
                    }
                }
                "--check-config" => {
                    if command != Command::Run {
                        return Err("Multiple commands".into());
                    }
                    command = Command::Check;
                    config = Some(PathBuf::from(args.get(i).ok_or("Config path required")?));
                    i += 1;
                }
                value => {
                    if command != Command::Run {
                        return Err("Unknown or multiple commands".into());
                    }
                    command = match value {
                        "configure" | "--settings" => Command::Configure,
                        "logs-ui" => Command::Logs,
                        "status-ui" => Command::Status,
                        "about" => Command::About,
                        "--version" => Command::Version,
                        "--help" | "-h" => Command::Help,
                        "--init-config" => Command::Init,
                        _ => return Err("Unknown command".into()),
                    };
                }
            }
        }
        if no_ui && command.action_id().is_none() {
            return Err("--no-ui requires a management action".into());
        }
        let protocol = env.get("AW_MODULE_ACTION_PROTOCOL");
        if let Some(protocol) = protocol {
            if protocol != "1" {
                return Err("Unsupported module action protocol".into());
            }
            if env.get("AW_MODULE_ACTION").map(String::as_str) != command.action_id() {
                return Err("Action ID does not match command".into());
            }
            if command.action_id().is_none() {
                return Err("Action context cannot launch a producer".into());
            }
        } else if env.contains_key("AW_MODULE_ACTION") {
            return Err("Action ID without protocol".into());
        }
        let env_testing = match env.get("AW_TESTING").map(String::as_str) {
            None | Some("0") => false,
            Some("1") => true,
            _ => return Err("AW_TESTING must be 0 or 1".into()),
        };
        let explicit_profile = profile.is_some();
        let profile = if cli_testing {
            if profile.as_deref().is_some_and(|p| p != "testing") {
                return Err("Conflicting testing/profile options".into());
            }
            "testing".to_owned()
        } else if let Some(profile) = profile {
            profile
        } else {
            let profile = env.get("AW_PROFILE").filter(|p| !p.is_empty()).cloned();
            match profile {
                Some(p) => p,
                None => if env_testing { "testing" } else { "default" }.into(),
            }
        };
        if profile.is_empty()
            || profile.len() > 32
            || !profile.as_bytes()[0].is_ascii_alphanumeric()
            || profile
                .bytes()
                .any(|b| !b.is_ascii_lowercase() && !b.is_ascii_digit() && b != b'-' && b != b'_')
        {
            return Err("Invalid AW profile".into());
        }
        let testing = profile == "testing";
        if !cli_testing
            && !explicit_profile
            && env.contains_key("AW_TESTING")
            && testing != env_testing
        {
            return Err("Conflicting AW profile/testing context".into());
        }
        let root = env.get("LOCALAPPDATA").ok_or("LOCALAPPDATA unavailable")?;
        let app = if profile == "default" {
            "activitywatch".to_owned()
        } else {
            format!("activitywatch-{profile}")
        };
        let base = PathBuf::from(root).join(app).join("aw-watcher-teams");
        let config_path = config.unwrap_or_else(|| base.join("config.json"));
        let server_override = server
            .or_else(|| env.get("AW_SERVER_URL").cloned())
            .map(|s| crate::validate_server(&s).map(|u| u.to_string()))
            .transpose()?;
        Ok(Self {
            command,
            no_ui,
            profile,
            testing,
            base,
            config_path,
            server_override,
        })
    }
    pub fn current() -> Result<Self, String> {
        Self::resolve(
            &std::env::args().skip(1).collect::<Vec<_>>(),
            &std::env::vars().collect(),
        )
    }
    pub fn defaults(&self) -> Config {
        Config {
            server: self.server_override.clone().unwrap_or_else(|| {
                format!(
                    "http://127.0.0.1:{}/",
                    if self.testing { 5666 } else { 5600 }
                )
            }),
            ..Default::default()
        }
    }
    pub fn ensure_config(&self) -> Result<(), Box<dyn std::error::Error>> {
        if !self.config_path.exists() {
            if let Some(parent) = self.config_path.parent() {
                fs::create_dir_all(parent)?;
            }
            // Do not replace an existing settings file if another configure action won the race.
            use std::io::Write;
            let mut file = match fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&self.config_path)
            {
                Ok(f) => f,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Ok(()),
                Err(e) => return Err(e.into()),
            };
            file.write_all(&serde_json::to_vec_pretty(&self.defaults())?)?;
            file.sync_all()?;
        }
        Ok(())
    }
    pub fn config(&self) -> Result<Config, Box<dyn std::error::Error>> {
        let bytes = fs::read(&self.config_path)?;
        let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
        let mut config: Config =
            serde_json::from_slice(bytes).map_err(|_| "Invalid module configuration")?;
        config.validate()?;
        if let Some(server) = &self.server_override {
            config.server = server.clone();
        }
        config.validate()?;
        Ok(config)
    }
}
