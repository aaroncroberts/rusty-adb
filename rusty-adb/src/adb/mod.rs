//! ADB device detection and connection management
//!
//! Provides an async-friendly wrapper around the `adb` binary for device
//! polling, state management, and file operations.
//!
//! # Strategy
//! - Device polling: `tokio::process::Command` running `adb devices -l` every 2s
//! - File browsing: `adb shell ls -la <path>` via subprocess (Toybox format)
//! - File transfers: `adb push/pull --progress`, parsing `[ XX%]` from stderr
//! - Startup: call `adb start-server` once at launch to warm the daemon
//!
//! # Module layout
//! - `mod.rs` — device state, `AdbClient`, and `adb devices` parsing
//! - `parser`  — Android `ls -la` output parser, pure and fully unit-tested

pub mod parser;
pub mod transfer;

pub use parser::AndroidEntry;
pub use transfer::{run_transfer, TransferDirection, TransferEvent, TransferJob, TransferStatus};

// ─── ADB Operations Trait ─────────────────────────────────────────────────────

/// Abstraction over the ADB filesystem operations used by the app.
///
/// [`AdbClient`] is the production implementation. [`MockAdbClient`]
/// (available in `adb::mock` under `#[cfg(test)]`) satisfies this trait for
/// unit tests without spawning a real ADB process.
///
/// # Note on object safety
/// This trait uses `impl Future` return types (RPITIT, stable since Rust 1.75)
/// and is therefore **not object-safe** — you cannot use `dyn AdbOperations`.
/// Callers must use generic bounds: `fn foo<C: AdbOperations>(client: &C)`.
#[allow(dead_code)]
pub trait AdbOperations: Clone + Send + Sync + 'static {
    /// List directory entries on the device at `path`.
    fn list_dir(
        &self,
        serial: &str,
        path: &std::path::Path,
    ) -> impl std::future::Future<Output = anyhow::Result<Vec<AndroidEntry>>> + Send;

    /// Rename `from` to `to` on the device.
    fn rename(
        &self,
        serial: &str,
        from: &std::path::Path,
        to: &std::path::Path,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send;

    /// Delete `path` on the device (recursive).
    fn delete(
        &self,
        serial: &str,
        path: &std::path::Path,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send;

    /// Pull a single file from the device to a local temp path.
    fn pull_to_temp(
        &self,
        serial: &str,
        remote: &std::path::Path,
    ) -> impl std::future::Future<Output = anyhow::Result<std::path::PathBuf>> + Send;

    /// Discover storage roots on the device (e.g. `/sdcard`, `/storage/…`).
    fn list_storage_roots(
        &self,
        serial: &str,
    ) -> impl std::future::Future<Output = Vec<std::path::PathBuf>> + Send;
}

// ─── AdbOperations impl for AdbClient ────────────────────────────────────────

impl AdbOperations for AdbClient {
    async fn list_dir(
        &self,
        serial: &str,
        path: &std::path::Path,
    ) -> anyhow::Result<Vec<AndroidEntry>> {
        // Calls the inherent AdbClient::list_dir (inherent methods beat trait dispatch)
        self.list_dir(serial, path).await
    }

    async fn rename(
        &self,
        serial: &str,
        from: &std::path::Path,
        to: &std::path::Path,
    ) -> anyhow::Result<()> {
        self.rename(serial, from, to).await
    }

    async fn delete(&self, serial: &str, path: &std::path::Path) -> anyhow::Result<()> {
        self.delete(serial, path).await
    }

    async fn pull_to_temp(
        &self,
        serial: &str,
        remote: &std::path::Path,
    ) -> anyhow::Result<std::path::PathBuf> {
        self.pull_to_temp(serial, remote).await
    }

    async fn list_storage_roots(&self, serial: &str) -> Vec<std::path::PathBuf> {
        self.list_storage_roots(serial).await
    }
}

// ─── Mock ADB Client (test only) ─────────────────────────────────────────────

#[cfg(test)]
pub mod mock {
    use super::{AdbOperations, AndroidEntry};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    /// Mock ADB client for unit tests.
    ///
    /// Pre-populate `entries` and `roots` to control what `list_dir` and
    /// `list_storage_roots` return. Set `fail_*` flags to simulate errors.
    /// Inspect `.calls()` after the test to verify which operations ran.
    #[derive(Debug, Clone)]
    pub struct MockAdbClient {
        /// Entries returned by `list_dir` (same for all calls).
        pub entries: Vec<AndroidEntry>,
        /// Roots returned by `list_storage_roots`.
        pub roots: Vec<PathBuf>,
        /// When `true`, `list_dir` returns an error.
        pub fail_list_dir: bool,
        /// When `true`, `rename` returns an error.
        pub fail_rename: bool,
        /// When `true`, `delete` returns an error.
        pub fail_delete: bool,
        /// Append-only log of method calls for post-test assertions.
        calls: Arc<Mutex<Vec<String>>>,
    }

    impl Default for MockAdbClient {
        fn default() -> Self {
            Self {
                entries: vec![],
                roots: vec![PathBuf::from("/sdcard")],
                fail_list_dir: false,
                fail_rename: false,
                fail_delete: false,
                calls: Arc::new(Mutex::new(vec![])),
            }
        }
    }

    impl MockAdbClient {
        /// Returns a snapshot of all method calls recorded so far.
        pub fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }

        /// Constructor: mock that fails on `list_dir`.
        pub fn failing_list_dir() -> Self {
            Self {
                fail_list_dir: true,
                ..Self::default()
            }
        }

        /// Constructor: mock that fails on `rename`.
        pub fn failing_rename() -> Self {
            Self {
                fail_rename: true,
                ..Self::default()
            }
        }

        /// Constructor: mock that fails on `delete`.
        pub fn failing_delete() -> Self {
            Self {
                fail_delete: true,
                ..Self::default()
            }
        }

        fn log(&self, call: impl Into<String>) {
            self.calls.lock().unwrap().push(call.into());
        }
    }

    impl AdbOperations for MockAdbClient {
        async fn list_dir(&self, serial: &str, path: &Path) -> anyhow::Result<Vec<AndroidEntry>> {
            self.log(format!("list_dir serial={serial} path={}", path.display()));
            if self.fail_list_dir {
                anyhow::bail!("mock list_dir failure");
            }
            Ok(self.entries.clone())
        }

        async fn rename(&self, serial: &str, from: &Path, to: &Path) -> anyhow::Result<()> {
            self.log(format!(
                "rename serial={serial} from={} to={}",
                from.display(),
                to.display()
            ));
            if self.fail_rename {
                anyhow::bail!("mock rename failure");
            }
            Ok(())
        }

        async fn delete(&self, serial: &str, path: &Path) -> anyhow::Result<()> {
            self.log(format!("delete serial={serial} path={}", path.display()));
            if self.fail_delete {
                anyhow::bail!("mock delete failure");
            }
            Ok(())
        }

        async fn pull_to_temp(&self, serial: &str, remote: &Path) -> anyhow::Result<PathBuf> {
            self.log(format!(
                "pull_to_temp serial={serial} remote={}",
                remote.display()
            ));
            Ok(PathBuf::from("/tmp/mock-pull-target"))
        }

        async fn list_storage_roots(&self, serial: &str) -> Vec<PathBuf> {
            self.log(format!("list_storage_roots serial={serial}"));
            self.roots.clone()
        }
    }
}

