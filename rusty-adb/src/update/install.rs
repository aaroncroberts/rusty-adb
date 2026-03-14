//! ADB install flow handlers: InstallAdb, InstallComplete, InstallFailed,
//! AdbNotFound, RetryAdbFind, DaemonStartFailed, RestartDaemon, OpenUrl.
use crate::{App, Message};
use crate::adb::AdbClient;
use crate::status_bar::AdbStatus;
use iced::Task;
use std::sync::Arc;

impl App {
    pub(super) fn adb_not_found(&mut self) -> Task<Message> {
        self.adb_status = AdbStatus::NotFound;
        Task::none()
    }

    pub(super) fn retry_adb_find(&mut self) -> Task<Message> {
        tracing::info!("retrying adb detection after install");
        self.install_log.clear();
        self.installing = false;
        Task::perform(
            async {
                let client = AdbClient::find().await.map_err(|e| e.to_string())?;
                if let Err(e) = client.start_server().await {
                    return Err(format!("daemon:{}", e));
                }
                Ok::<AdbClient, String>(client)
            },
            |result| match result {
                Ok(client) => {
                    tracing::info!(adb = %client.adb_path.display(), "adb found on retry");
                    Message::AdbReady(Arc::new(client))
                }
                Err(e) if e.starts_with("adb not found") => Message::AdbNotFound,
                Err(e) if e.starts_with("daemon:") => {
                    Message::DaemonStartFailed(e["daemon:".len()..].to_string())
                }
                Err(e) => Message::AdbError(e),
            },
        )
    }

    pub(super) fn install_adb(&mut self) -> Task<Message> {
        if self.installing {
            return Task::none(); // guard against double-tap
        }
        self.installing = true;
        self.install_log.clear();
        self.install_log.push("Starting installation…".to_string());
        tracing::info!("starting platform-tools install via package manager");

        Task::perform(
            async {
                #[cfg(target_os = "macos")]
                let mut child = tokio::process::Command::new("brew")
                    .args(["install", "--cask", "android-platform-tools"])
                    .env("HOMEBREW_NO_AUTO_UPDATE", "1")
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .map_err(|e| {
                        format!(
                            "brew not found: {e}. Install Homebrew from https://brew.sh"
                        )
                    })?;

                #[cfg(target_os = "windows")]
                let mut child = tokio::process::Command::new("winget")
                    .args(["install", "--id", "Google.PlatformTools", "--accept-source-agreements", "--accept-package-agreements"])
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .map_err(|e| format!("winget not found: {e}. Install App Installer from the Microsoft Store"))?;

                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                return Err("Automatic install not supported on this platform. Download from developer.android.com/tools/releases/platform-tools".to_string());

                let status = child.wait().await.map_err(|e| e.to_string())?;
                if status.success() {
                    Ok(())
                } else {
                    let code = status.code().unwrap_or(-1);
                    Err(format!("Package manager exited with code {code}. Check the log above for details."))
                }
            },
            |result| match result {
                Ok(()) => Message::InstallComplete,
                Err(e) => Message::InstallFailed(e),
            },
        )
    }

    pub(super) fn install_complete(&mut self) -> Task<Message> {
        tracing::info!("platform-tools install succeeded, retrying adb detection");
        self.install_log
            .push("[OK] Installation complete. Detecting adb...".to_string());
        self.installing = false;
        self.update(Message::RetryAdbFind)
    }

    pub(super) fn install_failed(&mut self, err: String) -> Task<Message> {
        tracing::warn!(error = %err, "platform-tools install failed");
        self.installing = false;
        self.install_log.push(format!("[ERR] {err}"));
        Task::none()
    }

    pub(super) fn daemon_start_failed(&mut self, msg: String) -> Task<Message> {
        tracing::warn!(error = %msg, "adb start-server failed");
        self.adb_status = AdbStatus::Error(format!("Daemon error: {msg}"));
        // Show error banner with note that user can restart daemon
        self.update(Message::ShowError(format!(
            "ADB daemon failed to start: {msg}. Use 'Restart Daemon' in the toolbar."
        )))
    }

    pub(super) fn restart_daemon(&mut self) -> Task<Message> {
        tracing::info!("restarting adb daemon (kill-server + start-server)");
        let client = match &self.adb_client {
            Some(c) => c.clone(),
            None => return Task::none(),
        };
        Task::perform(
            async move {
                // kill-server first (ignore errors — may already be dead)
                let _ = tokio::process::Command::new(&client.adb_path)
                    .arg("kill-server")
                    .status()
                    .await;
                // start-server fresh
                client.start_server().await.map_err(|e| e.to_string())?;
                Ok::<AdbClient, String>(client)
            },
            |result| match result {
                Ok(client) => {
                    tracing::info!("adb daemon restarted successfully");
                    Message::AdbReady(Arc::new(client))
                }
                Err(e) => Message::DaemonStartFailed(e),
            },
        )
    }

    pub(super) fn open_url(&mut self, url: String) -> Task<Message> {
        tracing::info!(url = %url, "opening URL in browser");
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("open").arg(&url).spawn();
        #[cfg(target_os = "windows")]
        let _ = std::process::Command::new("explorer").arg(&url).spawn();
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
        Task::none()
    }
}
