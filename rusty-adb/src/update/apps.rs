//! App management handlers: install APK, list packages, uninstall.
use crate::{App, Message};
use iced::Task;

impl App {
    // ── Package list ──────────────────────────────────────────────────────────

    /// Spawn `pm list packages -3 -f` and dispatch the result.
    pub(super) fn load_packages(&mut self) -> Task<Message> {
        let Some(client) = self.adb_client.clone() else {
            return Task::none();
        };
        let Some(serial) = self.active_serial.clone() else {
            return Task::none();
        };
        tracing::info!(serial = %serial, "loading installed packages");
        self.apps_loading = true;
        Task::perform(
            async move {
                client
                    .list_packages(&serial)
                    .await
                    .map_err(|e| e.to_string())
            },
            |result| match result {
                Ok(apps) => Message::AppsLoaded(apps),
                Err(e) => Message::AppsFailed(e),
            },
        )
    }

    pub(super) fn apps_loaded(
        &mut self,
        apps: Vec<crate::adb::InstalledApp>,
    ) -> Task<Message> {
        tracing::info!(count = apps.len(), "installed packages loaded");
        self.apps_loading = false;
        self.installed_apps = Some(apps);
        Task::none()
    }

    pub(super) fn apps_failed(&mut self, msg: String) -> Task<Message> {
        tracing::warn!(error = %msg, "failed to list packages");
        self.apps_loading = false;
        self.update(Message::ShowError(format!("Failed to list apps: {msg}")))
    }

    pub(super) fn apps_select_package(&mut self, pkg: String) -> Task<Message> {
        tracing::debug!(package = %pkg, "app selected for action");
        // Selecting a new package clears any pending confirmation
        self.uninstall_confirm = false;
        self.apps_selected = Some(pkg);
        Task::none()
    }

    // ── Uninstall ─────────────────────────────────────────────────────────────

    /// First click: set the confirmation flag so the UI shows a confirm row.
    pub(super) fn uninstall_app(&mut self) -> Task<Message> {
        let Some(ref pkg) = self.apps_selected else {
            return Task::none();
        };
        tracing::info!(package = %pkg, "uninstall requested — awaiting confirmation");
        self.uninstall_confirm = true;
        Task::none()
    }

    /// Second step: user confirmed — execute `adb uninstall`.
    pub(super) fn uninstall_confirmed(&mut self) -> Task<Message> {
        let Some(ref pkg) = self.apps_selected.clone() else {
            self.uninstall_confirm = false;
            return Task::none();
        };
        let Some(client) = self.adb_client.clone() else {
            self.uninstall_confirm = false;
            return Task::none();
        };
        let Some(serial) = self.active_serial.clone() else {
            self.uninstall_confirm = false;
            return Task::none();
        };
        let pkg = pkg.clone();
        tracing::info!(package = %pkg, serial = %serial, "executing adb uninstall");
        self.uninstall_confirm = false;
        self.apps_uninstalling = true;
        Task::perform(
            async move {
                client
                    .uninstall_package(&serial, &pkg)
                    .await
                    .map_err(|e| e.to_string())
            },
            |result| match result {
                Ok(()) => Message::UninstallComplete,
                Err(e) => Message::UninstallFailed(e),
            },
        )
    }

    /// User cancelled — dismiss the confirmation row.
    pub(super) fn uninstall_cancel(&mut self) -> Task<Message> {
        tracing::debug!("uninstall cancelled by user");
        self.uninstall_confirm = false;
        Task::none()
    }

    pub(super) fn uninstall_complete(&mut self) -> Task<Message> {
        let pkg = self.apps_selected.clone().unwrap_or_default();
        tracing::info!(package = %pkg, "package uninstalled successfully");
        self.apps_uninstalling = false;
        self.apps_selected = None;
        self.uninstall_confirm = false;
        // Refresh the list so the uninstalled app disappears, then show a toast.
        self.load_packages().chain(Task::done(Message::ShowToast(
            "App uninstalled".to_string(),
        )))
    }

    pub(super) fn uninstall_failed(&mut self, msg: String) -> Task<Message> {
        self.apps_uninstalling = false;
        self.uninstall_confirm = false;
        tracing::warn!(error = %msg, "uninstall failed");
        self.update(Message::ShowError(format!("Uninstall failed: {msg}")))
    }

    // ── Install APK ───────────────────────────────────────────────────────────

    /// First click: resolve the APK path and store it for the confirm banner.
    pub(super) fn install_apk(&mut self) -> Task<Message> {
        let local_path = self
            .local_pane
            .selected
            .first()
            .and_then(|&idx| self.local_pane.entries.get(idx))
            .map(|e| e.path.clone());
        let Some(local_path) = local_path else {
            return Task::none();
        };
        tracing::info!(
            path = %local_path.display(),
            "install APK requested — awaiting confirmation"
        );
        self.install_apk_confirm = Some(local_path);
        Task::none()
    }

    /// Second step: user confirmed — execute `adb install -r`.
    pub(super) fn install_apk_confirmed(&mut self) -> Task<Message> {
        let Some(local_path) = self.install_apk_confirm.take() else {
            return Task::none();
        };
        let Some(client) = self.adb_client.clone() else {
            return Task::none();
        };
        let Some(serial) = self.active_serial.clone() else {
            return Task::none();
        };
        tracing::info!(
            path = %local_path.display(),
            serial = %serial,
            "executing adb install"
        );
        self.apps_installing = true;
        Task::perform(
            async move {
                client
                    .install_apk(&serial, &local_path)
                    .await
                    .map_err(|e| e.to_string())
                    .map(|()| {
                        local_path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    })
            },
            |result| match result {
                Ok(name) => Message::InstallApkComplete(name),
                Err(e) => Message::InstallApkFailed(e),
            },
        )
    }

    /// User cancelled the install APK confirmation banner.
    pub(super) fn install_apk_cancel(&mut self) -> Task<Message> {
        tracing::debug!("APK install cancelled by user");
        self.install_apk_confirm = None;
        Task::none()
    }

    pub(super) fn install_apk_complete(&mut self, name: String) -> Task<Message> {
        self.apps_installing = false;
        tracing::info!(apk = %name, "APK installed successfully");
        self.update(Message::ShowToast(format!("Installed: {name}")))
    }

    pub(super) fn install_apk_failed(&mut self, msg: String) -> Task<Message> {
        self.apps_installing = false;
        tracing::warn!(error = %msg, "APK install failed");
        self.update(Message::ShowError(format!("Install failed: {msg}")))
    }
}