use std::path::PathBuf;
use std::str;

use anyhow::{Context, Result};
use tokio::process::Command;

// ─── App-level ADB Status ─────────────────────────────────────────────────────

/// Application-level ADB connection state, derived from the device list.
///
/// This is the high-level state shown in the status bar and used throughout
/// the app to gate UI — distinct from [`DeviceState`] which describes a single
/// raw `adb devices` entry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum AdbStatus {
    /// ADB binary not found on this system — install screen is shown
    NotFound,
    /// No device connected
    #[default]
    Disconnected,
    /// Device found but the user hasn't tapped "Allow" yet
    Unauthorized,
    /// ADB daemon is starting / device handshake in progress
    #[allow(dead_code)]
    Connecting(String),
    /// Device connected and authorized — carries the device display name
    Connected(String),
    /// An error occurred (human-readable message)
    Error(String),
}

impl AdbStatus {
    /// Human-readable status text shown in the status bar.
    pub fn text(&self) -> String {
        match self {
            AdbStatus::NotFound => "ADB not installed — follow the setup guide".to_string(),
            AdbStatus::Disconnected => "No device connected".to_string(),
            AdbStatus::Unauthorized => {
                "Device found — check your phone screen and tap Allow".to_string()
            }
            AdbStatus::Connecting(name) => format!("Connecting to {}…", name),
            AdbStatus::Connected(name) => format!("Connected: {}", name),
            AdbStatus::Error(msg) => format!("Error: {}", msg),
        }
    }
}

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
    ///      `%USERPROFILE%\AppData\Local\Android\Sdk\platform-tools\adb.exe`
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
                std::env::var("USERPROFILE").ok().map(|p| {
                    PathBuf::from(p).join("AppData\\Local\\Android\\Sdk\\platform-tools\\adb.exe")
                }),
                // Program Files (x86) — older SDK installs
                std::env::var("ProgramFiles(x86)").ok().map(|p| {
                    PathBuf::from(p).join("Android\\android-sdk\\platform-tools\\adb.exe")
                }),
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

