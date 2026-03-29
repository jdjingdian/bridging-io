use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bridgingio_app_api::{AgentTokenScopeView, AgentTokenSummaryView, ApiResponse, AppCommand};
use bridgingio_desktop_host::bridge::{BridgeError, BridgeIdentity, TrustedControlPlaneBridge};
use bridgingio_desktop_host::bundle::TauriShellHostSpec;
use bridgingio_desktop_host::host::{DesktopShellHost, ManagedRestartStatus, NotificationLevel};
use bridgingio_desktop_host::startup::{resolve_startup_route, StartupRoute};
use bridgingio_desktop_host::storage::{
    validate_runtime_root, RuntimeRootPreferenceStore, RuntimeRootValidationError,
};
use bridgingio_platform::{detect_host_platform_adapter, CapabilityStatus};
use serde::Serialize;
use serde_json::{json, Value};

use crate::bridge_transport::HostBridgeTransport;
use crate::sidecar::SidecarResolution;

#[derive(Clone, Debug, Serialize)]
pub struct StartupStateDto {
    pub route: String,
    pub runtime_root: Option<String>,
    pub transport_status: String,
    pub transport_kind: String,
    pub transport_diagnostics: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RuntimeRootValidationDto {
    pub ok: bool,
    pub message: String,
}

pub struct AppRuntimeState {
    store: RuntimeRootPreferenceStore,
    startup_route: StartupRoute,
    runtime_root: Option<PathBuf>,
    sidecar_resolution: SidecarResolution,
    sidecar_child: Option<Child>,
    bridge: Option<TrustedControlPlaneBridge<HostBridgeTransport>>,
    bridge_attached: bool,
    host_model: DesktopShellHost,
    transport_status: CapabilityStatus,
    transport_kind: String,
    transport_diagnostics: Vec<String>,
    shutdown_invoked: bool,
}

impl AppRuntimeState {
    pub fn new(
        preference_file: PathBuf,
        sidecar_resolution: SidecarResolution,
    ) -> Result<Self, String> {
        let store = RuntimeRootPreferenceStore::new(preference_file);
        let startup_route = resolve_startup_route(&store)
            .map_err(|err| format!("resolve startup route failed: {err}"))?;
        let runtime_root = match &startup_route {
            StartupRoute::Onboarding => None,
            StartupRoute::Recovery {
                saved_runtime_root, ..
            } => Some(saved_runtime_root.clone()),
            StartupRoute::Workspace { runtime_root } => Some(runtime_root.clone()),
        };

        let host_model = DesktopShellHost::new(TauriShellHostSpec::new(
            sidecar_resolution.executable.clone(),
        ));

        let adapter = detect_host_platform_adapter("info");
        let transport = adapter.control_plane_transport();
        let probe_root = runtime_root
            .clone()
            .unwrap_or_else(|| std::env::temp_dir().join("bridgingio-ui-managed-runtime-probe"));
        let runtime_paths = adapter
            .runtime_paths()
            .runtime_paths("bridgingio-ui-managed", &probe_root);
        let transport_diagnostics = transport
            .diagnostics("bridgingio-ui-managed", &runtime_paths)
            .into_iter()
            .map(|diag| {
                format!(
                    "{}:{}:{} | recovery={}",
                    diag.code,
                    diag.status.as_str(),
                    diag.message,
                    diag.recovery_hint
                )
            })
            .collect::<Vec<_>>();

        Ok(Self {
            store,
            startup_route,
            runtime_root,
            sidecar_resolution,
            sidecar_child: None,
            bridge: None,
            bridge_attached: false,
            host_model,
            transport_status: transport.status(),
            transport_kind: transport.transport_kind().to_string(),
            transport_diagnostics,
            shutdown_invoked: false,
        })
    }

    pub fn startup_state(&self) -> StartupStateDto {
        StartupStateDto {
            route: startup_route_label(&self.startup_route).to_string(),
            runtime_root: self
                .runtime_root
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            transport_status: self.transport_status.as_str().to_string(),
            transport_kind: self.transport_kind.clone(),
            transport_diagnostics: self.transport_diagnostics.clone(),
        }
    }

    pub fn validate_runtime_root_input(&self, path: &str) -> RuntimeRootValidationDto {
        let runtime_root = PathBuf::from(path);
        match validate_runtime_root(&runtime_root) {
            Ok(()) => RuntimeRootValidationDto {
                ok: true,
                message: "runtime root is writable".to_string(),
            },
            Err(err) => RuntimeRootValidationDto {
                ok: false,
                message: runtime_root_validation_message(&err),
            },
        }
    }

