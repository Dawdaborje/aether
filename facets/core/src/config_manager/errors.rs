use std::io;
use thiserror::Error;

/// A setting the user has to provide: there is deliberately no default for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingSetting {
    /// Where it lives in the config file: `app_dir`, or `table.key` such as
    /// `database.password`.
    pub key: &'static str,
    /// The command-line flag that can supply it instead, if there is one.
    pub flag: Option<&'static str>,
    /// The key is present but empty.
    pub empty: bool,
}

impl MissingSetting {
    fn describe(&self) -> String {
        let place = match self.key.split_once('.') {
            Some((table, key)) => format!("`{key}` under [{table}]"),
            None => format!("`{}` at the top level", self.key),
        };
        let state = if self.empty { "is empty" } else { "is not set" };
        match self.flag {
            Some(flag) => format!("{}: {state}. Set {place} in the config file, or pass {flag}.", self.key),
            None => format!("{}: {state}. Set {place} in the config file.", self.key),
        }
    }
}

fn describe_missing(file: Option<&str>, missing: &[MissingSetting]) -> String {
    let mut message = match file {
        Some(file) => format!("missing required configuration in {file}:\n"),
        None => "missing required configuration (no config file was found; looked for ./aether.toml):\n"
            .to_string(),
    };
    for setting in missing {
        message.push_str("  - ");
        message.push_str(&setting.describe());
        message.push('\n');
    }
    match file {
        Some(_) => {
            message.pop();
        }
        None => message.push_str(
            "Create a config file with `aether --gen aether_config`, or point to one with --config-file <path>.",
        ),
    }
    message
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config file '{path}': {source}")]
    Read { path: String, source: io::Error },

    #[error("failed to parse config file: {0}")]
    Parse(#[from] toml::de::Error),

    #[error("missing required config field '{0}'")]
    MissingField(String),

    /// Required settings that are neither in the config file nor given as flags.
    /// All of them are listed together so they can be fixed in one go.
    #[error("{}", describe_missing(.file.as_deref(), .missing))]
    MissingSettings {
        file: Option<String>,
        missing: Vec<MissingSetting>,
    },

    #[error("config field '{field}' must be of type {expected}")]
    InvalidType { field: String, expected: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_every_missing_setting_with_its_flag() {
        let error = ConfigError::MissingSettings {
            file: Some("/etc/aether.toml".into()),
            missing: vec![
                MissingSetting { key: "database.password", flag: Some("--db-password"), empty: false },
                MissingSetting { key: "app_dir", flag: Some("--app-dir"), empty: true },
                MissingSetting { key: "configuration.is_development_mode", flag: None, empty: false },
            ],
        };
        let text = error.to_string();
        assert!(text.starts_with("missing required configuration in /etc/aether.toml:"));
        assert!(text.contains("database.password: is not set. Set `password` under [database] in the config file, or pass --db-password."));
        assert!(text.contains("app_dir: is empty. Set `app_dir` at the top level in the config file, or pass --app-dir."));
        assert!(text.contains("configuration.is_development_mode: is not set. Set `is_development_mode` under [configuration] in the config file."));
        assert!(!text.contains("--gen aether_config"), "a file exists, so do not suggest creating one");
        assert!(text.ends_with("Set `is_development_mode` under [configuration] in the config file."));
    }

    #[test]
    fn explains_when_there_is_no_config_file() {
        let error = ConfigError::MissingSettings { file: None, missing: vec![] };
        let text = error.to_string();
        assert!(text.contains("no config file was found"));
        assert!(text.ends_with("--config-file <path>."));
    }
}