// ─── Device list parser ────────────────────────────────────────────────────────

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

        // Lines are normally tab-separated: <serial>\t<state>\t[key:value ...]
        // Some devices (e.g. those with no USB serial) emit space-padded lines
        // where the "serial" column is "?". Handle both formats.
        let (raw_serial, rest) = if let Some(tab) = line.find('\t') {
            (line[..tab].trim(), line[tab + 1..].trim())
        } else {
            // Space-padded: first whitespace-delimited token is the serial
            let trimmed = line.trim();
            match trimmed.split_once(|c: char| c.is_whitespace()) {
                Some((s, r)) => (s, r.trim()),
                None => continue,
            }
        };

        if raw_serial.is_empty() {
            continue;
        }

        // rest looks like: "device  product:panther model:Pixel_7 ..."
        let mut rest_parts = rest.splitn(2, ' ');
        let state_str = rest_parts.next().unwrap_or("offline");
        let state = DeviceState::from_str(state_str);

        let kv_part = rest_parts.next().unwrap_or("");
        let model = extract_kv(kv_part, "model").map(|m| m.replace('_', " "));
        let product = extract_kv(kv_part, "product");

        // When the serial is "?" ADB uses transport addressing. Store the
        // transport_id as "t:<id>" so the command builder can emit "-t <id>"
        // instead of "-s ?" (which ADB does not accept).
        let serial = if raw_serial == "?" {
            match extract_kv(kv_part, "transport_id") {
                Some(tid) => format!("t:{tid}"),
                None => {
                    tracing::warn!("device with '?' serial has no transport_id — skipping");
                    continue;
                }
            }
        } else {
            raw_serial.to_string()
        };

        devices.push(AdbDevice {
            serial,
            state,
            model,
            product,
        });
    }

    devices
}

/// Returns the ADB target flag pair for a device selector.
///
/// Serials stored as `"t:<id>"` (transport-addressed, e.g. when USB serial is
/// unknown) emit `["-t", "<id>"]`; real serials emit `["-s", "<serial>"]`.
fn target_args(serial: &str) -> [&str; 2] {
    if let Some(tid) = serial.strip_prefix("t:") {
        ["-t", tid]
    } else {
        ["-s", serial]
    }
}

