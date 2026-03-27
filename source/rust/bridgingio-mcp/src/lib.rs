use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use bridgingio_app_api::{
    ApiError, ApiErrorCode, ApiRequest, ApiResponse, AppApiLineCodec, AppCommand,
    ArtifactCacheSettingsView, ArtifactReadView, ControlPlaneView, CoreSettingsView,
    ModelPlaneHttpView, TimelineEntry, ToolchainDiagnosticView,
};
use bridgingio_artifacts::{
    ArtifactCacheBackend, ArtifactEvictionPolicy, ArtifactReadResult, ArtifactRefineMode,
    ArtifactStore, ArtifactStoreConfig,
};
use bridgingio_connectors::{ExecutableSource, ToolchainResolver};
use bridgingio_domain::{
    AccessScope, ArtifactRecord, CapabilitySummary, ChannelKind, ChannelStatus, ConnectionConfig,
    SessionRecord, SessionReusePolicy, SessionState, TargetKind, TargetProfile,
};
use bridgingio_engine::{
    CoreSettings, CoreSettingsStore, StandaloneConnectionSection, StandaloneTargetProfile,
    ToolchainSection,
};
use bridgingio_policy::{evaluate, OperationKind, PolicyDecision};
use bridgingio_providers::{GitProvider, TerminalProvider};
use bridgingio_secrets::{SecretVaultRouter, VaultError};
use serde_json::{json, Value};

#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityEnvelope {
    pub target_id: String,
    pub session_id: Option<String>,
    pub captured_at: SystemTime,
    pub capabilities: Vec<CapabilitySummary>,
}

pub trait CapabilityDiscovery {
    fn describe_target(
        &self,
        target: &TargetProfile,
        session: Option<&SessionRecord>,
    ) -> CapabilityEnvelope;
}

#[derive(Default)]
pub struct DefaultCapabilityDiscovery;

impl CapabilityDiscovery for DefaultCapabilityDiscovery {
    fn describe_target(
        &self,
        target: &TargetProfile,
        session: Option<&SessionRecord>,
    ) -> CapabilityEnvelope {
        let capabilities = session
            .and_then(|s| s.fingerprint.clone())
            .map(|f| f.capabilities)
            .unwrap_or_else(|| infer_capabilities(&target.kind));

        CapabilityEnvelope {
            target_id: target.id.clone(),
            session_id: session.map(|s| s.id.clone()),
            captured_at: SystemTime::now(),
            capabilities,
        }
    }
}

