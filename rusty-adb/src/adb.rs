//! ADB device detection and connection management
//!
//! Provides an async-friendly wrapper around the `adb` binary for device
//! polling, state management, and (later) file operations.
//!
//! # Strategy
//! - Device polling: `tokio::process::Command` running `adb devices -l` every 2s
//! - File browsing: `adb_client` crate (typed API, no text parsing)
//! - File transfers: `adb push/pull --progress`, parsing `[ XX%]` from stderr
//! - Startup: call `adb start-server` once at launch to warm the daemon

use std::path::PathBuf;
use std::str;

use anyhow::{Context, Result};
use tokio::process::Command;

// ─── Device State ──────────────────────────────────────────────────────────────

/// Authorization/connection state for a single ADB device entry
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceState {
    /// ADB sees the device and it is authorized
    Device,
    /// Device is connected but the user hasn't tapped "Allow" yet
    Unauthorized,
    /// Device is offline or unreachable
    Offline,
    /// Any other state string from `adb devices`
    Other(String),
}

impl DeviceState {
    fn from_str(s: &str) -> Self {
        match s.trim() {
            "device" => DeviceState::Device,
            "unauthorized" => DeviceState::Unauthorized,
            "offline" => DeviceState::Offline,
            other => DeviceState::Other(other.to_string()),
        }
    }
}

// ─── Device Info ───────────────────────────────────────────────────────────────

/// A connected (or otherwise visible) Android device
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdbDevice {
    /// USB/TCP serial identifier (e.g. "emulator-5554", "R5CWA0XXXXX")
    pub serial: String,
    /// Authorization/connection state
    pub state: DeviceState,
    /// Device model from `adb devices -l` (e.g. "Pixel_7")
    pub model: Option<String>,
    /// Product/codename (e.g. "panther")
    pub product: Option<String>,
}

impl AdbDevice {
    /// User-friendly display name: model if available, else serial
    pub fn display_name(&self) -> &str {
        self.model
            .as_deref()
            .filter(|m| !m.is_empty())
            .unwrap_or(&self.serial)
    }
}

// ─── ADB Client ────────────────────────────────────────────────────────────────

/// Thin wrapper around the `adb` CLI binary
#[derive(Debug, Clone)]
pub struct AdbClient {
    /// Resolved path to the `adb` binary
    pub adb_path: PathBuf,
}

impl AdbClient {
    /// Locate the `adb` binary.
    ///
    /// Search order:
    /// 1. `adb` on `$PATH` (checked via `which adb`)
    /// 2. `~/Library/Android/sdk/platform-tools/adb` (Android Studio default)
    pub async fn find() -> Result<Self> {
        // 1. Try PATH first
        if let Ok(output) = Command::new("which").arg("adb").output().await {
            if output.status.success() {
                let path_str = str::from_utf8(&output.stdout)?.trim().to_string();
                if !path_str.is_empty() {
                    tracing::debug!(adb = %path_str, "found adb on PATH");
                    return Ok(Self {
                        adb_path: PathBuf::from(path_str),
                    });
                }
            }
        }

        // 2. Android Studio default location
        if let Some(home) = dirs::home_dir() {
            let sdk_adb = home.join("Library/Android/sdk/platform-tools/adb");
            if sdk_adb.exists() {
                tracing::debug!(adb = %sdk_adb.display(), "found adb via Android SDK");
                return Ok(Self { adb_path: sdk_adb });
            }
        }

        anyhow::bail!(
            "adb not found. Install Android platform-tools:\n  \
             brew install --cask android-platform-tools"
        )
    }

    /// Start the ADB server in the background (idempotent — safe to call multiple times).
    pub async fn start_server(&self) -> Result<()> {
        let status = Command::new(&self.adb_path)
            .arg("start-server")
            .status()
            .await
            .context("failed to spawn adb start-server")?;

        if status.success() {
            tracing::info!("adb server started");
        } else {
            tracing::warn!("adb start-server exited non-zero (may already be running)");
        }
        Ok(())
    }