/// Extract `key:value` from a space-separated list of `key:value` pairs.
fn extract_kv(s: &str, key: &str) -> Option<String> {
    let prefix = format!("{}:", key);
    s.split_whitespace()
        .find(|tok| tok.starts_with(&prefix))
        .map(|tok| tok[prefix.len()..].to_string())
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Wrap a path in POSIX single quotes for safe use inside `adb shell` commands.
///
/// `adb shell ls -la <path>` sends the arguments through the Android shell,
/// which splits on whitespace.  Paths with spaces (e.g. artist folders) must
/// be quoted.  Single quotes inside the path are escaped using `'\''`.
///
/// Example:
/// ```text
/// /storage/external_sd/Music/2 Pac/  →  '/storage/external_sd/Music/2 Pac/'
/// /sdcard/it's here/                 →  '/sdcard/it'\''s here/'
/// ```
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Check the output of a fire-and-forget `adb shell` command.
///
/// Returns `Ok(())` when the command succeeded and produced no error output.
/// Returns `Err` when the process exited non-zero or either stream contains
/// "Permission denied".
///
/// `context` is a caller-supplied string included in the error message, e.g.
/// `"rename failed [serial=device1234 from=/sdcard/a to=/sdcard/b]"`.
fn check_adb_output(output: &std::process::Output, context: &str) -> Result<()> {
    let stderr = str::from_utf8(&output.stderr)
        .unwrap_or("")
        .trim()
        .to_string();
    let stdout = str::from_utf8(&output.stdout)
        .unwrap_or("")
        .trim()
        .to_string();
    if !output.status.success()
        || stderr.contains("Permission denied")
        || stdout.contains("Permission denied")
    {
        let msg = if !stderr.is_empty() { stderr } else { stdout };
        anyhow::bail!("{context}: {msg}");
    }
    Ok(())
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

        // Always append a trailing slash so the shell dereferences symlinks.
        // Without it, `ls -la /sdcard` (which is a symlink on many devices)
        // shows the symlink entry itself rather than the directory contents.
        let ls_target = format!("{}/", path_str.trim_end_matches('/'));

        // Pass the whole shell command as one argument so the Android shell
        // receives a pre-quoted string — paths with spaces would otherwise be
        // split by the shell into multiple tokens.
        let shell_cmd = format!("ls -la {}", shell_quote(&ls_target));
        let output = Command::new(&self.adb_path)
            .args(target_args(serial))
            .args(["shell", &shell_cmd])
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

        let entries = parser::parse_ls_output(stdout, path);
        tracing::info!(
            serial = %serial,
            path = %path_str,
            count = entries.len(),
            "directory listing loaded"
        );
        Ok(entries)
    }

    /// Rename a file or directory on the device using `adb shell mv`.
    pub async fn rename(
        &self,
        serial: &str,
        from: &std::path::Path,
        to: &std::path::Path,
    ) -> Result<()> {
        let from_str = from.to_string_lossy();
        let to_str = to.to_string_lossy();
        let shell_cmd = format!("mv {} {}", shell_quote(&from_str), shell_quote(&to_str));
        let output = Command::new(&self.adb_path)
            .args(target_args(serial))
            .args(["shell", &shell_cmd])
            .output()
            .await
            .context("failed to run adb shell mv")?;
        check_adb_output(
            &output,
            &format!("rename failed [serial={serial} from={from_str} to={to_str}]"),
        )?;
        Ok(())
    }

    /// Delete a file or directory on the device using `adb shell rm -rf`.
    pub async fn delete(&self, serial: &str, path: &std::path::Path) -> Result<()> {
        let path_str = path.to_string_lossy();
        let shell_cmd = format!("rm -rf {}", shell_quote(&path_str));
        let output = Command::new(&self.adb_path)
            .args(target_args(serial))
            .args(["shell", &shell_cmd])
            .output()
            .await
            .context("failed to run adb shell rm -rf")?;
        check_adb_output(
            &output,
            &format!("delete failed [serial={serial} path={path_str}]"),
        )?;
        Ok(())
    }

    /// Pull a single file from the device to the OS temp directory.
    ///
    /// Returns the local path of the pulled file.  The temp directory
    /// (`<tmp>/rusty-adb-preview/`) is created on first use.
    pub async fn pull_to_temp(&self, serial: &str, remote: &std::path::Path) -> Result<PathBuf> {
        let filename = remote
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("remote path has no filename"))?
            .to_string_lossy()
            .into_owned();

        let tmp_dir = std::env::temp_dir().join("rusty-adb-preview");
        tokio::fs::create_dir_all(&tmp_dir)
            .await
            .context("failed to create preview temp dir")?;

        let local_path = tmp_dir.join(&filename);
        let remote_str = remote.to_string_lossy();
        let local_str = local_path.to_string_lossy().into_owned();

        let output = Command::new(&self.adb_path)
            .args(target_args(serial))
            .args(["pull", &*remote_str, &*local_str])
            .output()
            .await
            .context("failed to run adb pull")?;

        if !output.status.success() {
            let stderr = str::from_utf8(&output.stderr).unwrap_or("").trim();
            anyhow::bail!("pull_to_temp failed [serial={serial} remote={remote_str}]: {stderr}");
        }

        tracing::info!(
            remote = %remote_str,
            local = %local_path.display(),
            "pulled file to temp for preview"
        );
        Ok(local_path)
    }

    /// Discover Android storage roots.
    ///
    /// Always includes `/sdcard` (legacy symlink present on all devices).
    /// Also probes for `/storage/emulated/0` (the real internal-storage path
    /// on Android 4.2+ multi-user devices) and any removable SD-card volumes
    /// under `/storage/` whose names are not `emulated` or `self`.
    ///
    /// Both `ls` commands run in parallel so there is no extra latency beyond
    /// what the concurrent `list_dir` call already incurs.
    pub async fn list_storage_roots(&self, serial: &str) -> Vec<PathBuf> {
        let mut roots = vec![PathBuf::from("/sdcard")];

        // Run both ls commands concurrently to minimise latency.
        let (storage_res, emulated_res) = tokio::join!(
            Command::new(&self.adb_path)
                .args(target_args(serial))
                .args(["shell", "ls", "/storage/"])
                .output(),
            Command::new(&self.adb_path)
                .args(target_args(serial))
                .args(["shell", "ls", "/storage/emulated/"])
                .output(),
        );

        // /storage/emulated/0 — primary-user internal storage on modern Android.
        // Only add if the directory actually exists (user "0" is listed).
        if let Ok(out) = emulated_res {
            if let Ok(s) = str::from_utf8(&out.stdout) {
                if s.lines().any(|l| l.trim() == "0") {
                    let path = PathBuf::from("/storage/emulated/0");
                    if !roots.contains(&path) {
                        tracing::debug!(serial = %serial, "found /storage/emulated/0");
                        roots.push(path);
                    }
                }
            }
        }

        // SD card volumes: non-emulated, non-self entries under /storage/.
        // Guard against error text (spaces/colons/slashes) leaking into names.
        let Ok(storage_out) = storage_res else {
            tracing::debug!(serial = %serial, count = roots.len(), "storage roots discovered (ls /storage/ failed)");
            return roots;
        };

        if let Ok(s) = str::from_utf8(&storage_out.stdout) {
            for name in s.lines().map(str::trim).filter(|n| !n.is_empty()) {
                if matches!(name, "emulated" | "self") {
                    continue;
                }
                // Reject lines containing spaces, colons, or a leading slash —
                // these indicate error output, never a valid volume UUID.
                if name.contains(' ') || name.contains(':') || name.starts_with('/') {
                    tracing::warn!(serial = %serial, line = %name, "ignoring suspicious /storage/ entry");
                    continue;
                }
                let candidate = PathBuf::from("/storage").join(name);
                if !roots.contains(&candidate) {
                    tracing::info!(serial = %serial, volume = %name, "external storage volume detected");
                    roots.push(candidate);
                }
            }
        }

        tracing::debug!(serial = %serial, count = roots.len(), "storage roots discovered");
        roots
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── adb devices parsing ───────────────────────────────────────────────────

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

    /// Devices without a USB serial emit space-padded lines with "?" as serial.
    /// The parser should fall back to transport_id and store it as "t:<id>".
    #[test]
    fn parse_question_mark_serial_uses_transport_id() {
        let input = "List of devices attached\n\
                     ?                      device usb:2-1 product:some_product model:Some_Model device:some_device transport_id:3\n";
        let devices = parse_devices(input);
        assert_eq!(devices.len(), 1);
        let d = &devices[0];
        assert_eq!(d.serial, "t:3", "should encode transport_id as selector");
        assert_eq!(d.state, DeviceState::Device);
        assert!(d.model.is_some(), "model should be parsed from kv pairs");
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

    // ── DeviceState::from_str ─────────────────────────────────────────────────

    #[test]
    fn device_state_from_device() {
        assert_eq!(DeviceState::from_str("device"), DeviceState::Device);
    }

    #[test]
    fn device_state_from_unauthorized() {
        assert_eq!(
            DeviceState::from_str("unauthorized"),
            DeviceState::Unauthorized
        );
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

    // ── AdbStatus::text ───────────────────────────────────────────────────────

    #[test]
    fn adb_status_text_disconnected() {
        assert_eq!(AdbStatus::Disconnected.text(), "No device connected");
    }

    #[test]
    fn adb_status_text_unauthorized() {
        assert!(AdbStatus::Unauthorized.text().contains("Allow"));
    }

    #[test]
    fn adb_status_text_connecting() {
        let s = AdbStatus::Connecting("Pixel 7".to_string()).text();
        assert!(s.contains("Pixel 7"), "expected device name in: {}", s);
    }

    #[test]
    fn adb_status_text_connected() {
        let s = AdbStatus::Connected("Pixel 7".to_string()).text();
        assert!(s.contains("Pixel 7") && s.contains("Connected"));
    }

    #[test]
    fn adb_status_text_error() {
        let s = AdbStatus::Error("timeout".to_string()).text();
        assert!(s.contains("Error") && s.contains("timeout"));
    }
}