fn infer_capabilities(kind: &TargetKind) -> Vec<CapabilitySummary> {
    let artifact_capability = CapabilitySummary {
        id: "artifact.reanalysis".into(),
        label: "artifact read and reanalysis".into(),
        supports_streaming: true,
        supports_file_transfer: false,
        requires_approval: false,
        typed_entrypoints: vec!["artifacts.read".into(), "artifacts.refine".into()],
        raw_fallback: false,
    };
    match kind {
        TargetKind::Ssh => vec![
            CapabilitySummary {
                id: "terminal.exec".into(),
                label: "execute shell command".into(),
                supports_streaming: true,
                supports_file_transfer: true,
                requires_approval: true,
                typed_entrypoints: vec![
                    "terminal.exec".into(),
                    "terminal.shell.open".into(),
                    "terminal.shell.write".into(),
                    "terminal.shell.read".into(),
                    "terminal.shell.interrupt".into(),
                    "terminal.shell.close".into(),
                ],
                raw_fallback: true,
            },
            artifact_capability,
            CapabilitySummary {
                id: "git.query".into(),
                label: "query repository state".into(),
                supports_streaming: false,
                supports_file_transfer: false,
                requires_approval: false,
                typed_entrypoints: vec!["git.status".into(), "git.diff".into()],
                raw_fallback: false,
            },
        ],
        TargetKind::Adb => vec![
            CapabilitySummary {
                id: "terminal.exec".into(),
                label: "execute adb shell command".into(),
                supports_streaming: true,
                supports_file_transfer: true,
                requires_approval: true,
                typed_entrypoints: vec![
                    "terminal.exec".into(),
                    "terminal.shell.open".into(),
                    "terminal.shell.write".into(),
                    "terminal.shell.read".into(),
                    "terminal.shell.interrupt".into(),
                    "terminal.shell.close".into(),
                ],
                raw_fallback: true,
            },
            artifact_capability,
        ],
        _ => vec![
            CapabilitySummary {
                id: "terminal.exec".into(),
                label: "execute command".into(),
                supports_streaming: false,
                supports_file_transfer: false,
                requires_approval: true,
                typed_entrypoints: vec!["terminal.exec".into()],
                raw_fallback: true,
            },
            artifact_capability,
        ],
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolRequestContext {
    pub agent_id: String,
    pub run_id: String,
    pub client_session_id: String,
    pub reuse_policy: SessionReusePolicy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolRequest {
    DescribeCapabilities {
        target: TargetProfile,
        session: Option<SessionRecord>,
    },
    TerminalExec {
        target_id: String,
        target_kind: TargetKind,
        context: ToolRequestContext,
        command: String,
        artifact_id: String,
    },
    ArtifactsRead {
        artifact_id: String,
        offset: usize,
        limit: usize,
    },
    ArtifactsRefine {
        source_artifact_id: String,
        pattern: String,
        mode: String,
        ignore_case: bool,
        processing_mode: String,
    },
    GitStatus {
        repo_path: String,
    },
    GitDiff {
        repo_path: String,
        reference: String,
    },
    GitLog {
        repo_path: String,
        max_count: usize,
    },
    TerminalExecRaw {
        target_id: String,
        target_kind: TargetKind,
        context: ToolRequestContext,
        command: String,
        artifact_id: String,
        operation: OperationKind,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolResult {
    Capabilities {
        envelope: CapabilityEnvelope,
    },
    Execution {
        artifact_id: String,
        logical_session_id: String,
        channel_id: String,
    },
    ArtifactRefined {
        artifact: ArtifactRecord,
        requested_processing_mode: String,
        resolved_processing_mode: String,
    },
    ArtifactRead {
        view: ArtifactReadResult,
    },
    GitOutput {
        output: String,
    },
    ApprovalRequired {
        reason: String,
    },
    Error {
        message: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArtifactProcessingMode {
    Source,
    Bridgingio,
    Auto,
}

impl ArtifactProcessingMode {
    fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "source" => Ok(Self::Source),
            "bridgingio" => Ok(Self::Bridgingio),
            "auto" => Ok(Self::Auto),
            other => Err(format!("invalid processing_mode: {other}")),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Bridgingio => "bridgingio",
            Self::Auto => "auto",
        }
    }

    fn resolve(self) -> Self {
        // Artifacts.refine works against cached text, so current execution side is BridgingIO.
        match self {
            Self::Source | Self::Bridgingio | Self::Auto => Self::Bridgingio,
        }
    }
}

struct ArtifactService {
    store: ArtifactStore,
}

impl ArtifactService {
    fn new(store: ArtifactStore) -> Self {
        Self { store }
    }

    fn usage(&self) -> bridgingio_artifacts::ArtifactStoreUsage {
        self.store.usage()
    }

    fn ingest_raw_artifact(
        &mut self,
        record: ArtifactRecord,
        chunks: Vec<String>,
    ) -> Result<ArtifactRecord, String> {
        self.store
            .ingest_raw_artifact(record, chunks)
            .map_err(|err| err.message)
    }

    fn read_artifact(
        &mut self,
        artifact_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<ArtifactReadResult, String> {
        self.store
            .read_artifact(artifact_id, offset, limit)
            .map_err(|err| err.message)
    }

    fn ingest_from_terminal_provider(
        &mut self,
        terminal_provider: &TerminalProvider,
        artifact_id: &str,
    ) -> Option<ArtifactRecord> {
        let record = terminal_provider
            .artifacts
            .artifacts
            .get(artifact_id)?
            .clone();
        let chunks = terminal_provider
            .artifacts
            .chunks
            .get(artifact_id)
            .cloned()
            .unwrap_or_default();
        self.ingest_raw_artifact(record, chunks).ok()
    }

    fn refine_with_filter(
        &mut self,
        source_artifact_id: &str,
        pattern: &str,
        mode_label: &str,
        ignore_case: bool,
        processing_mode: ArtifactProcessingMode,
    ) -> Result<(ArtifactRecord, ArtifactProcessingMode), String> {
        let refine_mode = parse_artifact_refine_mode(mode_label)?;
        let resolved_processing_mode = processing_mode.resolve();
        let record = self
            .store
            .refine_artifact(
                source_artifact_id,
                pattern,
                refine_mode,
                ignore_case,
                SystemTime::now(),
            )
            .map_err(|err| err.message)?;
        Ok((record, resolved_processing_mode))
    }
}

pub struct McpToolHandler {
    discovery: DefaultCapabilityDiscovery,
    terminal_provider: TerminalProvider,
    artifact_service: ArtifactService,
    metadata: bridgingio_engine::InMemoryMetadataStore,
}

impl Default for McpToolHandler {
    fn default() -> Self {
        Self::with_artifact_store(ArtifactStore::memory())
    }
}

impl McpToolHandler {
    pub fn with_artifact_store(store: ArtifactStore) -> Self {
        Self {
            discovery: DefaultCapabilityDiscovery,
            terminal_provider: TerminalProvider::default(),
            artifact_service: ArtifactService::new(store),
            metadata: bridgingio_engine::InMemoryMetadataStore::default(),
        }
    }

    pub fn metadata(&self) -> &bridgingio_engine::InMemoryMetadataStore {
        &self.metadata
    }

    pub fn metadata_mut(&mut self) -> &mut bridgingio_engine::InMemoryMetadataStore {
        &mut self.metadata
    }

    pub fn handle(&mut self, request: ToolRequest) -> ToolResult {
        match request {
            ToolRequest::DescribeCapabilities { target, session } => ToolResult::Capabilities {
                envelope: self.discovery.describe_target(&target, session.as_ref()),
            },
            ToolRequest::TerminalExec {
                target_id,
                target_kind,
                context,
                command,
                artifact_id,
            } => self.run_terminal(target_id, target_kind, context, command, artifact_id),
            ToolRequest::ArtifactsRead {
                artifact_id,
                offset,
                limit,
            } => match self
                .artifact_service
                .read_artifact(&artifact_id, offset, limit)
            {
                Ok(view) => {
                    self.metadata.upsert_artifact(view.record.clone());
                    ToolResult::ArtifactRead { view }
                }
                Err(message) => ToolResult::Error { message },
            },
            ToolRequest::ArtifactsRefine {
                source_artifact_id,
                pattern,
                mode,
                ignore_case,
                processing_mode,
            } => {
                let parsed_processing_mode = match ArtifactProcessingMode::parse(&processing_mode) {
                    Ok(mode) => mode,
                    Err(message) => return ToolResult::Error { message },
                };
                match self.artifact_service.refine_with_filter(
                    &source_artifact_id,
                    &pattern,
                    &mode,
                    ignore_case,
                    parsed_processing_mode,
                ) {
                    Ok((record, resolved_processing_mode)) => {
                        self.metadata.upsert_artifact(record.clone());
                        ToolResult::ArtifactRefined {
                            artifact: record,
                            requested_processing_mode: parsed_processing_mode.as_str().to_string(),
                            resolved_processing_mode: resolved_processing_mode.as_str().to_string(),
                        }
                    }
                    Err(err) => ToolResult::Error { message: err },
                }
            }
            ToolRequest::GitStatus { repo_path } => {
                let provider = GitProvider::new(repo_path);
                match provider.status() {
                    Ok(output) => ToolResult::GitOutput { output },
                    Err(err) => ToolResult::Error {
                        message: err.message,
                    },
                }
            }
            ToolRequest::GitDiff {
                repo_path,
                reference,
            } => {
                let provider = GitProvider::new(repo_path);
                match provider.diff(&reference) {
                    Ok(output) => ToolResult::GitOutput { output },
                    Err(err) => ToolResult::Error {
                        message: err.message,
                    },
                }
            }
            ToolRequest::GitLog {
                repo_path,
                max_count,
            } => {
                let provider = GitProvider::new(repo_path);
                match provider.log(max_count) {
                    Ok(output) => ToolResult::GitOutput { output },
                    Err(err) => ToolResult::Error {
                        message: err.message,
                    },
                }
            }
            ToolRequest::TerminalExecRaw {
                target_id,
                target_kind,
                context,
                command,
                artifact_id,
                operation,
            } => {
                let policy = bridgingio_domain::PolicyProfile::default();
                if matches!(
                    evaluate(&policy, operation),
                    PolicyDecision::RequireApproval
                ) {
                    return ToolResult::ApprovalRequired {
                        reason: "policy requires explicit approval".into(),
                    };
                }
                self.run_terminal(target_id, target_kind, context, command, artifact_id)
            }
        }
    }

    fn run_terminal(
        &mut self,
        target_id: String,
        target_kind: TargetKind,
        context: ToolRequestContext,
        command: String,
        artifact_id: String,
    ) -> ToolResult {
        let now = SystemTime::now();
        let scope = build_scope(&context, now);
        let logical = self.metadata.resolve_logical_session(
            &scope,
            &target_id,
            context.reuse_policy.clone(),
            now,
        );
        let transport = self.metadata.open_transport_session(
            &logical.logical_session_id,
            &target_id,
            target_kind,
            None,
            None,
            now,
        );
        let channel = self.metadata.open_channel(
            &logical.logical_session_id,
            &transport.transport_session_id,
            &target_id,
            ChannelKind::OneShotExec,
            Some("terminal.exec".into()),
            now,
        );

        let policy = bridgingio_domain::PolicyProfile::default();
        match self.terminal_provider.exec_local(
            &logical.logical_session_id,
            Some(&channel.channel_id),
            Some(&transport.transport_session_id),
            &command,
            &artifact_id,
            &policy,
        ) {
            Ok(record) => {
                let canonical_record = self
                    .artifact_service
                    .ingest_from_terminal_provider(&self.terminal_provider, &record.id)
                    .unwrap_or(record.clone());
                self.metadata.upsert_artifact(canonical_record.clone());
                ToolResult::Execution {
                    artifact_id: canonical_record.id,
                    logical_session_id: logical.logical_session_id,
                    channel_id: channel.channel_id,
                }
            }
            Err(err) => ToolResult::Error {
                message: err.message,
            },
        }
    }
}

fn build_scope(context: &ToolRequestContext, now: SystemTime) -> AccessScope {
    AccessScope {
        scope_id: scope_id_from_context(context),
        workspace_id: "default-workspace".into(),
        principal_id: "local-operator".into(),
        agent_id: context.agent_id.clone(),
        run_id: context.run_id.clone(),
        thread_id: None,
        client_session_id: context.client_session_id.clone(),
        origin: "mcp".into(),
        created_at: now,
    }
}

fn scope_id_from_context(context: &ToolRequestContext) -> String {
    format!(
        "scope:{}:{}:{}",
        context.agent_id, context.run_id, context.client_session_id
    )
}

#[derive(Debug)]
pub enum CoreRuntimeError {
    Config(String),
    Io(String),
    LockPoisoned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreHostMode {
    UiManagedEphemeral,
    StandaloneRun,
    StandaloneDetached,
}

impl CoreHostMode {
    fn as_label(self) -> &'static str {
        match self {
            CoreHostMode::UiManagedEphemeral => "ui-managed-ephemeral",
            CoreHostMode::StandaloneRun => "standalone-run",
            CoreHostMode::StandaloneDetached => "standalone-detached",
        }
    }

    fn requires_ui_attach(self) -> bool {
        matches!(self, CoreHostMode::UiManagedEphemeral)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreReadinessState {
    Starting,
    WaitingForUiAttach,
    Ready,
    ShuttingDown,
}

impl CoreReadinessState {
    fn as_label(self) -> &'static str {
        match self {
            CoreReadinessState::Starting => "starting",
            CoreReadinessState::WaitingForUiAttach => "waiting_for_ui_attach",
            CoreReadinessState::Ready => "ready",
            CoreReadinessState::ShuttingDown => "shutting_down",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct UiAttachment {
    ui_instance_id: String,
    ui_kind: String,
    scope_id: String,
    attached_at: SystemTime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SettingsUpdateRequest {
    model_plane_host: Option<String>,
    model_plane_port: Option<u16>,
    artifact_cache_backend: Option<String>,
    artifact_cache_root: Option<String>,
    artifact_cache_max_bytes: Option<u64>,
    artifact_cache_eviction_policy: Option<String>,
    tool_override_command: Option<String>,
    tool_override_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TargetCommandExecution {
    requested_target_ref: String,
    resolved_target_id: String,
    target_kind: String,
    command: String,
    connector_command: String,
    artifact_id: String,
    logical_session_id: String,
    channel_id: String,
    output: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TargetInspectionResult {
    requested_target_ref: String,
    resolved_target_id: String,
    target_kind: String,
    kernel_version: String,
    username: String,
    artifacts: Vec<String>,
    logical_session_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InteractiveShellHandle {
    shell_id: String,
    requested_target_ref: String,
    resolved_target_id: String,
    target_kind: String,
    logical_session_id: String,
    channel_id: String,
    prompt: String,
    cwd: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InteractiveShellWriteOutcome {
    shell_id: String,
    channel_id: String,
    logical_session_id: String,
    artifact_id: String,
    output: String,
    prompt: String,
    cwd: String,
    running: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InteractiveShellReadOutcome {
    shell_id: String,
    channel_id: String,
    logical_session_id: String,
    prompt: String,
    cwd: String,
    interrupted: bool,
    closed: bool,
    running: bool,
    lines: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InteractiveShellOwner {
    scope_id: String,
    logical_session_id: String,
    channel_id: String,
    target_id: String,
}

pub type SharedRuntime = Arc<Mutex<StandaloneCoreRuntime>>;

pub struct StandaloneCoreRuntime {
    host_mode: CoreHostMode,
    readiness_state: CoreReadinessState,
    attached_ui: Option<UiAttachment>,
    shutdown_requested: bool,
    pub settings_store: CoreSettingsStore,
    pub tool_handler: McpToolHandler,
    profiles: HashMap<String, TargetProfile>,
    target_refs: HashMap<String, String>,
    sessions: HashMap<String, SessionRecord>,
    interactive_shell_owners: HashMap<String, InteractiveShellOwner>,
    timeline: Vec<TimelineEntry>,
    next_session_seq: u64,
    next_timeline_seq: u64,
    next_internal_artifact_seq: u64,
    toolchain_diagnostics: Vec<ToolchainDiagnosticView>,
    toolchain_resolver: ToolchainResolver,
    _vault_router: SecretVaultRouter,
}

impl StandaloneCoreRuntime {
    pub fn from_settings_with_mode(
        settings: CoreSettings,
        toolchain_resolver: ToolchainResolver,
        host_mode: CoreHostMode,
    ) -> Result<Self, CoreRuntimeError> {
        let initial_state = if host_mode.requires_ui_attach() {
            CoreReadinessState::WaitingForUiAttach
        } else {
            CoreReadinessState::Ready
        };
        Self::from_settings_with_mode_and_state(
            settings,
            toolchain_resolver,
            host_mode,
            initial_state,
        )
    }

    pub fn from_settings(
        settings: CoreSettings,
        toolchain_resolver: ToolchainResolver,
    ) -> Result<Self, CoreRuntimeError> {
        Self::from_settings_with_mode(
            settings,
            toolchain_resolver,
            CoreHostMode::StandaloneRun,
        )
    }

    fn from_settings_with_mode_and_state(
        settings: CoreSettings,
        toolchain_resolver: ToolchainResolver,
        host_mode: CoreHostMode,
        readiness_state: CoreReadinessState,
    ) -> Result<Self, CoreRuntimeError> {
        let mut profiles = HashMap::new();
        let mut target_refs = HashMap::new();
        for configured in &settings.targets {
            let profile = to_target_profile(configured)?;
            let target_id = profile.id.clone();
            register_target_ref(&mut target_refs, &target_id, &target_id)?;
            for alias in &configured.aliases {
                register_target_ref(&mut target_refs, alias, &target_id)?;
            }
            profiles.insert(target_id, profile);
        }

        let mut vault_router = SecretVaultRouter::default();
        vault_router
            .set_active_backend(&settings.vault.backend)
            .map_err(vault_error_to_runtime)?;

        let artifact_store = ArtifactStore::new(artifact_store_config_from_settings(&settings)?)
            .map_err(|err| CoreRuntimeError::Config(err.message))?;
        let mut runtime = Self {
            host_mode,
            readiness_state,
            attached_ui: None,
            shutdown_requested: false,
            settings_store: CoreSettingsStore::from_settings(settings, None),
            tool_handler: McpToolHandler::with_artifact_store(artifact_store),
            profiles,
            target_refs,
            sessions: HashMap::new(),
            interactive_shell_owners: HashMap::new(),
            timeline: Vec::new(),
            next_session_seq: 0,
            next_timeline_seq: 0,
            next_internal_artifact_seq: 0,
            toolchain_diagnostics: Vec::new(),
            toolchain_resolver,
            _vault_router: vault_router,
        };
        runtime.refresh_toolchain_diagnostics();
        Ok(runtime)
    }

    pub fn from_config_file(
        path: impl AsRef<Path>,
        toolchain_resolver: ToolchainResolver,
    ) -> Result<Self, CoreRuntimeError> {
        let settings = CoreSettings::load_from_file(path.as_ref())
            .map_err(|err| CoreRuntimeError::Config(format!("{err:?}")))?;
        let mut runtime = Self::from_settings_with_mode(
            settings,
            toolchain_resolver,
            CoreHostMode::StandaloneRun,
        )?;
        runtime.settings_store.runtime_metadata.config_path =
            Some(path.as_ref().to_string_lossy().to_string());
        Ok(runtime)
    }

    pub fn shared(self) -> SharedRuntime {
        Arc::new(Mutex::new(self))
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    pub fn logical_session_count(&self) -> usize {
        self.tool_handler.metadata().logical_sessions.len()
    }

    pub fn host_mode(&self) -> CoreHostMode {
        self.host_mode
    }

    pub fn readiness_state(&self) -> CoreReadinessState {
        self.readiness_state
    }

    pub fn readiness_state_label(&self) -> &'static str {
        self.readiness_state.as_label()
    }

    pub fn model_plane_ready(&self) -> bool {
        self.readiness_state == CoreReadinessState::Ready
    }

    pub fn requires_ui_attach_before_model_plane(&self) -> bool {
        self.host_mode.requires_ui_attach()
    }

    pub fn model_plane_not_ready_reason(&self) -> Option<String> {
        if self.requires_ui_attach_before_model_plane() && !self.model_plane_ready() {
            return Some(self.readiness_state_label().to_string());
        }
        None
    }

    pub fn shutdown_requested(&self) -> bool {
        self.shutdown_requested
    }

    fn request_shutdown(&mut self) {
        self.readiness_state = CoreReadinessState::ShuttingDown;
        self.shutdown_requested = true;
    }

    fn push_timeline_entry(
        &mut self,
        session_id: String,
        command_preview: String,
        status: &str,
        artifact_id: Option<String>,
    ) {
        self.next_timeline_seq += 1;
        let entry = TimelineEntry {
            id: format!("timeline-{:06}", self.next_timeline_seq),
            session_id,
            command_preview,
            status: status.to_string(),
            artifact_id,
            created_at: SystemTime::now(),
        };
        self.timeline.push(entry);
    }

    fn resolve_target_profile_by_ref(&self, target_ref: &str) -> Option<TargetProfile> {
        let normalized = normalize_target_ref(target_ref);
        let target_id = self.target_refs.get(&normalized)?;
        self.profiles.get(target_id).cloned()
    }

    fn next_internal_artifact_hint(&mut self) -> String {
        self.next_internal_artifact_seq += 1;
        format!("tmp-artifact-{:06}", self.next_internal_artifact_seq)
    }

    fn execute_target_command(
        &mut self,
        target_ref: &str,
        context: ToolRequestContext,
        command: &str,
        artifact_id: Option<String>,
    ) -> Result<TargetCommandExecution, CoreRuntimeError> {
        let requested_target_ref = target_ref.to_string();
        let target = self
            .resolve_target_profile_by_ref(target_ref)
            .ok_or_else(|| CoreRuntimeError::Config(format!("target not found: {target_ref}")))?;
        let (resolved_path, _resolved_source) = self.resolve_transport_executable_for_target(&target);
        let connector_command = build_connector_command(&target, command, resolved_path.as_deref());
        let artifact_id = artifact_id.unwrap_or_else(|| self.next_internal_artifact_hint());
        let result = self.tool_handler.handle(ToolRequest::TerminalExec {
            target_id: target.id.clone(),
            target_kind: target.kind.clone(),
            context,
            command: connector_command.clone(),
            artifact_id,
        });

        let (artifact_id, logical_session_id, channel_id) = match result {
            ToolResult::Execution {
                artifact_id,
                logical_session_id,
                channel_id,
            } => (artifact_id, logical_session_id, channel_id),
            ToolResult::ApprovalRequired { reason } => {
                return Err(CoreRuntimeError::Config(format!(
                    "command requires approval: {reason}"
                )))
            }
            ToolResult::Error { message } => return Err(CoreRuntimeError::Config(message)),
            other => {
                return Err(CoreRuntimeError::Io(format!(
                    "unexpected execution result: {other:?}"
                )))
            }
        };

        let output_chunks = match self.tool_handler.handle(ToolRequest::ArtifactsRead {
            artifact_id: artifact_id.clone(),
            offset: 0,
            limit: 2000,
        }) {
            ToolResult::ArtifactRead { view } => view.chunks,
            _ => Vec::new(),
        };

        self.push_timeline_entry(
            logical_session_id.clone(),
            command.to_string(),
            "success",
            Some(artifact_id.clone()),
        );

        Ok(TargetCommandExecution {
            requested_target_ref,
            resolved_target_id: target.id.clone(),
            target_kind: target_kind_label(&target.kind),
            command: command.to_string(),
            connector_command,
            artifact_id,
            logical_session_id,
            channel_id,
            output: output_chunks.join("\n"),
        })
    }

    fn inspect_target_basic(
        &mut self,
        target_ref: &str,
        context: ToolRequestContext,
    ) -> Result<TargetInspectionResult, CoreRuntimeError> {
        let kernel = self.execute_target_command(target_ref, context.clone(), "uname -r", None)?;
        let user = self.execute_target_command(target_ref, context, "whoami", None)?;

        Ok(TargetInspectionResult {
            requested_target_ref: target_ref.to_string(),
            resolved_target_id: kernel.resolved_target_id.clone(),
            target_kind: kernel.target_kind.clone(),
            kernel_version: first_data_line(&kernel.output),
            username: first_data_line(&user.output),
            artifacts: vec![kernel.artifact_id, user.artifact_id],
            logical_session_id: user.logical_session_id,
        })
    }

    fn open_interactive_shell(
        &mut self,
        target_ref: &str,
        context: ToolRequestContext,
    ) -> Result<InteractiveShellHandle, CoreRuntimeError> {
        let requested_target_ref = target_ref.to_string();
        let target = self
            .resolve_target_profile_by_ref(target_ref)
            .ok_or_else(|| CoreRuntimeError::Config(format!("target not found: {target_ref}")))?;
        let now = SystemTime::now();
        let scope = build_scope(&context, now);
        let logical = self.tool_handler.metadata_mut().resolve_logical_session(
            &scope,
            &target.id,
            context.reuse_policy.clone(),
            now,
        );
        let (resolved_path, resolved_source) = self.resolve_transport_executable_for_target(&target);
        let launch_command =
            build_interactive_connector_command(&target, resolved_path.as_deref());
        let transport = self.tool_handler.metadata_mut().open_transport_session(
            &logical.logical_session_id,
            &target.id,
            target.kind.clone(),
            resolved_path,
            resolved_source,
            now,
        );
        let channel = self.tool_handler.metadata_mut().open_channel(
            &logical.logical_session_id,
            &transport.transport_session_id,
            &target.id,
            ChannelKind::InteractiveShell,
            Some("terminal.interactive".into()),
            now,
        );
        let shell = self
            .tool_handler
            .terminal_provider
            .open_interactive_shell_with_command(
                &logical.logical_session_id,
                &channel.channel_id,
                Some(&transport.transport_session_id),
                &target_kind_label(&target.kind),
                launch_command.as_deref(),
            );
        self.interactive_shell_owners.insert(
            shell.shell_id.clone(),
            InteractiveShellOwner {
                scope_id: scope.scope_id,
                logical_session_id: logical.logical_session_id.clone(),
                channel_id: channel.channel_id.clone(),
                target_id: target.id.clone(),
            },
        );

        Ok(InteractiveShellHandle {
            shell_id: shell.shell_id,
            requested_target_ref,
            resolved_target_id: target.id.clone(),
            target_kind: target_kind_label(&target.kind),
            logical_session_id: logical.logical_session_id,
            channel_id: channel.channel_id,
            prompt: shell.prompt,
            cwd: shell.cwd,
        })
    }

    fn write_interactive_shell(
        &mut self,
        shell_id: &str,
        input: &str,
        context: ToolRequestContext,
    ) -> Result<InteractiveShellWriteOutcome, CoreRuntimeError> {
        let owner = self.ensure_interactive_shell_access(shell_id, &context)?;
        let artifact_id = self.next_internal_artifact_hint();
        let policy = bridgingio_domain::PolicyProfile::default();
        let write_result = self
            .tool_handler
            .terminal_provider
            .write_interactive_shell(shell_id, input, &artifact_id, &policy)
            .map_err(|err| CoreRuntimeError::Config(err.message))?;
        let canonical_artifact = self
            .tool_handler
            .artifact_service
            .ingest_from_terminal_provider(
                &self.tool_handler.terminal_provider,
                &write_result.artifact_id,
            )
            .ok_or_else(|| {
                CoreRuntimeError::Io("failed to persist interactive shell artifact".into())
            })?;
        self.tool_handler
            .metadata_mut()
            .upsert_artifact(canonical_artifact.clone());

        Ok(InteractiveShellWriteOutcome {
            shell_id: write_result.shell_id,
            channel_id: owner.channel_id,
            logical_session_id: owner.logical_session_id,
            artifact_id: canonical_artifact.id,
            output: write_result.output,
            prompt: write_result.prompt,
            cwd: write_result.cwd,
            running: write_result.running,
        })
    }

    fn read_interactive_shell(
        &mut self,
        shell_id: &str,
        offset: usize,
        limit: usize,
        context: ToolRequestContext,
    ) -> Result<InteractiveShellReadOutcome, CoreRuntimeError> {
        let owner = self.ensure_interactive_shell_access(shell_id, &context)?;
        let state = self
            .tool_handler
            .terminal_provider
            .get_interactive_shell(shell_id)
            .map_err(|err| CoreRuntimeError::Config(err.message))?;
        let lines = self
            .tool_handler
            .terminal_provider
            .read_interactive_transcript(shell_id, offset, limit)
            .map_err(|err| CoreRuntimeError::Config(err.message))?;

        Ok(InteractiveShellReadOutcome {
            shell_id: shell_id.to_string(),
            channel_id: owner.channel_id,
            logical_session_id: owner.logical_session_id,
            prompt: state.prompt,
            cwd: state.cwd,
            interrupted: state.interrupted,
            closed: state.closed,
            running: state.running,
            lines,
        })
    }

    fn interrupt_interactive_shell(
        &mut self,
        shell_id: &str,
        context: ToolRequestContext,
    ) -> Result<InteractiveShellReadOutcome, CoreRuntimeError> {
        let owner = self.ensure_interactive_shell_access(shell_id, &context)?;
        self.tool_handler
            .terminal_provider
            .interrupt_interactive_shell(shell_id)
            .map_err(|err| CoreRuntimeError::Config(err.message))?;
        let state = self
            .tool_handler
            .terminal_provider
            .get_interactive_shell(shell_id)
            .map_err(|err| CoreRuntimeError::Config(err.message))?;
        Ok(InteractiveShellReadOutcome {
            shell_id: shell_id.to_string(),
            channel_id: owner.channel_id,
            logical_session_id: owner.logical_session_id,
            prompt: state.prompt,
            cwd: state.cwd,
            interrupted: state.interrupted,
            closed: state.closed,
            running: state.running,
            lines: Vec::new(),
        })
    }

    fn close_interactive_shell(
        &mut self,
        shell_id: &str,
        context: ToolRequestContext,
    ) -> Result<InteractiveShellReadOutcome, CoreRuntimeError> {
        let owner = self.ensure_interactive_shell_access(shell_id, &context)?;
        self.tool_handler
            .terminal_provider
            .close_interactive_shell(shell_id)
            .map_err(|err| CoreRuntimeError::Config(err.message))?;
        let now = SystemTime::now();
        let _ = self.tool_handler.metadata_mut().update_channel_status(
            &owner.channel_id,
            ChannelStatus::Closed,
            Some("interactive shell closed".into()),
            now,
        );
        let state = self
            .tool_handler
            .terminal_provider
            .get_interactive_shell(shell_id)
            .map_err(|err| CoreRuntimeError::Config(err.message))?;
        Ok(InteractiveShellReadOutcome {
            shell_id: shell_id.to_string(),
            channel_id: owner.channel_id,
            logical_session_id: owner.logical_session_id,
            prompt: state.prompt,
            cwd: state.cwd,
            interrupted: state.interrupted,
            closed: state.closed,
            running: state.running,
            lines: Vec::new(),
        })
    }

    fn ensure_interactive_shell_access(
        &self,
        shell_id: &str,
        context: &ToolRequestContext,
    ) -> Result<InteractiveShellOwner, CoreRuntimeError> {
        let owner = self
            .interactive_shell_owners
            .get(shell_id)
            .cloned()
            .ok_or_else(|| {
                CoreRuntimeError::Config(format!("interactive shell not found: {shell_id}"))
            })?;
        let scope_id = scope_id_from_context(context);
        if owner.scope_id != scope_id {
            return Err(CoreRuntimeError::Config(
                "interactive shell access denied for current scope".into(),
            ));
        }
        Ok(owner)
    }

    fn resolve_transport_executable_for_target(
        &self,
        target: &TargetProfile,
    ) -> (Option<String>, Option<String>) {
        let Some(command) = toolchain_command_for_kind(&target.kind) else {
            return (None, None);
        };
        let diagnostic = self.resolve_toolchain_diagnostic_for_target(target, command);
        (diagnostic.effective_path, diagnostic.effective_source)
    }

    fn refresh_toolchain_diagnostics(&mut self) {
        let mut target_ids = self.profiles.keys().cloned().collect::<Vec<_>>();
        target_ids.sort();
        let mut diagnostics = Vec::new();
        for target_id in target_ids {
            let Some(profile) = self.profiles.get(&target_id) else {
                continue;
            };
            let Some(command) = toolchain_command_for_kind(&profile.kind) else {
                continue;
            };
            diagnostics.push(self.resolve_toolchain_diagnostic_for_target(profile, command));
        }
        self.toolchain_diagnostics = diagnostics;
    }

    fn resolve_toolchain_diagnostic_for_target(
        &self,
        profile: &TargetProfile,
        command: &str,
    ) -> ToolchainDiagnosticView {
        let target_override_path = profile
            .toolchains
            .get(command)
            .and_then(|value| non_empty_path(value))
            .map(ToOwned::to_owned);
        let global_override_path = self
            .settings_store
            .settings
            .toolchains
            .get(command)
            .and_then(|section| non_empty_path(&section.path_override))
            .map(ToOwned::to_owned);

        let (resolution, diagnostics, override_scope) = if let Some(path) = target_override_path.as_deref() {
            let (target_resolution, target_diag) = self
                .toolchain_resolver
                .resolve_with_diagnostics(command, Some(Path::new(path)));
            match &target_resolution {
                Ok(selected) if selected.source == ExecutableSource::UserOverride => {
                    (target_resolution, target_diag, Some("target_override"))
                }
                _ => {
                    let (fallback_resolution, fallback_diag) = self
                        .toolchain_resolver
                        .resolve_with_diagnostics(
                            command,
                            global_override_path.as_deref().map(Path::new),
                        );
                    let scope = match &fallback_resolution {
                        Ok(selected)
                            if selected.source == ExecutableSource::UserOverride
                                && global_override_path.is_some() =>
                        {
                            Some("global_override")
                        }
                        _ => None,
                    };
                    (fallback_resolution, fallback_diag, scope)
                }
            }
        } else {
            let (fallback_resolution, fallback_diag) = self.toolchain_resolver.resolve_with_diagnostics(
                command,
                global_override_path.as_deref().map(Path::new),
            );
            let scope = match &fallback_resolution {
                Ok(selected)
                    if selected.source == ExecutableSource::UserOverride
                        && global_override_path.is_some() =>
                {
                    Some("global_override")
                }
                _ => None,
            };
            (fallback_resolution, fallback_diag, scope)
        };

        let effective_scope = match &resolution {
            Ok(selection) => Some(match selection.source {
                ExecutableSource::UserOverride => override_scope.unwrap_or("global_override"),
                ExecutableSource::SystemPath => "system_path",
                ExecutableSource::BuiltInFallback => "builtin_fallback",
            }),
            Err(_) => None,
        }
        .map(str::to_string);

        let effective_source = diagnostics.selected_source.clone();
        let effective_path = diagnostics
            .selected_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string());

        ToolchainDiagnosticView {
            command: command.to_string(),
            target_id: Some(profile.id.clone()),
            target_name: Some(profile.name.clone()),
            target_override_path,
            global_override_path,
            effective_scope,
            effective_source: effective_source.clone(),
            effective_path: effective_path.clone(),
            selected_source: effective_source,
            selected_path: effective_path,
        }
    }

    pub fn settings_view(&self) -> CoreSettingsView {
        let artifact_usage = self.tool_handler.artifact_service.usage();
        let mut toolchain_entries = self
            .settings_store
            .settings
            .toolchains
            .iter()
            .map(|(command, section)| bridgingio_app_api::ToolchainSettingsView {
                command: command.clone(),
                path_override: section.path_override.clone(),
                prefer_builtin_fallback: section.prefer_builtin_fallback,
            })
            .collect::<Vec<_>>();
        toolchain_entries.sort_by(|a, b| a.command.cmp(&b.command));
        CoreSettingsView {
            schema_version: self.settings_store.settings.schema_version,
            instance_name: self.settings_store.settings.core.instance_name.clone(),
            data_dir: self.settings_store.settings.core.data_dir.clone(),
            model_plane_http: ModelPlaneHttpView {
                host: self.settings_store.settings.model_plane.http.host.clone(),
                port: self.settings_store.settings.model_plane.http.port,
                allow_non_loopback: self
                    .settings_store
                    .settings
                    .model_plane
                    .http
                    .allow_non_loopback,
                auth_mode: self
                    .settings_store
                    .settings
                    .model_plane
                    .http
                    .auth
                    .mode
                    .clone(),
            },
            control_plane: ControlPlaneView {
                enabled: self.settings_store.settings.control_plane.enabled,
                transport: self.settings_store.settings.control_plane.transport.clone(),
                endpoint: self.settings_store.settings.control_plane.endpoint.clone(),
            },
            artifact_cache: ArtifactCacheSettingsView {
                backend: self
                    .settings_store
                    .settings
                    .storage
                    .artifacts
                    .backend
                    .clone(),
                root: self.settings_store.settings.storage.artifacts.root.clone(),
                max_bytes: self.settings_store.settings.storage.artifacts.max_bytes,
                eviction_policy: self
                    .settings_store
                    .settings
                    .storage
                    .artifacts
                    .eviction_policy
                    .clone(),
                used_bytes: artifact_usage.used_bytes,
                artifact_count: artifact_usage.artifact_count,
            },
            toolchains: toolchain_entries,
        }
    }

    fn persist_settings_to_disk(&self) -> Result<(), CoreRuntimeError> {
        let path = self
            .settings_store
            .runtime_metadata
            .config_path
            .as_ref()
            .ok_or_else(|| {
                CoreRuntimeError::Config("runtime config path unavailable for persistence".into())
            })?;
        let text = self.settings_store.settings.to_toml_string();
        std::fs::write(path, text)
            .map_err(|err| CoreRuntimeError::Io(format!("persist config: {err}")))
    }

    fn rebuild_profile_cache_from_settings(&mut self) -> Result<(), CoreRuntimeError> {
        let mut profiles = HashMap::new();
        let mut target_refs = HashMap::new();
        for configured in &self.settings_store.settings.targets {
            let profile = to_target_profile(configured)?;
            let target_id = profile.id.clone();
            register_target_ref(&mut target_refs, &target_id, &target_id)?;
            for alias in &configured.aliases {
                register_target_ref(&mut target_refs, alias, &target_id)?;
            }
            profiles.insert(target_id, profile.clone());
            self.tool_handler.metadata_mut().upsert_profile(profile);
        }
        self.profiles = profiles;
        self.target_refs = target_refs;
        Ok(())
    }

    fn upsert_profile_and_persist(
        &mut self,
        profile: TargetProfile,
    ) -> Result<String, CoreRuntimeError> {
        if profile.id.trim().is_empty() {
            return Err(CoreRuntimeError::Config("target id is required".into()));
        }
        if profile.name.trim().is_empty() {
            return Err(CoreRuntimeError::Config("target name is required".into()));
        }

        let standalone = to_standalone_target_profile(&profile)?;
        let mut settings = self.settings_store.settings.clone();
        if let Some(index) = settings
            .targets
            .iter()
            .position(|existing| existing.id == standalone.id)
        {
            settings.targets[index] = standalone;
        } else {
            settings.targets.push(standalone);
        }
        settings.validate().map_err(|err| {
            CoreRuntimeError::Config(format!("profile validation failed: {err:?}"))
        })?;

        self.settings_store.settings = settings;
        self.rebuild_profile_cache_from_settings()?;
        self.refresh_toolchain_diagnostics();
        self.persist_settings_to_disk()?;
        Ok("live_applied".to_string())
    }

    fn update_settings_and_persist(
        &mut self,
        update: SettingsUpdateRequest,
    ) -> Result<String, CoreRuntimeError> {
        let mut settings = self.settings_store.settings.clone();
        let mut restart_required = false;

        if let Some(host) = update.model_plane_host {
            settings.model_plane.http.host = host;
            restart_required = true;
        }
        if let Some(port) = update.model_plane_port {
            settings.model_plane.http.port = port;
            restart_required = true;
        }

        if let Some(backend) = update.artifact_cache_backend {
            settings.storage.artifacts.backend = backend;
            restart_required = true;
        }
        if let Some(root) = update.artifact_cache_root {
            settings.storage.artifacts.root = root;
            restart_required = true;
        }
        if let Some(max_bytes) = update.artifact_cache_max_bytes {
            settings.storage.artifacts.max_bytes = max_bytes;
            restart_required = true;
        }
        if let Some(eviction) = update.artifact_cache_eviction_policy {
            settings.storage.artifacts.eviction_policy = eviction;
            restart_required = true;
        }

        match (update.tool_override_command, update.tool_override_path) {
            (Some(command), Some(path)) => {
                let entry = settings.toolchains.entry(command).or_insert(
                    bridgingio_engine::ToolchainSection {
                        path_override: String::new(),
                        prefer_builtin_fallback: false,
                    },
                );
                entry.path_override = path;
            }
            (None, None) => {}
            _ => {
                return Err(CoreRuntimeError::Config(
                    "tool override update requires command and path".into(),
                ))
            }
        }

        settings.validate().map_err(|err| {
            CoreRuntimeError::Config(format!("settings validation failed: {err:?}"))
        })?;
        self.settings_store.settings = settings;
        self.refresh_toolchain_diagnostics();
        self.persist_settings_to_disk()?;

        if restart_required {
            Ok("restart_required".to_string())
        } else {
            Ok("live_applied".to_string())
        }
    }

    fn clear_artifact_cache_live(&mut self) -> Result<(), CoreRuntimeError> {
        let store = ArtifactStore::new(artifact_store_config_from_settings(
            &self.settings_store.settings,
        )?)
        .map_err(|err| CoreRuntimeError::Config(err.message))?;
        self.tool_handler.artifact_service = ArtifactService::new(store);
        self.tool_handler.metadata_mut().artifacts.clear();
        Ok(())
    }

    fn timeline_entries(&self, limit: usize) -> Vec<TimelineEntry> {
        let mut items = self.timeline.clone();
        items.sort_by_key(|item| std::cmp::Reverse(system_time_to_unix_millis(item.created_at)));
        items.into_iter().take(limit).collect()
    }

    fn artifact_records(&self, limit: usize) -> Vec<ArtifactRecord> {
        let mut items = self
            .tool_handler
            .metadata()
            .artifacts
            .values()
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by_key(|item| std::cmp::Reverse(system_time_to_unix_millis(item.created_at)));
        items.into_iter().take(limit).collect()
    }

    fn interactive_shell_views(&self) -> Vec<Value> {
        let mut views = self
            .interactive_shell_owners
            .iter()
            .filter_map(|(shell_id, owner)| {
                let state = self
                    .tool_handler
                    .terminal_provider
                    .get_interactive_shell(shell_id)
                    .ok()?;
                Some(json!({
                    "shell_id": state.shell_id,
                    "target_id": owner.target_id,
                    "logical_session_id": owner.logical_session_id,
                    "channel_id": owner.channel_id,
                    "prompt": state.prompt,
                    "cwd": state.cwd,
                    "closed": state.closed,
                    "running": state.running,
                }))
            })
            .collect::<Vec<_>>();
        views.sort_by(|a, b| {
            a["shell_id"]
                .as_str()
                .unwrap_or_default()
                .cmp(b["shell_id"].as_str().unwrap_or_default())
        });
        views
    }

    fn timeline_payload_json(&self, limit: usize) -> String {
        let timeline = self.timeline_json_items(limit);
        json!({
            "readiness_state": self.readiness_state_label(),
            "items": timeline,
        })
        .to_string()
    }

    fn artifacts_payload_json(&self, limit: usize) -> String {
        let artifacts = self.artifact_json_items(limit);
        json!({
            "readiness_state": self.readiness_state_label(),
            "items": artifacts,
        })
        .to_string()
    }

    fn interactive_shells_payload_json(&self) -> String {
        json!({
            "readiness_state": self.readiness_state_label(),
            "items": self.interactive_shell_views(),
        })
        .to_string()
    }

    fn bootstrap_payload_json(
        &mut self,
        timeline_limit: usize,
        artifact_limit: usize,
        transcript_limit: usize,
    ) -> String {
        let settings = self.settings_view();

        let mut targets = self.profiles.values().cloned().collect::<Vec<_>>();
        targets.sort_by(|a, b| a.id.cmp(&b.id));
        let targets_json = targets
            .into_iter()
            .map(|target| {
                let session = self
                    .sessions
                    .values()
                    .find(|session| session.target_id == target.id);
                let capabilities = infer_capabilities(&target.kind)
                    .into_iter()
                    .map(|capability| capability.id)
                    .collect::<Vec<_>>();
                json!({
                    "id": target.id,
                    "name": target.name,
                    "kind": target_kind_label(&target.kind),
                    "notes": target.notes,
                    "session_id": session.as_ref().map(|s| s.id.clone()),
                    "session_state": session.as_ref().map(|s| session_state_label(&s.state)),
                    "capability_ids": capabilities,
                })
            })
            .collect::<Vec<_>>();
        let mut profiles = self.profiles.values().cloned().collect::<Vec<_>>();
        profiles.sort_by(|a, b| a.id.cmp(&b.id));
        let profiles_json = profiles
            .iter()
            .map(target_profile_json_value)
            .collect::<Vec<_>>();

        let mut sessions = self.sessions.values().cloned().collect::<Vec<_>>();
        sessions.sort_by(|a, b| a.id.cmp(&b.id));
        let sessions_json = sessions
            .into_iter()
            .map(|session| {
                json!({
                    "id": session.id,
                    "target_id": session.target_id,
                    "state": session_state_label(&session.state),
                    "started_at_ms": system_time_to_unix_millis(session.started_at),
                    "last_activity_at_ms": system_time_to_unix_millis(session.last_activity_at),
                })
            })
            .collect::<Vec<_>>();

        let mut approvals = self
            .tool_handler
            .metadata()
            .approvals
            .values()
            .cloned()
            .collect::<Vec<_>>();
        approvals.sort_by(|a, b| a.id.cmp(&b.id));
        let approvals_json = approvals
            .into_iter()
            .map(|approval| {
                json!({
                    "id": approval.id,
                    "logical_session_id": approval.logical_session_id,
                    "channel_id": approval.channel_id,
                    "session_id": approval.session_id,
                    "command_preview": approval.command_preview,
                    "reason": approval.reason,
                    "status": approval_status_label(&approval.status),
                    "requested_at_ms": system_time_to_unix_millis(approval.requested_at),
                })
            })
            .collect::<Vec<_>>();

        let diagnostics = self
            .toolchain_diagnostics
            .iter()
            .map(|diag| {
                json!({
                    "command": diag.command,
                    "target_id": diag.target_id,
                    "target_name": diag.target_name,
                    "target_override_path": diag.target_override_path,
                    "global_override_path": diag.global_override_path,
                    "effective_scope": diag.effective_scope,
                    "effective_source": diag.effective_source,
                    "effective_path": diag.effective_path,
                    "selected_source": diag.selected_source,
                    "selected_path": diag.selected_path,
                })
            })
            .collect::<Vec<_>>();

        let shell_channels = self.interactive_shell_views();
        let transcripts = shell_channels
            .iter()
            .filter_map(|shell| shell["shell_id"].as_str())
            .filter_map(|shell_id| {
                self.tool_handler
                    .terminal_provider
                    .read_interactive_transcript(shell_id, 0, transcript_limit)
                    .ok()
                    .map(|lines| {
                        json!({
                            "shell_id": shell_id,
                            "offset": 0,
                            "limit": transcript_limit,
                            "lines": lines,
                        })
                    })
            })
            .collect::<Vec<_>>();

        let runtime_root = self.settings_store.settings.core.data_dir.clone();
        let runtime_root_path = PathBuf::from(&runtime_root);
        let runtime_root_diagnostics = json!({
            "path": runtime_root,
            "config_path": self.settings_store.runtime_metadata.config_path.clone(),
            "logs_dir": runtime_root_path.join("logs").to_string_lossy().to_string(),
            "state_dir": runtime_root_path.join("state").to_string_lossy().to_string(),
            "accessible": runtime_root_path.exists(),
        });

        json!({
            "host_mode": self.host_mode.as_label(),
            "readiness_state": self.readiness_state_label(),
            "model_plane_ready": self.model_plane_ready(),
            "targets": targets_json,
            "profiles": profiles_json,
            "sessions": sessions_json,
            "approvals": approvals_json,
            "runtime_root": runtime_root_diagnostics,
            "settings": {
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
                "toolchains": settings.toolchains.iter().map(|toolchain| {
                    json!({
                        "command": toolchain.command,
                        "path_override": toolchain.path_override,
                        "prefer_builtin_fallback": toolchain.prefer_builtin_fallback,
                    })
                }).collect::<Vec<_>>()
            },
            "diagnostics": diagnostics,
            "timeline": self.timeline_json_items(timeline_limit),
            "artifacts": self.artifact_json_items(artifact_limit),
            "shell_channels": shell_channels,
            "transcripts": transcripts,
        })
        .to_string()
    }

    fn timeline_json_items(&self, limit: usize) -> Vec<Value> {
        self.timeline_entries(limit)
            .into_iter()
            .map(|entry| {
                let target_id = self
                    .sessions
                    .get(&entry.session_id)
                    .map(|session| session.target_id.clone());
                json!({
                    "id": entry.id,
                    "session_id": entry.session_id,
                    "target_id": target_id,
                    "command_preview": entry.command_preview,
                    "status": entry.status,
                    "artifact_id": entry.artifact_id,
                    "created_at_ms": system_time_to_unix_millis(entry.created_at),
                })
            })
            .collect()
    }

    fn artifact_json_items(&self, limit: usize) -> Vec<Value> {
        self.artifact_records(limit)
            .into_iter()
            .map(|record| {
                json!({
                    "id": record.id,
                    "content_digest": record.content_digest,
                    "logical_session_id": record.logical_session_id,
                    "channel_id": record.channel_id,
                    "session_id": record.session_id,
                    "parent_id": record.parent_id,
                    "source_command": record.source_command,
                    "summary": record.summary,
                    "line_count": record.line_count,
                    "created_at_ms": system_time_to_unix_millis(record.created_at),
                })
            })
            .collect()
    }

    pub fn handle_app_request(&mut self, request: ApiRequest) -> ApiResponse {
        match request.command {
            AppCommand::AttachUi {
                ui_instance_id,
                ui_kind,
            } => {
                if !self.host_mode.requires_ui_attach() {
                    return ApiResponse::Attached {
                        request_id: request.request_id,
                        readiness_state: self.readiness_state_label().to_string(),
                        model_plane_ready: self.model_plane_ready(),
                    };
                }

                let scope_id = format!(
                    "scope:{}:{}:{}",
                    request.context.agent_id,
                    request.context.run_id,
                    request.context.client_session_id
                );
                if let Some(existing) = self.attached_ui.as_ref() {
                    if existing.ui_instance_id != ui_instance_id {
                        return error_response(
                            request.request_id,
                            ApiErrorCode::ValidationFailed,
                            "another ui instance already attached",
                        );
                    }
                }

                self.attached_ui = Some(UiAttachment {
                    ui_instance_id,
                    ui_kind,
                    scope_id,
                    attached_at: SystemTime::now(),
                });
                self.readiness_state = CoreReadinessState::Ready;
                ApiResponse::Attached {
                    request_id: request.request_id,
                    readiness_state: self.readiness_state_label().to_string(),
                    model_plane_ready: self.model_plane_ready(),
                }
            }
            AppCommand::GetBootstrapState {
                timeline_limit,
                artifact_limit,
                transcript_limit,
            } => ApiResponse::Bootstrap {
                request_id: request.request_id,
                payload_json: self.bootstrap_payload_json(
                    timeline_limit,
                    artifact_limit,
                    transcript_limit,
                ),
            },
            AppCommand::GetTimeline { limit } => ApiResponse::Timeline {
                request_id: request.request_id,
                payload_json: self.timeline_payload_json(limit),
            },
            AppCommand::GetArtifacts { limit } => ApiResponse::ArtifactsSnapshot {
                request_id: request.request_id,
                payload_json: self.artifacts_payload_json(limit),
            },
            AppCommand::ListInteractiveShells => ApiResponse::InteractiveShells {
                request_id: request.request_id,
                payload_json: self.interactive_shells_payload_json(),
            },
            AppCommand::ReadInteractiveTranscript {
                shell_id,
                offset,
                limit,
            } => {
                let context = ToolRequestContext {
                    agent_id: request.context.agent_id,
                    run_id: request.context.run_id,
                    client_session_id: request.context.client_session_id,
                    reuse_policy: request.context.reuse_policy,
                };
                let owner = match self.ensure_interactive_shell_access(&shell_id, &context) {
                    Ok(owner) => owner,
                    Err(CoreRuntimeError::Config(message)) => {
                        return error_response(
                            request.request_id,
                            ApiErrorCode::PermissionDenied,
                            &message,
                        );
                    }
                    Err(err) => {
                        return error_response(
                            request.request_id,
                            ApiErrorCode::Internal,
                            &format!("{err:?}"),
                        );
                    }
                };
                let lines = match self
                    .tool_handler
                    .terminal_provider
                    .read_interactive_transcript(&shell_id, offset, limit)
                {
                    Ok(lines) => lines,
                    Err(err) => {
                        return error_response(
                            request.request_id,
                            ApiErrorCode::NotFound,
                            &err.message,
                        );
                    }
                };
                ApiResponse::InteractiveTranscript {
                    request_id: request.request_id,
                    shell_id,
                    offset,
                    limit,
                    payload_json: json!({
                        "logical_session_id": owner.logical_session_id,
                        "channel_id": owner.channel_id,
                        "lines": lines,
                    })
                    .to_string(),
                }
            }
            AppCommand::RequestShutdown => {
                if !self.host_mode.requires_ui_attach() {
                    return error_response(
                        request.request_id,
                        ApiErrorCode::ValidationFailed,
                        "shutdown command is reserved for ui-managed mode",
                    );
                }
                self.request_shutdown();
                ApiResponse::ShutdownAccepted {
                    request_id: request.request_id,
                }
            }
            AppCommand::ListTargets => {
                let mut items = self.profiles.values().cloned().collect::<Vec<_>>();
                items.sort_by(|a, b| a.id.cmp(&b.id));
                ApiResponse::Targets {
                    request_id: request.request_id,
                    items,
                }
            }
            AppCommand::ListProfiles => {
                let mut items = self.profiles.values().cloned().collect::<Vec<_>>();
                items.sort_by(|a, b| a.id.cmp(&b.id));
                ApiResponse::Profiles {
                    request_id: request.request_id,
                    items,
                }
            }
            AppCommand::GetProfile { target_id } => {
                let profile = match self.profiles.get(&target_id) {
                    Some(profile) => profile,
                    None => {
                        return error_response(
                            request.request_id,
                            ApiErrorCode::NotFound,
                            "target profile not found",
                        )
                    }
                };
                ApiResponse::Profile {
                    request_id: request.request_id,
                    payload_json: target_profile_payload_json(profile),
                }
            }
            AppCommand::UpsertProfile { profile } => {
                match self.upsert_profile_and_persist(profile) {
                    Ok(apply_strategy) => ApiResponse::Accepted {
                        request_id: request.request_id,
                        apply_strategy: Some(apply_strategy),
                    },
                    Err(CoreRuntimeError::Config(message)) => error_response(
                        request.request_id,
                        ApiErrorCode::ValidationFailed,
                        &message,
                    ),
                    Err(err) => error_response(
                        request.request_id,
                        ApiErrorCode::Internal,
                        &format!("{err:?}"),
                    ),
                }
            }
            AppCommand::GetSettings => ApiResponse::Settings {
                request_id: request.request_id,
                settings: self.settings_view(),
            },
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
                let update = SettingsUpdateRequest {
                    model_plane_host,
                    model_plane_port,
                    artifact_cache_backend,
                    artifact_cache_root,
                    artifact_cache_max_bytes,
                    artifact_cache_eviction_policy,
                    tool_override_command,
                    tool_override_path,
                };
                match self.update_settings_and_persist(update) {
                    Ok(apply_strategy) => ApiResponse::Accepted {
                        request_id: request.request_id,
                        apply_strategy: Some(apply_strategy),
                    },
                    Err(CoreRuntimeError::Config(message)) => error_response(
                        request.request_id,
                        ApiErrorCode::ValidationFailed,
                        &message,
                    ),
                    Err(err) => error_response(
                        request.request_id,
                        ApiErrorCode::Internal,
                        &format!("{err:?}"),
                    ),
                }
            }
            AppCommand::ClearArtifactCache => match self.clear_artifact_cache_live() {
                Ok(()) => ApiResponse::Accepted {
                    request_id: request.request_id,
                    apply_strategy: Some("live_applied".to_string()),
                },
                Err(CoreRuntimeError::Config(message)) => error_response(
                    request.request_id,
                    ApiErrorCode::ValidationFailed,
                    &message,
                ),
                Err(err) => error_response(
                    request.request_id,
                    ApiErrorCode::Internal,
                    &format!("{err:?}"),
                ),
            },
            AppCommand::ListSessions => {
                let mut items = self.sessions.values().cloned().collect::<Vec<_>>();
                items.sort_by(|a, b| a.id.cmp(&b.id));
                ApiResponse::Sessions {
                    request_id: request.request_id,
                    items,
                }
            }
            AppCommand::ListApprovals => {
                let mut items = self
                    .tool_handler
                    .metadata()
                    .approvals
                    .values()
                    .cloned()
                    .collect::<Vec<_>>();
                items.sort_by(|a, b| a.id.cmp(&b.id));
                ApiResponse::Approvals {
                    request_id: request.request_id,
                    items,
                }
            }
            AppCommand::GetToolchainDiagnostics => ApiResponse::Diagnostics {
                request_id: request.request_id,
                items: self.toolchain_diagnostics.clone(),
            },
            AppCommand::OpenSession { target_id } => {
                let target = match self.profiles.get(&target_id) {
                    Some(target) => target,
                    None => {
                        return error_response(
                            request.request_id,
                            ApiErrorCode::NotFound,
                            "target not found",
                        )
                    }
                };
                self.next_session_seq += 1;
                let session_id = format!("session-{:06}", self.next_session_seq);
                let now = SystemTime::now();
                let mut session = SessionRecord::new(session_id.clone(), target.id.clone(), now);
                session.transition(SessionState::Connected, now, None);
                self.sessions.insert(session_id, session.clone());
                self.tool_handler
                    .metadata_mut()
                    .upsert_session(session.clone());
                ApiResponse::Session {
                    request_id: request.request_id,
                    session,
                }
            }
            AppCommand::Execute {
                session_id,
                command,
                stream: _,
            } => {
                let session = match self.sessions.get(&session_id) {
                    Some(session) => session,
                    None => {
                        return error_response(
                            request.request_id,
                            ApiErrorCode::NotFound,
                            "session not found",
                        )
                    }
                };
                let target = match self.profiles.get(&session.target_id) {
                    Some(target) => target,
                    None => {
                        return error_response(
                            request.request_id,
                            ApiErrorCode::NotFound,
                            "target profile not found for session",
                        )
                    }
                };
                let artifact_id = format!("ipc-artifact-{}", request.request_id);
                let command_preview = command.clone();
                let result = self.tool_handler.handle(ToolRequest::TerminalExec {
                    target_id: target.id.clone(),
                    target_kind: target.kind.clone(),
                    context: ToolRequestContext {
                        agent_id: request.context.agent_id,
                        run_id: request.context.run_id,
                        client_session_id: request.context.client_session_id,
                        reuse_policy: request.context.reuse_policy,
                    },
                    command,
                    artifact_id,
                });
                match result {
                    ToolResult::Execution {
                        artifact_id,
                        logical_session_id,
                        ..
                    } => {
                        self.push_timeline_entry(
                            logical_session_id,
                            command_preview.clone(),
                            "success",
                            Some(artifact_id.clone()),
                        );
                        match self.tool_handler.metadata().artifacts.get(&artifact_id) {
                            Some(artifact) => ApiResponse::Execution {
                                request_id: request.request_id,
                                artifact: artifact.clone(),
                            },
                            None => error_response(
                                request.request_id,
                                ApiErrorCode::Internal,
                                "artifact missing after execution",
                            ),
                        }
                    }
                    ToolResult::ApprovalRequired { reason } => {
                        self.push_timeline_entry(
                            session_id,
                            command_preview.clone(),
                            "waiting_approval",
                            None,
                        );
                        error_response(request.request_id, ApiErrorCode::PermissionDenied, &reason)
                    }
                    ToolResult::Error { message } => {
                        self.push_timeline_entry(session_id, command_preview, "failed", None);
                        error_response(request.request_id, ApiErrorCode::Internal, &message)
                    }
                    _ => error_response(
                        request.request_id,
                        ApiErrorCode::Internal,
                        "unexpected tool result",
                    ),
                }
            }
            AppCommand::ReadArtifact {
                artifact_id,
                offset,
                limit,
            } => match self
                .tool_handler
                .artifact_service
                .read_artifact(&artifact_id, offset, limit)
            {
                Ok(view) => {
                    self.tool_handler
                        .metadata_mut()
                        .upsert_artifact(view.record.clone());
                    ApiResponse::Artifact {
                        request_id: request.request_id,
                        artifact: ArtifactReadView {
                            record: view.record,
                            offset: view.offset,
                            limit: view.limit,
                            total_chunks: view.total_chunks,
                            chunks: view.chunks,
                        },
                    }
                }
                Err(message) => {
                    error_response(request.request_id, ApiErrorCode::NotFound, &message)
                }
            },
            AppCommand::RequestApproval { request: approval } => {
                self.tool_handler
                    .metadata_mut()
                    .upsert_approval(approval.clone());
                ApiResponse::Approval {
                    request_id: request.request_id,
                    request: approval,
                }
            }
        }
    }

    pub fn handle_mcp_request(&mut self, request: ToolRequest) -> ToolResult {
        self.tool_handler.handle(request)
    }
}

fn error_response(request_id: String, code: ApiErrorCode, message: &str) -> ApiResponse {
    ApiResponse::Error {
        request_id,
        error: ApiError {
            code,
            message: message.to_string(),
            retriable: false,
        },
    }
}

fn normalize_target_ref(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn register_target_ref(
    map: &mut HashMap<String, String>,
    reference: &str,
    target_id: &str,
) -> Result<(), CoreRuntimeError> {
    let normalized = normalize_target_ref(reference);
    if normalized.is_empty() {
        return Ok(());
    }
    if let Some(existing) = map.get(&normalized) {
        if existing != target_id {
            return Err(CoreRuntimeError::Config(format!(
                "target reference conflict: {reference} points to both {existing} and {target_id}"
            )));
        }
    } else {
        map.insert(normalized, target_id.to_string());
    }
    Ok(())
}

fn target_kind_label(kind: &TargetKind) -> String {
    match kind {
        TargetKind::Ssh => "ssh".to_string(),
        TargetKind::Adb => "adb".to_string(),
        TargetKind::Serial => "serial".to_string(),
        TargetKind::Docker => "docker".to_string(),
        TargetKind::Other(value) => value.clone(),
    }
}

fn toolchain_command_for_kind(kind: &TargetKind) -> Option<&'static str> {
    match kind {
        TargetKind::Ssh => Some("ssh"),
        TargetKind::Adb => Some("adb"),
        _ => None,
    }
}

fn non_empty_path(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn session_state_label(state: &SessionState) -> &'static str {
    match state {
        SessionState::Connecting => "connecting",
        SessionState::Connected => "connected",
        SessionState::Degraded => "degraded",
        SessionState::Failed => "failed",
        SessionState::Closed => "closed",
    }
}

fn approval_status_label(status: &bridgingio_domain::ApprovalStatus) -> &'static str {
    match status {
        bridgingio_domain::ApprovalStatus::Pending => "pending",
        bridgingio_domain::ApprovalStatus::Approved => "approved",
        bridgingio_domain::ApprovalStatus::Denied => "denied",
        bridgingio_domain::ApprovalStatus::Expired => "expired",
    }
}

fn system_time_to_unix_millis(ts: SystemTime) -> u128 {
    ts.duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn build_interactive_connector_command(
    target: &TargetProfile,
    resolved_executable_path: Option<&str>,
) -> Option<String> {
    let resolved_executable = |default: &str| {
        resolved_executable_path
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(shell_single_quote)
            .unwrap_or_else(|| default.to_string())
    };

    match (&target.kind, &target.connection) {
        (TargetKind::Adb, ConnectionConfig::Adb { serial, transport }) => {
            let mut tokens = vec![resolved_executable("adb")];
            let selector = serial
                .as_ref()
                .map(|v| v.trim())
                .filter(|v| !v.is_empty())
                .map(ToString::to_string);
            let transport = transport
                .as_ref()
                .map(|v| v.trim().to_ascii_lowercase())
                .filter(|v| !v.is_empty());
            match (transport.as_deref(), selector.as_deref()) {
                (Some("transport-id"), Some(value)) => {
                    tokens.push("-t".to_string());
                    tokens.push(value.to_string());
                }
                (_, Some(value)) => {
                    tokens.push("-s".to_string());
                    tokens.push(value.to_string());
                }
                (Some("emulator"), None) => tokens.push("-e".to_string()),
                (Some("device"), None) | (Some("usb"), None) => tokens.push("-d".to_string()),
                _ => {}
            }
            tokens.push("shell".to_string());
            Some(tokens.join(" "))
        }
        (
            TargetKind::Ssh,
            ConnectionConfig::Ssh {
                host,
                port,
                username,
            },
        ) => {
            let user_host = format!("{username}@{host}");
            Some(format!(
                "{} -p {} {}",
                resolved_executable("ssh"),
                port,
                shell_single_quote(&user_host)
            ))
        }
        _ => None,
    }
}

fn build_connector_command(
    target: &TargetProfile,
    command: &str,
    resolved_executable_path: Option<&str>,
) -> String {
    let resolved_executable = |default: &str| {
        resolved_executable_path
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(shell_single_quote)
            .unwrap_or_else(|| default.to_string())
    };

    match (&target.kind, &target.connection) {
        (TargetKind::Adb, ConnectionConfig::Adb { serial, transport }) => {
            if command.trim_start().starts_with("adb ") {
                return command.to_string();
            }
            let mut tokens = vec![resolved_executable("adb")];
            let selector = serial
                .as_ref()
                .map(|v| v.trim())
                .filter(|v| !v.is_empty())
                .map(ToString::to_string);
            let transport = transport
                .as_ref()
                .map(|v| v.trim().to_ascii_lowercase())
                .filter(|v| !v.is_empty());
            match (transport.as_deref(), selector.as_deref()) {
                (Some("transport-id"), Some(value)) => {
                    tokens.push("-t".to_string());
                    tokens.push(value.to_string());
                }
                (_, Some(value)) => {
                    tokens.push("-s".to_string());
                    tokens.push(value.to_string());
                }
                (Some("emulator"), None) => tokens.push("-e".to_string()),
                (Some("device"), None) | (Some("usb"), None) => tokens.push("-d".to_string()),
                _ => {}
            }
            tokens.push("shell".to_string());
            tokens.push(shell_single_quote(command));
            tokens.join(" ")
        }
        (
            TargetKind::Ssh,
            ConnectionConfig::Ssh {
                host,
                port,
                username,
            },
        ) => {
            if command.trim_start().starts_with("ssh ") {
                return command.to_string();
            }
            let user_host = format!("{username}@{host}");
            format!(
                "{} -p {} {} {}",
                resolved_executable("ssh"),
                port,
                shell_single_quote(&user_host),
                shell_single_quote(command)
            )
        }
        _ => command.to_string(),
    }
}

fn shell_single_quote(value: &str) -> String {
    #[cfg(windows)]
    {
        value.to_string()
    }
    #[cfg(not(windows))]
    {
        format!("'{}'", value.replace('\'', "'\"'\"'"))
    }
}

fn first_data_line(output: &str) -> String {
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("stderr:") {
            continue;
        }
        return trimmed.to_string();
    }
    String::new()
}

fn artifact_store_config_from_settings(
    settings: &CoreSettings,
) -> Result<ArtifactStoreConfig, CoreRuntimeError> {
    let backend = ArtifactCacheBackend::parse(&settings.storage.artifacts.backend)
        .map_err(|err| CoreRuntimeError::Config(err.message))?;
    let eviction_policy =
        ArtifactEvictionPolicy::parse(&settings.storage.artifacts.eviction_policy)
            .map_err(|err| CoreRuntimeError::Config(err.message))?;
    Ok(ArtifactStoreConfig {
        backend,
        root: expand_tilde_path(&settings.storage.artifacts.root),
        max_bytes: settings.storage.artifacts.max_bytes,
        eviction_policy,
    })
}

fn expand_tilde_path(raw: &str) -> PathBuf {
    if raw == "~" {
        return std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(raw));
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(raw)
}

fn to_target_profile(
    configured: &StandaloneTargetProfile,
) -> Result<TargetProfile, CoreRuntimeError> {
    let connection = match configured.kind {
        TargetKind::Ssh => to_ssh_connection(&configured.connection)?,
        TargetKind::Adb => to_adb_connection(&configured.connection),
        TargetKind::Serial => to_serial_connection(&configured.connection),
        TargetKind::Docker => to_docker_connection(&configured.connection),
        TargetKind::Other(_) => bridgingio_domain::ConnectionConfig::Custom {
            description: "custom target".into(),
        },
    };
    let mut metadata = bridgingio_domain::MetadataMap::new();
    if let Some(alias) = configured.aliases.first() {
        metadata.insert("alias".to_string(), alias.clone());
    }
    let toolchains = configured
        .toolchains
        .iter()
        .filter_map(|(command, section)| {
            let path = section.path_override.trim();
            if path.is_empty() {
                None
            } else {
                Some((command.clone(), path.to_string()))
            }
        })
        .collect::<bridgingio_domain::MetadataMap>();
    Ok(TargetProfile {
        id: configured.id.clone(),
        name: configured.display_name.clone(),
        kind: configured.kind.clone(),
        connection,
        credential_ref: configured.credential_ref.as_ref().map(|reference| {
            bridgingio_domain::CredentialRef {
                id: reference.clone(),
                provider: "vault".into(),
            }
        }),
        default_policy: bridgingio_domain::PolicyProfile::default(),
        notes: configured.notes.clone(),
        metadata,
        toolchains,
    })
}

fn to_ssh_connection(
    connection: &StandaloneConnectionSection,
) -> Result<bridgingio_domain::ConnectionConfig, CoreRuntimeError> {
    let host = connection
        .host
        .clone()
        .ok_or_else(|| CoreRuntimeError::Config("ssh target missing host".into()))?;
    let port = connection.port.unwrap_or(22);
    let username = connection
        .username
        .clone()
        .ok_or_else(|| CoreRuntimeError::Config("ssh target missing username".into()))?;
    Ok(bridgingio_domain::ConnectionConfig::Ssh {
        host,
        port,
        username,
    })
}

fn to_adb_connection(
    connection: &StandaloneConnectionSection,
) -> bridgingio_domain::ConnectionConfig {
    bridgingio_domain::ConnectionConfig::Adb {
        serial: connection.selector_value.clone(),
        transport: connection.selector_kind.clone(),
    }
}

fn to_serial_connection(connection: &StandaloneConnectionSection) -> bridgingio_domain::ConnectionConfig {
    let device = connection
        .selector_value
        .clone()
        .unwrap_or_else(|| "/dev/tty.usbmodem0".to_string());
    let baud_rate = connection
        .selector_kind
        .as_ref()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(115_200);
    bridgingio_domain::ConnectionConfig::Serial { device, baud_rate }
}

fn to_docker_connection(connection: &StandaloneConnectionSection) -> bridgingio_domain::ConnectionConfig {
    let container = connection
        .selector_value
        .clone()
        .unwrap_or_else(|| "container".to_string());
    bridgingio_domain::ConnectionConfig::Docker {
        container,
        context: connection.selector_kind.clone(),
    }
}

fn to_standalone_target_profile(
    profile: &TargetProfile,
) -> Result<StandaloneTargetProfile, CoreRuntimeError> {
    let mut connection = StandaloneConnectionSection::default();
    match &profile.connection {
        ConnectionConfig::Ssh {
            host,
            port,
            username,
        } => {
            connection.host = Some(host.clone());
            connection.port = Some(*port);
            connection.username = Some(username.clone());
        }
        ConnectionConfig::Adb { serial, transport } => {
            connection.selector_value = serial.clone();
            connection.selector_kind = transport.clone();
        }
        ConnectionConfig::Serial { device, baud_rate } => {
            connection.selector_value = Some(device.clone());
            connection.selector_kind = Some(baud_rate.to_string());
        }
        ConnectionConfig::Docker { container, context } => {
            connection.selector_value = Some(container.clone());
            connection.selector_kind = context.clone();
        }
        ConnectionConfig::Custom { description } => {
            connection.selector_value = Some(description.clone());
        }
    }

    let toolchains = profile
        .toolchains
        .iter()
        .filter_map(|(command, path_override)| {
            let trimmed = path_override.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some((
                    command.clone(),
                    ToolchainSection {
                        path_override: trimmed.to_string(),
                        prefer_builtin_fallback: false,
                    },
                ))
            }
        })
        .collect::<HashMap<_, _>>();

    Ok(StandaloneTargetProfile {
        id: profile.id.clone(),
        display_name: profile.name.clone(),
        kind: profile.kind.clone(),
        enabled: true,
        aliases: profile
            .metadata
            .get("alias")
            .map(|alias| vec![alias.clone()])
            .unwrap_or_default(),
        credential_ref: profile.credential_ref.as_ref().map(|value| value.id.clone()),
        notes: profile.notes.clone(),
        connection,
        toolchains,
        terminal_provider: bridgingio_engine::TerminalProviderSection {
            enabled: false,
            shell: None,
        },
        git_repositories: Vec::new(),
    })
}

fn target_profile_payload_json(profile: &TargetProfile) -> String {
    target_profile_json_value(profile).to_string()
}

fn target_profile_json_value(profile: &TargetProfile) -> Value {
    let alias = profile.metadata.get("alias").cloned().unwrap_or_default();
    let (connection_kind, connection_json) = match &profile.connection {
        ConnectionConfig::Ssh {
            host,
            port,
            username,
        } => (
            "ssh",
            json!({
                "host": host,
                "port": port,
                "username": username,
            }),
        ),
        ConnectionConfig::Adb { serial, transport } => (
            "adb",
            json!({
                "serial": serial,
                "transport": transport,
            }),
        ),
        ConnectionConfig::Serial { device, baud_rate } => (
            "serial",
            json!({
                "device": device,
                "baud_rate": baud_rate,
            }),
        ),
        ConnectionConfig::Docker { container, context } => (
            "docker",
            json!({
                "container": container,
                "context": context,
            }),
        ),
        ConnectionConfig::Custom { description } => (
            "custom",
            json!({
                "description": description,
            }),
        ),
    };

    json!({
        "id": profile.id,
        "name": profile.name,
        "kind": target_kind_label(&profile.kind),
        "alias": alias,
        "notes": profile.notes,
        "credential_ref": profile.credential_ref.as_ref().map(|value| value.id.clone()),
        "toolchains": profile.toolchains.iter().map(|(command, path_override)| {
            json!({
                "command": command,
                "path_override": path_override,
            })
        }).collect::<Vec<_>>(),
        "connection_kind": connection_kind,
        "connection": connection_json,
    })
}

fn vault_error_to_runtime(err: VaultError) -> CoreRuntimeError {
    CoreRuntimeError::Config(format!("{err:?}"))
}

pub fn control_plane_socket_path(settings: &CoreSettings) -> PathBuf {
    if settings.control_plane.endpoint == "auto" {
        std::env::temp_dir().join(format!("bridgingio-{}.sock", settings.core.instance_name))
    } else {
        PathBuf::from(&settings.control_plane.endpoint)
    }
}

fn model_plane_socket_addr(settings: &CoreSettings) -> Result<SocketAddr, CoreRuntimeError> {
    let host = settings
        .model_plane
        .http
        .host
        .parse::<IpAddr>()
        .map_err(|_| {
            CoreRuntimeError::Config("model_plane.http.host must be an IP address".into())
        })?;
    let is_loopback = host.is_loopback();
    if !is_loopback && !settings.model_plane.http.allow_non_loopback {
        return Err(CoreRuntimeError::Config(
            "non-loopback host requires allow_non_loopback=true".into(),
        ));
    }
    if !is_loopback
        && settings.model_plane.http.auth.required_when_non_loopback
        && settings.model_plane.http.auth.mode == "none"
    {
        return Err(CoreRuntimeError::Config(
            "non-loopback host requires auth mode when required_when_non_loopback=true".into(),
        ));
    }
    Ok(SocketAddr::new(host, settings.model_plane.http.port))
}

#[cfg(unix)]
pub struct ControlPlaneIpcServer {
    listener: UnixListener,
    socket_path: PathBuf,
    runtime: SharedRuntime,
}

#[cfg(unix)]
impl ControlPlaneIpcServer {
    pub fn bind(
        runtime: SharedRuntime,
        socket_path: impl AsRef<Path>,
    ) -> Result<Self, CoreRuntimeError> {
        let socket_path = socket_path.as_ref().to_path_buf();
        if socket_path.exists() {
            let _ = std::fs::remove_file(&socket_path);
        }
        let listener = UnixListener::bind(&socket_path)
            .map_err(|err| CoreRuntimeError::Io(format!("failed to bind ipc socket: {err}")))?;
        Ok(Self {
            listener,
            socket_path,
            runtime,
        })
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub fn serve_once(&self) -> Result<(), CoreRuntimeError> {
        let (mut stream, _) = self
            .listener
            .accept()
            .map_err(|err| CoreRuntimeError::Io(format!("accept ipc: {err}")))?;
        let mut line = String::new();
        {
            let mut reader = BufReader::new(&mut stream);
            reader
                .read_line(&mut line)
                .map_err(|err| CoreRuntimeError::Io(format!("read ipc request: {err}")))?;
        }
        let response = match AppApiLineCodec::decode_request_line(line.trim()) {
            Ok(request) => {
                let mut runtime = self
                    .runtime
                    .lock()
                    .map_err(|_| CoreRuntimeError::LockPoisoned)?;
                runtime.handle_app_request(request)
            }
            Err(err) => ApiResponse::Error {
                request_id: "unknown".into(),
                error: err,
            },
        };
        let payload = AppApiLineCodec::encode_response_line(&response);
        stream
            .write_all(format!("{payload}\n").as_bytes())
            .map_err(|err| CoreRuntimeError::Io(format!("write ipc response: {err}")))?;
        Ok(())
    }
}

#[cfg(unix)]
impl Drop for ControlPlaneIpcServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

#[cfg(unix)]
pub struct ControlPlaneIpcClient {
    socket_path: PathBuf,
}

#[cfg(unix)]
impl ControlPlaneIpcClient {
    pub fn new(socket_path: impl AsRef<Path>) -> Self {
        Self {
            socket_path: socket_path.as_ref().to_path_buf(),
        }
    }

    pub fn send(&self, request: &ApiRequest) -> Result<ApiResponse, CoreRuntimeError> {
        let mut stream = UnixStream::connect(&self.socket_path)
            .map_err(|err| CoreRuntimeError::Io(format!("connect ipc client: {err}")))?;
        let line = AppApiLineCodec::encode_request_line(request);
        stream
            .write_all(format!("{line}\n").as_bytes())
            .map_err(|err| CoreRuntimeError::Io(format!("write ipc request: {err}")))?;
        let mut response_line = String::new();
        {
            let mut reader = BufReader::new(&mut stream);
            reader
                .read_line(&mut response_line)
                .map_err(|err| CoreRuntimeError::Io(format!("read ipc response: {err}")))?;
        }
        AppApiLineCodec::decode_response_line(response_line.trim())
            .map_err(|err| CoreRuntimeError::Config(format!("decode response: {}", err.message)))
    }
}

pub struct ModelPlaneHttpServer {
    listener: TcpListener,
    runtime: SharedRuntime,
}

impl ModelPlaneHttpServer {
    pub fn bind(runtime: SharedRuntime, settings: &CoreSettings) -> Result<Self, CoreRuntimeError> {
        let addr = model_plane_socket_addr(settings)?;
        let listener = TcpListener::bind(addr)
            .map_err(|err| CoreRuntimeError::Io(format!("bind http listener: {err}")))?;
        Ok(Self { listener, runtime })
    }

    pub fn local_addr(&self) -> Result<SocketAddr, CoreRuntimeError> {
        self.listener
            .local_addr()
            .map_err(|err| CoreRuntimeError::Io(format!("local_addr: {err}")))
    }

    pub fn serve_once(&self) -> Result<(), CoreRuntimeError> {
        let (mut stream, _) = self
            .listener
            .accept()
            .map_err(|err| CoreRuntimeError::Io(format!("accept http: {err}")))?;
        handle_http_connection(&mut stream, &self.runtime)
    }
}

fn handle_http_connection(
    stream: &mut TcpStream,
    runtime: &SharedRuntime,
) -> Result<(), CoreRuntimeError> {
    let (method, path, body) = read_http_request(stream)?;
    trace_mcp(format!(
        "http request method={} path={} body_len={}",
        method,
        path,
        body.len()
    ));
    let (status, content_type, response_body) = match (method.as_str(), path.as_str()) {
        ("GET", "/health") => (200, "text/plain", "ok".to_string()),
        ("GET", "/state/sessions") => {
            let runtime = runtime.lock().map_err(|_| CoreRuntimeError::LockPoisoned)?;
            (
                200,
                "text/plain",
                format!("sessions={}", runtime.session_count()),
            )
        }
        ("GET", "/state/logical-sessions") => {
            let runtime = runtime.lock().map_err(|_| CoreRuntimeError::LockPoisoned)?;
            (
                200,
                "text/plain",
                format!("logical_sessions={}", runtime.logical_session_count()),
            )
        }
        ("POST", "/mcp") => handle_mcp_http_request(runtime, &body)?,
        ("POST", "/tool/terminal.exec") => {
            if let Some(reason) = {
                let runtime = runtime.lock().map_err(|_| CoreRuntimeError::LockPoisoned)?;
                runtime.model_plane_not_ready_reason()
            } {
                (
                    503,
                    "text/plain",
                    format!("result=not_ready|reason={reason}"),
                )
            } else {
            let params = parse_kv_body(&body);
            let mut runtime = runtime.lock().map_err(|_| CoreRuntimeError::LockPoisoned)?;
            let target_ref = optional_param(&params, "target_ref")
                .or_else(|| optional_param(&params, "target"))
                .or_else(|| optional_param(&params, "target_id"))
                .ok_or_else(|| CoreRuntimeError::Config("missing param target_id".into()))?;
            match runtime.execute_target_command(
                target_ref,
                ToolRequestContext {
                    agent_id: required_param(&params, "agent_id")?.to_string(),
                    run_id: required_param(&params, "run_id")?.to_string(),
                    client_session_id: required_param(&params, "client_session_id")?.to_string(),
                    reuse_policy: parse_reuse_policy(required_param(&params, "reuse_policy")?)?,
                },
                required_param(&params, "command")?,
                optional_param(&params, "artifact_id").map(ToString::to_string),
            ) {
                Ok(TargetCommandExecution {
                    artifact_id,
                    logical_session_id,
                    channel_id,
                    ..
                }) => (
                    200,
                    "text/plain",
                    format!(
                        "result=execution|artifact_id={artifact_id}|logical_session_id={logical_session_id}|channel_id={channel_id}"
                    ),
                ),
                Err(CoreRuntimeError::Config(message))
                    if message.contains("requires approval") =>
                {
                    (
                        403,
                        "text/plain",
                        format!("result=approval_required|reason={message}"),
                    )
                }
                Err(CoreRuntimeError::Config(message)) => {
                    (400, "text/plain", format!("result=error|message={message}"))
                }
                Err(err) => (
                    500,
                    "text/plain",
                    format!("result=error|message={err:?}"),
                ),
            }
            }
        }
        _ => (404, "text/plain", "not found".to_string()),
    };
    trace_mcp(format!(
        "http response method={} path={} status={} content_type={} body_len={}",
        method,
        path,
        status,
        content_type,
        response_body.len()
    ));
    write_http_response(stream, status, content_type, &response_body)
}

fn handle_mcp_http_request(
    runtime: &SharedRuntime,
    body: &str,
) -> Result<(u16, &'static str, String), CoreRuntimeError> {
    let not_ready_reason = {
        let runtime = runtime.lock().map_err(|_| CoreRuntimeError::LockPoisoned)?;
        runtime.model_plane_not_ready_reason()
    };
    if let Some(reason) = not_ready_reason {
        let id = serde_json::from_str::<Value>(body)
            .ok()
            .and_then(|value| value.get("id").cloned())
            .unwrap_or(Value::Null);
        return Ok((
            503,
            "application/json",
            jsonrpc_error(id, -32001, &format!("model plane not ready: {reason}")),
        ));
    }

    let request_value: Value = match serde_json::from_str(body) {
        Ok(value) => value,
        Err(err) => {
            return Ok((
                200,
                "application/json",
                jsonrpc_error(Value::Null, -32700, &format!("parse error: {err}")),
            ))
        }
    };

    let id = request_value.get("id").cloned().unwrap_or(Value::Null);
    let id_brief = jsonrpc_id_brief(&id);
    let Some(method) = request_value.get("method").and_then(Value::as_str) else {
        return Ok((
            200,
            "application/json",
            jsonrpc_error(id, -32600, "invalid request: missing method"),
        ));
    };
    if method == "tools/call" {
        let tool_name = request_value
            .get("params")
            .and_then(Value::as_object)
            .and_then(|map| map.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("<missing>");
        trace_mcp(format!(
            "mcp request id={} method={} tool={}",
            id_brief, method, tool_name
        ));
    } else {
        trace_mcp(format!("mcp request id={} method={}", id_brief, method));
    }
    let params = request_value.get("params").cloned().unwrap_or(Value::Null);

    if method.starts_with("notifications/") && id.is_null() {
        return Ok((202, "application/json", String::new()));
    }

    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "resources": {
                    "listChanged": false
                },
                "tools": {
                    "listChanged": false
                }
            },
            "serverInfo": {
                "name": "bridgingio-core",
                "version": env!("CARGO_PKG_VERSION")
            }
        })),
        "tools/list" => Ok(json!({
            "tools": mcp_tools()
        })),
        "resources/list" => Ok(json!({
            "resources": mcp_resources()
        })),
        "resources/templates/list" => Ok(json!({
            "resourceTemplates": mcp_resource_templates()
        })),
        "resources/read" => handle_mcp_resources_read(&params),
        "tools/call" => handle_mcp_tools_call(runtime, &params),
        "notifications/initialized" => Ok(json!({})),
        _ => Err(jsonrpc_error(
            id.clone(),
            -32601,
            &format!("method not found: {method}"),
        )),
    };

    let response_body = match result {
        Ok(payload) => jsonrpc_success(id.clone(), payload),
        Err(error_body) => normalize_jsonrpc_error_id(error_body, &id),
    };
    trace_mcp(format!(
        "mcp response id={} method={} body_len={}",
        id_brief,
        method,
        response_body.len()
    ));
    Ok((200, "application/json", response_body))
}

#[derive(Clone, Copy)]
struct ToolCatalogEntry {
    name: &'static str,
    title: &'static str,
    capability_id: &'static str,
    short_description: &'static str,
    detailed_description: &'static str,
    input_schema: fn() -> Value,
}

#[derive(Clone, Copy)]
struct CapabilityCatalogEntry {
    id: &'static str,
    title: &'static str,
    short_description: &'static str,
    detailed_description: &'static str,
    related_tools: &'static [&'static str],
}

fn mcp_tools() -> Vec<Value> {
    mcp_tool_catalog()
        .iter()
        .map(|entry| {
            json!({
                "name": entry.name,
                "title": entry.title,
                "description": entry.short_description,
                "inputSchema": (entry.input_schema)()
            })
        })
        .collect()
}

fn mcp_tool_catalog() -> &'static [ToolCatalogEntry] {
    &[
        ToolCatalogEntry {
            name: "bridgingio.terminal.exec",
            title: "Execute Command",
            capability_id: "terminal.exec",
            short_description: "Run one-shot command execution on a configured target alias or id.",
            detailed_description: "Use this when you need a single command result and do not need to keep shell state. For large text output, collect broadly first and then use artifacts.refine for post-filtering to reduce repeated target calls and token waste.",
            input_schema: schema_terminal_exec,
        },
        ToolCatalogEntry {
            name: "bridgingio.artifacts.read",
            title: "Read Artifact Chunks",
            capability_id: "artifact.reanalysis",
            short_description: "Read cached output chunks from an existing raw or derived artifact.",
            detailed_description: "Use this to page through cached text with offset and limit. It works for artifacts generated by any text-producing target or provider and avoids rerunning the original remote command.",
            input_schema: schema_artifacts_read,
        },
        ToolCatalogEntry {
            name: "bridgingio.artifacts.refine",
            title: "Refine Artifact",
            capability_id: "artifact.reanalysis",
            short_description: "Create a derived artifact by keyword or regex filtering from a source artifact.",
            detailed_description: "Use this for second-pass analysis after raw collection. Supports mode auto/keyword/regex, grep-style flags, and explicit processing_mode (source, bridgingio, auto). The current implementation resolves processing on cached artifacts in BridgingIO and records requested/resolved processing mode in output metadata.",
            input_schema: schema_artifacts_refine,
        },
        ToolCatalogEntry {
            name: "bridgingio.terminal.shell.open",
            title: "Open Interactive Shell",
            capability_id: "terminal.interactive_shell",
            short_description: "Open an interactive shell handle for multi-step command workflows.",
            detailed_description: "Use this when tasks require shell state continuity such as cd, export, and iterative troubleshooting. The returned shell_id is used with shell.write/read/interrupt/close.",
            input_schema: schema_terminal_shell_open,
        },
        ToolCatalogEntry {
            name: "bridgingio.terminal.shell.write",
            title: "Write Interactive Shell",
            capability_id: "terminal.interactive_shell",
            short_description: "Send one input command to an opened interactive shell handle.",
            detailed_description: "Use this to append commands into an existing interactive shell channel. The tool returns current output, prompt, cwd, and a generated artifact id for transcript segments.",
            input_schema: schema_terminal_shell_write,
        },
        ToolCatalogEntry {
            name: "bridgingio.terminal.shell.read",
            title: "Read Interactive Transcript",
            capability_id: "terminal.interactive_shell",
            short_description: "Read transcript lines from an interactive shell handle with pagination.",
            detailed_description: "Use this to inspect shell transcript incrementally with offset and limit. It helps stream long-running interactions without replaying full transcript every round.",
            input_schema: schema_terminal_shell_read,
        },
        ToolCatalogEntry {
            name: "bridgingio.terminal.shell.interrupt",
            title: "Interrupt Interactive Shell",
            capability_id: "terminal.interactive_shell",
            short_description: "Request an interrupt signal on an opened interactive shell handle.",
            detailed_description: "Use this to stop long-running foreground tasks in interactive mode while preserving channel context for follow-up commands.",
            input_schema: schema_terminal_shell_interrupt,
        },
        ToolCatalogEntry {
            name: "bridgingio.terminal.shell.close",
            title: "Close Interactive Shell",
            capability_id: "terminal.interactive_shell",
            short_description: "Close an opened interactive shell handle and finalize channel lifecycle.",
            detailed_description: "Use this when the interactive task is complete or needs cleanup. Closing prevents further writes and keeps audit/session boundaries explicit.",
            input_schema: schema_terminal_shell_close,
        },
        ToolCatalogEntry {
            name: "bridgingio.target.inspect_basic",
            title: "Inspect Basic Target Info",
            capability_id: "target.inspect",
            short_description: "Collect kernel version and username from the target in structured form.",
            detailed_description: "Use this for low-risk environment fingerprint bootstrap. It runs stable one-shot commands and returns normalized fields for follow-up decision making.",
            input_schema: schema_target_inspect_basic,
        },
        ToolCatalogEntry {
            name: "bridgingio.capability.describe",
            title: "Describe Capability",
            capability_id: "capability.documentation",
            short_description: "Get detailed English documentation for a capability id or tool name.",
            detailed_description: "Use this after tools/list when short descriptions are not enough. It returns detailed guidance including usage intent, parameter semantics, and related tools from the core-owned capability catalog.",
            input_schema: schema_capability_describe,
        },
    ]
}

fn mcp_capability_catalog() -> &'static [CapabilityCatalogEntry] {
    &[
        CapabilityCatalogEntry {
            id: "terminal.exec",
            title: "Terminal One-Shot Execution",
            short_description: "Run isolated one-shot commands on a configured target.",
            detailed_description: "Recommended for read-style checks and small command workflows where shell state persistence is unnecessary. Pair with artifact.reanalysis when output is large and needs iterative filtering.",
            related_tools: &["bridgingio.terminal.exec"],
        },
        CapabilityCatalogEntry {
            id: "terminal.interactive_shell",
            title: "Terminal Interactive Shell",
            short_description: "Maintain shell context across multiple writes and reads.",
            detailed_description: "Recommended for workflows that depend on sequential context, environment mutation, or long-running troubleshooting sessions. Access is scoped by agent/run/client_session.",
            related_tools: &[
                "bridgingio.terminal.shell.open",
                "bridgingio.terminal.shell.write",
                "bridgingio.terminal.shell.read",
                "bridgingio.terminal.shell.interrupt",
                "bridgingio.terminal.shell.close",
            ],
        },
        CapabilityCatalogEntry {
            id: "artifact.reanalysis",
            title: "Artifact Reanalysis",
            short_description: "Read and refine cached text artifacts independent of target type.",
            detailed_description: "Treats post-collection text analysis as a first-class capability. Any target/provider that emits text artifacts can reuse artifacts.read and artifacts.refine without relying on SSH or ADB specific semantics.",
            related_tools: &["bridgingio.artifacts.read", "bridgingio.artifacts.refine"],
        },
        CapabilityCatalogEntry {
            id: "target.inspect",
            title: "Target Basic Inspection",
            short_description: "Gather basic kernel and identity information from target.",
            detailed_description: "Provides quick environment fingerprint hints before running deeper workflows, and returns structured fields suitable for policy-aware prompts.",
            related_tools: &["bridgingio.target.inspect_basic"],
        },
        CapabilityCatalogEntry {
            id: "capability.documentation",
            title: "Capability Documentation",
            short_description: "Read detailed docs for tool and capability entries.",
            detailed_description: "Provides a second-layer documentation channel that complements tools/list short descriptions, with stable lookup by tool name or capability id.",
            related_tools: &["bridgingio.capability.describe"],
        },
    ]
}

fn mcp_resources() -> Vec<Value> {
    let mut resources = Vec::new();
    for capability in mcp_capability_catalog() {
        let uri = format!("bridgingio://capability/{}", capability.id);
        resources.push(json!({
            "uri": uri,
            "name": capability.title,
            "description": capability.short_description,
            "mimeType": "application/json"
        }));
    }
    for tool in mcp_tool_catalog() {
        let uri = format!("bridgingio://tool/{}", tool.name);
        resources.push(json!({
            "uri": uri,
            "name": tool.title,
            "description": tool.short_description,
            "mimeType": "application/json"
        }));
    }
    resources
}

fn mcp_resource_templates() -> Vec<Value> {
    vec![json!({
        "uriTemplate": "bridgingio://{kind}/{id}",
        "name": "BridgingIO Catalog Entry",
        "description": "Read detailed documentation for a tool or capability (kind: tool|capability).",
        "mimeType": "application/json"
    })]
}

fn schema_terminal_exec() -> Value {
    json!({
        "type": "object",
        "required": ["target", "command"],
        "properties": {
            "target": {
                "type": "string",
                "description": "Target ID or alias from core config"
            },
            "command": {
                "type": "string",
                "description": "Shell command to run on target"
            },
            "agent_id": {"type": "string", "default": "agent-mcp"},
            "run_id": {"type": "string", "default": "run-mcp"},
            "client_session_id": {"type": "string", "default": "client-mcp"},
            "reuse_policy": {
                "type": "string",
                "enum": ["always_new", "reuse_if_alive", "resume_or_create"],
                "default": "reuse_if_alive"
            }
        }
    })
}

fn schema_artifacts_read() -> Value {
    json!({
        "type": "object",
        "required": ["artifact_id"],
        "properties": {
            "artifact_id": {"type": "string"},
            "offset": {"type": "integer", "minimum": 0, "default": 0},
            "limit": {"type": "integer", "minimum": 1, "default": 200}
        }
    })
}

fn schema_artifacts_refine() -> Value {
    json!({
        "type": "object",
        "required": ["source_artifact_id", "pattern"],
        "properties": {
            "source_artifact_id": {"type": "string"},
            "derived_artifact_id": {"type": "string"},
            "pattern": {"type": "string", "description": "Keyword or regex pattern (for example: system_server|netd)"},
            "mode": {"type": "string", "enum": ["auto", "keyword", "regex"], "default": "auto"},
            "ignore_case": {"type": "boolean", "default": false},
            "grep_flags": {"type": "string", "description": "Optional grep-like flags, supports E and i (for example: Ei)"},
            "processing_mode": {"type": "string", "enum": ["source", "bridgingio", "auto"], "default": "auto", "description": "Text processing ownership preference for this refinement request."}
        }
    })
}

fn schema_terminal_shell_open() -> Value {
    json!({
        "type": "object",
        "required": ["target"],
        "properties": {
            "target": {
                "type": "string",
                "description": "Target ID or alias from core config"
            },
            "agent_id": {"type": "string", "default": "agent-mcp"},
            "run_id": {"type": "string", "default": "run-mcp"},
            "client_session_id": {"type": "string", "default": "client-mcp"},
            "reuse_policy": {
                "type": "string",
                "enum": ["always_new", "reuse_if_alive", "resume_or_create"],
                "default": "reuse_if_alive"
            }
        }
    })
}

fn schema_terminal_shell_write() -> Value {
    json!({
        "type": "object",
        "required": ["shell_id", "input"],
        "properties": {
            "shell_id": {"type": "string"},
            "input": {"type": "string"},
            "agent_id": {"type": "string", "default": "agent-mcp"},
            "run_id": {"type": "string", "default": "run-mcp"},
            "client_session_id": {"type": "string", "default": "client-mcp"},
            "reuse_policy": {
                "type": "string",
                "enum": ["always_new", "reuse_if_alive", "resume_or_create"],
                "default": "reuse_if_alive"
            }
        }
    })
}

fn schema_terminal_shell_read() -> Value {
    json!({
        "type": "object",
        "required": ["shell_id"],
        "properties": {
            "shell_id": {"type": "string"},
            "offset": {"type": "integer", "minimum": 0, "default": 0},
            "limit": {"type": "integer", "minimum": 1, "default": 100},
            "agent_id": {"type": "string", "default": "agent-mcp"},
            "run_id": {"type": "string", "default": "run-mcp"},
            "client_session_id": {"type": "string", "default": "client-mcp"},
            "reuse_policy": {
                "type": "string",
                "enum": ["always_new", "reuse_if_alive", "resume_or_create"],
                "default": "reuse_if_alive"
            }
        }
    })
}

fn schema_terminal_shell_interrupt() -> Value {
    json!({
        "type": "object",
        "required": ["shell_id"],
        "properties": {
            "shell_id": {"type": "string"},
            "agent_id": {"type": "string", "default": "agent-mcp"},
            "run_id": {"type": "string", "default": "run-mcp"},
            "client_session_id": {"type": "string", "default": "client-mcp"},
            "reuse_policy": {
                "type": "string",
                "enum": ["always_new", "reuse_if_alive", "resume_or_create"],
                "default": "reuse_if_alive"
            }
        }
    })
}

fn schema_terminal_shell_close() -> Value {
    json!({
        "type": "object",
        "required": ["shell_id"],
        "properties": {
            "shell_id": {"type": "string"},
            "agent_id": {"type": "string", "default": "agent-mcp"},
            "run_id": {"type": "string", "default": "run-mcp"},
            "client_session_id": {"type": "string", "default": "client-mcp"},
            "reuse_policy": {
                "type": "string",
                "enum": ["always_new", "reuse_if_alive", "resume_or_create"],
                "default": "reuse_if_alive"
            }
        }
    })
}

fn schema_target_inspect_basic() -> Value {
    json!({
        "type": "object",
        "required": ["target"],
        "properties": {
            "target": {
                "type": "string",
                "description": "Target ID or alias from core config"
            },
            "agent_id": {"type": "string", "default": "agent-mcp"},
            "run_id": {"type": "string", "default": "run-mcp"},
            "client_session_id": {"type": "string", "default": "client-mcp"},
            "reuse_policy": {
                "type": "string",
                "enum": ["always_new", "reuse_if_alive", "resume_or_create"],
                "default": "reuse_if_alive"
            }
        }
    })
}

fn schema_capability_describe() -> Value {
    json!({
        "type": "object",
        "required": ["id"],
        "properties": {
            "id": {"type": "string", "description": "Capability id or tool name"},
            "kind": {"type": "string", "enum": ["auto", "tool", "capability"], "default": "auto"}
        }
    })
}

fn detail_for_tool(tool_name: &str) -> Option<Value> {
    let entry = mcp_tool_catalog()
        .iter()
        .find(|entry| entry.name == tool_name)?;
    Some(json!({
        "id": entry.name,
        "kind": "tool",
        "title": entry.title,
        "capability_id": entry.capability_id,
        "short_description": entry.short_description,
        "detailed_description": entry.detailed_description,
        "input_schema": (entry.input_schema)()
    }))
}

fn detail_for_capability(capability_id: &str) -> Option<Value> {
    let entry = mcp_capability_catalog()
        .iter()
        .find(|entry| entry.id == capability_id)?;
    Some(json!({
        "id": entry.id,
        "kind": "capability",
        "title": entry.title,
        "short_description": entry.short_description,
        "detailed_description": entry.detailed_description,
        "related_tools": entry.related_tools
    }))
}

fn describe_catalog_entry(query_id: &str, kind: &str) -> Option<Value> {
    let tool_aliases = tool_query_aliases(query_id);
    let capability_aliases = capability_query_aliases(query_id);
    match kind {
        "tool" => tool_aliases
            .iter()
            .find_map(|candidate| detail_for_tool(candidate)),
        "capability" => capability_aliases
            .iter()
            .find_map(|candidate| detail_for_capability(candidate)),
        _ => tool_aliases
            .iter()
            .find_map(|candidate| detail_for_tool(candidate))
            .or_else(|| {
                capability_aliases
                    .iter()
                    .find_map(|candidate| detail_for_capability(candidate))
            }),
    }
}

fn tool_query_aliases(query_id: &str) -> Vec<String> {
    let mut aliases = Vec::new();
    let raw = query_id.trim().to_ascii_lowercase();
    if raw.is_empty() {
        return aliases;
    }
    push_unique_alias(&mut aliases, raw.clone());

    let normalized = raw.replace('/', ".");
    push_unique_alias(&mut aliases, normalized.clone());

    let dotted = normalized.replace('_', ".");
    push_unique_alias(&mut aliases, dotted.clone());

    if let Some(tail) = dotted.strip_prefix("bridgingio.") {
        push_unique_alias(&mut aliases, format!("bridgingio_{tail}").replace('.', "_"));
    }
    if let Some(tail) = normalized.strip_prefix("bridgingio_") {
        push_unique_alias(
            &mut aliases,
            format!("bridgingio.{}", tail.replace('_', ".")),
        );
    }

    if !dotted.starts_with("bridgingio.") {
        push_unique_alias(&mut aliases, format!("bridgingio.{dotted}"));
    }
    if !normalized.starts_with("bridgingio_") {
        push_unique_alias(
            &mut aliases,
            format!("bridgingio_{}", normalized.replace('.', "_")),
        );
    }

    aliases
}

fn capability_query_aliases(query_id: &str) -> Vec<String> {
    let mut aliases = Vec::new();
    let raw = query_id.trim().to_ascii_lowercase();
    if raw.is_empty() {
        return aliases;
    }
    push_unique_alias(&mut aliases, raw.clone());
    push_unique_alias(&mut aliases, raw.replace('/', "."));
    push_unique_alias(&mut aliases, raw.replace('_', "."));
    if let Some(tail) = raw.strip_prefix("bridgingio.") {
        push_unique_alias(&mut aliases, tail.to_string());
    }
    if let Some(tail) = raw.strip_prefix("bridgingio_") {
        push_unique_alias(&mut aliases, tail.replace('_', "."));
    }
    aliases
}

fn push_unique_alias(aliases: &mut Vec<String>, candidate: String) {
    if candidate.is_empty() {
        return;
    }
    if !aliases.iter().any(|existing| existing == &candidate) {
        aliases.push(candidate);
    }
}

fn handle_mcp_resources_read(params: &Value) -> Result<Value, String> {
    let Some(params_map) = params.as_object() else {
        return Err(jsonrpc_error(
            Value::Null,
            -32602,
            "invalid params: expected object",
        ));
    };
    let uri = params_map
        .get("uri")
        .and_then(Value::as_str)
        .ok_or_else(|| jsonrpc_error(Value::Null, -32602, "invalid params: missing uri"))?;

    let detail = if let Some(tool_name) = uri.strip_prefix("bridgingio://tool/") {
        describe_catalog_entry(tool_name, "tool")
    } else if let Some(capability_id) = uri.strip_prefix("bridgingio://capability/") {
        describe_catalog_entry(capability_id, "capability")
    } else {
        None
    }
    .ok_or_else(|| jsonrpc_error(Value::Null, -32602, &format!("resource not found: {uri}")))?;

    Ok(json!({
        "contents": [
            {
                "uri": uri,
                "mimeType": "application/json",
                "text": detail.to_string()
            }
        ]
    }))
}

fn handle_mcp_tools_call(runtime: &SharedRuntime, params: &Value) -> Result<Value, String> {
    let Some(params_map) = params.as_object() else {
        return Err(jsonrpc_error(
            Value::Null,
            -32602,
            "invalid params: expected object",
        ));
    };
    let Some(tool_name) = params_map.get("name").and_then(Value::as_str) else {
        return Err(jsonrpc_error(
            Value::Null,
            -32602,
            "invalid params: missing tool name",
        ));
    };
    let arguments = params_map
        .get("arguments")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let context = tool_context_from_args(&arguments)
        .map_err(|message| jsonrpc_error(Value::Null, -32602, &message))?;
    let mut runtime = runtime
        .lock()
        .map_err(|_| jsonrpc_error(Value::Null, -32603, "runtime lock poisoned"))?;

    match tool_name {
        "bridgingio.terminal.exec" => {
            let target = required_json_string(&arguments, "target")
                .map_err(|message| jsonrpc_error(Value::Null, -32602, &message))?;
            let command = required_json_string(&arguments, "command")
                .map_err(|message| jsonrpc_error(Value::Null, -32602, &message))?;
            let outcome = runtime
                .execute_target_command(target, context, command, None)
                .map_err(|err| jsonrpc_error(Value::Null, -32603, &format!("{err:?}")))?;
            let structured = json!({
                "requested_target_ref": outcome.requested_target_ref,
                "resolved_target_id": outcome.resolved_target_id,
                "target_kind": outcome.target_kind,
                "artifact_id": outcome.artifact_id,
                "logical_session_id": outcome.logical_session_id,
                "channel_id": outcome.channel_id,
                "command": outcome.command,
                "connector_command": outcome.connector_command,
                "output": outcome.output
            });
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!(
                            "executed on target={} (resolved={}) artifact={} logical_session={} channel={}",
                            structured["requested_target_ref"].as_str().unwrap_or("unknown"),
                            structured["resolved_target_id"].as_str().unwrap_or("unknown"),
                            structured["artifact_id"].as_str().unwrap_or("unknown"),
                            structured["logical_session_id"].as_str().unwrap_or("unknown"),
                            structured["channel_id"].as_str().unwrap_or("unknown")
                        )
                    }
                ],
                "structuredContent": structured,
                "isError": false
            }))
        }
        "bridgingio.artifacts.read" => {
            let artifact_id = required_json_string(&arguments, "artifact_id")
                .map_err(|message| jsonrpc_error(Value::Null, -32602, &message))?;
            let offset = optional_json_u64(&arguments, "offset").unwrap_or(0) as usize;
            let limit = optional_json_u64(&arguments, "limit").unwrap_or(200) as usize;
            let result = runtime.tool_handler.handle(ToolRequest::ArtifactsRead {
                artifact_id: artifact_id.to_string(),
                offset,
                limit,
            });
            let view = match result {
                ToolResult::ArtifactRead { view } => view,
                ToolResult::Error { message } => {
                    return Err(jsonrpc_error(Value::Null, -32603, &message));
                }
                other => {
                    return Err(jsonrpc_error(
                        Value::Null,
                        -32603,
                        &format!("unexpected result from artifacts.read: {other:?}"),
                    ));
                }
            };
            let structured = json!({
                "artifact_id": view.record.id,
                "content_digest": view.record.content_digest,
                "logical_session_id": view.record.logical_session_id,
                "transport_session_id": view.record.transport_session_id,
                "channel_id": view.record.channel_id,
                "parent_id": view.record.parent_id,
                "source_command": view.record.source_command,
                "summary": view.record.summary,
                "byte_count": view.record.byte_count,
                "line_count": view.record.line_count,
                "offset": offset,
                "limit": limit,
                "returned_line_count": view.chunks.len(),
                "total_chunks": view.total_chunks,
                "chunks": view.chunks
            });
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!(
                            "artifact read completed artifact_id={} line_count={}",
                            structured["artifact_id"].as_str().unwrap_or("unknown"),
                            structured["returned_line_count"].as_u64().unwrap_or(0)
                        )
                    }
                ],
                "structuredContent": structured,
                "isError": false
            }))
        }
        "bridgingio.artifacts.refine" => {
            let source_artifact_id = required_json_string(&arguments, "source_artifact_id")
                .map_err(|message| jsonrpc_error(Value::Null, -32602, &message))?;
            let pattern = required_json_string(&arguments, "pattern")
                .map_err(|message| jsonrpc_error(Value::Null, -32602, &message))?;
            let derived_artifact_id = optional_json_string(&arguments, "derived_artifact_id")
                .unwrap_or_else(|| runtime.next_internal_artifact_hint());
            let mut mode = optional_json_string(&arguments, "mode")
                .unwrap_or_else(|| "auto".to_string())
                .to_ascii_lowercase();
            let mut ignore_case = optional_json_bool(&arguments, "ignore_case").unwrap_or(false);
            let processing_mode = optional_json_string(&arguments, "processing_mode")
                .unwrap_or_else(|| "auto".to_string())
                .to_ascii_lowercase();
            if let Some(flags) = optional_json_string(&arguments, "grep_flags") {
                apply_grep_flags_to_refine_options(flags.as_str(), &mut mode, &mut ignore_case)?;
            }
            let result = runtime.tool_handler.handle(ToolRequest::ArtifactsRefine {
                source_artifact_id: source_artifact_id.to_string(),
                pattern: pattern.to_string(),
                mode: mode.clone(),
                ignore_case,
                processing_mode: processing_mode.clone(),
            });
            let (artifact, requested_processing_mode, resolved_processing_mode) = match result {
                ToolResult::ArtifactRefined {
                    artifact,
                    requested_processing_mode,
                    resolved_processing_mode,
                } => (
                    artifact,
                    requested_processing_mode,
                    resolved_processing_mode,
                ),
                ToolResult::Error { message } => {
                    return Err(jsonrpc_error(Value::Null, -32603, &message));
                }
                other => {
                    return Err(jsonrpc_error(
                        Value::Null,
                        -32603,
                        &format!("unexpected result from artifacts.refine: {other:?}"),
                    ));
                }
            };
            let preview = match runtime.tool_handler.handle(ToolRequest::ArtifactsRead {
                artifact_id: artifact.id.clone(),
                offset: 0,
                limit: 200,
            }) {
                ToolResult::ArtifactRead { view } => view.chunks,
                _ => Vec::new(),
            };
            let structured = json!({
                "source_artifact_id": source_artifact_id,
                "artifact_id": artifact.id,
                "derived_artifact_id": derived_artifact_id,
                "content_digest": artifact.content_digest,
                "logical_session_id": artifact.logical_session_id,
                "channel_id": artifact.channel_id,
                "parent_id": artifact.parent_id,
                "source_command": artifact.source_command,
                "summary": artifact.summary,
                "byte_count": artifact.byte_count,
                "pattern": pattern,
                "mode": mode,
                "ignore_case": ignore_case,
                "requested_processing_mode": requested_processing_mode,
                "resolved_processing_mode": resolved_processing_mode,
                "line_count": preview.len(),
                "preview": preview
            });
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!(
                            "artifact refined source={} derived={} line_count={}",
                            structured["source_artifact_id"].as_str().unwrap_or("unknown"),
                            structured["artifact_id"].as_str().unwrap_or("unknown"),
                            structured["line_count"].as_u64().unwrap_or(0)
                        )
                    }
                ],
                "structuredContent": structured,
                "isError": false
            }))
        }
        "bridgingio.capability.describe" => {
            let query_id = required_json_string(&arguments, "id")
                .map_err(|message| jsonrpc_error(Value::Null, -32602, &message))?;
            let kind = optional_json_string(&arguments, "kind")
                .unwrap_or_else(|| "auto".to_string())
                .to_ascii_lowercase();
            if !matches!(kind.as_str(), "auto" | "tool" | "capability") {
                return Err(jsonrpc_error(
                    Value::Null,
                    -32602,
                    &format!("invalid kind for capability describe: {kind}"),
                ));
            }
            let detail = describe_catalog_entry(query_id, &kind).ok_or_else(|| {
                jsonrpc_error(
                    Value::Null,
                    -32602,
                    &format!("capability or tool not found: {query_id}"),
                )
            })?;
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!(
                            "detail loaded for {} ({})",
                            detail["id"].as_str().unwrap_or("unknown"),
                            detail["kind"].as_str().unwrap_or("unknown")
                        )
                    }
                ],
                "structuredContent": detail,
                "isError": false
            }))
        }
        "bridgingio.terminal.shell.open" => {
            let target = required_json_string(&arguments, "target")
                .map_err(|message| jsonrpc_error(Value::Null, -32602, &message))?;
            let handle = runtime
                .open_interactive_shell(target, context)
                .map_err(|err| jsonrpc_error(Value::Null, -32603, &format!("{err:?}")))?;
            let structured = json!({
                "mode": "interactive_shell",
                "shell_id": handle.shell_id,
                "requested_target_ref": handle.requested_target_ref,
                "resolved_target_id": handle.resolved_target_id,
                "target_kind": handle.target_kind,
                "logical_session_id": handle.logical_session_id,
                "channel_id": handle.channel_id,
                "prompt": handle.prompt,
                "cwd": handle.cwd
            });
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!(
                            "interactive shell opened shell_id={} target={} logical_session={} channel={}",
                            structured["shell_id"].as_str().unwrap_or("unknown"),
                            structured["resolved_target_id"].as_str().unwrap_or("unknown"),
                            structured["logical_session_id"].as_str().unwrap_or("unknown"),
                            structured["channel_id"].as_str().unwrap_or("unknown")
                        )
                    }
                ],
                "structuredContent": structured,
                "isError": false
            }))
        }
        "bridgingio.terminal.shell.write" => {
            let shell_id = required_json_string(&arguments, "shell_id")
                .map_err(|message| jsonrpc_error(Value::Null, -32602, &message))?;
            let input = required_json_string(&arguments, "input")
                .map_err(|message| jsonrpc_error(Value::Null, -32602, &message))?;
            let outcome = runtime
                .write_interactive_shell(shell_id, input, context)
                .map_err(|err| jsonrpc_error(Value::Null, -32603, &format!("{err:?}")))?;
            let structured = json!({
                "mode": "interactive_shell",
                "shell_id": outcome.shell_id,
                "channel_id": outcome.channel_id,
                "logical_session_id": outcome.logical_session_id,
                "artifact_id": outcome.artifact_id,
                "output": outcome.output,
                "prompt": outcome.prompt,
                "cwd": outcome.cwd,
                "running": outcome.running
            });
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!(
                            "interactive shell write completed shell_id={} artifact={} running={}",
                            structured["shell_id"].as_str().unwrap_or("unknown"),
                            structured["artifact_id"].as_str().unwrap_or("unknown"),
                            structured["running"].as_bool().unwrap_or(false)
                        )
                    }
                ],
                "structuredContent": structured,
                "isError": false
            }))
        }
        "bridgingio.terminal.shell.read" => {
            let shell_id = required_json_string(&arguments, "shell_id")
                .map_err(|message| jsonrpc_error(Value::Null, -32602, &message))?;
            let offset = optional_json_u64(&arguments, "offset").unwrap_or(0) as usize;
            let limit = optional_json_u64(&arguments, "limit").unwrap_or(100) as usize;
            let outcome = runtime
                .read_interactive_shell(shell_id, offset, limit, context)
                .map_err(|err| jsonrpc_error(Value::Null, -32603, &format!("{err:?}")))?;
            let structured = json!({
                "mode": "interactive_shell",
                "shell_id": outcome.shell_id,
                "channel_id": outcome.channel_id,
                "logical_session_id": outcome.logical_session_id,
                "prompt": outcome.prompt,
                "cwd": outcome.cwd,
                "interrupted": outcome.interrupted,
                "closed": outcome.closed,
                "running": outcome.running,
                "lines": outcome.lines
            });
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!(
                            "interactive transcript lines={} shell_id={} running={}",
                            structured["lines"].as_array().map(|v| v.len()).unwrap_or(0),
                            structured["shell_id"].as_str().unwrap_or("unknown"),
                            structured["running"].as_bool().unwrap_or(false)
                        )
                    }
                ],
                "structuredContent": structured,
                "isError": false
            }))
        }
        "bridgingio.terminal.shell.interrupt" => {
            let shell_id = required_json_string(&arguments, "shell_id")
                .map_err(|message| jsonrpc_error(Value::Null, -32602, &message))?;
            let outcome = runtime
                .interrupt_interactive_shell(shell_id, context)
                .map_err(|err| jsonrpc_error(Value::Null, -32603, &format!("{err:?}")))?;
            let structured = json!({
                "mode": "interactive_shell",
                "shell_id": outcome.shell_id,
                "channel_id": outcome.channel_id,
                "logical_session_id": outcome.logical_session_id,
                "prompt": outcome.prompt,
                "cwd": outcome.cwd,
                "interrupted": outcome.interrupted,
                "closed": outcome.closed,
                "running": outcome.running
            });
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!(
                            "interactive shell interrupt requested shell_id={}",
                            structured["shell_id"].as_str().unwrap_or("unknown")
                        )
                    }
                ],
                "structuredContent": structured,
                "isError": false
            }))
        }
        "bridgingio.terminal.shell.close" => {
            let shell_id = required_json_string(&arguments, "shell_id")
                .map_err(|message| jsonrpc_error(Value::Null, -32602, &message))?;
            let outcome = runtime
                .close_interactive_shell(shell_id, context)
                .map_err(|err| jsonrpc_error(Value::Null, -32603, &format!("{err:?}")))?;
            let structured = json!({
                "mode": "interactive_shell",
                "shell_id": outcome.shell_id,
                "channel_id": outcome.channel_id,
                "logical_session_id": outcome.logical_session_id,
                "prompt": outcome.prompt,
                "cwd": outcome.cwd,
                "interrupted": outcome.interrupted,
                "closed": outcome.closed,
                "running": outcome.running
            });
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!(
                            "interactive shell closed shell_id={}",
                            structured["shell_id"].as_str().unwrap_or("unknown")
                        )
                    }
                ],
                "structuredContent": structured,
                "isError": false
            }))
        }
        "bridgingio.target.inspect_basic" => {
            let target = required_json_string(&arguments, "target")
                .map_err(|message| jsonrpc_error(Value::Null, -32602, &message))?;
            let result = runtime
                .inspect_target_basic(target, context)
                .map_err(|err| jsonrpc_error(Value::Null, -32603, &format!("{err:?}")))?;
            let structured = json!({
                "requested_target_ref": result.requested_target_ref,
                "resolved_target_id": result.resolved_target_id,
                "target_kind": result.target_kind,
                "kernel_version": result.kernel_version,
                "username": result.username,
                "logical_session_id": result.logical_session_id,
                "artifacts": result.artifacts
            });
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!(
                            "target={} kernel_version={} username={}",
                            structured["resolved_target_id"].as_str().unwrap_or("unknown"),
                            structured["kernel_version"].as_str().unwrap_or("unknown"),
                            structured["username"].as_str().unwrap_or("unknown")
                        )
                    }
                ],
                "structuredContent": structured,
                "isError": false
            }))
        }
        _ => Err(jsonrpc_error(
            Value::Null,
            -32601,
            &format!("tool not found: {tool_name}"),
        )),
    }
}

