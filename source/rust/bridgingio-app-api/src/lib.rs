use std::time::SystemTime;

use bridgingio_domain::{
    ApprovalRequestRecord, ArtifactRecord, ConnectionConfig, CredentialRef, PolicyProfile,
    SessionRecord, SessionReusePolicy, TargetKind, TargetProfile,
    TARGET_TERMINAL_CONCURRENCY_METADATA_KEY, TARGET_TERMINAL_FAMILY_METADATA_KEY,
    TARGET_TERMINAL_SHELL_METADATA_KEY,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiRequestContext {
    pub agent_id: String,
    pub run_id: String,
    pub client_session_id: String,
    pub reuse_policy: SessionReusePolicy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelPlaneHttpView {
    pub host: String,
    pub port: u16,
    pub allow_non_loopback: bool,
    pub auth_mode: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlPlaneView {
    pub enabled: bool,
    pub transport: String,
    pub endpoint: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactCacheSettingsView {
    pub backend: String,
    pub root: String,
    pub max_bytes: u64,
    pub eviction_policy: String,
    pub used_bytes: u64,
    pub artifact_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolchainSettingsView {
    pub command: String,
    pub path_override: String,
    pub prefer_builtin_fallback: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeLogSettingsView {
    pub level: String,
    pub root: String,
    pub used_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultStatusView {
    pub status: String,
    pub configured_backend: String,
    pub binding_backend: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreSettingsView {
    pub schema_version: u32,
    pub instance_name: String,
    pub data_dir: String,
    pub model_plane_http: ModelPlaneHttpView,
    pub control_plane: ControlPlaneView,
    pub artifact_cache: ArtifactCacheSettingsView,
    pub runtime_logs: RuntimeLogSettingsView,
    pub vault: VaultStatusView,
    pub toolchains: Vec<ToolchainSettingsView>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolchainDiagnosticView {
    pub command: String,
    pub target_id: Option<String>,
    pub target_name: Option<String>,
    pub target_terminal_family: Option<String>,
    pub target_shell_dialect: Option<String>,
    pub target_terminal_concurrency_policy: Option<String>,
    pub target_override_path: Option<String>,
    pub global_override_path: Option<String>,
    pub effective_scope: Option<String>,
    pub effective_source: Option<String>,
    pub effective_path: Option<String>,
    pub selected_source: Option<String>,
    pub selected_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentTokenScopeView {
    pub scope_profile: Option<String>,
    pub target_ids: Vec<String>,
    pub tool_ids: Vec<String>,
    pub max_risk_envelope: Option<String>,
    pub allow_open_shell: Option<bool>,
    pub allow_write_shell_input: Option<bool>,
    pub allow_artifact_cross_principal: Option<bool>,
    pub allow_delegation: Option<bool>,
    pub allow_admin_actions: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentTokenSummaryView {
    pub token_id: String,
    pub label: String,
    pub principal_summary: String,
    pub status: String,
    pub scope_profile: String,
    pub target_scope_summary: String,
    pub active_scope_version: u32,
    pub created_at: SystemTime,
    pub last_used_at: Option<SystemTime>,
    pub expires_at: Option<SystemTime>,
    pub revoked_at: Option<SystemTime>,
    pub revoke_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateAgentTokenResult {
    pub plaintext_token: String,
    pub summary: AgentTokenSummaryView,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimelineSourceGroupSummaryView {
    pub group_key: String,
    pub group_kind: String,
    pub group_label: String,
    pub principal_summary: String,
    pub user_agent_summary: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppCommand {
    AttachUi {
        host_id: String,
        ui_session_id: String,
        ui_kind: String,
    },
    ProbeHostInstance,
    GetBootstrapState {
        timeline_limit: usize,
        artifact_limit: usize,
        transcript_limit: usize,
    },
    GetTimeline {
        limit: usize,
    },
    GetArtifacts {
        limit: usize,
    },
    ListInteractiveShells,
    ReadInteractiveTranscript {
        shell_id: String,
        offset: usize,
        limit: usize,
    },
    RequestShutdown,
    ListTargets,
    ListProfiles,
    GetProfile {
        target_id: String,
    },
    UpsertProfile {
        profile: TargetProfile,
    },
    GetSettings,
    UpdateSettings {
        core_log_level: Option<String>,
        model_plane_host: Option<String>,
        model_plane_port: Option<u16>,
        artifact_cache_backend: Option<String>,
        artifact_cache_root: Option<String>,
        artifact_cache_max_bytes: Option<u64>,
        artifact_cache_eviction_policy: Option<String>,
        tool_override_command: Option<String>,
        tool_override_path: Option<String>,
    },
    ClearArtifactCache,
    ListSessions,
    ListApprovals,
    GetToolchainDiagnostics,
    CreateAgentToken {
        label: String,
        expires_in_seconds: Option<u64>,
        scope: AgentTokenScopeView,
        attestation_id: Option<String>,
    },
    ListAgentTokens,
    RevokeAgentToken {
        token_id: String,
        reason: Option<String>,
    },
    UpdateAgentTokenScope {
        token_id: String,
        scope: AgentTokenScopeView,
        reason: Option<String>,
        attestation_id: Option<String>,
    },
    OpenSession {
        target_id: String,
    },
    Execute {
        session_id: String,
        command: String,
        stream: bool,
    },
    ReadArtifact {
        artifact_id: String,
        offset: usize,
        limit: usize,
    },
    RequestApproval {
        request: ApprovalRequestRecord,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiRequest {
    pub request_id: String,
    pub context: ApiRequestContext,
    pub command: AppCommand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimelineEntry {
    pub id: String,
    pub session_id: String,
    pub command_preview: String,
    pub status: String,
    pub source_group: TimelineSourceGroupSummaryView,
    pub artifact_id: Option<String>,
    pub created_at: SystemTime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactReadView {
    pub record: ArtifactRecord,
    pub offset: usize,
    pub limit: usize,
    pub total_chunks: usize,
    pub chunks: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiEvent {
    SessionStateChanged { session: SessionRecord },
    CommandTimelineEntry { entry: TimelineEntry },
    ApprovalUpdated { request: ApprovalRequestRecord },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiErrorCode {
    NotFound,
    PermissionDenied,
    ValidationFailed,
    DependencyUnavailable,
    Internal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiError {
    pub code: ApiErrorCode,
    pub message: String,
    pub retriable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiResponse {
    Attached {
        request_id: String,
        readiness_state: String,
        model_plane_ready: bool,
    },
    HostInstanceProbe {
        request_id: String,
        payload_json: String,
    },
    OwnershipConflict {
        request_id: String,
        payload_json: String,
    },
    Bootstrap {
        request_id: String,
        payload_json: String,
    },
    Timeline {
        request_id: String,
        payload_json: String,
    },
    ArtifactsSnapshot {
        request_id: String,
        payload_json: String,
    },
    InteractiveShells {
        request_id: String,
        payload_json: String,
    },
    InteractiveTranscript {
        request_id: String,
        shell_id: String,
        offset: usize,
        limit: usize,
        payload_json: String,
    },
    ShutdownAccepted {
        request_id: String,
    },
    NotReady {
        request_id: String,
        reason: String,
    },
    Accepted {
        request_id: String,
        apply_strategy: Option<String>,
    },
    Targets {
        request_id: String,
        items: Vec<TargetProfile>,
    },
    Profiles {
        request_id: String,
        items: Vec<TargetProfile>,
    },
    Profile {
        request_id: String,
        payload_json: String,
    },
    Settings {
        request_id: String,
        settings: CoreSettingsView,
    },
    Sessions {
        request_id: String,
        items: Vec<SessionRecord>,
    },
    Approvals {
        request_id: String,
        items: Vec<ApprovalRequestRecord>,
    },
    Diagnostics {
        request_id: String,
        items: Vec<ToolchainDiagnosticView>,
    },
    AgentTokenCreated {
        request_id: String,
        result: CreateAgentTokenResult,
    },
    AgentTokens {
        request_id: String,
        items: Vec<AgentTokenSummaryView>,
    },
    AgentTokenRevoked {
        request_id: String,
        summary: AgentTokenSummaryView,
    },
    Session {
        request_id: String,
        session: SessionRecord,
    },
    Execution {
        request_id: String,
        artifact: ArtifactRecord,
    },
    Artifact {
        request_id: String,
        artifact: ArtifactReadView,
    },
    Approval {
        request_id: String,
        request: ApprovalRequestRecord,
    },
    Error {
        request_id: String,
        error: ApiError,
    },
}

pub struct AppApiLineCodec;

impl AppApiLineCodec {
    pub fn encode_request_line(request: &ApiRequest) -> String {
        let mut base = format!(
            "request_id={}|agent_id={}|run_id={}|client_session_id={}|reuse_policy={}",
            request.request_id,
            request.context.agent_id,
            request.context.run_id,
            request.context.client_session_id,
            reuse_policy_label(&request.context.reuse_policy),
        );
        match &request.command {
            AppCommand::AttachUi {
                host_id,
                ui_session_id,
                ui_kind,
            } => {
                base.push_str("|command=attach_ui");
                base.push_str(&format!(
                    "|host_id={}|ui_session_id={}|ui_kind={}",
                    escape(host_id),
                    escape(ui_session_id),
                    escape(ui_kind)
                ));
            }
            AppCommand::ProbeHostInstance => {
                base.push_str("|command=probe_host_instance");
            }
            AppCommand::GetBootstrapState {
                timeline_limit,
                artifact_limit,
                transcript_limit,
            } => {
                base.push_str("|command=get_bootstrap_state");
                base.push_str(&format!(
                    "|timeline_limit={timeline_limit}|artifact_limit={artifact_limit}|transcript_limit={transcript_limit}"
                ));
            }
            AppCommand::GetTimeline { limit } => {
                base.push_str("|command=get_timeline");
                base.push_str(&format!("|limit={limit}"));
            }
            AppCommand::GetArtifacts { limit } => {
                base.push_str("|command=get_artifacts");
                base.push_str(&format!("|limit={limit}"));
            }
            AppCommand::ListInteractiveShells => {
                base.push_str("|command=list_interactive_shells");
            }
            AppCommand::ReadInteractiveTranscript {
                shell_id,
                offset,
                limit,
            } => {
                base.push_str("|command=read_interactive_transcript");
                base.push_str(&format!(
                    "|shell_id={}|offset={offset}|limit={limit}",
                    escape(shell_id)
                ));
            }
            AppCommand::RequestShutdown => {
                base.push_str("|command=request_shutdown");
            }
            AppCommand::ListTargets => base.push_str("|command=list_targets"),
            AppCommand::ListProfiles => base.push_str("|command=list_profiles"),
            AppCommand::GetProfile { target_id } => {
                base.push_str("|command=get_profile");
                base.push_str(&format!("|target_id={}", escape(target_id)));
            }
            AppCommand::GetSettings => base.push_str("|command=get_settings"),
            AppCommand::UpdateSettings {
                core_log_level,
                model_plane_host,
                model_plane_port,
                artifact_cache_backend,
                artifact_cache_root,
                artifact_cache_max_bytes,
                artifact_cache_eviction_policy,
                tool_override_command,
                tool_override_path,
            } => {
                base.push_str("|command=update_settings");
                if let Some(value) = core_log_level {
                    base.push_str(&format!("|core_log_level={}", escape(value)));
                }
                if let Some(value) = model_plane_host {
                    base.push_str(&format!("|model_plane_host={}", escape(value)));
                }
                if let Some(value) = model_plane_port {
                    base.push_str(&format!("|model_plane_port={value}"));
                }
                if let Some(value) = artifact_cache_backend {
                    base.push_str(&format!("|artifact_cache_backend={}", escape(value)));
                }
                if let Some(value) = artifact_cache_root {
                    base.push_str(&format!("|artifact_cache_root={}", escape(value)));
                }
                if let Some(value) = artifact_cache_max_bytes {
                    base.push_str(&format!("|artifact_cache_max_bytes={value}"));
                }
                if let Some(value) = artifact_cache_eviction_policy {
                    base.push_str(&format!(
                        "|artifact_cache_eviction_policy={}",
                        escape(value)
                    ));
                }
                if let Some(value) = tool_override_command {
                    base.push_str(&format!("|tool_override_command={}", escape(value)));
                }
                if let Some(value) = tool_override_path {
                    base.push_str(&format!("|tool_override_path={}", escape(value)));
                }
            }
            AppCommand::ClearArtifactCache => base.push_str("|command=clear_artifact_cache"),
            AppCommand::ListSessions => base.push_str("|command=list_sessions"),
            AppCommand::ListApprovals => base.push_str("|command=list_approvals"),
            AppCommand::GetToolchainDiagnostics => base.push_str("|command=get_diagnostics"),
            AppCommand::CreateAgentToken {
                label,
                expires_in_seconds,
                scope,
                attestation_id,
            } => {
                base.push_str("|command=create_agent_token");
                base.push_str(&format!("|label={}", escape(label)));
                if let Some(value) = expires_in_seconds {
                    base.push_str(&format!("|expires_in_seconds={value}"));
                }
                append_scope_fields(&mut base, scope);
                if let Some(value) = attestation_id {
                    base.push_str(&format!("|attestation_id={}", escape(value)));
                }
            }
            AppCommand::ListAgentTokens => base.push_str("|command=list_agent_tokens"),
            AppCommand::RevokeAgentToken { token_id, reason } => {
                base.push_str("|command=revoke_agent_token");
                base.push_str(&format!("|token_id={}", escape(token_id)));
                if let Some(value) = reason {
                    base.push_str(&format!("|reason={}", escape(value)));
                }
            }
            AppCommand::UpdateAgentTokenScope {
                token_id,
                scope,
                reason,
                attestation_id,
            } => {
                base.push_str("|command=update_agent_token_scope");
                base.push_str(&format!("|token_id={}", escape(token_id)));
                append_scope_fields(&mut base, scope);
                if let Some(value) = reason {
                    base.push_str(&format!("|reason={}", escape(value)));
                }
                if let Some(value) = attestation_id {
                    base.push_str(&format!("|attestation_id={}", escape(value)));
                }
            }
            AppCommand::OpenSession { target_id } => {
                base.push_str("|command=open_session");
                base.push_str(&format!("|target_id={target_id}"));
            }
            AppCommand::Execute {
                session_id,
                command,
                stream,
            } => {
                base.push_str("|command=execute");
                base.push_str(&format!(
                    "|session_id={}|exec={}|stream={}",
                    escape(session_id),
                    escape(command),
                    stream
                ));
            }
            AppCommand::ReadArtifact {
                artifact_id,
                offset,
                limit,
            } => {
                base.push_str("|command=read_artifact");
                base.push_str(&format!(
                    "|artifact_id={artifact_id}|offset={offset}|limit={limit}"
                ));
            }
            AppCommand::UpsertProfile { profile } => {
                base.push_str("|command=upsert_profile");
                base.push_str(&format!(
                    "|target_id={}|target_name={}|target_kind={}",
                    profile.id,
                    escape(&profile.name),
                    target_kind_label(&profile.kind),
                ));
                if let Some(alias) = profile.metadata.get("alias") {
                    base.push_str(&format!("|target_alias={}", escape(alias)));
                }
                if let Some(shell) = profile.metadata.get(TARGET_TERMINAL_SHELL_METADATA_KEY) {
                    base.push_str(&format!("|target_terminal_shell={}", escape(shell)));
                }
                if let Some(family) = profile.metadata.get(TARGET_TERMINAL_FAMILY_METADATA_KEY) {
                    base.push_str(&format!("|target_terminal_family={}", escape(family)));
                }
                if let Some(policy) = profile
                    .metadata
                    .get(TARGET_TERMINAL_CONCURRENCY_METADATA_KEY)
                {
                    base.push_str(&format!("|target_terminal_concurrency={}", escape(policy)));
                }
                if let Some(notes) = profile.notes.as_ref() {
                    base.push_str(&format!("|notes={}", escape(notes)));
                }
                if let Some(credential) = profile.credential_ref.as_ref() {
                    base.push_str(&format!("|credential_ref={}", escape(&credential.id)));
                }
                let mut toolchain_entries = profile.toolchains.iter().collect::<Vec<_>>();
                toolchain_entries.sort_by(|a, b| a.0.cmp(b.0));
                base.push_str(&format!(
                    "|target_toolchain_count={}",
                    toolchain_entries.len()
                ));
                for (index, (command, path_override)) in toolchain_entries.iter().enumerate() {
                    base.push_str(&format!(
                        "|target_toolchain_{index}_command={}|target_toolchain_{index}_path_override={}",
                        escape(command),
                        escape(path_override)
                    ));
                }
                match &profile.connection {
                    ConnectionConfig::Ssh {
                        host,
                        port,
                        username,
                    } => {
                        base.push_str(&format!(
                            "|ssh_host={}|ssh_port={}|ssh_username={}",
                            escape(host),
                            port,
                            escape(username)
                        ));
                    }
                    ConnectionConfig::Adb { serial, transport } => {
                        if let Some(serial) = serial.as_ref() {
                            base.push_str(&format!("|adb_serial={}", escape(serial)));
                        }
                        if let Some(transport) = transport.as_ref() {
                            base.push_str(&format!("|adb_transport={}", escape(transport)));
                        }
                    }
                    ConnectionConfig::Serial { device, baud_rate } => {
                        base.push_str(&format!(
                            "|serial_device={}|serial_baud={}",
                            escape(device),
                            baud_rate
                        ));
                    }
                    ConnectionConfig::Docker { container, context } => {
                        base.push_str(&format!("|docker_container={}", escape(container)));
                        if let Some(context) = context.as_ref() {
                            base.push_str(&format!("|docker_context={}", escape(context)));
                        }
                    }
                    ConnectionConfig::Custom { description } => {
                        base.push_str(&format!("|custom_description={}", escape(description)));
                    }
                }
            }
            AppCommand::RequestApproval { request } => {
                base.push_str("|command=request_approval");
                base.push_str(&format!("|approval_id={}", request.id));
            }
        }
        base
    }

    pub fn decode_request_line(line: &str) -> Result<ApiRequest, ApiError> {
        let map = parse_kv_pairs(line);
        let request_id = required(&map, "request_id")?.to_string();
        let context = ApiRequestContext {
            agent_id: required(&map, "agent_id")?.to_string(),
            run_id: required(&map, "run_id")?.to_string(),
            client_session_id: required(&map, "client_session_id")?.to_string(),
            reuse_policy: parse_reuse_policy(required(&map, "reuse_policy")?)?,
        };
        let command = match required(&map, "command")? {
            "attach_ui" => AppCommand::AttachUi {
                host_id: optional(&map, "host_id")
                    .map(unescape)
                    .or_else(|| optional(&map, "ui_instance_id").map(unescape))
                    .ok_or_else(|| invalid_request("missing field: host_id"))?,
                ui_session_id: optional(&map, "ui_session_id")
                    .map(unescape)
                    .or_else(|| optional(&map, "ui_instance_id").map(unescape))
                    .ok_or_else(|| invalid_request("missing field: ui_session_id"))?,
                ui_kind: unescape(required(&map, "ui_kind")?),
            },
            "probe_host_instance" => AppCommand::ProbeHostInstance,
            "get_bootstrap_state" => AppCommand::GetBootstrapState {
                timeline_limit: required(&map, "timeline_limit")?
                    .parse::<usize>()
                    .map_err(|_| invalid_request("timeline_limit must be usize"))?,
                artifact_limit: required(&map, "artifact_limit")?
                    .parse::<usize>()
                    .map_err(|_| invalid_request("artifact_limit must be usize"))?,
                transcript_limit: required(&map, "transcript_limit")?
                    .parse::<usize>()
                    .map_err(|_| invalid_request("transcript_limit must be usize"))?,
            },
            "get_timeline" => AppCommand::GetTimeline {
                limit: required(&map, "limit")?
                    .parse::<usize>()
                    .map_err(|_| invalid_request("limit must be usize"))?,
            },
            "get_artifacts" => AppCommand::GetArtifacts {
                limit: required(&map, "limit")?
                    .parse::<usize>()
                    .map_err(|_| invalid_request("limit must be usize"))?,
            },
            "list_interactive_shells" => AppCommand::ListInteractiveShells,
            "read_interactive_transcript" => AppCommand::ReadInteractiveTranscript {
                shell_id: unescape(required(&map, "shell_id")?),
                offset: required(&map, "offset")?
                    .parse::<usize>()
                    .map_err(|_| invalid_request("offset must be usize"))?,
                limit: required(&map, "limit")?
                    .parse::<usize>()
                    .map_err(|_| invalid_request("limit must be usize"))?,
            },
            "request_shutdown" => AppCommand::RequestShutdown,
            "list_targets" => AppCommand::ListTargets,
            "list_profiles" => AppCommand::ListProfiles,
            "get_profile" => AppCommand::GetProfile {
                target_id: unescape(required(&map, "target_id")?),
            },
            "upsert_profile" => AppCommand::UpsertProfile {
                profile: parse_profile_from_fields(&map)?,
            },
            "get_settings" => AppCommand::GetSettings,
            "update_settings" => AppCommand::UpdateSettings {
                core_log_level: optional(&map, "core_log_level").map(unescape),
                model_plane_host: optional(&map, "model_plane_host").map(unescape),
                model_plane_port: parse_optional_u16(optional(&map, "model_plane_port"))?,
                artifact_cache_backend: optional(&map, "artifact_cache_backend").map(unescape),
                artifact_cache_root: optional(&map, "artifact_cache_root").map(unescape),
                artifact_cache_max_bytes: parse_optional_u64(optional(
                    &map,
                    "artifact_cache_max_bytes",
                ))?,
                artifact_cache_eviction_policy: optional(&map, "artifact_cache_eviction_policy")
                    .map(unescape),
                tool_override_command: optional(&map, "tool_override_command").map(unescape),
                tool_override_path: optional(&map, "tool_override_path").map(unescape),
            },
            "clear_artifact_cache" => AppCommand::ClearArtifactCache,
            "list_sessions" => AppCommand::ListSessions,
            "list_approvals" => AppCommand::ListApprovals,
            "get_diagnostics" => AppCommand::GetToolchainDiagnostics,
            "create_agent_token" => AppCommand::CreateAgentToken {
                label: unescape(required(&map, "label")?),
                expires_in_seconds: parse_optional_u64(optional(&map, "expires_in_seconds"))?,
                scope: parse_scope_from_fields(&map)?,
                attestation_id: optional(&map, "attestation_id").map(unescape),
            },
            "list_agent_tokens" => AppCommand::ListAgentTokens,
            "revoke_agent_token" => AppCommand::RevokeAgentToken {
                token_id: unescape(required(&map, "token_id")?),
                reason: optional(&map, "reason").map(unescape),
            },
            "update_agent_token_scope" => AppCommand::UpdateAgentTokenScope {
                token_id: unescape(required(&map, "token_id")?),
                scope: parse_scope_from_fields(&map)?,
                reason: optional(&map, "reason").map(unescape),
                attestation_id: optional(&map, "attestation_id").map(unescape),
            },
            "open_session" => AppCommand::OpenSession {
                target_id: required(&map, "target_id")?.to_string(),
            },
            "execute" => AppCommand::Execute {
                session_id: required(&map, "session_id")?.to_string(),
                command: unescape(required(&map, "exec")?),
                stream: required(&map, "stream")?
                    .parse::<bool>()
                    .map_err(|_| invalid_request("stream must be boolean"))?,
            },
            "read_artifact" => AppCommand::ReadArtifact {
                artifact_id: required(&map, "artifact_id")?.to_string(),
                offset: required(&map, "offset")?
                    .parse::<usize>()
                    .map_err(|_| invalid_request("offset must be usize"))?,
                limit: required(&map, "limit")?
                    .parse::<usize>()
                    .map_err(|_| invalid_request("limit must be usize"))?,
            },
            "request_approval" => AppCommand::RequestApproval {
                request: ApprovalRequestRecord {
                    id: required(&map, "approval_id")?.to_string(),
                    scope_id: None,
                    logical_session_id: String::new(),
                    channel_id: None,
                    session_id: String::new(),
                    reason: String::new(),
                    command_preview: String::new(),
                    requested_at: SystemTime::UNIX_EPOCH,
                    status: bridgingio_domain::ApprovalStatus::Pending,
                },
            },
            other => return Err(invalid_request(&format!("unsupported command: {other}"))),
        };
        Ok(ApiRequest {
            request_id,
            context,
            command,
        })
    }

    pub fn encode_response_line(response: &ApiResponse) -> String {
        match response {
            ApiResponse::Attached {
                request_id,
                readiness_state,
                model_plane_ready,
            } => format!(
                "kind=attached|request_id={request_id}|readiness_state={}|model_plane_ready={model_plane_ready}",
                escape(readiness_state)
            ),
            ApiResponse::HostInstanceProbe {
                request_id,
                payload_json,
            } => format!(
                "kind=host_instance_probe|request_id={request_id}|payload={}",
                escape(payload_json)
            ),
            ApiResponse::OwnershipConflict {
                request_id,
                payload_json,
            } => format!(
                "kind=ownership_conflict|request_id={request_id}|payload={}",
                escape(payload_json)
            ),
            ApiResponse::Bootstrap {
                request_id,
                payload_json,
            } => format!(
                "kind=bootstrap|request_id={request_id}|payload={}",
                escape(payload_json)
            ),
            ApiResponse::Timeline {
                request_id,
                payload_json,
            } => format!(
                "kind=timeline|request_id={request_id}|payload={}",
                escape(payload_json)
            ),
            ApiResponse::ArtifactsSnapshot {
                request_id,
                payload_json,
            } => format!(
                "kind=artifacts_snapshot|request_id={request_id}|payload={}",
                escape(payload_json)
            ),
            ApiResponse::InteractiveShells {
                request_id,
                payload_json,
            } => format!(
                "kind=interactive_shells|request_id={request_id}|payload={}",
                escape(payload_json)
            ),
            ApiResponse::InteractiveTranscript {
                request_id,
                shell_id,
                offset,
                limit,
                payload_json,
            } => format!(
                "kind=interactive_transcript|request_id={request_id}|shell_id={}|offset={offset}|limit={limit}|payload={}",
                escape(shell_id),
                escape(payload_json)
            ),
            ApiResponse::ShutdownAccepted { request_id } => {
                format!("kind=shutdown_accepted|request_id={request_id}")
            }
            ApiResponse::NotReady { request_id, reason } => format!(
                "kind=not_ready|request_id={request_id}|reason={}",
                escape(reason)
            ),
            ApiResponse::Accepted {
                request_id,
                apply_strategy,
            } => {
                let mut line = format!("kind=accepted|request_id={request_id}");
                if let Some(value) = apply_strategy.as_ref() {
                    line.push_str(&format!("|apply_strategy={}", escape(value)));
                }
                line
            }
            ApiResponse::Targets { request_id, items } => {
                format!("kind=targets|request_id={request_id}|count={}", items.len())
            }
            ApiResponse::Profiles { request_id, items } => {
                format!("kind=profiles|request_id={request_id}|count={}", items.len())
            }
            ApiResponse::Profile {
                request_id,
                payload_json,
            } => format!(
                "kind=profile|request_id={request_id}|payload={}",
                escape(payload_json)
            ),
            ApiResponse::Settings {
                request_id,
                settings,
            } => {
                let mut line = format!(
                    "kind=settings|request_id={request_id}|schema_version={}|instance_name={}|data_dir={}|host={}|port={}|allow_non_loopback={}|auth_mode={}|control_enabled={}|control_transport={}|control_endpoint={}|artifact_backend={}|artifact_root={}|artifact_max_bytes={}|artifact_eviction_policy={}|artifact_used_bytes={}|artifact_count={}|log_level={}|logs_root={}|logs_used_bytes={}|vault_status={}|vault_configured_backend={}|vault_binding_backend={}|toolchain_count={}",
                    settings.schema_version,
                    escape(&settings.instance_name),
                    escape(&settings.data_dir),
                    settings.model_plane_http.host,
                    settings.model_plane_http.port,
                    settings.model_plane_http.allow_non_loopback,
                    settings.model_plane_http.auth_mode,
                    settings.control_plane.enabled,
                    settings.control_plane.transport,
                    escape(&settings.control_plane.endpoint),
                    settings.artifact_cache.backend,
                    escape(&settings.artifact_cache.root),
                    settings.artifact_cache.max_bytes,
                    settings.artifact_cache.eviction_policy,
                    settings.artifact_cache.used_bytes,
                    settings.artifact_cache.artifact_count,
                    escape(&settings.runtime_logs.level),
                    escape(&settings.runtime_logs.root),
                    settings.runtime_logs.used_bytes,
                    escape(&settings.vault.status),
                    escape(&settings.vault.configured_backend),
                    escape(&settings.vault.binding_backend),
                    settings.toolchains.len(),
                );
                for (index, toolchain) in settings.toolchains.iter().enumerate() {
                    line.push_str(&format!(
                        "|toolchain_{index}_command={}|toolchain_{index}_path_override={}|toolchain_{index}_prefer_builtin_fallback={}",
                        escape(&toolchain.command),
                        escape(&toolchain.path_override),
                        toolchain.prefer_builtin_fallback
                    ));
                }
                line
            }
            ApiResponse::Sessions { request_id, items } => {
                format!("kind=sessions|request_id={request_id}|count={}", items.len())
            }
            ApiResponse::Approvals { request_id, items } => {
                format!("kind=approvals|request_id={request_id}|count={}", items.len())
            }
            ApiResponse::Diagnostics { request_id, items } => {
                format!("kind=diagnostics|request_id={request_id}|count={}", items.len())
            }
            ApiResponse::AgentTokenCreated { request_id, result } => {
                let mut line = format!(
                    "kind=agent_token_created|request_id={request_id}|plaintext_token={}",
                    escape(&result.plaintext_token)
                );
                append_agent_token_summary_fields(&mut line, &result.summary, "");
                line
            }
            ApiResponse::AgentTokens { request_id, items } => {
                let mut line = format!(
                    "kind=agent_tokens|request_id={request_id}|count={}",
                    items.len()
                );
                for (index, item) in items.iter().enumerate() {
                    append_agent_token_summary_fields(
                        &mut line,
                        item,
                        &format!("token_{index}_"),
                    );
                }
                line
            }
            ApiResponse::AgentTokenRevoked {
                request_id,
                summary,
            } => {
                let mut line = format!("kind=agent_token_revoked|request_id={request_id}");
                append_agent_token_summary_fields(&mut line, summary, "");
                line
            }
            ApiResponse::Session {
                request_id,
                session,
            } => format!(
                "kind=session|request_id={request_id}|session_id={}|target_id={}|state={:?}",
                session.id, session.target_id, session.state
            ),
            ApiResponse::Execution {
                request_id,
                artifact,
            } => format!(
                "kind=execution|request_id={request_id}|artifact_id={}",
                artifact.id
            ),
            ApiResponse::Artifact {
                request_id,
                artifact,
            } => format!(
                "kind=artifact|request_id={request_id}|artifact_id={}",
                artifact.record.id
            ),
            ApiResponse::Approval {
                request_id,
                request,
            } => format!(
                "kind=approval|request_id={request_id}|approval_id={}",
                request.id
            ),
            ApiResponse::Error { request_id, error } => format!(
                "kind=error|request_id={request_id}|code={:?}|message={}|retriable={}",
                error.code,
                escape(&error.message),
                error.retriable
            ),
        }
    }

    pub fn decode_response_line(line: &str) -> Result<ApiResponse, ApiError> {
        let map = parse_kv_pairs(line);
        let kind = required(&map, "kind")?;
        let request_id = required(&map, "request_id")?.to_string();
        match kind {
            "attached" => Ok(ApiResponse::Attached {
                request_id,
                readiness_state: unescape(required(&map, "readiness_state")?),
                model_plane_ready: required(&map, "model_plane_ready")?
                    .parse()
                    .map_err(|_| invalid_request("model_plane_ready must be bool"))?,
            }),
            "host_instance_probe" => Ok(ApiResponse::HostInstanceProbe {
                request_id,
                payload_json: unescape(required(&map, "payload")?),
            }),
            "ownership_conflict" => Ok(ApiResponse::OwnershipConflict {
                request_id,
                payload_json: unescape(required(&map, "payload")?),
            }),
            "bootstrap" => Ok(ApiResponse::Bootstrap {
                request_id,
                payload_json: unescape(required(&map, "payload")?),
            }),
            "timeline" => Ok(ApiResponse::Timeline {
                request_id,
                payload_json: unescape(required(&map, "payload")?),
            }),
            "artifacts_snapshot" => Ok(ApiResponse::ArtifactsSnapshot {
                request_id,
                payload_json: unescape(required(&map, "payload")?),
            }),
            "interactive_shells" => Ok(ApiResponse::InteractiveShells {
                request_id,
                payload_json: unescape(required(&map, "payload")?),
            }),
            "interactive_transcript" => Ok(ApiResponse::InteractiveTranscript {
                request_id,
                shell_id: unescape(required(&map, "shell_id")?),
                offset: required(&map, "offset")?
                    .parse()
                    .map_err(|_| invalid_request("offset must be usize"))?,
                limit: required(&map, "limit")?
                    .parse()
                    .map_err(|_| invalid_request("limit must be usize"))?,
                payload_json: unescape(required(&map, "payload")?),
            }),
            "shutdown_accepted" => Ok(ApiResponse::ShutdownAccepted { request_id }),
            "not_ready" => Ok(ApiResponse::NotReady {
                request_id,
                reason: unescape(required(&map, "reason")?),
            }),
            "accepted" => Ok(ApiResponse::Accepted {
                request_id,
                apply_strategy: optional(&map, "apply_strategy").map(unescape),
            }),
            "targets" => Ok(ApiResponse::Targets {
                request_id,
                items: Vec::new(),
            }),
            "profiles" => Ok(ApiResponse::Profiles {
                request_id,
                items: Vec::new(),
            }),
            "profile" => Ok(ApiResponse::Profile {
                request_id,
                payload_json: unescape(required(&map, "payload")?),
            }),
            "sessions" => Ok(ApiResponse::Sessions {
                request_id,
                items: Vec::new(),
            }),
            "approvals" => Ok(ApiResponse::Approvals {
                request_id,
                items: Vec::new(),
            }),
            "diagnostics" => Ok(ApiResponse::Diagnostics {
                request_id,
                items: Vec::new(),
            }),
            "agent_token_created" => Ok(ApiResponse::AgentTokenCreated {
                request_id,
                result: CreateAgentTokenResult {
                    plaintext_token: unescape(required(&map, "plaintext_token")?),
                    summary: parse_agent_token_summary_from_fields(&map, "")?,
                },
            }),
            "agent_tokens" => {
                let count = optional(&map, "count")
                    .map(|raw| {
                        raw.parse::<usize>()
                            .map_err(|_| invalid_request("count must be usize"))
                    })
                    .transpose()?
                    .unwrap_or(0);
                let mut items = Vec::with_capacity(count);
                for index in 0..count {
                    items.push(parse_agent_token_summary_from_fields(
                        &map,
                        &format!("token_{index}_"),
                    )?);
                }
                Ok(ApiResponse::AgentTokens { request_id, items })
            }
            "agent_token_revoked" => Ok(ApiResponse::AgentTokenRevoked {
                request_id,
                summary: parse_agent_token_summary_from_fields(&map, "")?,
            }),
            "settings" => Ok(ApiResponse::Settings {
                request_id,
                settings: CoreSettingsView {
                    schema_version: required(&map, "schema_version")?
                        .parse()
                        .map_err(|_| invalid_request("schema_version must be u32"))?,
                    instance_name: unescape(required(&map, "instance_name")?),
                    data_dir: optional(&map, "data_dir").map(unescape).unwrap_or_default(),
                    model_plane_http: ModelPlaneHttpView {
                        host: required(&map, "host")?.to_string(),
                        port: required(&map, "port")?
                            .parse()
                            .map_err(|_| invalid_request("port must be u16"))?,
                        allow_non_loopback: required(&map, "allow_non_loopback")?
                            .parse()
                            .map_err(|_| invalid_request("allow_non_loopback must be bool"))?,
                        auth_mode: required(&map, "auth_mode")?.to_string(),
                    },
                    control_plane: ControlPlaneView {
                        enabled: optional(&map, "control_enabled")
                            .map(|raw| raw.parse::<bool>())
                            .transpose()
                            .map_err(|_| invalid_request("control_enabled must be bool"))?
                            .unwrap_or(true),
                        transport: required(&map, "control_transport")?.to_string(),
                        endpoint: optional(&map, "control_endpoint")
                            .map(unescape)
                            .unwrap_or_default(),
                    },
                    artifact_cache: ArtifactCacheSettingsView {
                        backend: required(&map, "artifact_backend")?.to_string(),
                        root: unescape(required(&map, "artifact_root")?),
                        max_bytes: required(&map, "artifact_max_bytes")?
                            .parse()
                            .map_err(|_| invalid_request("artifact_max_bytes must be u64"))?,
                        eviction_policy: required(&map, "artifact_eviction_policy")?.to_string(),
                        used_bytes: required(&map, "artifact_used_bytes")?
                            .parse()
                            .map_err(|_| invalid_request("artifact_used_bytes must be u64"))?,
                        artifact_count: required(&map, "artifact_count")?
                            .parse()
                            .map_err(|_| invalid_request("artifact_count must be usize"))?,
                    },
                    runtime_logs: RuntimeLogSettingsView {
                        level: optional(&map, "log_level")
                            .map(unescape)
                            .unwrap_or_else(|| "info".to_string()),
                        root: optional(&map, "logs_root")
                            .map(unescape)
                            .unwrap_or_default(),
                        used_bytes: parse_optional_u64(optional(&map, "logs_used_bytes"))?
                            .unwrap_or(0),
                    },
                    vault: VaultStatusView {
                        status: optional(&map, "vault_status")
                            .map(unescape)
                            .unwrap_or_else(|| "unknown".to_string()),
                        configured_backend: optional(&map, "vault_configured_backend")
                            .map(unescape)
                            .unwrap_or_default(),
                        binding_backend: optional(&map, "vault_binding_backend")
                            .map(unescape)
                            .unwrap_or_default(),
                    },
                    toolchains: {
                        let toolchain_count = optional(&map, "toolchain_count")
                            .map(|raw| {
                                raw.parse::<usize>()
                                    .map_err(|_| invalid_request("toolchain_count must be usize"))
                            })
                            .transpose()?
                            .unwrap_or(0);
                        let mut items = Vec::with_capacity(toolchain_count);
                        for index in 0..toolchain_count {
                            items.push(ToolchainSettingsView {
                                command: unescape(required(
                                    &map,
                                    &format!("toolchain_{index}_command"),
                                )?),
                                path_override: unescape(required(
                                    &map,
                                    &format!("toolchain_{index}_path_override"),
                                )?),
                                prefer_builtin_fallback: required(
                                    &map,
                                    &format!("toolchain_{index}_prefer_builtin_fallback"),
                                )?
                                .parse()
                                .map_err(|_| {
                                    invalid_request(
                                        "toolchain_<n>_prefer_builtin_fallback must be bool",
                                    )
                                })?,
                            });
                        }
                        items
                    },
                },
            }),
            "session" => Ok(ApiResponse::Accepted {
                request_id,
                apply_strategy: None,
            }),
            "execution" => Ok(ApiResponse::Accepted {
                request_id,
                apply_strategy: None,
            }),
            "artifact" => Ok(ApiResponse::Accepted {
                request_id,
                apply_strategy: None,
            }),
            "approval" => Ok(ApiResponse::Accepted {
                request_id,
                apply_strategy: None,
            }),
            "error" => Ok(ApiResponse::Error {
                request_id,
                error: ApiError {
                    code: match required(&map, "code")? {
                        "NotFound" => ApiErrorCode::NotFound,
                        "PermissionDenied" => ApiErrorCode::PermissionDenied,
                        "ValidationFailed" => ApiErrorCode::ValidationFailed,
                        "DependencyUnavailable" => ApiErrorCode::DependencyUnavailable,
                        _ => ApiErrorCode::Internal,
                    },
                    message: unescape(required(&map, "message")?),
                    retriable: required(&map, "retriable")?
                        .parse()
                        .map_err(|_| invalid_request("retriable must be bool"))?,
                },
            }),
            _ => Err(invalid_request("unsupported response kind")),
        }
    }
}

fn parse_kv_pairs(line: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for token in line.trim().split('|') {
        if token.is_empty() {
            continue;
        }
        if let Some((k, v)) = token.split_once('=') {
            map.insert(k.to_string(), v.to_string());
        }
    }
    map
}

fn required<'a>(
    map: &'a std::collections::HashMap<String, String>,
    key: &str,
) -> Result<&'a str, ApiError> {
    map.get(key)
        .map(String::as_str)
        .ok_or_else(|| invalid_request(&format!("missing field: {key}")))
}

fn optional<'a>(map: &'a std::collections::HashMap<String, String>, key: &str) -> Option<&'a str> {
    map.get(key).map(String::as_str)
}

fn parse_optional_u16(value: Option<&str>) -> Result<Option<u16>, ApiError> {
    match value {
        Some(raw) => raw
            .parse::<u16>()
            .map(Some)
            .map_err(|_| invalid_request("field must be u16")),
        None => Ok(None),
    }
}

fn parse_optional_u64(value: Option<&str>) -> Result<Option<u64>, ApiError> {
    match value {
        Some(raw) => raw
            .parse::<u64>()
            .map(Some)
            .map_err(|_| invalid_request("field must be u64")),
        None => Ok(None),
    }
}

fn parse_optional_u32(value: Option<&str>) -> Result<Option<u32>, ApiError> {
    match value {
        Some(raw) => raw
            .parse::<u32>()
            .map(Some)
            .map_err(|_| invalid_request("field must be u32")),
        None => Ok(None),
    }
}

fn parse_optional_bool(value: Option<&str>) -> Result<Option<bool>, ApiError> {
    match value {
        Some(raw) => raw
            .parse::<bool>()
            .map(Some)
            .map_err(|_| invalid_request("field must be bool")),
        None => Ok(None),
    }
}

fn parse_optional_system_time_ms(value: Option<&str>) -> Result<Option<SystemTime>, ApiError> {
    match value {
        Some(raw) => raw
            .parse::<u64>()
            .map(|millis| Some(std::time::UNIX_EPOCH + std::time::Duration::from_millis(millis)))
            .map_err(|_| invalid_request("field must be unix milliseconds")),
        None => Ok(None),
    }
}

fn to_unix_millis(value: SystemTime) -> u64 {
    value
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn encode_string_list(values: &[String]) -> String {
    values
        .iter()
        .map(|item| escape(item))
        .collect::<Vec<_>>()
        .join(",")
}

fn parse_string_list(value: Option<&str>) -> Vec<String> {
    value
        .map(|raw| {
            raw.split(',')
                .map(unescape)
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn append_scope_fields(line: &mut String, scope: &AgentTokenScopeView) {
    if let Some(value) = scope.scope_profile.as_ref() {
        line.push_str(&format!("|scope_profile={}", escape(value)));
    }
    if !scope.target_ids.is_empty() {
        line.push_str(&format!(
            "|target_ids={}",
            escape(&encode_string_list(&scope.target_ids))
        ));
    }
    if !scope.tool_ids.is_empty() {
        line.push_str(&format!(
            "|tool_ids={}",
            escape(&encode_string_list(&scope.tool_ids))
        ));
    }
    if let Some(value) = scope.max_risk_envelope.as_ref() {
        line.push_str(&format!("|max_risk_envelope={}", escape(value)));
    }
    if let Some(value) = scope.allow_open_shell {
        line.push_str(&format!("|allow_open_shell={value}"));
    }
    if let Some(value) = scope.allow_write_shell_input {
        line.push_str(&format!("|allow_write_shell_input={value}"));
    }
    if let Some(value) = scope.allow_artifact_cross_principal {
        line.push_str(&format!("|allow_artifact_cross_principal={value}"));
    }
    if let Some(value) = scope.allow_delegation {
        line.push_str(&format!("|allow_delegation={value}"));
    }
    if let Some(value) = scope.allow_admin_actions {
        line.push_str(&format!("|allow_admin_actions={value}"));
    }
}

fn parse_scope_from_fields(
    map: &std::collections::HashMap<String, String>,
) -> Result<AgentTokenScopeView, ApiError> {
    Ok(AgentTokenScopeView {
        scope_profile: optional(map, "scope_profile").map(unescape),
        target_ids: parse_string_list(optional(map, "target_ids").map(unescape).as_deref()),
        tool_ids: parse_string_list(optional(map, "tool_ids").map(unescape).as_deref()),
        max_risk_envelope: optional(map, "max_risk_envelope").map(unescape),
        allow_open_shell: parse_optional_bool(optional(map, "allow_open_shell"))?,
        allow_write_shell_input: parse_optional_bool(optional(map, "allow_write_shell_input"))?,
        allow_artifact_cross_principal: parse_optional_bool(optional(
            map,
            "allow_artifact_cross_principal",
        ))?,
        allow_delegation: parse_optional_bool(optional(map, "allow_delegation"))?,
        allow_admin_actions: parse_optional_bool(optional(map, "allow_admin_actions"))?,
    })
}

fn append_agent_token_summary_fields(
    line: &mut String,
    summary: &AgentTokenSummaryView,
    prefix: &str,
) {
    line.push_str(&format!("|{prefix}token_id={}", escape(&summary.token_id)));
    line.push_str(&format!("|{prefix}label={}", escape(&summary.label)));
    line.push_str(&format!(
        "|{prefix}principal_summary={}",
        escape(&summary.principal_summary)
    ));
    line.push_str(&format!("|{prefix}status={}", escape(&summary.status)));
    line.push_str(&format!(
        "|{prefix}scope_profile={}",
        escape(&summary.scope_profile)
    ));
    line.push_str(&format!(
        "|{prefix}target_scope_summary={}",
        escape(&summary.target_scope_summary)
    ));
    line.push_str(&format!(
        "|{prefix}active_scope_version={}",
        summary.active_scope_version
    ));
    line.push_str(&format!(
        "|{prefix}created_at_ms={}",
        to_unix_millis(summary.created_at)
    ));
    if let Some(value) = summary.last_used_at {
        line.push_str(&format!(
            "|{prefix}last_used_at_ms={}",
            to_unix_millis(value)
        ));
    }
    if let Some(value) = summary.expires_at {
        line.push_str(&format!("|{prefix}expires_at_ms={}", to_unix_millis(value)));
    }
    if let Some(value) = summary.revoked_at {
        line.push_str(&format!("|{prefix}revoked_at_ms={}", to_unix_millis(value)));
    }
    if let Some(value) = summary.revoke_reason.as_ref() {
        line.push_str(&format!("|{prefix}revoke_reason={}", escape(value)));
    }
}

fn parse_agent_token_summary_from_fields(
    map: &std::collections::HashMap<String, String>,
    prefix: &str,
) -> Result<AgentTokenSummaryView, ApiError> {
    let key = |suffix: &str| -> String { format!("{prefix}{suffix}") };
    Ok(AgentTokenSummaryView {
        token_id: unescape(required(map, &key("token_id"))?),
        label: unescape(required(map, &key("label"))?),
        principal_summary: unescape(required(map, &key("principal_summary"))?),
        status: unescape(required(map, &key("status"))?),
        scope_profile: unescape(required(map, &key("scope_profile"))?),
        target_scope_summary: unescape(required(map, &key("target_scope_summary"))?),
        active_scope_version: parse_optional_u32(optional(map, &key("active_scope_version")))?
            .ok_or_else(|| invalid_request("missing field: active_scope_version"))?,
        created_at: parse_optional_system_time_ms(optional(map, &key("created_at_ms")))?
            .ok_or_else(|| invalid_request("missing field: created_at_ms"))?,
        last_used_at: parse_optional_system_time_ms(optional(map, &key("last_used_at_ms")))?,
        expires_at: parse_optional_system_time_ms(optional(map, &key("expires_at_ms")))?,
        revoked_at: parse_optional_system_time_ms(optional(map, &key("revoked_at_ms")))?,
        revoke_reason: optional(map, &key("revoke_reason")).map(unescape),
    })
}

fn parse_reuse_policy(raw: &str) -> Result<SessionReusePolicy, ApiError> {
    match raw {
        "always_new" => Ok(SessionReusePolicy::AlwaysNew),
        "reuse_if_alive" => Ok(SessionReusePolicy::ReuseIfAlive),
        "resume_or_create" => Ok(SessionReusePolicy::ResumeOrCreate),
        _ => Err(invalid_request("invalid reuse_policy")),
    }
}

fn reuse_policy_label(policy: &SessionReusePolicy) -> &'static str {
    match policy {
        SessionReusePolicy::AlwaysNew => "always_new",
        SessionReusePolicy::ReuseIfAlive => "reuse_if_alive",
        SessionReusePolicy::ResumeOrCreate => "resume_or_create",
    }
}

fn parse_profile_from_fields(
    map: &std::collections::HashMap<String, String>,
) -> Result<TargetProfile, ApiError> {
    let id = required(map, "target_id")?.to_string();
    let name = unescape(required(map, "target_name")?);
    let kind = parse_target_kind(required(map, "target_kind")?);
    let connection = match &kind {
        TargetKind::Ssh => ConnectionConfig::Ssh {
            host: unescape(required(map, "ssh_host")?),
            port: required(map, "ssh_port")?
                .parse::<u16>()
                .map_err(|_| invalid_request("ssh_port must be u16"))?,
            username: unescape(required(map, "ssh_username")?),
        },
        TargetKind::Adb => ConnectionConfig::Adb {
            serial: optional(map, "adb_serial").map(unescape),
            transport: optional(map, "adb_transport").map(unescape),
        },
        TargetKind::Serial => ConnectionConfig::Serial {
            device: unescape(required(map, "serial_device")?),
            baud_rate: required(map, "serial_baud")?
                .parse::<u32>()
                .map_err(|_| invalid_request("serial_baud must be u32"))?,
        },
        TargetKind::Docker => ConnectionConfig::Docker {
            container: unescape(required(map, "docker_container")?),
            context: optional(map, "docker_context").map(unescape),
        },
        TargetKind::Other(_) => ConnectionConfig::Custom {
            description: optional(map, "custom_description")
                .map(unescape)
                .unwrap_or_else(|| "custom target".to_string()),
        },
    };

    let mut metadata = std::collections::BTreeMap::new();
    if let Some(alias) = optional(map, "target_alias").map(unescape) {
        if !alias.trim().is_empty() {
            metadata.insert("alias".to_string(), alias);
        }
    }
    if let Some(shell) = optional(map, "target_terminal_shell").map(unescape) {
        let normalized = shell.trim().to_string();
        if !normalized.is_empty() {
            metadata.insert(TARGET_TERMINAL_SHELL_METADATA_KEY.to_string(), normalized);
        }
    }
    if let Some(family) = optional(map, "target_terminal_family").map(unescape) {
        let normalized = family.trim().to_string();
        if !normalized.is_empty() {
            metadata.insert(TARGET_TERMINAL_FAMILY_METADATA_KEY.to_string(), normalized);
        }
    }
    if let Some(policy) = optional(map, "target_terminal_concurrency").map(unescape) {
        let normalized = policy.trim().to_string();
        if !normalized.is_empty() {
            metadata.insert(
                TARGET_TERMINAL_CONCURRENCY_METADATA_KEY.to_string(),
                normalized,
            );
        }
    }

    let mut toolchains = std::collections::BTreeMap::new();
    let count = optional(map, "target_toolchain_count")
        .map(|raw| {
            raw.parse::<usize>()
                .map_err(|_| invalid_request("target_toolchain_count must be usize"))
        })
        .transpose()?
        .unwrap_or(0);
    for index in 0..count {
        let command_key = format!("target_toolchain_{index}_command");
        let path_key = format!("target_toolchain_{index}_path_override");
        let command = unescape(required(map, &command_key)?).trim().to_string();
        let path_override = unescape(required(map, &path_key)?).trim().to_string();
        if command.is_empty() {
            return Err(invalid_request(
                "target toolchain command must be non-empty",
            ));
        }
        toolchains.insert(command, path_override);
    }

    Ok(TargetProfile {
        id,
        name,
        kind,
        connection,
        credential_ref: optional(map, "credential_ref").map(|raw| CredentialRef {
            id: unescape(raw),
            provider: "vault".into(),
        }),
        default_policy: PolicyProfile::default(),
        notes: optional(map, "notes").map(unescape),
        metadata,
        toolchains,
    })
}

fn parse_target_kind(raw: &str) -> TargetKind {
    match raw.trim().to_ascii_lowercase().as_str() {
        "ssh" => TargetKind::Ssh,
        "adb" => TargetKind::Adb,
        "serial" => TargetKind::Serial,
        "docker" => TargetKind::Docker,
        other => TargetKind::Other(other.to_string()),
    }
}

fn target_kind_label(kind: &TargetKind) -> &'static str {
    match kind {
        TargetKind::Ssh => "ssh",
        TargetKind::Adb => "adb",
        TargetKind::Serial => "serial",
        TargetKind::Docker => "docker",
        TargetKind::Other(_) => "custom",
    }
}

fn invalid_request(message: &str) -> ApiError {
    ApiError {
        code: ApiErrorCode::ValidationFailed,
        message: message.to_string(),
        retriable: false,
    }
}

fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('|', "\\p")
        .replace('\n', "\\n")
}

fn unescape(value: &str) -> String {
    value
        .replace("\\n", "\n")
        .replace("\\p", "|")
        .replace("\\\\", "\\")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::SystemTime;

    use bridgingio_domain::{ConnectionConfig, SessionReusePolicy, TargetKind, TargetProfile};

    use super::{
        AgentTokenScopeView, AgentTokenSummaryView, ApiRequest, ApiRequestContext, ApiResponse,
        AppApiLineCodec, AppCommand, ArtifactCacheSettingsView, ControlPlaneView, CoreSettingsView,
        CreateAgentTokenResult, ModelPlaneHttpView, RuntimeLogSettingsView, VaultStatusView,
    };

    #[test]
    fn encodes_and_decodes_execute_request() {
        let request = ApiRequest {
            request_id: "req-1".into(),
            context: ApiRequestContext {
                agent_id: "agent-a".into(),
                run_id: "run-1".into(),
                client_session_id: "client-1".into(),
                reuse_policy: SessionReusePolicy::ReuseIfAlive,
            },
            command: AppCommand::Execute {
                session_id: "session-1".into(),
                command: "echo hello".into(),
                stream: true,
            },
        };

        let line = AppApiLineCodec::encode_request_line(&request);
        let parsed = AppApiLineCodec::decode_request_line(&line).expect("decode");
        assert!(matches!(parsed.command, AppCommand::Execute { .. }));
    }

    #[test]
    fn encodes_and_decodes_attach_request() {
        let request = ApiRequest {
            request_id: "req-attach".into(),
            context: ApiRequestContext {
                agent_id: "ui-agent".into(),
                run_id: "ui-run".into(),
                client_session_id: "ui-client".into(),
                reuse_policy: SessionReusePolicy::ReuseIfAlive,
            },
            command: AppCommand::AttachUi {
                host_id: "swiftui-host".into(),
                ui_session_id: "swiftui-session".into(),
                ui_kind: "swiftui-macos".into(),
            },
        };
        let line = AppApiLineCodec::encode_request_line(&request);
        let parsed = AppApiLineCodec::decode_request_line(&line).expect("decode");
        match parsed.command {
            AppCommand::AttachUi {
                host_id,
                ui_session_id,
                ui_kind,
            } => {
                assert_eq!(host_id, "swiftui-host");
                assert_eq!(ui_session_id, "swiftui-session");
                assert_eq!(ui_kind, "swiftui-macos");
            }
            other => panic!("expected attach_ui command, got {other:?}"),
        }
    }

    #[test]
    fn decodes_legacy_attach_ui_instance_id_as_host_and_session() {
        let line = "request_id=req-attach|agent_id=ui-agent|run_id=ui-run|client_session_id=ui-client|reuse_policy=reuse_if_alive|command=attach_ui|ui_instance_id=swiftui-main|ui_kind=swiftui-macos";
        let parsed = AppApiLineCodec::decode_request_line(line).expect("decode");
        match parsed.command {
            AppCommand::AttachUi {
                host_id,
                ui_session_id,
                ui_kind,
            } => {
                assert_eq!(host_id, "swiftui-main");
                assert_eq!(ui_session_id, "swiftui-main");
                assert_eq!(ui_kind, "swiftui-macos");
            }
            other => panic!("expected attach_ui command, got {other:?}"),
        }
    }

    #[test]
    fn encodes_settings_response_line() {
        let line = AppApiLineCodec::encode_response_line(&ApiResponse::Settings {
            request_id: "r1".into(),
            settings: CoreSettingsView {
                schema_version: 1,
                instance_name: "bridgingio".into(),
                data_dir: "~/.bridgingio".into(),
                model_plane_http: ModelPlaneHttpView {
                    host: "127.0.0.1".into(),
                    port: 19718,
                    allow_non_loopback: false,
                    auth_mode: "none".into(),
                },
                control_plane: ControlPlaneView {
                    enabled: true,
                    transport: "platform-ipc".into(),
                    endpoint: "auto".into(),
                },
                artifact_cache: ArtifactCacheSettingsView {
                    backend: "filesystem".into(),
                    root: "~/.bridgingio/artifacts".into(),
                    max_bytes: 1024,
                    eviction_policy: "lru".into(),
                    used_bytes: 128,
                    artifact_count: 2,
                },
                runtime_logs: RuntimeLogSettingsView {
                    level: "info".into(),
                    root: "~/.bridgingio/logs".into(),
                    used_bytes: 512,
                },
                vault: VaultStatusView {
                    status: "ready".into(),
                    configured_backend: "builtin-encrypted".into(),
                    binding_backend: "native-memory-shim".into(),
                },
                toolchains: Vec::new(),
            },
        });
        assert!(line.contains("kind=settings"));
        assert!(line.contains("port=19718"));
        assert!(line.contains("artifact_backend=filesystem"));
    }

    #[test]
    fn decodes_settings_response_line() {
        let line = "kind=settings|request_id=r1|schema_version=1|instance_name=bridgingio|host=127.0.0.1|port=19718|allow_non_loopback=false|auth_mode=none|control_transport=platform-ipc|artifact_backend=filesystem|artifact_root=~/.bridgingio/artifacts|artifact_max_bytes=2048|artifact_eviction_policy=lru|artifact_used_bytes=256|artifact_count=3";
        let response = AppApiLineCodec::decode_response_line(line).expect("decode");
        match response {
            ApiResponse::Settings { settings, .. } => {
                assert_eq!(settings.model_plane_http.port, 19718);
                assert_eq!(settings.model_plane_http.host, "127.0.0.1");
                assert_eq!(settings.artifact_cache.backend, "filesystem");
                assert_eq!(settings.artifact_cache.max_bytes, 2048);
            }
            other => panic!("expected settings response, got {other:?}"),
        }
    }

    #[test]
    fn encodes_and_decodes_bootstrap_payload_response() {
        let response = ApiResponse::Bootstrap {
            request_id: "boot-1".into(),
            payload_json: "{\"targets\":[]}".into(),
        };
        let line = AppApiLineCodec::encode_response_line(&response);
        let parsed = AppApiLineCodec::decode_response_line(&line).expect("decode response");
        match parsed {
            ApiResponse::Bootstrap { payload_json, .. } => {
                assert_eq!(payload_json, "{\"targets\":[]}");
            }
            other => panic!("expected bootstrap response, got {other:?}"),
        }
    }

    #[test]
    fn encodes_and_decodes_upsert_profile_with_target_toolchains() {
        let mut toolchains = BTreeMap::new();
        toolchains.insert(
            "adb".to_string(),
            "/Applications/AndroidStudio.app/adb".to_string(),
        );
        let request = ApiRequest {
            request_id: "req-upsert-profile".into(),
            context: ApiRequestContext {
                agent_id: "ui-agent".into(),
                run_id: "ui-run".into(),
                client_session_id: "ui-client".into(),
                reuse_policy: SessionReusePolicy::ReuseIfAlive,
            },
            command: AppCommand::UpsertProfile {
                profile: TargetProfile {
                    id: "android-emulator".into(),
                    name: "Android Emulator".into(),
                    kind: TargetKind::Adb,
                    connection: ConnectionConfig::Adb {
                        serial: Some("emulator-5554".into()),
                        transport: Some("serial".into()),
                    },
                    credential_ref: None,
                    default_policy: bridgingio_domain::PolicyProfile::default(),
                    notes: None,
                    metadata: BTreeMap::new(),
                    toolchains,
                },
            },
        };

        let line = AppApiLineCodec::encode_request_line(&request);
        let parsed = AppApiLineCodec::decode_request_line(&line).expect("decode request");
        match parsed.command {
            AppCommand::UpsertProfile { profile } => {
                assert_eq!(
                    profile.toolchains.get("adb").map(String::as_str),
                    Some("/Applications/AndroidStudio.app/adb")
                );
            }
            other => panic!("expected upsert_profile command, got {other:?}"),
        }
    }

    #[test]
    fn encodes_and_decodes_settings_toolchains() {
        let response = ApiResponse::Settings {
            request_id: "req-settings".into(),
            settings: CoreSettingsView {
                schema_version: 1,
                instance_name: "bridgingio".into(),
                data_dir: "~/.bridgingio".into(),
                model_plane_http: ModelPlaneHttpView {
                    host: "127.0.0.1".into(),
                    port: 19718,
                    allow_non_loopback: false,
                    auth_mode: "none".into(),
                },
                control_plane: ControlPlaneView {
                    enabled: true,
                    transport: "platform-ipc".into(),
                    endpoint: "auto".into(),
                },
                artifact_cache: ArtifactCacheSettingsView {
                    backend: "filesystem".into(),
                    root: "~/.bridgingio/artifacts".into(),
                    max_bytes: 1024,
                    eviction_policy: "lru".into(),
                    used_bytes: 128,
                    artifact_count: 2,
                },
                runtime_logs: RuntimeLogSettingsView {
                    level: "info".into(),
                    root: "~/.bridgingio/logs".into(),
                    used_bytes: 512,
                },
                vault: VaultStatusView {
                    status: "ready".into(),
                    configured_backend: "builtin-encrypted".into(),
                    binding_backend: "native-memory-shim".into(),
                },
                toolchains: vec![super::ToolchainSettingsView {
                    command: "adb".into(),
                    path_override: "/opt/homebrew/bin/adb".into(),
                    prefer_builtin_fallback: false,
                }],
            },
        };
        let line = AppApiLineCodec::encode_response_line(&response);
        let parsed = AppApiLineCodec::decode_response_line(&line).expect("decode settings line");
        match parsed {
            ApiResponse::Settings { settings, .. } => {
                assert_eq!(settings.toolchains.len(), 1);
                assert_eq!(settings.toolchains[0].command, "adb");
                assert_eq!(
                    settings.toolchains[0].path_override,
                    "/opt/homebrew/bin/adb"
                );
            }
            other => panic!("expected settings response, got {other:?}"),
        }
    }

    #[test]
    fn encodes_and_decodes_create_agent_token_request() {
        let request = ApiRequest {
            request_id: "req-token-create".into(),
            context: ApiRequestContext {
                agent_id: "ui-agent".into(),
                run_id: "ui-run".into(),
                client_session_id: "ui-client".into(),
                reuse_policy: SessionReusePolicy::ReuseIfAlive,
            },
            command: AppCommand::CreateAgentToken {
                label: "nightly-runner".into(),
                expires_in_seconds: Some(1800),
                scope: AgentTokenScopeView {
                    scope_profile: Some("strict-default".into()),
                    target_ids: vec!["target-a".into(), "target-b".into()],
                    tool_ids: Vec::new(),
                    max_risk_envelope: Some("deny-all".into()),
                    allow_open_shell: Some(false),
                    allow_write_shell_input: Some(false),
                    allow_artifact_cross_principal: Some(false),
                    allow_delegation: Some(false),
                    allow_admin_actions: Some(false),
                },
                attestation_id: Some("attest-001".into()),
            },
        };

        let line = AppApiLineCodec::encode_request_line(&request);
        let parsed = AppApiLineCodec::decode_request_line(&line).expect("decode");
        match parsed.command {
            AppCommand::CreateAgentToken {
                label,
                expires_in_seconds,
                scope,
                attestation_id,
            } => {
                assert_eq!(label, "nightly-runner");
                assert_eq!(expires_in_seconds, Some(1800));
                assert_eq!(scope.scope_profile.as_deref(), Some("strict-default"));
                assert_eq!(scope.target_ids, vec!["target-a", "target-b"]);
                assert_eq!(attestation_id.as_deref(), Some("attest-001"));
            }
            other => panic!("expected create token command, got {other:?}"),
        }
    }

    #[test]
    fn encodes_and_decodes_agent_token_responses() {
        let summary = AgentTokenSummaryView {
            token_id: "token-000001".into(),
            label: "nightly-runner".into(),
            principal_summary: "principal-000001".into(),
            status: "active".into(),
            scope_profile: "strict-default".into(),
            target_scope_summary: "targets:2".into(),
            active_scope_version: 1,
            created_at: SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(10),
            last_used_at: None,
            expires_at: Some(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(3600)),
            revoked_at: None,
            revoke_reason: None,
        };
        let response = ApiResponse::AgentTokenCreated {
            request_id: "req-token-create".into(),
            result: CreateAgentTokenResult {
                plaintext_token: "agt_opaque_token".into(),
                summary: summary.clone(),
            },
        };
        let line = AppApiLineCodec::encode_response_line(&response);
        let parsed = AppApiLineCodec::decode_response_line(&line).expect("decode");
        match parsed {
            ApiResponse::AgentTokenCreated { result, .. } => {
                assert_eq!(result.plaintext_token, "agt_opaque_token");
                assert_eq!(result.summary.token_id, "token-000001");
                assert_eq!(result.summary.scope_profile, "strict-default");
            }
            other => panic!("expected token created response, got {other:?}"),
        }

        let list_line = AppApiLineCodec::encode_response_line(&ApiResponse::AgentTokens {
            request_id: "req-token-list".into(),
            items: vec![summary],
        });
        let list_parsed = AppApiLineCodec::decode_response_line(&list_line).expect("decode list");
        match list_parsed {
            ApiResponse::AgentTokens { items, .. } => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].token_id, "token-000001");
            }
            other => panic!("expected token list response, got {other:?}"),
        }
    }
}