    pub fn complete_onboarding_runtime_root(&mut self, path: &str) -> Result<(), String> {
        let runtime_root = PathBuf::from(path);
        validate_runtime_root(&runtime_root)
            .map_err(|err| runtime_root_validation_message(&err))?;
        self.store
            .save(&runtime_root)
            .map_err(|err| format!("save runtime root preference failed: {err}"))?;
        self.startup_route = StartupRoute::Workspace {
            runtime_root: runtime_root.clone(),
        };
        self.runtime_root = Some(runtime_root);
        self.bridge = None;
        self.bridge_attached = false;
        self.host_model
            .update_restart_status(ManagedRestartStatus::Idle);
        Ok(())
    }

    pub fn mark_window_open_or_focus(&mut self) {
        self.host_model.open_or_focus_main_window();
    }

    pub fn record_notification(&mut self, title: &str, body: &str) {
        self.host_model
            .push_notification(title, body, NotificationLevel::Info);
    }

    pub fn execute_workspace_command(
        &mut self,
        command: &str,
        payload: Value,
    ) -> Result<Value, String> {
        match command {
            "get_bootstrap_state" => {
                self.ensure_bridge_ready()?;
                let timeline_limit = payload_u64(&payload, "timeline_limit").unwrap_or(120) as usize;
                let artifact_limit = payload_u64(&payload, "artifact_limit").unwrap_or(120) as usize;
                let transcript_limit = payload_u64(&payload, "transcript_limit").unwrap_or(120) as usize;
                let raw = self
                    .bridge_mut()?
                    .get_bootstrap_state(timeline_limit, artifact_limit, transcript_limit)
                    .map_err(format_bridge_error)?;
                match serde_json::from_str(&raw) {
                    Ok(value) => Ok(value),
                    Err(_) => Ok(json!({ "payload_json": raw })),
                }
            }
            "get_settings" => {
                self.ensure_bridge_ready()?;
                let settings = self
                    .bridge_mut()?
                    .get_settings()
                    .map_err(format_bridge_error)?;
                Ok(settings_to_json(&settings))
            }
            "update_settings" => {
                self.ensure_bridge_ready()?;
                let apply_strategy = self
                    .bridge_mut()?
                    .update_settings(bridgingio_desktop_host::bridge::SettingsUpdateInput {
                        core_log_level: payload_string(&payload, "core_log_level"),
                        model_plane_host: payload_string(&payload, "model_plane_host"),
                        model_plane_port: payload_u64(&payload, "model_plane_port").map(|v| v as u16),
                        artifact_cache_backend: payload_string(&payload, "artifact_cache_backend"),
                        artifact_cache_root: payload_string(&payload, "artifact_cache_root"),
                        artifact_cache_max_bytes: payload_u64(&payload, "artifact_cache_max_bytes"),
                        artifact_cache_eviction_policy: payload_string(
                            &payload,
                            "artifact_cache_eviction_policy",
                        ),
                        tool_override_command: payload_string(&payload, "tool_override_command"),
                        tool_override_path: payload_string(&payload, "tool_override_path"),
                    })
                    .map_err(format_bridge_error)?;
                if apply_strategy.as_deref() == Some("restart_required") {
                    self.host_model
                        .update_restart_status(ManagedRestartStatus::RestartRequired);
                } else {
                    self.host_model
                        .update_restart_status(ManagedRestartStatus::Connected);
                }
                Ok(json!({ "apply_strategy": apply_strategy }))
            }
            "list_agent_tokens" => {
                self.ensure_bridge_ready()?;
                let items = self
                    .bridge_mut()?
                    .list_agent_tokens()
                    .map_err(format_bridge_error)?;
                Ok(Value::Array(
                    items.iter().map(agent_token_summary_to_json).collect(),
                ))
            }
            "revoke_agent_token" => {
                self.ensure_bridge_ready()?;
                let token_id = payload_string(&payload, "token_id")
                    .ok_or_else(|| "token_id is required for revoke_agent_token".to_string())?;
                let reason = payload_string(&payload, "reason");
                let summary = self
                    .bridge_mut()?
                    .revoke_agent_token(token_id, reason)
                    .map_err(format_bridge_error)?;
                Ok(agent_token_summary_to_json(&summary))
            }
            "create_agent_token" => {
                self.ensure_bridge_ready()?;
                let label = payload_string(&payload, "label")
                    .ok_or_else(|| "label is required for create_agent_token".to_string())?;
                let expires_in_seconds = payload_u64(&payload, "expires_in_seconds");
                let scope = parse_scope(payload_value(&payload, "scope"))
                    .unwrap_or_else(default_scope);
                let attestation_id = payload_string(&payload, "attestation_id");

                let response = self
                    .bridge_mut()?
                    .send_command(AppCommand::CreateAgentToken {
                        label,
                        expires_in_seconds,
                        scope,
                        attestation_id,
                    })
                    .map_err(format_bridge_error)?;
                match response {
                    ApiResponse::AgentTokenCreated { result, .. } => Ok(json!({
                        "result": {
                            "plaintext_token": result.plaintext_token,
                            "summary": agent_token_summary_to_json(&result.summary),
                        }
                    })),
                    ApiResponse::Error { error, .. } => Err(error.message),
                    other => Err(format!(
                        "unexpected response for create_agent_token: {:?}",
                        other
                    )),
                }
            }
            "read_artifact" => {
                self.ensure_bridge_ready()?;
                let artifact_id = payload_string(&payload, "artifact_id")
                    .ok_or_else(|| "artifact_id is required for read_artifact".to_string())?;
                let offset = payload_u64(&payload, "offset").unwrap_or(0) as usize;
                let limit = payload_u64(&payload, "limit").unwrap_or(120) as usize;
                let response = self
                    .bridge_mut()?
                    .send_command(AppCommand::ReadArtifact {
                        artifact_id,
                        offset,
                        limit,
                    })
                    .map_err(format_bridge_error)?;
                match response {
                    ApiResponse::Artifact { artifact, .. } => Ok(json!({
                        "artifact": {
                            "offset": artifact.offset,
                            "limit": artifact.limit,
                            "total_chunks": artifact.total_chunks,
                            "chunks": artifact.chunks,
                        }
                    })),
                    ApiResponse::Error { error, .. } => Err(error.message),
                    other => Err(format!("unexpected response for read_artifact: {:?}", other)),
                }
            }
            "upsert_profile" => Err(
                "upsert_profile is deferred in current host baseline; trusted bootstrap/read paths are available"
                    .to_string(),
            ),
            "unlock_vault" => Err(
                "unlock_vault is deferred until control-plane command contract is finalized".to_string(),
            ),
            other => Err(format!("unsupported workspace command: {other}")),
        }
    }

