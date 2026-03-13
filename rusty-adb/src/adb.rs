//! ADB device detection and connection management
//!
//! Provides an async-friendly wrapper around the `adb` binary for device
//! polling, state management, and (later) file operations.
//!
//! # Strategy
//! - Device polling: `tokio::process::Command` running `adb devices -l` every 2s
//! - File browsing: `adb shell ls -la <path>` via subprocess (Toybox format)
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
    /// 1. `adb` on `$PATH` (via `where` on Windows, `which` on Unix)
    /// 2. Platform-specific Android SDK default locations
    ///    - macOS: `~/Library/Android/sdk/platform-tools/adb`
    ///    - Windows: `%LOCALAPPDATA%\Android\Sdk\platform-tools\adb.exe`
    ///              `%USERPROFILE%\AppData\Local\Android\Sdk\platform-tools\adb.exe`
    pub async fn find() -> Result<Self> {
        // 1. Try PATH first (cross-platform)
        #[cfg(target_os = "windows")]
        let which_cmd = "where";
        #[cfg(not(target_os = "windows"))]
        let which_cmd = "which";

        if let Ok(output) = Command::new(which_cmd).arg("adb").output().await {
            if output.status.success() {
                let path_str = str::from_utf8(&output.stdout)?.trim().to_string();
                // `where` on Windows may return multiple lines; take the first
                let first = path_str.lines().next().unwrap_or("").trim();
                if !first.is_empty() {
                    tracing::debug!(adb = %first, "found adb on PATH");
                    return Ok(Self {
                        adb_path: PathBuf::from(first),
                    });
                }
            }
        }

        // 2. Platform-specific SDK default locations
        #[cfg(target_os = "macos")]
        {
            if let Some(home) = dirs::home_dir() {
                let sdk_adb = home.join("Library/Android/sdk/platform-tools/adb");
                if sdk_adb.exists() {
                    tracing::debug!(adb = %sdk_adb.display(), "found adb via Android SDK (macOS)");
                    return Ok(Self { adb_path: sdk_adb });
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            let candidates: Vec<PathBuf> = [
                // Android Studio default (via LOCALAPPDATA)
                std::env::var("LOCALAPPDATA")
                    .ok()
                    .map(|p| PathBuf::from(p).join("Android\\Sdk\\platform-tools\\adb.exe")),
                // Fallback via USERPROFILE
                std::env::var("USERPROFILE")
                    .ok()
                    .map(|p| PathBuf::from(p).join("AppData\\Local\\Android\\Sdk\\platform-tools\\adb.exe")),
                // Program Files (x86) — older SDK installs
                std::env::var("ProgramFiles(x86)")
                    .ok()
                    .map(|p| PathBuf::from(p).join("Android\\android-sdk\\platform-tools\\adb.exe")),
            ]
            .into_iter()
            .flatten()
            .collect();

            for path in candidates {
                if path.exists() {
                    tracing::debug!(adb = %path.display(), "found adb via Android SDK (Windows)");
                    return Ok(Self { adb_path: path });
                }
            }
        }

        #[cfg(target_os = "macos")]
        anyhow::bail!(
            "adb not found. Install Android platform-tools:\n  \
             brew install --cask android-platform-tools\n  \
             or install Android Studio and open SDK Manager."
        );

        #[cfg(target_os = "windows")]
        anyhow::bail!(
            "adb not found. Install Android platform-tools:\n  \
             winget install Google.PlatformTools\n  \
             or install Android Studio and open SDK Manager."
        );

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        anyhow::bail!(
            "adb not found. Install Android platform-tools from:\n  \
             https://developer.android.com/tools/releases/platform-tools"
        );
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

    /// Disconnect a device by serial (calls `adb disconnect SERIAL`).
    ///
    /// ADB transport connections (USB) can't be truly disconnected this way,
    /// but it clears the TCP connection and forces a re-authorization prompt.
    /// Errors are non-fatal — the device poll will reflect the new state.
    pub async fn disconnect(&self, serial: &str) -> Result<()> {
        let status = Command::new(&self.adb_path)
            .args(["disconnect", serial])
            .status()
            .await
            .context("failed to spawn adb disconnect")?;
        if !status.success() {
            tracing::warn!(serial, "adb disconnect exited non-zero");
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

// ─── Android Filesystem Entry ─────────────────────────────────────────────────

/// A single entry returned by `adb shell ls -la`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidEntry {
    /// File or directory name (no path prefix)
    pub name: String,
    /// Full absolute path on the device
    pub path: PathBuf,
    /// Byte size (0 for directories — ls reports block size, not meaningful)
    pub size: u64,
    /// Pre-formatted modified date "YYYY-MM-DD" (or "--" if unknown)
    pub modified: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub is_hidden: bool,
}

impl AndroidEntry {
    /// Human-readable file size; directories always show "--".
    pub fn size_display(&self) -> String {
        if self.is_dir {
            "--".to_string()
        } else {
            format_android_size(self.size)
        }
    }
}

fn format_android_size(bytes: u64) -> String {
    const KB: u64 = 1_024;
    const MB: u64 = 1_024 * KB;
    const GB: u64 = 1_024 * MB;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

// ─── Directory Listing ────────────────────────────────────────────────────────

impl AdbClient {
    /// Run `adb -s <serial> shell ls -la <path>` and return parsed entries.
    ///
    /// Returns `Err` on permission denied or ADB errors.  `.` and `..` are
    /// filtered out; the caller is responsible for adding a `..` navigation
    /// entry when appropriate.
    pub async fn list_dir(
        &self,
        serial: &str,
        path: &std::path::Path,
    ) -> Result<Vec<AndroidEntry>> {
        let path_str = path.to_string_lossy();

        let output = Command::new(&self.adb_path)
            .args(["-s", serial, "shell", "ls", "-la", &*path_str])
            .output()
            .await
            .context("failed to run adb shell ls -la")?;

        let stdout = str::from_utf8(&output.stdout).context("adb output not UTF-8")?;
        let stderr = str::from_utf8(&output.stderr).unwrap_or("");

        // Permission denied can appear on either stdout or stderr
        if stdout.contains("Permission denied") || stderr.contains("Permission denied") {
            anyhow::bail!("Permission denied: {}", path_str);
        }
        if stdout.contains("No such file or directory") {
            anyhow::bail!("No such file or directory: {}", path_str);
        }

        tracing::debug!(
            serial = %serial,
            path = %path_str,
            raw = %stdout.trim(),
            "adb shell ls -la raw output"
        );

        let entries = parse_ls_output(stdout, path);
        tracing::info!(
            serial = %serial,
            path = %path_str,
            count = entries.len(),
            "directory listing loaded"
        );
        Ok(entries)
    }

    /// Discover Android storage roots: always includes `/sdcard`; also
    /// returns any SD-card entries from `/storage/` that are not `emulated`
    /// or `self`.
    pub async fn list_storage_roots(&self, serial: &str) -> Vec<PathBuf> {
        let mut roots = vec![PathBuf::from("/sdcard")];

        let Ok(output) = Command::new(&self.adb_path)
            .args(["-s", serial, "shell", "ls", "/storage/"])
            .output()
            .await
        else {
            return roots;
        };

        if let Ok(s) = str::from_utf8(&output.stdout) {
            for name in s.lines().map(str::trim).filter(|n| !n.is_empty()) {
                if name != "emulated" && name != "self" {
                    let candidate = PathBuf::from("/storage").join(name);
                    if !roots.contains(&candidate) {
                        tracing::info!(serial = %serial, volume = %name, "SD card volume detected");
                        roots.push(candidate);
                    }
                }
            }
        }

        tracing::debug!(serial = %serial, count = roots.len(), "storage roots discovered");
        roots
    }
}

// ─── Android ls parser ────────────────────────────────────────────────────────

/// Parse the output of `adb shell ls -la <dir>` (Android Toybox format).
///
/// Example line:
/// ```text
/// drwxrwxrwx 17 root   sdcard_rw 3452 2024-01-15 12:00 DCIM
/// lrwxrwxrwx  1 root   sdcard_rw   21 2024-01-01 00:00 sdcard0 -> /storage/emulated/0
/// ```
pub fn parse_ls_output(output: &str, parent: &std::path::Path) -> Vec<AndroidEntry> {
    output
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let result = parse_ls_line(trimmed, parent);
            // Warn on non-empty, non-header lines that we couldn't parse —
            // this helps diagnose OEM-specific ls format variations.
            if result.is_none()
                && !trimmed.is_empty()
                && !trimmed.starts_with("total ")
                && !trimmed.contains("Permission denied")
                && !trimmed.contains("No such file")
                && trimmed != "."
                && trimmed != ".."
            {
                // Only warn for lines that look like they should be entries
                // (start with a permission character: -, d, l, c, b, p, s)
                if matches!(trimmed.chars().next(), Some('-' | 'd' | 'l' | 'c' | 'b' | 'p' | 's')) {
                    tracing::warn!(
                        line = %trimmed,
                        parent = %parent.display(),
                        "unrecognised adb ls line — may be an OEM ls format variation"
                    );
                }
            }
            result
        })
        .collect()
}

fn parse_ls_line(line: &str, parent: &std::path::Path) -> Option<AndroidEntry> {
    // Skip blank lines and the "total N" header
    if line.is_empty() || line.starts_with("total ") {
        return None;
    }
    // Skip lines that are error messages
    if line.contains("Permission denied") || line.contains("No such file") {
        return None;
    }

    let perms_char = line.chars().next()?;
    let is_dir = perms_char == 'd';
    let is_symlink = perms_char == 'l';

    // Tokenise: perms nlinks owner group size date time name...
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 8 {
        return None;
    }

    // Find the date token — always "YYYY-MM-DD" (10 chars, '-' at 4 and 7)
    let date_idx = tokens.iter().position(|t| {
        t.len() == 10
            && t.as_bytes().get(4) == Some(&b'-')
            && t.as_bytes().get(7) == Some(&b'-')
    })?;

    if date_idx < 1 || date_idx + 2 >= tokens.len() {
        return None;
    }

    let size: u64 = tokens[date_idx - 1].parse().ok()?;
    let date_str = tokens[date_idx];       // "2024-01-15"
    let time_str = tokens[date_idx + 1];   // "12:00"
    let modified = format!("{} {}", date_str, &time_str[..5]); // "2024-01-15 12:00"

    // The name is everything after the time token in the original line.
    // We find the time token's offset carefully (it appears after the date).
    let date_offset = line.find(date_str)?;
    let after_date = &line[date_offset + date_str.len()..];
    let time_offset_in_after = after_date.find(time_str)?;
    let after_time = after_date[time_offset_in_after + time_str.len()..].trim();

    // Symlinks: "name -> /path/to/target" — strip the link target
    let name_raw = if is_symlink {
        after_time.split(" -> ").next().unwrap_or(after_time)
    } else {
        after_time
    };
    let name = name_raw.trim();

    // Skip self and parent directory entries
    if name == "." || name == ".." || name.is_empty() {
        return None;
    }

    Some(AndroidEntry {
        path: parent.join(name),
        is_dir,
        is_symlink,
        is_hidden: name.starts_with('.'),
        size,
        modified: modified[..10].to_string(), // keep just "YYYY-MM-DD"
        name: name.to_string(),
    })
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

    // ── Android ls parser tests ───────────────────────────────────────────────

    const SAMPLE_LS: &str = "\
total 48
drwxrwxrwx 17 root   sdcard_rw 3452 2024-01-15 12:00 .
drwxr-x---  5 root   root      4096 2024-01-01 00:00 ..
drwxrwxrwx  2 root   sdcard_rw 4096 2024-01-10 08:30 DCIM
drwxrwxrwx  2 root   sdcard_rw 4096 2024-01-14 09:15 Download
lrwxrwxrwx  1 root   sdcard_rw   21 2024-01-01 00:00 sdcard0 -> /storage/emulated/0
-rw-rw----  1 root   sdcard_rw 1234 2024-01-15 10:00 notes.txt
";

    #[test]
    fn ls_parses_directories() {
        let parent = std::path::Path::new("/sdcard");
        let entries = parse_ls_output(SAMPLE_LS, parent);
        let dcim = entries.iter().find(|e| e.name == "DCIM").unwrap();
        assert!(dcim.is_dir);
        assert_eq!(dcim.path, parent.join("DCIM"));
        assert_eq!(dcim.modified, "2024-01-10");
    }

    #[test]
    fn ls_parses_files() {
        let parent = std::path::Path::new("/sdcard");
        let entries = parse_ls_output(SAMPLE_LS, parent);
        let f = entries.iter().find(|e| e.name == "notes.txt").unwrap();
        assert!(!f.is_dir);
        assert_eq!(f.size, 1234);
        assert_eq!(f.size_display(), "1 KB");
    }

    #[test]
    fn ls_parses_symlinks_strips_target() {
        let parent = std::path::Path::new("/sdcard");
        let entries = parse_ls_output(SAMPLE_LS, parent);
        let link = entries.iter().find(|e| e.name == "sdcard0").unwrap();
        assert!(link.is_symlink);
        assert_eq!(link.name, "sdcard0");
        // path should not include " -> /storage/emulated/0"
        assert_eq!(link.path, parent.join("sdcard0"));
    }

    #[test]
    fn ls_skips_dot_entries() {
        let parent = std::path::Path::new("/sdcard");
        let entries = parse_ls_output(SAMPLE_LS, parent);
        assert!(entries.iter().all(|e| e.name != "." && e.name != ".."));
    }

    #[test]
    fn ls_correct_entry_count() {
        let parent = std::path::Path::new("/sdcard");
        // Should have: DCIM, Download, sdcard0, notes.txt → 4
        let entries = parse_ls_output(SAMPLE_LS, parent);
        assert_eq!(entries.len(), 4);
    }

    #[test]
    fn ls_hidden_file_flagged() {
        let input = "-rw-rw----  1 root sdcard_rw 100 2024-01-15 10:00 .nomedia\n";
        let entries = parse_ls_output(input, std::path::Path::new("/sdcard"));
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_hidden);
    }

    #[test]
    fn ls_empty_output() {
        let entries = parse_ls_output("total 0\n", std::path::Path::new("/sdcard"));
        assert!(entries.is_empty());
    }

    #[test]
    fn android_entry_dir_size_display() {
        let e = AndroidEntry {
            name: "DCIM".into(),
            path: PathBuf::from("/sdcard/DCIM"),
            size: 4096,
            modified: "2024-01-10".into(),
            is_dir: true,
            is_symlink: false,
            is_hidden: false,
        };
        assert_eq!(e.size_display(), "--");
    }

    // ── DeviceState::from_str ─────────────────────────────────────────────────

    #[test]
    fn device_state_from_device() {
        assert_eq!(DeviceState::from_str("device"), DeviceState::Device);
    }

    #[test]
    fn device_state_from_unauthorized() {
        assert_eq!(DeviceState::from_str("unauthorized"), DeviceState::Unauthorized);
    }

    #[test]
    fn device_state_from_offline() {
        assert_eq!(DeviceState::from_str("offline"), DeviceState::Offline);
    }

    #[test]
    fn device_state_from_unknown() {
        assert_eq!(
            DeviceState::from_str("fastboot"),
            DeviceState::Other("fastboot".to_string())
        );
    }

    // ── format_android_size ───────────────────────────────────────────────────

    #[test]
    fn android_size_bytes() {
        assert_eq!(format_android_size(0), "0 B");
        assert_eq!(format_android_size(999), "999 B");
    }

    #[test]
    fn android_size_kb() {
        assert_eq!(format_android_size(2048), "2 KB");
    }

    #[test]
    fn android_size_mb() {
        assert_eq!(format_android_size(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn android_size_gb() {
        assert_eq!(format_android_size(2 * 1024 * 1024 * 1024), "2.0 GB");
    }

    // ── parse_ls_output edge cases ────────────────────────────────────────────

    #[test]
    fn ls_permission_denied_line_skipped() {
        let input = "ls: /sdcard/private: Permission denied\n\
                     -rw-rw----  1 root sdcard_rw 100 2024-01-15 10:00 notes.txt\n";
        let entries = parse_ls_output(input, std::path::Path::new("/sdcard"));
        // Permission denied line skipped; notes.txt should still parse
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "notes.txt");
    }

    #[test]
    fn ls_file_with_spaces_in_name() {
        let input =
            "-rw-rw----  1 root sdcard_rw 1234 2024-01-15 10:00 my vacation photos.jpg\n";
        let entries = parse_ls_output(input, std::path::Path::new("/sdcard"));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "my vacation photos.jpg");
    }
}
