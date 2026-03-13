//! Application configuration loaded from `~/.rusty-adb/config.yml`.
//!
//! If the file does not exist, all fields fall back to their defaults.
//! Unknown YAML keys are silently ignored, so future fields can be added
//! without breaking older config files.

use serde::{Deserialize, Serialize};
use std::path::Path;

// ─── Root ──────────────────────────────────────────────────────────────────

/// Top-level application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub log: LogConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            log: LogConfig::default(),
        }
    }
}

impl AppConfig {
    /// Load config from `path`.  Returns defaults if the file is missing or
    /// cannot be parsed, printing a warning to stderr in the latter case.
    pub fn load(path: &Path) -> Self {
        let content = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => return Self::default(), // file absent — use defaults
        };
        serde_yaml::from_str(&content).unwrap_or_else(|err| {
            eprintln!(
                "rusty-adb: warning: could not parse {}: {err}",
                path.display()
            );
            Self::default()
        })
    }
}

// ─── Log section ───────────────────────────────────────────────────────────

/// `log:` section of config.yml.
///
/// Example:
/// ```yaml
/// log:
///   level: debug          # trace | debug | info | warn | error
///   console_enabled: true
///   file_enabled: true
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LogConfig {
    /// Global log level filter sent to `tracing_subscriber`.
    /// Supports `RUST_LOG` syntax, e.g. `"debug"` or `"rusty_adb=debug,info"`.
    pub level: String,

    /// Whether to emit log output to the console (stderr).
    pub console_enabled: bool,

    /// Whether to write log files to `~/.rusty-adb/`.
    pub file_enabled: bool,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            console_enabled: true,
            file_enabled: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn defaults_when_file_missing() {
        let cfg = AppConfig::load(Path::new("/nonexistent/config.yml"));
        assert_eq!(cfg.log.level, "info");
        assert!(cfg.log.console_enabled);
        assert!(cfg.log.file_enabled);
    }

    #[test]
    fn parses_valid_yaml() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            f,
            "log:\n  level: debug\n  console_enabled: false\n  file_enabled: true"
        )
        .unwrap();
        let cfg = AppConfig::load(f.path());
        assert_eq!(cfg.log.level, "debug");
        assert!(!cfg.log.console_enabled);
        assert!(cfg.log.file_enabled);
    }

    #[test]
    fn partial_yaml_fills_defaults() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "log:\n  level: warn").unwrap();
        let cfg = AppConfig::load(f.path());
        assert_eq!(cfg.log.level, "warn");
        assert!(cfg.log.console_enabled); // default
    }

    #[test]
    fn invalid_yaml_returns_defaults() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "{{{{not valid yaml: [[[").unwrap();
        let cfg = AppConfig::load(f.path());
        assert_eq!(cfg.log.level, "info"); // default
    }
}