fn required_json_string<'a>(
    args: &'a serde_json::Map<String, Value>,
    key: &str,
) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing required string argument: {key}"))
}

fn optional_json_string(args: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn optional_json_u64(args: &serde_json::Map<String, Value>, key: &str) -> Option<u64> {
    args.get(key).and_then(Value::as_u64)
}

fn optional_json_bool(args: &serde_json::Map<String, Value>, key: &str) -> Option<bool> {
    args.get(key).and_then(Value::as_bool)
}

fn parse_artifact_refine_mode(mode_label: &str) -> Result<ArtifactRefineMode, String> {
    match mode_label {
        "keyword" => Ok(ArtifactRefineMode::Keyword),
        "regex" => Ok(ArtifactRefineMode::Regex),
        "auto" => Ok(ArtifactRefineMode::Auto),
        other => Err(format!("invalid artifact refine mode: {other}")),
    }
}

fn apply_grep_flags_to_refine_options(
    flags: &str,
    mode: &mut String,
    ignore_case: &mut bool,
) -> Result<(), String> {
    for ch in flags.chars() {
        match ch {
            'E' | 'e' => *mode = "regex".to_string(),
            'i' | 'I' => *ignore_case = true,
            '-' | ' ' => {}
            _ => {
                return Err(jsonrpc_error(
                    Value::Null,
                    -32602,
                    &format!("unsupported grep_flags option: {ch}"),
                ));
            }
        }
    }
    Ok(())
}

fn tool_context_from_args(
    args: &serde_json::Map<String, Value>,
) -> Result<ToolRequestContext, String> {
    let reuse_policy_label =
        optional_json_string(args, "reuse_policy").unwrap_or_else(|| "reuse_if_alive".to_string());
    let reuse_policy = parse_reuse_policy(&reuse_policy_label)
        .map_err(|_| format!("invalid reuse_policy: {reuse_policy_label}"))?;
    Ok(ToolRequestContext {
        agent_id: optional_json_string(args, "agent_id").unwrap_or_else(|| "agent-mcp".to_string()),
        run_id: optional_json_string(args, "run_id").unwrap_or_else(|| "run-mcp".to_string()),
        client_session_id: optional_json_string(args, "client_session_id")
            .unwrap_or_else(|| "client-mcp".to_string()),
        reuse_policy,
    })
}

fn jsonrpc_success(id: Value, result: Value) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
    .to_string()
}

