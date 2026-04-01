use std::path::{Path, PathBuf};

const BUNDLED_INDEX_HTML: &str = include_str!("../../../ui/tauri-console-web/index.html");
const BUNDLED_ONBOARDING_HTML: &str = include_str!("../../../ui/tauri-console-web/onboarding.html");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidecarSpec {
    pub executable: PathBuf,
    pub args: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BundledWebviewSpec {
    pub entrypoint: String,
    pub onboarding_route: String,
    pub workspace_route: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TauriShellHostSpec {
    pub app_id: String,
    pub main_window_label: String,
    pub tray_menu_id: String,
    pub single_window: bool,
    pub webview: BundledWebviewSpec,
    pub sidecar: SidecarSpec,
}

impl TauriShellHostSpec {
    pub fn new(core_sidecar: impl Into<PathBuf>) -> Self {
        Self {
            app_id: "io.bridging.desktop".to_string(),
            main_window_label: "main".to_string(),
            tray_menu_id: "main-tray".to_string(),
            single_window: true,
            webview: BundledWebviewSpec {
                entrypoint: "bundled://index.html".to_string(),
                onboarding_route: "#/onboarding".to_string(),
                workspace_route: "#/workspace".to_string(),
            },
            sidecar: SidecarSpec {
                executable: core_sidecar.into(),
                args: vec!["ui-managed-ephemeral".to_string()],
            },
        }
    }

    pub fn with_runtime_root(mut self, runtime_root: &Path) -> Self {
        self.sidecar.args.extend([
            "--runtime-root".to_string(),
            runtime_root.to_string_lossy().to_string(),
        ]);
        self
    }

    pub fn sidecar_commandline(&self) -> Vec<String> {
        let mut cmd = Vec::with_capacity(self.sidecar.args.len() + 1);
        cmd.push(self.sidecar.executable.to_string_lossy().to_string());
        cmd.extend(self.sidecar.args.iter().cloned());
        cmd
    }

    pub fn bundled_index_html(&self) -> &'static str {
        BUNDLED_INDEX_HTML
    }

    pub fn bundled_onboarding_html(&self) -> &'static str {
        BUNDLED_ONBOARDING_HTML
    }
}

#[cfg(test)]
mod tests {
    use super::TauriShellHostSpec;
    use std::path::Path;

    fn validate_onboarding_ui_contract(html: &str) -> Result<(), String> {
        let required_snippets = [
            "id=\"pick-runtime-root\"",
            "id=\"validate-runtime-root\"",
            "id=\"continue-workspace\"",
            "id=\"runtime-root-path\"",
            "choose runtime root",
            "validate writable access",
            "complete_onboarding_runtime_root",
        ];
        let normalized = html.to_ascii_lowercase();
        for snippet in required_snippets {
            if !normalized.contains(&snippet.to_ascii_lowercase()) {
                return Err(format!(
                    "onboarding ui contract missing required snippet: {snippet}"
                ));
            }
        }

        let forbidden_snippets = ["skip", "cancel", "enter workspace anyway"];
        for snippet in forbidden_snippets {
            if normalized.contains(snippet) {
                return Err(format!(
                    "onboarding ui contract contains forbidden bypass snippet: {snippet}"
                ));
            }
        }

        Ok(())
    }

    fn validate_workspace_markup(html: &str) -> Result<(), String> {
        let required_snippets = [
            "id=\"workspace-shell\"",
            "id=\"nav-targets\"",
            "id=\"nav-timeline\"",
            "id=\"nav-settings\"",
            "id=\"timeline-actors-panel\"",
            "id=\"timeline-cards-panel\"",
            "id=\"timeline-detail-panel\"",
            "id=\"settings-core-runtime\"",
            "id=\"settings-storage-cache\"",
            "id=\"settings-tokens\"",
            "id=\"settings-vault\"",
            "id=\"refresh-tokens-btn\"",
            "id=\"vault-unlock-btn\"",
            "id=\"vault-issue-btn\"",
            "id=\"vault-token-once-panel\"",
            "id=\"vault-copy-token-btn\"",
            "id=\"vault-dismiss-token-btn\"",
            "trusted local verification",
        ];

        let normalized = html.to_ascii_lowercase();
        for snippet in required_snippets {
            if !normalized.contains(&snippet.to_ascii_lowercase()) {
                return Err(format!(
                    "workspace markup missing required snippet: {snippet}"
                ));
            }
        }

        let forbidden_snippets = [
            "workspace shell placeholder",
            "targets / timeline / settings routes",
        ];
        for snippet in forbidden_snippets {
            if normalized.contains(snippet) {
                return Err(format!(
                    "workspace markup still contains placeholder snippet: {snippet}"
                ));
            }
        }

        Ok(())
    }

