use std::time::SystemTime;

use bridgingio_domain::{
    ApprovalRequestRecord, ArtifactRecord, ConnectionConfig, CredentialRef, PolicyProfile,
    SessionRecord, SessionReusePolicy, TargetKind, TargetProfile,
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
pub struct CoreSettingsView {
    pub schema_version: u32,
    pub instance_name: String,
    pub data_dir: String,
    pub model_plane_http: ModelPlaneHttpView,
    pub control_plane: ControlPlaneView,
    pub artifact_cache: ArtifactCacheSettingsView,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolchainDiagnosticView {
    pub command: String,
    pub selected_source: Option<String>,
    pub selected_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppCommand {
    AttachUi {
        ui_instance_id: String,
        ui_kind: String,
    },
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
                ui_instance_id,
                ui_kind,
            } => {
                base.push_str("|command=attach_ui");
                base.push_str(&format!(
                    "|ui_instance_id={}|ui_kind={}",
                    escape(ui_instance_id),
                    escape(ui_kind)
                ));
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
                if let Some(notes) = profile.notes.as_ref() {
                    base.push_str(&format!("|notes={}", escape(notes)));
                }
                if let Some(credential) = profile.credential_ref.as_ref() {
                    base.push_str(&format!("|credential_ref={}", escape(&credential.id)));
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
                        base.push_str(&format!(
                            "|custom_description={}",
                            escape(description)
                        ));
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
                ui_instance_id: unescape(required(&map, "ui_instance_id")?),
                ui_kind: unescape(required(&map, "ui_kind")?),
            },
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
                model_plane_host: optional(&map, "model_plane_host").map(unescape),
                model_plane_port: parse_optional_u16(optional(&map, "model_plane_port"))?,
                artifact_cache_backend: optional(&map, "artifact_cache_backend").map(unescape),
                artifact_cache_root: optional(&map, "artifact_cache_root").map(unescape),
                artifact_cache_max_bytes: parse_optional_u64(optional(
                    &map,
                    "artifact_cache_max_bytes",
                ))?,
                artifact_cache_eviction_policy: optional(
                    &map,
                    "artifact_cache_eviction_policy",
                )
                .map(unescape),
                tool_override_command: optional(&map, "tool_override_command").map(unescape),
                tool_override_path: optional(&map, "tool_override_path").map(unescape),
            },
            "clear_artifact_cache" => AppCommand::ClearArtifactCache,
            "list_sessions" => AppCommand::ListSessions,
            "list_approvals" => AppCommand::ListApprovals,
            "get_diagnostics" => AppCommand::GetToolchainDiagnostics,
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
            } => format!(
                "kind=settings|request_id={request_id}|schema_version={}|instance_name={}|host={}|port={}|allow_non_loopback={}|auth_mode={}|control_transport={}|artifact_backend={}|artifact_root={}|artifact_max_bytes={}|artifact_eviction_policy={}|artifact_used_bytes={}|artifact_count={}",
                settings.schema_version,
                escape(&settings.instance_name),
                settings.model_plane_http.host,
                settings.model_plane_http.port,
                settings.model_plane_http.allow_non_loopback,
                settings.model_plane_http.auth_mode,
                settings.control_plane.transport,
                settings.artifact_cache.backend,
                escape(&settings.artifact_cache.root),
                settings.artifact_cache.max_bytes,
                settings.artifact_cache.eviction_policy,
                settings.artifact_cache.used_bytes,
                settings.artifact_cache.artifact_count,
            ),
            ApiResponse::Sessions { request_id, items } => {
                format!("kind=sessions|request_id={request_id}|count={}", items.len())
            }
            ApiResponse::Approvals { request_id, items } => {
                format!("kind=approvals|request_id={request_id}|count={}", items.len())
            }
            ApiResponse::Diagnostics { request_id, items } => {
                format!("kind=diagnostics|request_id={request_id}|count={}", items.len())
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
            "settings" => Ok(ApiResponse::Settings {
                request_id,
                settings: CoreSettingsView {
                    schema_version: required(&map, "schema_version")?
                        .parse()
                        .map_err(|_| invalid_request("schema_version must be u32"))?,
                    instance_name: unescape(required(&map, "instance_name")?),
                    data_dir: String::new(),
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
                        enabled: true,
                        transport: required(&map, "control_transport")?.to_string(),
                        endpoint: String::new(),
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

fn optional<'a>(
    map: &'a std::collections::HashMap<String, String>,
    key: &str,
) -> Option<&'a str> {
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
    use bridgingio_domain::SessionReusePolicy;

    use super::{
        ApiRequest, ApiRequestContext, ApiResponse, AppApiLineCodec, AppCommand,
        ArtifactCacheSettingsView, ControlPlaneView, CoreSettingsView, ModelPlaneHttpView,
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
                ui_instance_id: "swiftui-main".into(),
                ui_kind: "swiftui-macos".into(),
            },
        };
        let line = AppApiLineCodec::encode_request_line(&request);
        let parsed = AppApiLineCodec::decode_request_line(&line).expect("decode");
        match parsed.command {
            AppCommand::AttachUi {
                ui_instance_id,
                ui_kind,
            } => {
                assert_eq!(ui_instance_id, "swiftui-main");
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
}