    fn ensure_bridge_ready(&mut self) -> Result<(), String> {
        let runtime_root = self.runtime_root.clone().ok_or_else(|| {
            "runtime root is not configured; complete onboarding first".to_string()
        })?;

        if self.transport_status != CapabilityStatus::Ready {
            return Err(format!(
                "local transport is {} (kind={}). Diagnostics: {}",
                self.transport_status.as_str(),
                self.transport_kind,
                self.transport_diagnostics.join(" || ")
            ));
        }

        if self.bridge.is_none() {
            self.bridge = Some(self.build_bridge(&runtime_root));
            self.bridge_attached = false;
        }
        if self.bridge_attached {
            return Ok(());
        }

        // First try to reuse a previously running core process from the same runtime root.
        if self
            .try_attach_with_retries(8, Duration::from_millis(120))
            .is_ok()
        {
            return Ok(());
        }

        if self.sidecar_child.is_none() {
            self.best_effort_shutdown_existing_core(&runtime_root);
        }

        self.spawn_sidecar_if_needed(&runtime_root)?;
        self.bridge = Some(self.build_bridge(&runtime_root));
        self.bridge_attached = false;

        self.try_attach_with_retries(40, Duration::from_millis(150))
    }

    fn spawn_sidecar_if_needed(&mut self, runtime_root: &Path) -> Result<(), String> {
        if let Some(child) = self.sidecar_child.as_mut() {
            match child.try_wait() {
                Ok(None) => return Ok(()),
                Ok(Some(_)) | Err(_) => {
                    self.sidecar_child = None;
                }
            }
        }

        if !self.sidecar_resolution.executable.exists() {
            let hint = self
                .sidecar_resolution
                .candidates
                .iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!(
                "bridgingio-core sidecar not found. resolution_strategy={} candidates=[{}]",
                self.sidecar_resolution.strategy, hint
            ));
        }

        let spec = TauriShellHostSpec::new(self.sidecar_resolution.executable.clone())
            .with_runtime_root(runtime_root);
        let mut command = Command::new(&spec.sidecar.executable);
        command.args(&spec.sidecar.args);
        command.stdin(Stdio::null());
        command.stdout(Stdio::null());
        command.stderr(Stdio::null());

