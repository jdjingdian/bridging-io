use bridgingio_app_api::{
    AgentTokenSummaryView, ApiRequest, ApiRequestContext, ApiResponse, AppApiLineCodec, AppCommand,
    CoreSettingsView,
};
use bridgingio_domain::SessionReusePolicy;

pub trait BridgeTransport {
    fn request_response(&self, request_line: &str) -> Result<String, String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeError {
    Transport(String),
    Protocol(String),
    UnexpectedResponse { expected: &'static str, got: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BridgeIdentity {
    pub host_id: String,
    pub ui_session_id: String,
    pub ui_kind: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SettingsUpdateInput {
    pub core_log_level: Option<String>,
    pub model_plane_host: Option<String>,
    pub model_plane_port: Option<u16>,
    pub artifact_cache_backend: Option<String>,
    pub artifact_cache_root: Option<String>,
    pub artifact_cache_max_bytes: Option<u64>,
    pub artifact_cache_eviction_policy: Option<String>,
    pub tool_override_command: Option<String>,
    pub tool_override_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelPlaneEndpointApplyResult {
    pub apply_strategy: Option<String>,
    pub requires_managed_restart: bool,
    pub requires_management_address_handoff: bool,
}

pub struct TrustedControlPlaneBridge<T: BridgeTransport> {
    transport: T,
    identity: BridgeIdentity,
    request_sequence: u64,
}

impl<T: BridgeTransport> TrustedControlPlaneBridge<T> {
    pub fn new(transport: T, identity: BridgeIdentity) -> Self {
        Self {
            transport,
            identity,
            request_sequence: 0,
        }
    }

    pub fn identity(&self) -> &BridgeIdentity {
        &self.identity
    }

    pub fn routes_through_http_admin_port(&self) -> bool {
        false
    }

    pub fn attach_ui(&mut self) -> Result<(), BridgeError> {
        let response = self.send(AppCommand::AttachUi {
            host_id: self.identity.host_id.clone(),
            ui_session_id: self.identity.ui_session_id.clone(),
            ui_kind: self.identity.ui_kind.clone(),
        })?;
        match response {
            ApiResponse::Attached { .. } => Ok(()),
            other => Err(BridgeError::UnexpectedResponse {
                expected: "attached",
                got: response_kind(&other),
            }),
        }
    }

    pub fn probe_host_instance(&mut self) -> Result<String, BridgeError> {
        let response = self.send(AppCommand::ProbeHostInstance)?;
        match response {
            ApiResponse::HostInstanceProbe { payload_json, .. } => Ok(payload_json),
            other => Err(BridgeError::UnexpectedResponse {
                expected: "host_instance_probe",
                got: response_kind(&other),
            }),
        }
    }

    pub fn get_bootstrap_state(
        &mut self,
        timeline_limit: usize,
        artifact_limit: usize,
        transcript_limit: usize,
    ) -> Result<String, BridgeError> {
        let response = self.send(AppCommand::GetBootstrapState {
            timeline_limit,
            artifact_limit,
            transcript_limit,
        })?;
        match response {
            ApiResponse::Bootstrap { payload_json, .. } => Ok(payload_json),
            other => Err(BridgeError::UnexpectedResponse {
                expected: "bootstrap",
                got: response_kind(&other),
            }),
        }
    }

    pub fn update_model_plane(
        &mut self,
        host: Option<String>,
        port: Option<u16>,
    ) -> Result<Option<String>, BridgeError> {
        self.update_settings(SettingsUpdateInput {
            model_plane_host: host,
            model_plane_port: port,
            ..SettingsUpdateInput::default()
        })
    }

    pub fn update_model_plane_endpoint(
        &mut self,
        host: Option<String>,
        port: Option<u16>,
    ) -> Result<ModelPlaneEndpointApplyResult, BridgeError> {
        let apply_strategy = self.update_model_plane(host, port)?;
        Ok(ModelPlaneEndpointApplyResult {
            requires_managed_restart: apply_strategy.as_deref() == Some("restart_required"),
            requires_management_address_handoff: false,
            apply_strategy,
        })
    }

    pub fn get_settings(&mut self) -> Result<CoreSettingsView, BridgeError> {
        let response = self.send(AppCommand::GetSettings)?;
        match response {
            ApiResponse::Settings { settings, .. } => Ok(settings),
            other => Err(BridgeError::UnexpectedResponse {
                expected: "settings",
                got: response_kind(&other),
            }),
        }
    }

    pub fn update_settings(
        &mut self,
        update: SettingsUpdateInput,
    ) -> Result<Option<String>, BridgeError> {
        let response = self.send(AppCommand::UpdateSettings {
            core_log_level: update.core_log_level,
            model_plane_host: update.model_plane_host,
            model_plane_port: update.model_plane_port,
            artifact_cache_backend: update.artifact_cache_backend,
            artifact_cache_root: update.artifact_cache_root,
            artifact_cache_max_bytes: update.artifact_cache_max_bytes,
            artifact_cache_eviction_policy: update.artifact_cache_eviction_policy,
            tool_override_command: update.tool_override_command,
            tool_override_path: update.tool_override_path,
        })?;
        match response {
            ApiResponse::Accepted { apply_strategy, .. } => Ok(apply_strategy),
            other => Err(BridgeError::UnexpectedResponse {
                expected: "accepted",
                got: response_kind(&other),
            }),
        }
    }

    pub fn list_agent_tokens(&mut self) -> Result<Vec<AgentTokenSummaryView>, BridgeError> {
        let response = self.send(AppCommand::ListAgentTokens)?;
        match response {
            ApiResponse::AgentTokens { items, .. } => Ok(items),
            other => Err(BridgeError::UnexpectedResponse {
                expected: "agent_tokens",
                got: response_kind(&other),
            }),
        }
    }

    pub fn revoke_agent_token(
        &mut self,
        token_id: String,
        reason: Option<String>,
    ) -> Result<AgentTokenSummaryView, BridgeError> {
        let response = self.send(AppCommand::RevokeAgentToken { token_id, reason })?;
        match response {
            ApiResponse::AgentTokenRevoked { summary, .. } => Ok(summary),
            other => Err(BridgeError::UnexpectedResponse {
                expected: "agent_token_revoked",
                got: response_kind(&other),
            }),
        }
    }

    pub fn request_shutdown(&mut self) -> Result<(), BridgeError> {
        let response = self.send(AppCommand::RequestShutdown)?;
        match response {
            ApiResponse::ShutdownAccepted { .. } | ApiResponse::Accepted { .. } => Ok(()),
            other => Err(BridgeError::UnexpectedResponse {
                expected: "shutdown_accepted",
                got: response_kind(&other),
            }),
        }
    }

    pub fn send_command(&mut self, command: AppCommand) -> Result<ApiResponse, BridgeError> {
        self.send(command)
    }

    fn send(&mut self, command: AppCommand) -> Result<ApiResponse, BridgeError> {
        self.request_sequence += 1;
        let request = ApiRequest {
            request_id: format!("desktop-host-{}", self.request_sequence),
            context: ApiRequestContext {
                agent_id: "desktop-host".to_string(),
                run_id: "desktop-run".to_string(),
                client_session_id: self.identity.ui_session_id.clone(),
                reuse_policy: SessionReusePolicy::ReuseIfAlive,
            },
            command,
        };
        let request_line = AppApiLineCodec::encode_request_line(&request);
        let response_line = self
            .transport
            .request_response(&request_line)
            .map_err(BridgeError::Transport)?;
        AppApiLineCodec::decode_response_line(&response_line)
            .map_err(|err| BridgeError::Protocol(err.message))
    }
}

fn response_kind(response: &ApiResponse) -> String {
    match response {
        ApiResponse::Attached { .. } => "attached",
        ApiResponse::HostInstanceProbe { .. } => "host_instance_probe",
        ApiResponse::OwnershipConflict { .. } => "ownership_conflict",
        ApiResponse::Bootstrap { .. } => "bootstrap",
        ApiResponse::Timeline { .. } => "timeline",
        ApiResponse::ArtifactsSnapshot { .. } => "artifacts_snapshot",
        ApiResponse::InteractiveShells { .. } => "interactive_shells",
        ApiResponse::InteractiveTranscript { .. } => "interactive_transcript",
        ApiResponse::ShutdownAccepted { .. } => "shutdown_accepted",
        ApiResponse::NotReady { .. } => "not_ready",
        ApiResponse::Accepted { .. } => "accepted",
        ApiResponse::Targets { .. } => "targets",
        ApiResponse::Profiles { .. } => "profiles",
        ApiResponse::Profile { .. } => "profile",
        ApiResponse::Settings { .. } => "settings",
        ApiResponse::Sessions { .. } => "sessions",
        ApiResponse::Approvals { .. } => "approvals",
        ApiResponse::Diagnostics { .. } => "diagnostics",
        ApiResponse::AgentTokenCreated { .. } => "agent_token_created",
        ApiResponse::AgentTokens { .. } => "agent_tokens",
        ApiResponse::AgentTokenRevoked { .. } => "agent_token_revoked",
        ApiResponse::AgentTokenDeleted { .. } => "agent_token_deleted",
        ApiResponse::AgentTokenLabelUpdated { .. } => "agent_token_label_updated",
        ApiResponse::LocalAdminIntentCreated { .. } => "local_admin_intent_created",
        ApiResponse::LocalAdminAttestationCompleted { .. } => {
            "local_admin_attestation_completed"
        }
        ApiResponse::VaultState { .. } => "vault_state",
        ApiResponse::VaultInitialized { .. } => "vault_initialized",
        ApiResponse::VaultDeleted { .. } => "vault_deleted",
        ApiResponse::VaultUnlocked { .. } => "vault_unlocked",
        ApiResponse::VaultLocked { .. } => "vault_locked",
        ApiResponse::Session { .. } => "session",
        ApiResponse::Execution { .. } => "execution",
        ApiResponse::Artifact { .. } => "artifact",
        ApiResponse::Approval { .. } => "approval",
        ApiResponse::Error { .. } => "error",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::{BridgeIdentity, BridgeTransport, SettingsUpdateInput, TrustedControlPlaneBridge};
    use bridgingio_app_api::{
        ApiResponse, AppApiLineCodec, AppCommand, ArtifactCacheSettingsView, ControlPlaneView,
        CoreSettingsView, ModelPlaneHttpView, RuntimeLogSettingsView, VaultStatusView,
    };
    use bridgingio_domain::SessionReusePolicy;
    use std::cell::RefCell;

    #[derive(Default)]
    struct FakeTransport {
        requests: RefCell<Vec<String>>,
        responses: RefCell<Vec<String>>,
    }

    impl FakeTransport {
        fn push_response(&self, response: ApiResponse) {
            self.responses
                .borrow_mut()
                .push(AppApiLineCodec::encode_response_line(&response));
        }

        fn requests(&self) -> Vec<String> {
            self.requests.borrow().clone()
        }
    }

    impl BridgeTransport for FakeTransport {
        fn request_response(&self, request_line: &str) -> Result<String, String> {
            self.requests.borrow_mut().push(request_line.to_string());
            if self.responses.borrow().is_empty() {
                return Err("missing fake response".to_string());
            }
            Ok(self.responses.borrow_mut().remove(0))
        }
    }

    #[test]
    fn attach_uses_trusted_control_plane_request() {
        let transport = FakeTransport::default();
        transport.push_response(ApiResponse::Attached {
            request_id: "desktop-host-1".to_string(),
            readiness_state: "attached".to_string(),
            model_plane_ready: true,
        });

        let mut bridge = TrustedControlPlaneBridge::new(
            transport,
            BridgeIdentity {
                host_id: "tauri-host-1".to_string(),
                ui_session_id: "tauri-session-1".to_string(),
                ui_kind: "tauri-shell".to_string(),
            },
        );

        bridge.attach_ui().expect("attach");
        assert!(!bridge.routes_through_http_admin_port());

        let request_lines = bridge.transport.requests();
        assert_eq!(request_lines.len(), 1);
        assert!(!request_lines[0].contains("http://"));
        assert!(!request_lines[0].contains("https://"));

        let decoded = AppApiLineCodec::decode_request_line(&request_lines[0]).expect("decode");
        assert_eq!(
            decoded.context.reuse_policy,
            SessionReusePolicy::ReuseIfAlive
        );
        match decoded.command {
            AppCommand::AttachUi {
                host_id,
                ui_session_id,
                ui_kind,
            } => {
                assert_eq!(host_id, "tauri-host-1");
                assert_eq!(ui_session_id, "tauri-session-1");
                assert_eq!(ui_kind, "tauri-shell");
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn update_model_plane_returns_apply_strategy() {
        let transport = FakeTransport::default();
        transport.push_response(ApiResponse::Accepted {
            request_id: "desktop-host-1".to_string(),
            apply_strategy: Some("restart_required".to_string()),
        });

        let mut bridge = TrustedControlPlaneBridge::new(
            transport,
            BridgeIdentity {
                host_id: "tauri-host-1".to_string(),
                ui_session_id: "tauri-session-1".to_string(),
                ui_kind: "tauri-shell".to_string(),
            },
        );

        let strategy = bridge
            .update_model_plane(Some("127.0.0.1".to_string()), Some(19718))
            .expect("update");
        assert_eq!(strategy, Some("restart_required".to_string()));

        let request_line = bridge
            .transport
            .requests()
            .into_iter()
            .next()
            .expect("request");
        let decoded = AppApiLineCodec::decode_request_line(&request_line).expect("decode");
        match decoded.command {
            AppCommand::UpdateSettings {
                model_plane_host,
                model_plane_port,
                ..
            } => {
                assert_eq!(model_plane_host, Some("127.0.0.1".to_string()));
                assert_eq!(model_plane_port, Some(19718));
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn update_model_plane_endpoint_semantics_never_requires_ui_handoff() {
        let transport = FakeTransport::default();
        transport.push_response(ApiResponse::Accepted {
            request_id: "desktop-host-1".to_string(),
            apply_strategy: Some("restart_required".to_string()),
        });

        let mut bridge = TrustedControlPlaneBridge::new(
            transport,
            BridgeIdentity {
                host_id: "tauri-host-1".to_string(),
                ui_session_id: "tauri-session-1".to_string(),
                ui_kind: "tauri-shell".to_string(),
            },
        );

        let result = bridge
            .update_model_plane_endpoint(Some("127.0.0.1".to_string()), Some(19719))
            .expect("update semantics");
        assert_eq!(result.apply_strategy.as_deref(), Some("restart_required"));
        assert!(result.requires_managed_restart);
        assert!(!result.requires_management_address_handoff);
    }

    #[test]
    fn settings_and_token_interfaces_roundtrip() {
        let transport = FakeTransport::default();
        transport.push_response(ApiResponse::Settings {
            request_id: "desktop-host-1".to_string(),
            settings: CoreSettingsView {
                schema_version: 1,
                instance_name: "bridgingio".to_string(),
                data_dir: "/tmp/bridgingio".to_string(),
                model_plane_http: ModelPlaneHttpView {
                    host: "127.0.0.1".to_string(),
                    port: 19718,
                    allow_non_loopback: false,
                    auth_mode: "none".to_string(),
                },
                control_plane: ControlPlaneView {
                    enabled: true,
                    transport: "platform-ipc".to_string(),
                    endpoint: "auto".to_string(),
                },
                artifact_cache: ArtifactCacheSettingsView {
                    backend: "filesystem".to_string(),
                    root: "/tmp/bridgingio/artifacts".to_string(),
                    max_bytes: 1024,
                    eviction_policy: "lru".to_string(),
                    used_bytes: 64,
                    artifact_count: 2,
                },
                runtime_logs: RuntimeLogSettingsView {
                    level: "info".to_string(),
                    root: "/tmp/bridgingio/logs".to_string(),
                    used_bytes: 32,
                },
                vault: VaultStatusView {
                    status: "ready".to_string(),
                    configured_backend: "builtin-encrypted".to_string(),
                    binding_backend: "native-memory-shim".to_string(),
                },
                toolchains: Vec::new(),
            },
        });
        transport.push_response(ApiResponse::AgentTokens {
            request_id: "desktop-host-2".to_string(),
            items: Vec::new(),
        });

        let mut bridge = TrustedControlPlaneBridge::new(
            transport,
            BridgeIdentity {
                host_id: "tauri-host-1".to_string(),
                ui_session_id: "tauri-session-1".to_string(),
                ui_kind: "tauri-shell".to_string(),
            },
        );

        let settings = bridge.get_settings().expect("settings");
        assert_eq!(settings.model_plane_http.port, 19718);
        assert_eq!(settings.vault.status, "ready");
        let tokens = bridge.list_agent_tokens().expect("tokens");
        assert!(tokens.is_empty());
    }

    #[test]
    fn update_settings_supports_log_level_field() {
        let transport = FakeTransport::default();
        transport.push_response(ApiResponse::Accepted {
            request_id: "desktop-host-1".to_string(),
            apply_strategy: Some("restart_required".to_string()),
        });

        let mut bridge = TrustedControlPlaneBridge::new(
            transport,
            BridgeIdentity {
                host_id: "tauri-host-1".to_string(),
                ui_session_id: "tauri-session-1".to_string(),
                ui_kind: "tauri-shell".to_string(),
            },
        );
        let _ = bridge
            .update_settings(SettingsUpdateInput {
                core_log_level: Some("debug".to_string()),
                ..SettingsUpdateInput::default()
            })
            .expect("update settings");

        let request_line = bridge
            .transport
            .requests()
            .into_iter()
            .next()
            .expect("request");
        let decoded = AppApiLineCodec::decode_request_line(&request_line).expect("decode");
        match decoded.command {
            AppCommand::UpdateSettings { core_log_level, .. } => {
                assert_eq!(core_log_level.as_deref(), Some("debug"));
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }
}