    #[test]
    fn host_spec_defaults_to_single_window_and_tray() {
        let spec = TauriShellHostSpec::new("/tmp/bridgingio-core");
        assert!(spec.single_window);
        assert_eq!(spec.main_window_label, "main");
        assert_eq!(spec.tray_menu_id, "main-tray");
        assert_eq!(spec.webview.entrypoint, "bundled://index.html");
        assert_eq!(spec.sidecar.args, vec!["ui-managed-ephemeral"]);
    }

    #[test]
    fn sidecar_command_includes_runtime_root_when_selected() {
        let spec = TauriShellHostSpec::new("/tmp/bridgingio-core")
            .with_runtime_root(Path::new("/tmp/runtime-root"));
        let cmd = spec.sidecar_commandline();
        assert_eq!(cmd[0], "/tmp/bridgingio-core");
        assert!(cmd.contains(&"ui-managed-ephemeral".to_string()));
        assert!(cmd.contains(&"--runtime-root".to_string()));
        assert!(cmd.contains(&"/tmp/runtime-root".to_string()));
    }

    #[test]
    fn bundled_webview_assets_are_embedded() {
        let spec = TauriShellHostSpec::new("/tmp/bridgingio-core");
        assert!(spec.bundled_index_html().contains("#/onboarding"));
        validate_workspace_markup(spec.bundled_index_html()).expect("workspace markup contract");
        validate_onboarding_ui_contract(spec.bundled_onboarding_html())
            .expect("onboarding markup contract");
    }

    #[test]
    fn workspace_navigation_and_timeline_grouping_ui_contract() {
        let spec = TauriShellHostSpec::new("/tmp/bridgingio-core");
        let html = spec.bundled_index_html().to_ascii_lowercase();
        for snippet in [
            "window.location.hash = `#/workspace/${button.dataset.view}`",
            "const view = [\"targets\", \"timeline\", \"settings\"]",
            "group_kind: \"token_label\"",
            "group_kind: \"http_fingerprint\"",
            "id=\"timeline-actors-panel\"",
            "id=\"timeline-cards-panel\"",
            "id=\"timeline-detail-panel\"",
        ] {
            assert!(
                html.contains(&snippet.to_ascii_lowercase()),
                "workspace timeline/nav contract missing: {snippet}"
            );
        }
    }

    #[test]
    fn settings_security_ui_contract_covers_token_revoke_and_vault_unlock() {
        let spec = TauriShellHostSpec::new("/tmp/bridgingio-core");
        let html = spec.bundled_index_html().to_ascii_lowercase();
        for snippet in [
            "id=\"settings-tokens\"",
            "id=\"refresh-tokens-btn\"",
            "revoke_agent_token",
            "id=\"settings-vault\"",
            "id=\"vault-unlock-btn\"",
            "requiretrustedverification(\"vault_unlock\")",
            "bridgecommand(\"unlock_vault\"",
            "id=\"vault-token-once-panel\"",
            "id=\"vault-copy-token-btn\"",
            "id=\"vault-dismiss-token-btn\"",
            "function copyissuedtokenresult()",
        ] {
            assert!(
                html.contains(&snippet.to_ascii_lowercase()),
                "settings security contract missing: {snippet}"
            );
        }
    }

    #[test]
    fn settings_security_ui_contract_covers_one_time_token_result_lifecycle() {
        let spec = TauriShellHostSpec::new("/tmp/bridgingio-core");
        let html = spec.bundled_index_html().to_ascii_lowercase();
        for snippet in [
            "one-time token result",
            "this plaintext token is shown once",
            "function renderissuedtokenresult()",
            "function clearissuedtokenresult()",
            "clearissuedtokenresult();",
            "state.issuedtokenresult = null",
            "plaintext token is shown once",
            "token summaries never show plaintext values",
        ] {
            assert!(
                html.contains(&snippet.to_ascii_lowercase()),
                "one-time token result contract missing: {snippet}"
            );
        }
    }

    #[test]
    fn settings_security_ui_contract_covers_locked_state_unlock_and_issue_revoke_flow() {
        let spec = TauriShellHostSpec::new("/tmp/bridgingio-core");
        let html = spec.bundled_index_html().to_ascii_lowercase();
        for snippet in [
            "function rendersettings()",
            "lockstatelower !== \"unlocked\"",
            "lockstatelower === \"unavailable\" || lockstatelower === \"uninitialized\"",
            "vault must be unlocked before issuing a long-lived token.",
            "bridgecommand(\"get_vault_state\"",
            "bridgecommand(\"unlock_vault\"",
            "bridgecommand(\"create_agent_token\"",
            "bridgecommand(\"revoke_agent_token\"",
            "vault unlock flow completed through trusted host verification.",
            "renderissuedtokenresult();",
        ] {
            assert!(
                html.contains(&snippet.to_ascii_lowercase()),
                "settings locked/unlock/token flow contract missing: {snippet}"
            );
        }
    }
}