fn jsonrpc_error(id: Value, code: i64, message: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
    .to_string()
}

fn normalize_jsonrpc_error_id(error_body: String, request_id: &Value) -> String {
    let parsed = serde_json::from_str::<Value>(&error_body);
    let Ok(mut payload) = parsed else {
        return error_body;
    };
    let Some(object) = payload.as_object_mut() else {
        return error_body;
    };
    if !object.contains_key("error") {
        return error_body;
    }
    object.insert("id".to_string(), request_id.clone());
    payload.to_string()
}

fn trace_mcp(message: impl AsRef<str>) {
    if !mcp_trace_enabled() {
        return;
    }
    eprintln!("[bridgingio-mcp] {}", message.as_ref());
}

fn mcp_trace_enabled() -> bool {
    match std::env::var("BRIDGINGIO_MCP_TRACE") {
        Ok(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        }
        Err(_) => false,
    }
}

fn jsonrpc_id_brief(id: &Value) -> String {
    if id.is_null() {
        return "null".to_string();
    }
    if let Some(number) = id.as_i64() {
        return number.to_string();
    }
    if let Some(number) = id.as_u64() {
        return number.to_string();
    }
    if let Some(text) = id.as_str() {
        return text.to_string();
    }
    id.to_string()
}

fn read_http_request(stream: &mut TcpStream) -> Result<(String, String, String), CoreRuntimeError> {
    let mut buffer = Vec::<u8>::new();
    let mut header_end = None;
    loop {
        let mut chunk = [0u8; 1024];
        let read = stream
            .read(&mut chunk)
            .map_err(|err| CoreRuntimeError::Io(format!("read http request: {err}")))?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(pos) = find_subslice(&buffer, b"\r\n\r\n") {
            header_end = Some(pos + 4);
            break;
        }
    }
    let header_end =
        header_end.ok_or_else(|| CoreRuntimeError::Io("invalid http request".into()))?;
    let header_text = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let mut lines = header_text.lines();
    let request_line = lines
        .next()
        .ok_or_else(|| CoreRuntimeError::Io("missing request line".into()))?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts
        .next()
        .ok_or_else(|| CoreRuntimeError::Io("missing method".into()))?
        .to_string();
    let path = request_parts
        .next()
        .ok_or_else(|| CoreRuntimeError::Io("missing path".into()))?
        .to_string();

    let mut content_length = 0usize;
    for line in lines {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("content-length:") {
            let (_, value) = line
                .split_once(':')
                .ok_or_else(|| CoreRuntimeError::Io("invalid content-length".into()))?;
            content_length = value.trim().parse::<usize>().unwrap_or(0);
        }
    }

    let mut body_bytes = buffer[header_end..].to_vec();
    while body_bytes.len() < content_length {
        let mut chunk = vec![0u8; content_length - body_bytes.len()];
        let read = stream
            .read(&mut chunk)
            .map_err(|err| CoreRuntimeError::Io(format!("read http body: {err}")))?;
        if read == 0 {
            break;
        }
        body_bytes.extend_from_slice(&chunk[..read]);
    }
    let body = String::from_utf8_lossy(&body_bytes).to_string();
    Ok((method, path, body))
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn write_http_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &str,
) -> Result<(), CoreRuntimeError> {
    let status_text = match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        503 => "Service Unavailable",
        _ => "Internal Server Error",
    };
    let response = format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream
        .write_all(response.as_bytes())
        .map_err(|err| CoreRuntimeError::Io(format!("write http response: {err}")))
}