        self.host_model
            .update_restart_status(ManagedRestartStatus::Restarting);
        let child = command
            .spawn()
            .map_err(|err| format!("spawn bridgingio-core sidecar failed: {err}"))?;
        self.sidecar_child = Some(child);
        self.bridge = None;
        self.bridge_attached = false;
        Ok(())
    }

    pub fn shutdown_for_app_exit(&mut self) {
        if self.shutdown_invoked {
            return;
        }
        self.shutdown_invoked = true;

        if self.transport_status == CapabilityStatus::Ready {
            if let Some(runtime_root) = self.runtime_root.clone() {
                self.best_effort_shutdown_existing_core(&runtime_root);
            }
        }

        self.terminate_owned_sidecar();
        self.bridge = None;
        self.bridge_attached = false;
    }

    fn try_attach_with_retries(&mut self, retries: usize, delay: Duration) -> Result<(), String> {
        let mut last_error = None;
        for _ in 0..retries {
            let result = self.bridge_mut()?.attach_ui();
            match result {
                Ok(()) => {
                    self.bridge_attached = true;
                    self.host_model
                        .update_restart_status(ManagedRestartStatus::Connected);
                    return Ok(());
                }
                Err(err) => {
                    last_error = Some(format_bridge_error(err));
                    thread::sleep(delay);
                }
            }
        }
        let error = last_error.unwrap_or_else(|| "attach ui failed".to_string());
        self.host_model
            .update_restart_status(ManagedRestartStatus::Failed(error.clone()));
        Err(error)
    }

    fn best_effort_shutdown_existing_core(&self, runtime_root: &Path) {
        if !control_plane_endpoint_hint_exists(runtime_root) {
            return;
        }

        let mut bridge = self.build_bridge(runtime_root);
        let _ = bridge.attach_ui();
        let _ = bridge.request_shutdown();
        wait_for_control_plane_release(runtime_root, Duration::from_secs(4));
    }

    fn terminate_owned_sidecar(&mut self) {
        if let Some(mut child) = self.sidecar_child.take() {
            match child.try_wait() {
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
    }

    fn build_bridge(&self, runtime_root: &Path) -> TrustedControlPlaneBridge<HostBridgeTransport> {
        let unsupported_reason = if self.transport_status == CapabilityStatus::Ready {
            None
        } else {
            Some(format!(
                "transport unavailable: {} / {}",
                self.transport_kind,
                self.transport_status.as_str()
            ))
        };
        let transport = HostBridgeTransport::from_runtime_root(runtime_root, unsupported_reason);
        let pid = std::process::id();
        let session_stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        TrustedControlPlaneBridge::new(
            transport,
            BridgeIdentity {
                host_id: format!("tauri-host-{pid}"),
                ui_session_id: format!("tauri-session-{session_stamp}"),
                ui_kind: "tauri-shell".to_string(),
            },
        )
    }

    fn bridge_mut(
        &mut self,
    ) -> Result<&mut TrustedControlPlaneBridge<HostBridgeTransport>, String> {
        self.bridge
            .as_mut()
            .ok_or_else(|| "trusted bridge is not initialized".to_string())
    }
}

impl Drop for AppRuntimeState {
    fn drop(&mut self) {
        self.shutdown_for_app_exit();
    }
}

fn startup_route_label(route: &StartupRoute) -> &'static str {
    match route {
        StartupRoute::Onboarding | StartupRoute::Recovery { .. } => "onboarding",
        StartupRoute::Workspace { .. } => "workspace",
    }
}

fn runtime_root_validation_message(err: &RuntimeRootValidationError) -> String {
    format!("runtime root validation failed: {}", err.recovery_hint())
}

fn format_bridge_error(err: BridgeError) -> String {
    match err {
        BridgeError::Transport(message) => format!("transport error: {message}"),
        BridgeError::Protocol(message) => format!("protocol error: {message}"),
        BridgeError::UnexpectedResponse { expected, got } => {
            format!("unexpected response: expected {expected}, got {got}")
        }
    }
}

fn payload_value<'a>(payload: &'a Value, key: &str) -> Option<&'a Value> {
    payload
        .get(key)
        .or_else(|| payload.get("payload").and_then(|inner| inner.get(key)))
}

