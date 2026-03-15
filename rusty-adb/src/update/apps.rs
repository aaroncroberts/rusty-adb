//! App management handlers: install APK, list packages, uninstall.
use crate::{App, Message};
use iced::Task;

impl App {
    // ── Open / Close ──────────────────────────────────────────────────────────

    // ── Package list ──────────────────────────────────────────────────────────

    /// Spawn `pm list packages -3 -f` and dispatch the result.
    pub(super) fn load_packages(&mut self) -> Task<Message> {
        let Some(client) = self.adb_client.clone() else {
            return Task::none();
        };
        let Some(serial) = self.active_serial.clone() else {
            return Task::none();
        };
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
        self.apps_selected = Some(pkg);
        Task::none()
    }

    // ── Uninstall ─────────────────────────────────────────────────────────────

    pub(super) fn uninstall_app(&mut self) -> Task<Message> {
        let Some(ref pkg) = self.apps_selected.clone() else {
            return Task::none();
        };
        let Some(client) = self.adb_client.clone() else {
            return Task::none();
        };
        let Some(serial) = self.active_serial.clone() else {
            return Task::none();
        };
        let pkg = pkg.clone();
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

    pub(super) fn uninstall_complete(&mut self) -> Task<Message> {
        self.apps_uninstalling = false;
        self.apps_selected = None;
        // Refresh the list so the uninstalled app disappears, then show a toast.
        // Use Task::done to inject a follow-up message rather than calling update() twice.
        self.load_packages().chain(Task::done(Message::ShowToast(
            "App uninstalled".to_string(),
        )))
    }

    pub(super) fn uninstall_failed(&mut self, msg: String) -> Task<Message> {
        self.apps_uninstalling = false;
        tracing::warn!(error = %msg, "uninstall failed");
        self.update(Message::ShowError(format!("Uninstall failed: {msg}")))
    }

    // ── Install APK ───────────────────────────────────────────────────────────

    pub(super) fn install_apk(&mut self) -> Task<Message> {
        // Resolve the first selected entry's path from the local pane
        let local_path = self
            .local_pane
            .selected
            .first()
            .and_then(|&idx| self.local_pane.entries.get(idx))
            .map(|e| e.path.clone());
        let Some(local_path) = local_path else {
            return Task::none();
        };
        let Some(client) = self.adb_client.clone() else {
            return Task::none();
        };
        let Some(serial) = self.active_serial.clone() else {
            return Task::none();
        };
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

    pub(super) fn install_apk_complete(&mut self, name: String) -> Task<Message> {
        self.apps_installing = false;
        tracing::info!(apk = %name, "APK installed");
        self.update(Message::ShowToast(format!("Installed: {name}")))
    }

    pub(super) fn install_apk_failed(&mut self, msg: String) -> Task<Message> {
        self.apps_installing = false;
        tracing::warn!(error = %msg, "APK install failed");
        self.update(Message::ShowError(format!("Install failed: {msg}")))
    }
}
