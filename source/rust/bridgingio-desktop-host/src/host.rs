use crate::bundle::TauriShellHostSpec;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManagedRestartStatus {
    Idle,
    Saving,
    RestartRequired,
    Restarting,
    Connected,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecoveryAction {
    RetryRestart,
    OpenLogs,
    ReconfigureRuntimeRoot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostNotification {
    pub title: String,
    pub body: String,
    pub level: NotificationLevel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MainWindowState {
    pub label: String,
    pub is_open: bool,
    pub is_focused: bool,
    pub open_count: usize,
}

pub struct DesktopShellHost {
    spec: TauriShellHostSpec,
    window: MainWindowState,
    restart_status: ManagedRestartStatus,
    notifications: Vec<HostNotification>,
    recovery_actions: Vec<RecoveryAction>,
}

impl DesktopShellHost {
    pub fn new(spec: TauriShellHostSpec) -> Self {
        let window = MainWindowState {
            label: spec.main_window_label.clone(),
            is_open: false,
            is_focused: false,
            open_count: 0,
        };
        Self {
            spec,
            window,
            restart_status: ManagedRestartStatus::Idle,
            notifications: Vec::new(),
            recovery_actions: Vec::new(),
        }
    }

    pub fn spec(&self) -> &TauriShellHostSpec {
        &self.spec
    }

    pub fn window_state(&self) -> &MainWindowState {
        &self.window
    }

    pub fn restart_status(&self) -> &ManagedRestartStatus {
        &self.restart_status
    }

    pub fn notifications(&self) -> &[HostNotification] {
        &self.notifications
    }

    pub fn recovery_actions(&self) -> &[RecoveryAction] {
        &self.recovery_actions
    }

    pub fn open_or_focus_main_window(&mut self) {
        if !self.window.is_open {
            self.window.is_open = true;
            self.window.open_count = 1;
        }
        self.window.is_focused = true;
    }

    pub fn tray_activate(&mut self) {
        self.open_or_focus_main_window();
    }

    pub fn mark_window_blurred(&mut self) {
        self.window.is_focused = false;
    }

    pub fn push_notification(
        &mut self,
        title: impl Into<String>,
        body: impl Into<String>,
        level: NotificationLevel,
    ) {
        self.notifications.push(HostNotification {
            title: title.into(),
            body: body.into(),
            level,
        });
    }

    pub fn update_restart_status(&mut self, status: ManagedRestartStatus) {
        self.restart_status = status;
        self.recovery_actions.clear();
        if let ManagedRestartStatus::Failed(_) = self.restart_status {
            self.recovery_actions.extend([
                RecoveryAction::RetryRestart,
                RecoveryAction::OpenLogs,
                RecoveryAction::ReconfigureRuntimeRoot,
            ]);
        }
    }

    pub fn restart_status_badge(&self) -> &'static str {
        match self.restart_status {
            ManagedRestartStatus::Idle => "idle",
            ManagedRestartStatus::Saving => "saving",
            ManagedRestartStatus::RestartRequired => "restart_required",
            ManagedRestartStatus::Restarting => "restarting",
            ManagedRestartStatus::Connected => "connected",
            ManagedRestartStatus::Failed(_) => "failed",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DesktopShellHost, ManagedRestartStatus, NotificationLevel, RecoveryAction};
    use crate::bundle::TauriShellHostSpec;

    #[test]
    fn tray_activation_keeps_single_main_window() {
        let spec = TauriShellHostSpec::new("/tmp/bridgingio-core");
        let mut host = DesktopShellHost::new(spec);

        host.tray_activate();
        host.mark_window_blurred();
        host.tray_activate();

        assert!(host.window_state().is_open);
        assert!(host.window_state().is_focused);
        assert_eq!(host.window_state().open_count, 1);
    }

    #[test]
    fn restart_failure_publishes_recovery_actions() {
        let spec = TauriShellHostSpec::new("/tmp/bridgingio-core");
        let mut host = DesktopShellHost::new(spec);

        host.update_restart_status(ManagedRestartStatus::Failed("attach timeout".to_string()));
        assert_eq!(host.restart_status_badge(), "failed");
        assert_eq!(
            host.recovery_actions(),
            &[
                RecoveryAction::RetryRestart,
                RecoveryAction::OpenLogs,
                RecoveryAction::ReconfigureRuntimeRoot,
            ]
        );
    }

    #[test]
    fn host_can_emit_system_notifications() {
        let spec = TauriShellHostSpec::new("/tmp/bridgingio-core");
        let mut host = DesktopShellHost::new(spec);

        host.push_notification(
            "Managed restart",
            "Core restarted successfully",
            NotificationLevel::Info,
        );

        assert_eq!(host.notifications().len(), 1);
        assert_eq!(host.notifications()[0].title, "Managed restart");
        assert_eq!(host.notifications()[0].body, "Core restarted successfully");
    }
}