fn parse_kv_body(body: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();
    for token in body.split('|') {
        if let Some((k, v)) = token.split_once('=') {
            params.insert(k.to_string(), v.to_string());
        }
    }
    params
}

fn required_param<'a>(
    map: &'a HashMap<String, String>,
    key: &str,
) -> Result<&'a str, CoreRuntimeError> {
    map.get(key)
        .map(String::as_str)
        .ok_or_else(|| CoreRuntimeError::Config(format!("missing param {key}")))
}

fn optional_param<'a>(map: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    map.get(key).map(String::as_str)
}

fn parse_reuse_policy(value: &str) -> Result<SessionReusePolicy, CoreRuntimeError> {
    match value {
        "always_new" => Ok(SessionReusePolicy::AlwaysNew),
        "reuse_if_alive" => Ok(SessionReusePolicy::ReuseIfAlive),
        "resume_or_create" => Ok(SessionReusePolicy::ResumeOrCreate),
        _ => Err(CoreRuntimeError::Config("invalid reuse_policy".into())),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use bridgingio_app_api::{ApiRequest, ApiRequestContext, ApiResponse, AppCommand};
    use bridgingio_connectors::{BuiltInBinarySpec, BuiltInDistributionKind, ExecutableResolver};
    use bridgingio_domain::{ConnectionConfig, PolicyProfile, TargetKind, TargetProfile};

    use super::{
        CapabilityDiscovery, ControlPlaneIpcClient, ControlPlaneIpcServer, CoreRuntimeError,
        DefaultCapabilityDiscovery, McpToolHandler, ModelPlaneHttpServer, StandaloneCoreRuntime,
        ToolRequest, ToolRequestContext, ToolResult,
    };

    fn context(
        agent: &str,
        run: &str,
        client: &str,
        reuse_policy: bridgingio_domain::SessionReusePolicy,
    ) -> ToolRequestContext {
        ToolRequestContext {
            agent_id: agent.into(),
            run_id: run.into(),
            client_session_id: client.into(),
            reuse_policy,
        }
    }

    fn temp_dir(prefix: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = PathBuf::from("/tmp").join(format!("bridgingio-mcp-{prefix}-{stamp}"));
        fs::create_dir_all(&root).expect("create temp dir");
        root
    }

    fn request_context() -> ApiRequestContext {
        ApiRequestContext {
            agent_id: "ui-agent".into(),
            run_id: "ui-run".into(),
            client_session_id: "ui-client".into(),
            reuse_policy: bridgingio_domain::SessionReusePolicy::ReuseIfAlive,
        }
    }

    #[test]
    fn returns_structured_capabilities_for_ssh_target() {
        let target = bridgingio_domain::TargetProfile {
            id: "target-ssh".into(),
            name: "test".into(),
            kind: bridgingio_domain::TargetKind::Ssh,
            connection: ConnectionConfig::Ssh {
                host: "127.0.0.1".into(),
                port: 22,
                username: "dev".into(),
            },
            credential_ref: None,
            default_policy: PolicyProfile::default(),
            notes: None,
            metadata: Default::default(),
            toolchains: Default::default(),
        };

        let discovery = DefaultCapabilityDiscovery;
        let envelope = discovery.describe_target(&target, None);
        assert!(!envelope.capabilities.is_empty());
        assert!(envelope
            .capabilities
            .iter()
            .any(|c| c.id == "terminal.exec"));
        assert!(envelope
            .capabilities
            .iter()
            .any(|c| c.id == "artifact.reanalysis"));
    }

    #[test]
    fn raw_exec_returns_approval_required_for_sensitive_operation() {
        let mut handler = McpToolHandler::default();
        let result = handler.handle(ToolRequest::TerminalExecRaw {
            target_id: "target-1".into(),
            target_kind: bridgingio_domain::TargetKind::Ssh,
            context: context(
                "agent-1",
                "run-1",
                "client-1",
                bridgingio_domain::SessionReusePolicy::ReuseIfAlive,
            ),
            command: "rm -rf /tmp/demo".into(),
            artifact_id: "artifact-1".into(),
            operation: bridgingio_policy::OperationKind::Delete,
        });
        assert!(matches!(result, ToolResult::ApprovalRequired { .. }));
    }

    #[test]
    fn standalone_runtime_loads_minimal_config() {
        let config = bridgingio_engine::CoreSettings::from_toml_str(
            &bridgingio_engine::CoreSettings::minimal_example().replace("port = 19718", "port = 0"),
        )
        .expect("parse");
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(Vec::new()),
            "/tmp",
            vec![BuiltInBinarySpec {
                command: "ssh".into(),
                relative_path: PathBuf::from("bin/ssh"),
                distribution: BuiltInDistributionKind::StandalonePackage,
            }],
        );
        let runtime = StandaloneCoreRuntime::from_settings(config, resolver).expect("runtime");
        assert_eq!(runtime.settings_store.settings.schema_version, 1);
    }

    #[cfg(unix)]
    #[test]
    fn ipc_and_http_can_bind_from_same_runtime() {
        let root = PathBuf::from("/tmp/bridgingio-mcp-ipc-http-bind");
        fs::create_dir_all(&root).expect("create root");
        let config = bridgingio_engine::CoreSettings::from_toml_str(
            &bridgingio_engine::CoreSettings::minimal_example().replace("port = 19718", "port = 0"),
        )
        .expect("parse");
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(Vec::new()),
            &root,
            Vec::new(),
        );
        let runtime = StandaloneCoreRuntime::from_settings(config.clone(), resolver)
            .expect("runtime")
            .shared();

        let socket = root.join("control-plane.sock");
        let ipc = match ControlPlaneIpcServer::bind(runtime.clone(), &socket) {
            Ok(ipc) => ipc,
            Err(CoreRuntimeError::Io(message)) if message.contains("Operation not permitted") => {
                return;
            }
            Err(err) => panic!("ipc: {err:?}"),
        };
        let _http = ModelPlaneHttpServer::bind(runtime, &config).expect("http");
        let _client = ControlPlaneIpcClient::new(ipc.socket_path());
    }

    #[test]
    fn update_settings_persists_and_returns_restart_required() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = PathBuf::from("/tmp").join(format!("bridgingio-update-settings-{stamp}"));
        fs::create_dir_all(&root).expect("create root");
        let config_path = root.join("managed-core.toml");
        fs::write(&config_path, bridgingio_engine::CoreSettings::minimal_example()).expect("write config");

        let settings = bridgingio_engine::CoreSettings::load_from_file(&config_path).expect("load");
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(Vec::new()),
            &root,
            Vec::new(),
        );
        let mut runtime = StandaloneCoreRuntime::from_settings_with_mode(
            settings,
            resolver,
            super::CoreHostMode::UiManagedEphemeral,
        )
        .expect("runtime");
        runtime.settings_store.runtime_metadata.config_path =
            Some(config_path.to_string_lossy().to_string());

        let response = runtime.handle_app_request(ApiRequest {
            request_id: "req-settings".into(),
            context: ApiRequestContext {
                agent_id: "ui-agent".into(),
                run_id: "ui-run".into(),
                client_session_id: "ui-client".into(),
                reuse_policy: bridgingio_domain::SessionReusePolicy::ReuseIfAlive,
            },
            command: AppCommand::UpdateSettings {
                model_plane_host: Some("127.0.0.1".into()),
                model_plane_port: Some(19719),
                artifact_cache_backend: None,
                artifact_cache_root: None,
                artifact_cache_max_bytes: None,
                artifact_cache_eviction_policy: None,
                tool_override_command: None,
                tool_override_path: None,
            },
        });

        match response {
            ApiResponse::Accepted { apply_strategy, .. } => {
                assert_eq!(apply_strategy.as_deref(), Some("restart_required"));
            }
            other => panic!("expected accepted response, got {other:?}"),
        }

        let persisted = fs::read_to_string(&config_path).expect("read persisted");
        assert!(persisted.contains("port = 19719"), "persisted config: {persisted}");
        let reloaded = bridgingio_engine::CoreSettings::load_from_file(&config_path).expect("reload");
        assert_eq!(reloaded.model_plane.http.port, 19719);
    }

    #[test]
    fn rejects_non_loopback_without_protection() {
        let mut text = bridgingio_engine::CoreSettings::minimal_example().to_string();
        text = text.replace("host = \"127.0.0.1\"", "host = \"0.0.0.0\"");
        let err = bridgingio_engine::CoreSettings::from_toml_str(&text).expect_err("must reject");
        assert!(matches!(
            err,
            bridgingio_engine::ConfigError::NonLoopbackExplicitEnableRequired(_)
        ));
    }

    #[test]
    fn shell_single_quote_is_platform_compatible() {
        let value = "hello world";
        #[cfg(windows)]
        assert_eq!(super::shell_single_quote(value), "hello world");
        #[cfg(not(windows))]
        assert_eq!(super::shell_single_quote(value), "'hello world'");
    }

    #[test]
    fn connector_command_uses_resolved_adb_executable_path() {
        let target = TargetProfile {
            id: "adb-target".into(),
            name: "adb-target".into(),
            kind: TargetKind::Adb,
            connection: ConnectionConfig::Adb {
                serial: Some("emulator-5554".into()),
                transport: Some("usb".into()),
            },
            credential_ref: None,
            default_policy: PolicyProfile::default(),
            notes: None,
            metadata: BTreeMap::new(),
            toolchains: BTreeMap::new(),
        };
        let command = super::build_connector_command(
            &target,
            "uname -r",
            Some("/opt/homebrew/bin/adb"),
        );
        #[cfg(windows)]
        assert_eq!(command, "/opt/homebrew/bin/adb -s emulator-5554 shell uname -r");
        #[cfg(not(windows))]
        assert_eq!(
            command,
            "'/opt/homebrew/bin/adb' -s emulator-5554 shell 'uname -r'"
        );
    }

    #[test]
    fn connector_command_prefers_serial_selector_over_usb_transport() {
        let target = TargetProfile {
            id: "adb-target".into(),
            name: "adb-target".into(),
            kind: TargetKind::Adb,
            connection: ConnectionConfig::Adb {
                serial: Some("emulator-5554".into()),
                transport: Some("usb".into()),
            },
            credential_ref: None,
            default_policy: PolicyProfile::default(),
            notes: None,
            metadata: BTreeMap::new(),
            toolchains: BTreeMap::new(),
        };
        let command = super::build_connector_command(&target, "uname -r", None);
        #[cfg(windows)]
        assert_eq!(command, "adb -s emulator-5554 shell uname -r");
        #[cfg(not(windows))]
        assert_eq!(command, "adb -s emulator-5554 shell 'uname -r'");
    }

    #[test]
    fn connector_command_uses_resolved_ssh_executable_path() {
        let target = TargetProfile {
            id: "ssh-target".into(),
            name: "ssh-target".into(),
            kind: TargetKind::Ssh,
            connection: ConnectionConfig::Ssh {
                host: "10.1.1.8".into(),
                port: 2222,
                username: "root".into(),
            },
            credential_ref: None,
            default_policy: PolicyProfile::default(),
            notes: None,
            metadata: BTreeMap::new(),
            toolchains: BTreeMap::new(),
        };
        let command = super::build_connector_command(
            &target,
            "uname -r",
            Some("/opt/homebrew/bin/ssh"),
        );
        #[cfg(windows)]
        assert_eq!(command, "/opt/homebrew/bin/ssh -p 2222 root@10.1.1.8 uname -r");
        #[cfg(not(windows))]
        assert_eq!(
            command,
            "'/opt/homebrew/bin/ssh' -p 2222 'root@10.1.1.8' 'uname -r'"
        );
    }

    #[test]
    fn connector_command_builds_ssh_command_from_target_connection() {
        let target = TargetProfile {
            id: "ssh-target".into(),
            name: "ssh-target".into(),
            kind: TargetKind::Ssh,
            connection: ConnectionConfig::Ssh {
                host: "192.168.56.2".into(),
                port: 22,
                username: "ubuntu".into(),
            },
            credential_ref: None,
            default_policy: PolicyProfile::default(),
            notes: None,
            metadata: BTreeMap::new(),
            toolchains: BTreeMap::new(),
        };
        let command = super::build_connector_command(&target, "whoami", None);
        #[cfg(windows)]
        assert_eq!(command, "ssh -p 22 ubuntu@192.168.56.2 whoami");
        #[cfg(not(windows))]
        assert_eq!(command, "ssh -p 22 'ubuntu@192.168.56.2' 'whoami'");
    }

    #[test]
    fn connector_command_keeps_explicit_ssh_invocation_unchanged() {
        let target = TargetProfile {
            id: "ssh-target".into(),
            name: "ssh-target".into(),
            kind: TargetKind::Ssh,
            connection: ConnectionConfig::Ssh {
                host: "192.168.56.2".into(),
                port: 22,
                username: "ubuntu".into(),
            },
            credential_ref: None,
            default_policy: PolicyProfile::default(),
            notes: None,
            metadata: BTreeMap::new(),
            toolchains: BTreeMap::new(),
        };
        let raw = "ssh -p 22 ubuntu@192.168.56.2 whoami";
        let command = super::build_connector_command(&target, raw, Some("/opt/homebrew/bin/ssh"));
        assert_eq!(command, raw);
    }

    #[test]
    fn interactive_connector_command_uses_resolved_adb_path_and_selector() {
        let target = TargetProfile {
            id: "adb-target".into(),
            name: "adb-target".into(),
            kind: TargetKind::Adb,
            connection: ConnectionConfig::Adb {
                serial: Some("emulator-5554".into()),
                transport: Some("usb".into()),
            },
            credential_ref: None,
            default_policy: PolicyProfile::default(),
            notes: None,
            metadata: BTreeMap::new(),
            toolchains: BTreeMap::new(),
        };
        let command = super::build_interactive_connector_command(
            &target,
            Some("/opt/homebrew/bin/adb"),
        );
        #[cfg(windows)]
        assert_eq!(
            command.as_deref(),
            Some("/opt/homebrew/bin/adb -s emulator-5554 shell")
        );
        #[cfg(not(windows))]
        assert_eq!(
            command.as_deref(),
            Some("'/opt/homebrew/bin/adb' -s emulator-5554 shell")
        );
    }

    #[test]
    fn interactive_connector_command_uses_resolved_ssh_path() {
        let target = TargetProfile {
            id: "ssh-target".into(),
            name: "ssh-target".into(),
            kind: TargetKind::Ssh,
            connection: ConnectionConfig::Ssh {
                host: "10.1.1.8".into(),
                port: 2222,
                username: "root".into(),
            },
            credential_ref: None,
            default_policy: PolicyProfile::default(),
            notes: None,
            metadata: BTreeMap::new(),
            toolchains: BTreeMap::new(),
        };
        let command = super::build_interactive_connector_command(
            &target,
            Some("/opt/homebrew/bin/ssh"),
        );
        #[cfg(windows)]
        assert_eq!(
            command.as_deref(),
            Some("/opt/homebrew/bin/ssh -p 2222 root@10.1.1.8")
        );
        #[cfg(not(windows))]
        assert_eq!(
            command.as_deref(),
            Some("'/opt/homebrew/bin/ssh' -p 2222 'root@10.1.1.8'")
        );
    }

    #[test]
    fn toolchain_resolution_prefers_target_then_global_then_system_path() {
        let root = temp_dir("toolchain-precedence");
        let target_adb = root.join("target-adb");
        let global_adb = root.join("global-adb");
        let system_dir = root.join("system");
        let system_adb = system_dir.join("adb");
        let bundled_root = root.join("bundled");
        fs::create_dir_all(&system_dir).expect("create system dir");
        fs::create_dir_all(&bundled_root).expect("create bundled dir");
        fs::write(&target_adb, "binary").expect("write target adb");
        fs::write(&global_adb, "binary").expect("write global adb");
        fs::write(&system_adb, "binary").expect("write system adb");
        fs::write(bundled_root.join("adb"), "binary").expect("write bundled adb");

        let mut text = bridgingio_engine::CoreSettings::complete_example().to_string();
        text = text.replace("/opt/homebrew/bin/adb", &global_adb.to_string_lossy());
        text = text.replace(
            "/Applications/AndroidStudio.app/Contents/sdk/platform-tools/adb",
            &target_adb.to_string_lossy(),
        );
        let config_path = root.join("managed-core.toml");
        fs::write(&config_path, text).expect("write config");
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(vec![system_dir.clone()]),
            &bundled_root,
            vec![BuiltInBinarySpec {
                command: "adb".into(),
                relative_path: PathBuf::from("adb"),
                distribution: BuiltInDistributionKind::StandalonePackage,
            }],
        );
        let mut runtime = StandaloneCoreRuntime::from_config_file(&config_path, resolver)
            .expect("runtime from config");

        let diagnostics = runtime.handle_app_request(ApiRequest {
            request_id: "diag-1".into(),
            context: request_context(),
            command: AppCommand::GetToolchainDiagnostics,
        });
        let adb_diag = match diagnostics {
            ApiResponse::Diagnostics { items, .. } => items
                .into_iter()
                .find(|item| {
                    item.target_id.as_deref() == Some("android-emulator")
                        && item.command == "adb"
                })
                .expect("adb diagnostics for android-emulator"),
            other => panic!("unexpected diagnostics response: {other:?}"),
        };
        assert_eq!(adb_diag.effective_scope.as_deref(), Some("target_override"));
        assert_eq!(
            adb_diag.effective_path.as_deref(),
            Some(target_adb.to_string_lossy().as_ref())
        );

        let mut cleared_toolchains = BTreeMap::new();
        cleared_toolchains.insert("adb".to_string(), String::new());
        let response = runtime.handle_app_request(ApiRequest {
            request_id: "upsert-clear-target".into(),
            context: request_context(),
            command: AppCommand::UpsertProfile {
                profile: TargetProfile {
                    id: "android-emulator".into(),
                    name: "Android Emulator".into(),
                    kind: TargetKind::Adb,
                    connection: ConnectionConfig::Adb {
                        serial: Some("emulator-5554".into()),
                        transport: Some("serial".into()),
                    },
                    credential_ref: Some(bridgingio_domain::CredentialRef {
                        id: "vault://bridgingio/adb/default".into(),
                        provider: "vault".into(),
                    }),
                    default_policy: PolicyProfile::default(),
                    notes: None,
                    metadata: BTreeMap::new(),
                    toolchains: cleared_toolchains,
                },
            },
        });
        match response {
            ApiResponse::Accepted { apply_strategy, .. } => {
                assert_eq!(apply_strategy.as_deref(), Some("live_applied"));
            }
            other => panic!("unexpected upsert response: {other:?}"),
        }

        let diagnostics = runtime.handle_app_request(ApiRequest {
            request_id: "diag-2".into(),
            context: request_context(),
            command: AppCommand::GetToolchainDiagnostics,
        });
        let adb_diag = match diagnostics {
            ApiResponse::Diagnostics { items, .. } => items
                .into_iter()
                .find(|item| {
                    item.target_id.as_deref() == Some("android-emulator")
                        && item.command == "adb"
                })
                .expect("adb diagnostics after clearing target override"),
            other => panic!("unexpected diagnostics response: {other:?}"),
        };
        assert_eq!(adb_diag.effective_scope.as_deref(), Some("global_override"));
        assert_eq!(
            adb_diag.effective_path.as_deref(),
            Some(global_adb.to_string_lossy().as_ref())
        );

        let response = runtime.handle_app_request(ApiRequest {
            request_id: "update-global-clear".into(),
            context: request_context(),
            command: AppCommand::UpdateSettings {
                model_plane_host: None,
                model_plane_port: None,
                artifact_cache_backend: None,
                artifact_cache_root: None,
                artifact_cache_max_bytes: None,
                artifact_cache_eviction_policy: None,
                tool_override_command: Some("adb".into()),
                tool_override_path: Some(String::new()),
            },
        });
        match response {
            ApiResponse::Accepted { apply_strategy, .. } => {
                assert_eq!(apply_strategy.as_deref(), Some("live_applied"));
            }
            other => panic!("unexpected update settings response: {other:?}"),
        }

        let diagnostics = runtime.handle_app_request(ApiRequest {
            request_id: "diag-3".into(),
            context: request_context(),
            command: AppCommand::GetToolchainDiagnostics,
        });
        let adb_diag = match diagnostics {
            ApiResponse::Diagnostics { items, .. } => items
                .into_iter()
                .find(|item| {
                    item.target_id.as_deref() == Some("android-emulator")
                        && item.command == "adb"
                })
                .expect("adb diagnostics after clearing global override"),
            other => panic!("unexpected diagnostics response: {other:?}"),
        };
        assert_eq!(adb_diag.effective_scope.as_deref(), Some("system_path"));
        assert_eq!(
            adb_diag.effective_path.as_deref(),
            Some(system_adb.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn toolchain_resolution_falls_back_to_builtin_when_path_missing() {
        let root = temp_dir("toolchain-builtin-fallback");
        let system_dir = root.join("system-empty");
        let bundled_root = root.join("bundled");
        fs::create_dir_all(&system_dir).expect("create empty system dir");
        fs::create_dir_all(&bundled_root).expect("create bundled dir");
        let bundled_adb = bundled_root.join("adb");
        fs::write(&bundled_adb, "binary").expect("write bundled adb");

        let mut text = bridgingio_engine::CoreSettings::complete_example().to_string();
        text = text.replace("/opt/homebrew/bin/adb", "");
        text = text.replace(
            "/Applications/AndroidStudio.app/Contents/sdk/platform-tools/adb",
            "",
        );
        let config_path = root.join("managed-core.toml");
        fs::write(&config_path, text).expect("write config");
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(vec![system_dir]),
            &bundled_root,
            vec![BuiltInBinarySpec {
                command: "adb".into(),
                relative_path: PathBuf::from("adb"),
                distribution: BuiltInDistributionKind::StandalonePackage,
            }],
        );
        let mut runtime = StandaloneCoreRuntime::from_config_file(&config_path, resolver)
            .expect("runtime from config");

        let diagnostics = runtime.handle_app_request(ApiRequest {
            request_id: "diag-builtin".into(),
            context: request_context(),
            command: AppCommand::GetToolchainDiagnostics,
        });
        let adb_diag = match diagnostics {
            ApiResponse::Diagnostics { items, .. } => items
                .into_iter()
                .find(|item| {
                    item.target_id.as_deref() == Some("android-emulator")
                        && item.command == "adb"
                })
                .expect("adb diagnostics for android-emulator"),
            other => panic!("unexpected diagnostics response: {other:?}"),
        };
        assert_eq!(adb_diag.effective_scope.as_deref(), Some("builtin_fallback"));
        assert_eq!(
            adb_diag.effective_path.as_deref(),
            Some(bundled_adb.to_string_lossy().as_ref())
        );
    }
}