    /// Run `adb devices -l` and return a parsed list of devices.
    ///
    /// Returns an empty `Vec` (not an error) when the ADB server is not running
    /// or no devices are attached.
    pub async fn list_devices(&self) -> Result<Vec<AdbDevice>> {
        let output = Command::new(&self.adb_path)
            .args(["devices", "-l"])
            .output()
            .await
            .context("failed to run adb devices -l")?;

        let stdout = str::from_utf8(&output.stdout).context("adb output not UTF-8")?;
        tracing::debug!(raw = %stdout.trim(), "adb devices -l output");

        Ok(parse_devices(stdout))
    }
}

// ─── Parser ────────────────────────────────────────────────────────────────────

/// Parse the output of `adb devices -l` into a list of [`AdbDevice`]s.
///
/// Example input:
/// ```text
/// List of devices attached
/// R5CWA0XXXXX  device  product:panther model:Pixel_7 device:panther transport_id:1
/// emulator-5554  unauthorized
/// ```
fn parse_devices(output: &str) -> Vec<AdbDevice> {
    let mut devices = Vec::new();

    for line in output.lines() {
        // Skip the header line and blank lines
        if line.starts_with("List of devices") || line.trim().is_empty() {
            continue;
        }

        // Each line: <serial>\t<state>\t[key:value ...]
        let mut parts = line.splitn(2, '\t');
        let serial = match parts.next() {
            Some(s) if !s.trim().is_empty() => s.trim().to_string(),
            _ => continue,
        };

        let rest = parts.next().unwrap_or("").trim();
        // rest looks like: "device  product:panther model:Pixel_7 ..."
        let mut rest_parts = rest.splitn(2, ' ');
        let state_str = rest_parts.next().unwrap_or("offline");
        let state = DeviceState::from_str(state_str);

        let kv_part = rest_parts.next().unwrap_or("");
        let model = extract_kv(kv_part, "model").map(|m| m.replace('_', " "));
        let product = extract_kv(kv_part, "product");

        devices.push(AdbDevice {
            serial,
            state,
            model,
            product,
        });
    }

    devices
}

/// Extract `key:value` from a space-separated list of `key:value` pairs.
fn extract_kv<'a>(s: &'a str, key: &str) -> Option<String> {
    let prefix = format!("{}:", key);
    s.split_whitespace()
        .find(|tok| tok.starts_with(&prefix))
        .map(|tok| tok[prefix.len()..].to_string())
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_authorized_device() {
        let input = "List of devices attached\n\
                     R5CWA0XXXXX\tdevice product:panther model:Pixel_7 device:panther transport_id:1\n";
        let devices = parse_devices(input);
        assert_eq!(devices.len(), 1);
        let d = &devices[0];
        assert_eq!(d.serial, "R5CWA0XXXXX");
        assert_eq!(d.state, DeviceState::Device);
        assert_eq!(d.model.as_deref(), Some("Pixel 7"));
        assert_eq!(d.product.as_deref(), Some("panther"));
    }

    #[test]
    fn parse_unauthorized_device() {
        let input = "List of devices attached\nemulator-5554\tunauthorized\n";
        let devices = parse_devices(input);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].state, DeviceState::Unauthorized);
        assert!(devices[0].model.is_none());
    }

    #[test]
    fn parse_empty_output() {
        let input = "List of devices attached\n";
        let devices = parse_devices(input);
        assert!(devices.is_empty());
    }

    #[test]
    fn display_name_prefers_model() {
        let d = AdbDevice {
            serial: "R5CWA0XXXXX".to_string(),
            state: DeviceState::Device,
            model: Some("Pixel 7".to_string()),
            product: None,
        };
        assert_eq!(d.display_name(), "Pixel 7");
    }

    #[test]
    fn display_name_falls_back_to_serial() {
        let d = AdbDevice {
            serial: "emulator-5554".to_string(),
            state: DeviceState::Device,
            model: None,
            product: None,
        };
        assert_eq!(d.display_name(), "emulator-5554");
    }
}