fn payload_string(payload: &Value, key: &str) -> Option<String> {
    payload_value(payload, key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn payload_u64(payload: &Value, key: &str) -> Option<u64> {
    payload_value(payload, key).and_then(|value| {
        if let Some(raw) = value.as_u64() {
            return Some(raw);
        }
        value.as_str().and_then(|raw| raw.parse::<u64>().ok())
    })
}

fn payload_bool(payload: &Value, key: &str) -> Option<bool> {
    payload_value(payload, key).and_then(Value::as_bool)
}

fn parse_scope(value: Option<&Value>) -> Option<AgentTokenScopeView> {
    let scope = value?;
    let target_ids = scope
        .get("target_ids")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let tool_ids = scope
        .get("tool_ids")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Some(AgentTokenScopeView {
        scope_profile: scope
            .get("scope_profile")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        target_ids,
        tool_ids,
        max_risk_envelope: scope
            .get("max_risk_envelope")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        allow_open_shell: payload_bool(scope, "allow_open_shell"),
        allow_write_shell_input: payload_bool(scope, "allow_write_shell_input"),
        allow_artifact_cross_principal: payload_bool(scope, "allow_artifact_cross_principal"),
        allow_delegation: payload_bool(scope, "allow_delegation"),
        allow_admin_actions: payload_bool(scope, "allow_admin_actions"),
    })
}

fn default_scope() -> AgentTokenScopeView {
    AgentTokenScopeView {
        scope_profile: Some("default".to_string()),
        target_ids: Vec::new(),
        tool_ids: Vec::new(),
        max_risk_envelope: None,
        allow_open_shell: Some(false),
        allow_write_shell_input: Some(false),
        allow_artifact_cross_principal: Some(false),
        allow_delegation: Some(false),
        allow_admin_actions: Some(false),
    }
}

fn settings_to_json(settings: &bridgingio_app_api::CoreSettingsView) -> Value {
    json!({
        "schema_version": settings.schema_version,
        "instance_name": settings.instance_name,
        "data_dir": settings.data_dir,
        "model_plane_http": {
            "host": settings.model_plane_http.host,
            "port": settings.model_plane_http.port,
            "allow_non_loopback": settings.model_plane_http.allow_non_loopback,
            "auth_mode": settings.model_plane_http.auth_mode,
        },
        "control_plane": {
            "enabled": settings.control_plane.enabled,
            "transport": settings.control_plane.transport,
            "endpoint": settings.control_plane.endpoint,
        },
        "artifact_cache": {
            "backend": settings.artifact_cache.backend,
            "root": settings.artifact_cache.root,
            "max_bytes": settings.artifact_cache.max_bytes,
            "eviction_policy": settings.artifact_cache.eviction_policy,
            "used_bytes": settings.artifact_cache.used_bytes,
            "artifact_count": settings.artifact_cache.artifact_count,
        },
        "runtime_logs": {
            "level": settings.runtime_logs.level,
            "root": settings.runtime_logs.root,
            "used_bytes": settings.runtime_logs.used_bytes,
        },
        "vault": {
            "status": settings.vault.status,
            "configured_backend": settings.vault.configured_backend,
            "binding_backend": settings.vault.binding_backend,
        },
    })
}

fn agent_token_summary_to_json(summary: &AgentTokenSummaryView) -> Value {
    json!({
        "token_id": summary.token_id,
        "label": summary.label,
        "principal_summary": summary.principal_summary,
        "status": summary.status,
        "scope_profile": summary.scope_profile,
        "target_scope_summary": summary.target_scope_summary,
        "active_scope_version": summary.active_scope_version,
        "created_at_ms": system_time_ms(summary.created_at),
        "last_used_at_ms": summary.last_used_at.and_then(system_time_ms),
        "expires_at_ms": summary.expires_at.and_then(system_time_ms),
        "revoked_at_ms": summary.revoked_at.and_then(system_time_ms),
        "revoke_reason": summary.revoke_reason,
    })
}

fn system_time_ms(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|value| value.as_millis() as u64)
}

#[cfg(unix)]
fn control_plane_endpoint_hint_exists(runtime_root: &Path) -> bool {
    runtime_root.join("state/control-plane.sock").exists()
}

#[cfg(not(unix))]
fn control_plane_endpoint_hint_exists(_runtime_root: &Path) -> bool {
    true
}

fn wait_for_control_plane_release(runtime_root: &Path, timeout: Duration) {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if !control_plane_endpoint_hint_exists(runtime_root) {
            break;
        }
        thread::sleep(Duration::from_millis(120));
    }
}
