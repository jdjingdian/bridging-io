use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use bridgingio_app_api::{
    AgentTokenScopeView, AgentTokenSummaryView, ApiError, ApiErrorCode, ApiRequest, ApiResponse,
    AppApiLineCodec, AppCommand, ArtifactCacheSettingsView, ArtifactReadView, ControlPlaneView,
    CoreSettingsView, CreateAgentTokenResult as ApiCreateAgentTokenResult, ModelPlaneHttpView,
    LocalAdminActionIntentView, LocalAdminAttestationView,
    RuntimeLogSettingsView, TimelineEntry, TimelineSourceGroupSummaryView, ToolchainDiagnosticView,
    VaultProtectorSummaryView, VaultSecretSummaryView, VaultStateProjectionView, VaultStatusView,
    VaultUnlockPolicySummaryView,
};
use bridgingio_artifacts::{
    ArtifactCacheBackend, ArtifactEvictionPolicy, ArtifactReadResult, ArtifactRefineMode,
    ArtifactStore, ArtifactStoreConfig,
};
use bridgingio_connectors::{
    target_shell_dialect_for, terminal_concurrency_policy_for, terminal_target_family_for,
    AdbConnector, CommandInvocation, ExecutableSource, InvocationKind, InvocationResolution,
    SshConnector, TerminalConnector, ToolchainResolver, TARGET_TERMINAL_CONCURRENCY_METADATA_KEY,
    TARGET_TERMINAL_FAMILY_METADATA_KEY, TARGET_TERMINAL_SHELL_METADATA_KEY,
};
use bridgingio_domain::{
    AccessScope, ArtifactRecord, CapabilitySummary, ChannelKind, ChannelStatus, CommonErrorCode,
    ConnectionConfig, ContractStatus, CredentialRef, ErrorDomain, SessionRecord,
    SessionReusePolicy, SessionState, SharedError, TargetKind, TargetProfile,
};
use bridgingio_engine::{
    CoreSettings, CoreSettingsStore, StandaloneConnectionSection, StandaloneTargetProfile,
    StandaloneTerminalSection, ToolchainSection,
};
use bridgingio_platform::{
    detect_host_platform_adapter, CapabilityStatus, HostPlatformAdapter, RuntimeLogCategory,
    RuntimeLogLevel,
};
use bridgingio_policy::{evaluate, OperationKind, PolicyDecision};
use bridgingio_providers::{GitProvider, TerminalProvider};
use bridgingio_secrets::{
    normalize_credential_ref, AgentTokenSummary, CreateAgentTokenRequest, LocalAdminActionIntent,
    LocalAdminActionKind, LocalAdminAttestationRecord, SecretVaultRouter,
    SshAgentBrokerPrepareRequest, SshHostKeyPolicy, SshKeyPassphraseHandling, TokenScopeInput,
    UnlockVaultRequest, UpdateAgentTokenScopeRequest, VaultError, VaultLockState,
    VaultReadinessState, VaultUnlockPolicy, VaultUnlockTriggerPolicy,
};
use sha2::{Digest, Sha256};
use serde_json::{json, Value};

#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};

const TARGET_SSH_HOST_KEY_POLICY_METADATA_KEY: &str = "ssh.host_key_policy";
const TARGET_SSH_ALLOW_IDENTITY_FALLBACK_METADATA_KEY: &str = "ssh.allow_identity_fallback";
const TARGET_SSH_RUNTIME_PASSPHRASE_PROMPT_METADATA_KEY: &str = "ssh.runtime_passphrase_prompt";
const TARGET_SSH_DELIVERY_MODE_METADATA_KEY: &str = "ssh.delivery_mode";
const TARGET_STORAGE_CLASS_METADATA_KEY: &str = "target.storage_class";
const TARGET_ACCESS_CLASS_METADATA_KEY: &str = "target.access_class";
const TARGET_SEALED_PROFILE_REF_METADATA_KEY: &str = "target.sealed_profile_ref";
const TARGET_CATALOG_PROJECTION_STATE_METADATA_KEY: &str = "target.catalog_projection_state";
const TARGET_SEALED_DESCRIPTOR_DIAGNOSTIC_METADATA_KEY: &str =
    "target.sealed_descriptor_diagnostic";

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
    pub principal_id: Option<String>,
    pub agent_id: String,
    pub run_id: String,
    pub client_session_id: String,
    pub reuse_policy: SessionReusePolicy,
    pub timeline_source: Option<TimelineSourceGroupSummaryView>,
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
        invocation: Option<CommandInvocation>,
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
                invocation,
            } => self.run_terminal(
                target_id,
                target_kind,
                context,
                command,
                artifact_id,
                invocation,
            ),
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
                self.run_terminal(target_id, target_kind, context, command, artifact_id, None)
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
        invocation: Option<CommandInvocation>,
    ) -> ToolResult {
        let now = SystemTime::now();
        let scope = build_scope(&context, now);
        let logical = self.metadata.resolve_logical_session(
            &scope,
            &target_id,
            context.reuse_policy.clone(),
            now,
        );
        let (resolved_path, resolved_source) = invocation
            .as_ref()
            .map(|resolved| {
                (
                    resolved.resolution.effective_path.clone(),
                    resolved.resolution.effective_source.clone(),
                )
            })
            .unwrap_or((None, None));
        let transport = match self.metadata.open_transport_session(
            &logical.logical_session_id,
            &target_id,
            target_kind,
            resolved_path,
            resolved_source,
            now,
        ) {
            Ok(record) => record,
            Err(err) => {
                return ToolResult::Error {
                    message: err.message(),
                };
            }
        };
        let channel = self.metadata.open_channel(
            &logical.logical_session_id,
            &transport.transport_session_id,
            &target_id,
            ChannelKind::OneShotExec,
            Some("terminal.exec".into()),
            now,
        );

        let policy = bridgingio_domain::PolicyProfile::default();
        let execution_result = if let Some(invocation) = invocation.as_ref() {
            self.terminal_provider.exec_structured_invocation(
                &logical.logical_session_id,
                Some(&channel.channel_id),
                Some(&transport.transport_session_id),
                invocation,
                &command,
                &artifact_id,
                &policy,
            )
        } else {
            self.terminal_provider.exec_local(
                &logical.logical_session_id,
                Some(&channel.channel_id),
                Some(&transport.transport_session_id),
                &command,
                &artifact_id,
                &policy,
            )
        };
        let completed_at = SystemTime::now();
        let _ = self.metadata.update_channel_status(
            &channel.channel_id,
            ChannelStatus::Closed,
            Some("one-shot execution completed".into()),
            completed_at,
        );
        let _ = self.metadata.close_transport_session(
            &transport.transport_session_id,
            "one-shot execution completed",
            completed_at,
        );

        match execution_result {
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
        principal_id: context
            .principal_id
            .clone()
            .unwrap_or_else(|| "local-operator".into()),
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

fn generate_core_instance_id() -> String {
    let stamp = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!("core-{stamp}-{}", process::id())
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
    host_id: String,
    ui_session_id: String,
    ui_kind: String,
    scope_id: String,
    attached_at: SystemTime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SettingsUpdateRequest {
    core_log_level: Option<String>,
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
    executed_command: String,
    invocation: Option<CommandInvocation>,
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
    invocation: Option<CommandInvocation>,
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
    invocation: Option<CommandInvocation>,
    launch_strategy: String,
    launch_fallback_applied: bool,
    launch_diagnostics: Vec<String>,
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
    transport_session_id: String,
    target_id: String,
    ssh_broker_session_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum McpTargetResolutionPolicy {
    AutoExecute,
    ConfirmIfFamily,
    ConfirmIfRelated,
    ConfirmAlways,
}

impl McpTargetResolutionPolicy {
    fn parse(raw: &str) -> Result<Self, CoreRuntimeError> {
        match raw.trim() {
            "auto_execute" => Ok(Self::AutoExecute),
            "confirm_if_family" => Ok(Self::ConfirmIfFamily),
            "confirm_if_related" => Ok(Self::ConfirmIfRelated),
            "confirm_always" => Ok(Self::ConfirmAlways),
            other => Err(CoreRuntimeError::Config(format!(
                "invalid MCP target resolution policy: {other}"
            ))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::AutoExecute => "auto_execute",
            Self::ConfirmIfFamily => "confirm_if_family",
            Self::ConfirmIfRelated => "confirm_if_related",
            Self::ConfirmAlways => "confirm_always",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TargetStorageClass {
    Plain,
    SealedOverlay,
    SealedFull,
}

impl TargetStorageClass {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "plain" => Some(Self::Plain),
            "sealed-overlay" => Some(Self::SealedOverlay),
            "sealed-full" => Some(Self::SealedFull),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::SealedOverlay => "sealed-overlay",
            Self::SealedFull => "sealed-full",
        }
    }

    fn is_plain(self) -> bool {
        matches!(self, Self::Plain)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TargetAccessClass {
    AnonymousLocal,
    TokenScoped,
}

impl TargetAccessClass {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "anonymous-local" => Some(Self::AnonymousLocal),
            "token-scoped" => Some(Self::TokenScoped),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::AnonymousLocal => "anonymous-local",
            Self::TokenScoped => "token-scoped",
        }
    }

    fn allows_anonymous_local(self) -> bool {
        matches!(self, Self::AnonymousLocal)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct McpTargetDescriptor {
    target_id: String,
    enabled: bool,
    display_name: String,
    kind: TargetKind,
    aliases: Vec<String>,
    notes: Option<String>,
    connection_summary: String,
    storage_class: TargetStorageClass,
    access_class: TargetAccessClass,
    sealed_profile_ref: Option<String>,
}

impl McpTargetDescriptor {
    fn references(&self) -> Vec<&str> {
        let mut refs = Vec::with_capacity(2 + self.aliases.len());
        refs.push(self.target_id.as_str());
        refs.push(self.display_name.as_str());
        for alias in &self.aliases {
            refs.push(alias.as_str());
        }
        refs
    }

    fn related_references(&self) -> Vec<&str> {
        let mut refs = Vec::with_capacity(1 + self.aliases.len());
        refs.push(self.target_id.as_str());
        for alias in &self.aliases {
            refs.push(alias.as_str());
        }
        refs
    }

    fn allows_anonymous_execution(self: &Self) -> bool {
        self.storage_class.is_plain() && self.access_class.allows_anonymous_local()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TargetResolutionResolved {
    requested_target_ref: String,
    resolved_target_id: String,
    matched_via: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TargetResolutionCandidate {
    target_id: String,
    reasons: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TargetResolutionConfirmation {
    requested_target_ref: String,
    policy: McpTargetResolutionPolicy,
    policy_reason: String,
    exact_match: Option<TargetResolutionCandidate>,
    related_candidates: Vec<TargetResolutionCandidate>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TargetResolutionNotFound {
    requested_target_ref: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TargetResolutionResult {
    Resolved(TargetResolutionResolved),
    ConfirmationRequired(TargetResolutionConfirmation),
    NotFound(TargetResolutionNotFound),
}

pub type SharedRuntime = Arc<Mutex<StandaloneCoreRuntime>>;

pub struct StandaloneCoreRuntime {
    core_instance_id: String,
    host_mode: CoreHostMode,
    readiness_state: CoreReadinessState,
    attached_ui: Option<UiAttachment>,
    shutdown_requested: bool,
    host_platform_adapter: Box<dyn HostPlatformAdapter>,
    pub settings_store: CoreSettingsStore,
    pub tool_handler: McpToolHandler,
    profiles: HashMap<String, TargetProfile>,
    target_refs: HashMap<String, String>,
    target_descriptors: HashMap<String, McpTargetDescriptor>,
    target_resolution_policy: McpTargetResolutionPolicy,
    sessions: HashMap<String, SessionRecord>,
    interactive_shell_owners: HashMap<String, InteractiveShellOwner>,
    timeline: Vec<TimelineEntry>,
    next_session_seq: u64,
    next_timeline_seq: u64,
    next_internal_artifact_seq: u64,
    toolchain_diagnostics: Vec<ToolchainDiagnosticView>,
    toolchain_diagnostics_by_target: HashMap<String, ToolchainDiagnosticView>,
    toolchain_resolver: ToolchainResolver,
    vault_router: SecretVaultRouter,
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
        Self::from_settings_with_mode(settings, toolchain_resolver, CoreHostMode::StandaloneRun)
    }

    fn from_settings_with_mode_and_state(
        mut settings: CoreSettings,
        toolchain_resolver: ToolchainResolver,
        host_mode: CoreHostMode,
        readiness_state: CoreReadinessState,
    ) -> Result<Self, CoreRuntimeError> {
        let host_platform_adapter = detect_host_platform_adapter(&settings.core.log_level);
        resolve_runtime_path_defaults(&mut settings, host_platform_adapter.as_ref())?;
        host_platform_adapter.runtime_logger().log(
            RuntimeLogLevel::Info,
            RuntimeLogCategory::Startup,
            &format!(
                "host platform adapter initialized (platform={})",
                host_platform_adapter.host_platform().as_str()
            ),
        );

        let (profiles, target_refs, target_descriptors) = build_target_runtime_indexes(&settings)?;
        let target_resolution_policy =
            McpTargetResolutionPolicy::parse(&settings.policies.mcp_target_resolution_policy)?;

        let mut vault_router = SecretVaultRouter::default();
        vault_router
            .set_active_backend(&settings.vault.backend)
            .map_err(vault_error_to_runtime)?;
        vault_router
            .set_unlock_policy(vault_unlock_policy_from_settings(&settings)?)
            .map_err(vault_error_to_runtime)?;
        host_platform_adapter.runtime_logger().log(
            RuntimeLogLevel::Info,
            RuntimeLogCategory::Vault,
            &format!(
                "vault backend initialized: configured={} active={}",
                settings.vault.backend,
                vault_router.active_backend()
            ),
        );

        let artifact_store = ArtifactStore::new(artifact_store_config_from_settings(
            &settings,
            host_platform_adapter.as_ref(),
        )?)
        .map_err(|err| CoreRuntimeError::Config(err.message))?;
        let mut runtime = Self {
            core_instance_id: generate_core_instance_id(),
            host_mode,
            readiness_state,
            attached_ui: None,
            shutdown_requested: false,
            host_platform_adapter,
            settings_store: CoreSettingsStore::from_settings(settings, None),
            tool_handler: McpToolHandler::with_artifact_store(artifact_store),
            profiles,
            target_refs,
            target_descriptors,
            target_resolution_policy,
            sessions: HashMap::new(),
            interactive_shell_owners: HashMap::new(),
            timeline: Vec::new(),
            next_session_seq: 0,
            next_timeline_seq: 0,
            next_internal_artifact_seq: 0,
            toolchain_diagnostics: Vec::new(),
            toolchain_diagnostics_by_target: HashMap::new(),
            toolchain_resolver,
            vault_router,
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

    pub fn core_instance_id(&self) -> &str {
        &self.core_instance_id
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

    fn capability_health_snapshot_json(&self) -> Value {
        let platform_snapshot = self.host_platform_adapter.snapshot();
        let vault_diag = self.vault_router.active_backend_diagnostics().ok();
        let vault_status = vault_diag
            .as_ref()
            .map(|diag| vault_readiness_to_capability_status(&diag.status))
            .unwrap_or(platform_snapshot.native_vault_status);
        let vault_message = vault_diag
            .as_ref()
            .map(|diag| diag.message.clone())
            .unwrap_or_else(|| {
                "vault readiness diagnostics unavailable; host capability snapshot is used".into()
            });
        let unresolved_toolchains = self
            .toolchain_diagnostics
            .iter()
            .filter(|diag| diag.effective_path.is_none())
            .map(|diag| {
                let target = diag
                    .target_id
                    .clone()
                    .unwrap_or_else(|| "global".to_string());
                format!("{target}:{}", diag.command)
            })
            .collect::<Vec<_>>();
        let toolchain_status = if unresolved_toolchains.is_empty() {
            platform_snapshot.toolchain_locator_status
        } else {
            CapabilityStatus::Degraded
        };
        let statuses = [
            toolchain_status,
            vault_status,
            platform_snapshot.runtime_logger_status,
            platform_snapshot.output_decoder_status,
        ];
        json!({
            "state": aggregate_health_state(&statuses),
            "host_platform": platform_snapshot.host_platform.as_str(),
            "toolchain": {
                "status": toolchain_status.as_str(),
                "source_label": self.host_platform_adapter.toolchain_locator().source_label(),
                "resolution_order": self.host_platform_adapter.toolchain_locator().resolution_order(),
                "unresolved_targets": unresolved_toolchains,
            },
            "vault": {
                "status": vault_status.as_str(),
                "configured_backend": self.settings_store.settings.vault.backend.clone(),
                "binding_backend": self.host_platform_adapter.native_vault_binding().backend_label(),
                "readiness_message": vault_message,
            },
            "logger": {
                "status": platform_snapshot.runtime_logger_status.as_str(),
                "level": self.host_platform_adapter.runtime_logger().level().as_str(),
                "categories": runtime_log_category_labels(),
            },
            "decoder": {
                "status": platform_snapshot.output_decoder_status.as_str(),
                "strategy": "platform-default decoding with UTF-8 fallback and newline normalization",
                "decode_diagnostics_policy": "fallback/replacement/newline normalization events are logged under decode category",
                "raw_bytes_canonical_future": {
                    "status": "deferred",
                    "impact": [
                        "artifact storage must preserve canonical raw bytes and derive text views",
                        "artifact API will need explicit raw/text read modes and metadata",
                        "content digest and dedup boundaries should be defined on raw bytes"
                    ],
                },
            },
        })
    }

    pub fn shutdown_requested(&self) -> bool {
        self.shutdown_requested
    }

    pub fn requires_startup_secret_input(&self) -> bool {
        matches!(
            self.vault_router.unlock_policy().trigger_policy,
            VaultUnlockTriggerPolicy::OnCoreStart
        ) && matches!(self.vault_router.vault_lock_state(), VaultLockState::Locked)
            && self
                .vault_router
                .unlock_policy()
                .allowed_methods
                .iter()
                .any(|method| method.eq_ignore_ascii_case("passphrase"))
    }

    pub fn complete_startup_unlock_with_secret(
        &mut self,
        secret: Option<&str>,
    ) -> Result<(), CoreRuntimeError> {
        if !matches!(
            self.vault_router.unlock_policy().trigger_policy,
            VaultUnlockTriggerPolicy::OnCoreStart
        ) {
            return Ok(());
        }
        if matches!(self.vault_router.vault_lock_state(), VaultLockState::Unlocked) {
            return Ok(());
        }
        if let Some(secret) = secret {
            self.vault_router
                .unlock_with_passphrase(secret)
                .map_err(vault_error_to_runtime)?;
        }
        match self.vault_router.vault_lock_state() {
            VaultLockState::Unlocked => Ok(()),
            VaultLockState::Unavailable => Err(CoreRuntimeError::Config(
                "vault startup unlock is unavailable".into(),
            )),
            VaultLockState::Locked | VaultLockState::Unlocking | VaultLockState::Uninitialized => {
                Err(CoreRuntimeError::Config(
                    "vault startup unlock did not complete".into(),
                ))
            }
        }
    }

    fn request_shutdown(&mut self) {
        self.host_platform_adapter.runtime_logger().log(
            RuntimeLogLevel::Info,
            RuntimeLogCategory::Startup,
            "runtime shutdown requested",
        );
        self.readiness_state = CoreReadinessState::ShuttingDown;
        self.shutdown_requested = true;
    }

    fn host_instance_probe_payload(&self) -> Value {
        let settings = &self.settings_store.settings;
        let runtime_root = settings.core.data_dir.clone();
        let runtime_paths = self.host_platform_adapter.runtime_paths().runtime_paths(
            &settings.core.instance_name,
            Path::new(&settings.core.data_dir),
        );
        let attached_owner = self.attached_ui.as_ref().map(|attachment| {
            json!({
                "host_id": attachment.host_id,
                "ui_session_id": attachment.ui_session_id,
                "ui_kind": attachment.ui_kind,
                "scope_id": attachment.scope_id,
                "attached_at_ms": system_time_to_unix_millis(attachment.attached_at),
            })
        });
        json!({
            "core_instance_id": self.core_instance_id,
            "host_mode": self.host_mode.as_label(),
            "ownership_mode": self.host_mode.as_label(),
            "pid": process::id(),
            "started_at_ms": system_time_to_unix_millis(self.settings_store.runtime_metadata.started_at),
            "runtime_root": runtime_root,
            "readiness_state": self.readiness_state_label(),
            "control_plane": {
                "transport": settings.control_plane.transport,
                "endpoint": runtime_paths.control_plane_endpoint,
            },
            "model_plane": {
                "enabled": settings.model_plane.http.enabled,
                "host": settings.model_plane.http.host,
                "port": settings.model_plane.http.port,
            },
            "attached_owner": attached_owner,
        })
    }

    fn ownership_conflict_payload_json(
        &self,
        requested_host_id: &str,
        requested_ui_session_id: &str,
        requested_ui_kind: &str,
    ) -> String {
        json!({
            "reason": "ownership_conflict",
            "message": "another owner is already attached",
            "recovery_hint": "close the current owner or reconcile the running instance before retrying attach",
            "requested_owner": {
                "host_id": requested_host_id,
                "ui_session_id": requested_ui_session_id,
                "ui_kind": requested_ui_kind,
            },
            "current_owner": self.attached_ui.as_ref().map(|owner| {
                json!({
                    "host_id": owner.host_id,
                    "ui_session_id": owner.ui_session_id,
                    "ui_kind": owner.ui_kind,
                    "scope_id": owner.scope_id,
                    "attached_at_ms": system_time_to_unix_millis(owner.attached_at),
                })
            }),
            "instance": self.host_instance_probe_payload(),
        })
        .to_string()
    }

    fn push_timeline_entry(
        &mut self,
        session_id: String,
        command_preview: String,
        status: &str,
        source_group: TimelineSourceGroupSummaryView,
        artifact_id: Option<String>,
    ) {
        self.next_timeline_seq += 1;
        let entry = TimelineEntry {
            id: format!("timeline-{:06}", self.next_timeline_seq),
            session_id,
            command_preview,
            status: status.to_string(),
            source_group,
            artifact_id,
            created_at: SystemTime::now(),
        };
        self.timeline.push(entry);
    }

    fn local_ui_timeline_source(
        context: &bridgingio_app_api::ApiRequestContext,
    ) -> TimelineSourceGroupSummaryView {
        let group_key = format!(
            "trusted-ui:{}:{}",
            context.agent_id, context.client_session_id
        );
        TimelineSourceGroupSummaryView {
            group_key,
            group_kind: "trusted_ui".to_string(),
            group_label: "Trusted Desktop UI".to_string(),
            principal_summary: "local-operator".to_string(),
            user_agent_summary: None,
        }
    }

    fn timeline_source_from_tool_context(
        context: &ToolRequestContext,
    ) -> TimelineSourceGroupSummaryView {
        context
            .timeline_source
            .clone()
            .unwrap_or_else(|| TimelineSourceGroupSummaryView {
                group_key: format!("mcp:{}:{}", context.agent_id, context.client_session_id),
                group_kind: "mcp_actor".to_string(),
                group_label: "MCP actor".to_string(),
                principal_summary: context
                    .principal_id
                    .clone()
                    .unwrap_or_else(|| "local-operator".to_string()),
                user_agent_summary: None,
            })
    }

    fn resolve_target_profile_by_ref(&self, target_ref: &str) -> Option<TargetProfile> {
        let target_id = self.resolve_target_id_by_ref(target_ref)?;
        self.profiles.get(&target_id).cloned()
    }

    fn resolve_target_id_by_ref(&self, target_ref: &str) -> Option<String> {
        let normalized = normalize_target_ref(target_ref);
        self.target_refs.get(&normalized).cloned()
    }

    fn resolve_target_descriptor_by_ref(&self, target_ref: &str) -> Option<&McpTargetDescriptor> {
        let normalized = normalize_target_ref(target_ref);
        let target_id = self.target_refs.get(&normalized)?;
        self.target_descriptors.get(target_id)
    }

    fn catalog_projected_profiles(&mut self) -> Vec<TargetProfile> {
        let mut target_ids = self.profiles.keys().cloned().collect::<Vec<_>>();
        target_ids.sort();
        target_ids
            .into_iter()
            .filter_map(|target_id| self.project_catalog_profile_by_id(&target_id).ok())
            .collect()
    }

    fn project_catalog_profile_by_id(
        &mut self,
        target_id: &str,
    ) -> Result<TargetProfile, CoreRuntimeError> {
        let profile = self
            .profiles
            .get(target_id)
            .cloned()
            .ok_or_else(|| CoreRuntimeError::Config(format!("target not found: {target_id}")))?;
        let descriptor = self
            .target_descriptors
            .get(target_id)
            .cloned()
            .ok_or_else(|| CoreRuntimeError::Config(format!("target descriptor not found: {target_id}")))?;
        if descriptor.storage_class.is_plain() {
            let mut projected = profile;
            apply_target_catalog_projection_metadata(&mut projected, "plain", None);
            return Ok(projected);
        }

        if !matches!(self.vault_router.vault_lock_state(), VaultLockState::Unlocked) {
            return Ok(redacted_sealed_catalog_profile(
                &profile,
                "sealed-redacted-locked",
                Some("sealed target descriptor is redacted while vault is locked"),
            ));
        }

        match self.load_verified_sealed_overlay(&descriptor) {
            Ok(overlay) => {
                let mut projected = profile;
                apply_sealed_overlay_to_target_profile(&mut projected, &overlay)
                    .map_err(|err| CoreRuntimeError::Config(format!("{err:?}")))?;
                apply_target_catalog_projection_metadata(&mut projected, "sealed-resolved", None);
                Ok(projected)
            }
            Err(CoreRuntimeError::Config(message)) => Ok(redacted_sealed_catalog_profile(
                &profile,
                "sealed-redacted-tamper",
                Some(&message),
            )),
            Err(other) => Err(other),
        }
    }

    fn resolve_target_profile_for_execution_by_ref(
        &mut self,
        target_ref: &str,
    ) -> Result<TargetProfile, CoreRuntimeError> {
        let target_id = self
            .resolve_target_id_by_ref(target_ref)
            .ok_or_else(|| CoreRuntimeError::Config(format!("target not found: {target_ref}")))?;
        let mut target = self
            .profiles
            .get(&target_id)
            .cloned()
            .ok_or_else(|| CoreRuntimeError::Config(format!("target not found: {target_ref}")))?;
        let descriptor = self
            .target_descriptors
            .get(&target_id)
            .cloned()
            .ok_or_else(|| {
                CoreRuntimeError::Config(format!("target descriptor not found: {target_id}"))
            })?;
        if descriptor.storage_class.is_plain() {
            return Ok(target);
        }
        if !matches!(self.vault_router.vault_lock_state(), VaultLockState::Unlocked) {
            return Err(CoreRuntimeError::Config(format!(
                "sealed target `{}` requires unlocked vault before execution",
                descriptor.target_id
            )));
        }
        let overlay = self.load_verified_sealed_overlay(&descriptor)?;
        apply_sealed_overlay_to_target_profile(&mut target, &overlay)
            .map_err(|err| CoreRuntimeError::Config(format!("{err:?}")))?;
        Ok(target)
    }

    fn load_verified_sealed_overlay(
        &mut self,
        descriptor: &McpTargetDescriptor,
    ) -> Result<Value, CoreRuntimeError> {
        let sealed_profile_ref = descriptor
            .sealed_profile_ref
            .as_deref()
            .ok_or_else(|| {
                CoreRuntimeError::Config(format!(
                    "sealed target `{}` is missing sealed_profile_ref",
                    descriptor.target_id
                ))
            })?;
        let lease = self
            .vault_router
            .use_for_signing(
                sealed_profile_ref,
                format!("target-overlay:{}", descriptor.target_id),
            )
            .map_err(vault_error_to_runtime)?;
        let overlay_bytes = lease.with_secret_bytes(|bytes| bytes.to_vec());
        let overlay: Value = serde_json::from_slice(&overlay_bytes).map_err(|err| {
            CoreRuntimeError::Config(format!(
                "sealed target `{}` overlay payload is not valid json: {err}",
                descriptor.target_id
            ))
        })?;
        let actual_digest = overlay
            .get("public_descriptor_digest")
            .and_then(Value::as_str)
            .map(|value| value.trim().to_ascii_lowercase())
            .ok_or_else(|| {
                CoreRuntimeError::Config(format!(
                    "sealed target `{}` overlay missing public_descriptor_digest",
                    descriptor.target_id
                ))
            })?;
        let expected_digest = public_descriptor_digest_for_target_descriptor(descriptor);
        if actual_digest != expected_digest {
            return Err(CoreRuntimeError::Config(format!(
                "sealed target `{}` descriptor tamper detected (expected digest {}, got {})",
                descriptor.target_id, expected_digest, actual_digest
            )));
        }
        Ok(overlay)
    }

    fn anonymous_loopback_compat_enabled(&self) -> bool {
        if !self
            .settings_store
            .settings
            .model_plane
            .http
            .auth
            .allow_loopback_anonymous_compat
        {
            return false;
        }
        self.settings_store
            .settings
            .model_plane
            .http
            .host
            .parse::<IpAddr>()
            .map(|host| host.is_loopback())
            .unwrap_or(false)
    }

    fn resolve_target_for_mcp(&self, target_ref: &str) -> TargetResolutionResult {
        let requested_target_ref = target_ref.trim().to_string();
        if requested_target_ref.is_empty() {
            return TargetResolutionResult::NotFound(TargetResolutionNotFound {
                requested_target_ref,
            });
        }

        let normalized = normalize_target_ref(target_ref);
        let relaxed = normalize_target_ref_relaxed(target_ref);
        let mut descriptors = self
            .target_descriptors
            .values()
            .filter(|descriptor| descriptor.enabled)
            .collect::<Vec<_>>();
        descriptors.sort_by(|left, right| left.target_id.cmp(&right.target_id));

        let mut exact_ids = descriptors
            .iter()
            .filter(|descriptor| {
                descriptor
                    .references()
                    .into_iter()
                    .any(|reference| normalize_target_ref(reference) == normalized)
            })
            .map(|descriptor| descriptor.target_id.clone())
            .collect::<Vec<_>>();
        exact_ids.sort();
        exact_ids.dedup();
        if exact_ids.len() > 1 {
            return TargetResolutionResult::ConfirmationRequired(TargetResolutionConfirmation {
                requested_target_ref,
                policy: self.target_resolution_policy,
                policy_reason: "exact_match_ambiguous".into(),
                exact_match: None,
                related_candidates: exact_ids
                    .into_iter()
                    .map(|target_id| TargetResolutionCandidate {
                        target_id,
                        reasons: vec!["exact_match_ambiguous".into()],
                    })
                    .collect(),
            });
        }
        let exact_match_id = exact_ids.first().cloned();

        let mut relaxed_ids = descriptors
            .iter()
            .filter(|descriptor| {
                !relaxed.is_empty()
                    && descriptor
                        .references()
                        .into_iter()
                        .any(|reference| normalize_target_ref_relaxed(reference) == relaxed)
            })
            .map(|descriptor| descriptor.target_id.clone())
            .collect::<Vec<_>>();
        relaxed_ids.sort();
        relaxed_ids.dedup();

        let (resolved_target_id, matched_via) = if let Some(exact) = exact_match_id.clone() {
            (Some(exact), Some("exact".to_string()))
        } else if relaxed_ids.len() == 1 {
            (Some(relaxed_ids[0].clone()), Some("normalized".to_string()))
        } else {
            (None, None)
        };

        if resolved_target_id.is_none() && relaxed_ids.len() > 1 {
            return TargetResolutionResult::ConfirmationRequired(TargetResolutionConfirmation {
                requested_target_ref,
                policy: self.target_resolution_policy,
                policy_reason: "normalized_match_ambiguous".into(),
                exact_match: None,
                related_candidates: relaxed_ids
                    .into_iter()
                    .map(|target_id| TargetResolutionCandidate {
                        target_id,
                        reasons: vec!["normalized_match_ambiguous".into()],
                    })
                    .collect(),
            });
        }

        let mut candidate_reasons: HashMap<String, HashSet<String>> = HashMap::new();
        for descriptor in &descriptors {
            if Some(descriptor.target_id.as_str()) == resolved_target_id.as_deref() {
                continue;
            }
            if descriptor
                .related_references()
                .into_iter()
                .any(|reference| is_family_related_reference(&normalized, reference))
            {
                candidate_reasons
                    .entry(descriptor.target_id.clone())
                    .or_default()
                    .insert("family_related".to_string());
            }
            if descriptor
                .related_references()
                .into_iter()
                .any(|reference| {
                    is_typo_related_reference(
                        &normalized,
                        &relaxed,
                        reference,
                        Some(descriptor.target_id.as_str()) == exact_match_id.as_deref(),
                    )
                })
            {
                candidate_reasons
                    .entry(descriptor.target_id.clone())
                    .or_default()
                    .insert("typo_related".to_string());
            }
        }

        let mut related_candidates = candidate_reasons
            .into_iter()
            .map(|(target_id, reasons)| {
                let mut reasons = reasons.into_iter().collect::<Vec<_>>();
                reasons.sort();
                TargetResolutionCandidate { target_id, reasons }
            })
            .collect::<Vec<_>>();
        related_candidates.sort_by(|left, right| left.target_id.cmp(&right.target_id));

        let has_family_candidates = related_candidates.iter().any(|candidate| {
            candidate
                .reasons
                .iter()
                .any(|reason| reason == "family_related")
        });

        if let Some(resolved_target_id) = resolved_target_id {
            let exact_match = exact_match_id.is_some();
            let mut policy_related_candidates = related_candidates.clone();
            if matches!(
                self.target_resolution_policy,
                McpTargetResolutionPolicy::ConfirmIfFamily
            ) {
                policy_related_candidates.retain(|candidate| {
                    candidate
                        .reasons
                        .iter()
                        .any(|reason| reason == "family_related")
                });
            }
            let should_confirm = match self.target_resolution_policy {
                McpTargetResolutionPolicy::AutoExecute => false,
                McpTargetResolutionPolicy::ConfirmIfFamily => exact_match && has_family_candidates,
                McpTargetResolutionPolicy::ConfirmIfRelated => {
                    exact_match && !related_candidates.is_empty()
                }
                McpTargetResolutionPolicy::ConfirmAlways => {
                    exact_match && !related_candidates.is_empty()
                }
            };

            if should_confirm {
                let policy_reason = match self.target_resolution_policy {
                    McpTargetResolutionPolicy::AutoExecute => "policy_auto_execute",
                    McpTargetResolutionPolicy::ConfirmIfFamily => "policy_confirm_if_family",
                    McpTargetResolutionPolicy::ConfirmIfRelated => "policy_confirm_if_related",
                    McpTargetResolutionPolicy::ConfirmAlways => "policy_confirm_always",
                }
                .to_string();
                return TargetResolutionResult::ConfirmationRequired(
                    TargetResolutionConfirmation {
                        requested_target_ref,
                        policy: self.target_resolution_policy,
                        policy_reason,
                        exact_match: Some(TargetResolutionCandidate {
                            target_id: resolved_target_id,
                            reasons: vec!["exact_match".into()],
                        }),
                        related_candidates: policy_related_candidates,
                    },
                );
            }

            return TargetResolutionResult::Resolved(TargetResolutionResolved {
                requested_target_ref,
                resolved_target_id,
                matched_via: matched_via.unwrap_or_else(|| "exact".into()),
            });
        }

        if !related_candidates.is_empty() {
            return TargetResolutionResult::ConfirmationRequired(TargetResolutionConfirmation {
                requested_target_ref,
                policy: self.target_resolution_policy,
                policy_reason: "related_candidates_found".into(),
                exact_match: None,
                related_candidates,
            });
        }

        TargetResolutionResult::NotFound(TargetResolutionNotFound {
            requested_target_ref,
        })
    }

    fn target_candidate_summary_json(&mut self, candidate: &TargetResolutionCandidate) -> Value {
        let descriptor = self.target_descriptors.get(&candidate.target_id).cloned();
        let fallback_profile = self.profiles.get(&candidate.target_id).cloned();
        let projected_profile = self.project_catalog_profile_by_id(&candidate.target_id).ok();
        let profile = projected_profile.as_ref().or(fallback_profile.as_ref());
        let toolchain_summary = profile
            .and_then(|target| self.toolchain_diagnostic_for_target(target))
            .map(|diagnostic| {
                json!({
                    "command": diagnostic.command,
                    "effective_path": diagnostic.effective_path,
                    "effective_source": diagnostic.effective_source,
                    "effective_scope": diagnostic.effective_scope,
                })
            })
            .unwrap_or(Value::Null);

        let metadata = self.tool_handler.metadata();
        let active_logical_sessions = metadata
            .logical_sessions
            .values()
            .filter(|record| record.target_id == candidate.target_id && record.status.is_alive())
            .count();
        let active_transport_sessions = metadata
            .transport_sessions
            .values()
            .filter(|record| {
                record.target_id == candidate.target_id
                    && matches!(
                        record.status,
                        bridgingio_domain::TransportSessionStatus::Connecting
                            | bridgingio_domain::TransportSessionStatus::Connected
                            | bridgingio_domain::TransportSessionStatus::Degraded
                    )
            })
            .count();
        let active_channels = metadata
            .channels
            .values()
            .filter(|record| {
                record.target_id == candidate.target_id
                    && !matches!(
                        record.status,
                        bridgingio_domain::ChannelStatus::Failed
                            | bridgingio_domain::ChannelStatus::Closed
                    )
            })
            .count();

        json!({
            "canonical_target_id": candidate.target_id,
            "display_name": descriptor
                .as_ref()
                .map(|item| item.display_name.clone())
                .unwrap_or_else(|| candidate.target_id.clone()),
            "kind": descriptor
                .as_ref()
                .map(|item| target_kind_label(&item.kind))
                .or_else(|| profile.map(|item| target_kind_label(&item.kind)))
                .unwrap_or_else(|| "unknown".to_string()),
            "enabled": descriptor.as_ref().map(|item| item.enabled).unwrap_or(true),
            "aliases": descriptor
                .as_ref()
                .map(|item| item.aliases.clone())
                .unwrap_or_default(),
            "notes": profile
                .and_then(|item| item.notes.clone())
                .or_else(|| descriptor.as_ref().and_then(|item| item.notes.clone())),
            "storage_class": descriptor
                .as_ref()
                .map(|item| item.storage_class.as_str())
                .unwrap_or("plain"),
            "access_class": descriptor
                .as_ref()
                .map(|item| item.access_class.as_str())
                .unwrap_or("anonymous-local"),
            "sealed_profile_ref": descriptor
                .as_ref()
                .and_then(|item| item.sealed_profile_ref.clone()),
            "catalog_projection_state": profile
                .and_then(|item| {
                    item.metadata
                        .get(TARGET_CATALOG_PROJECTION_STATE_METADATA_KEY)
                        .cloned()
                }),
            "sealed_descriptor_diagnostic": profile.and_then(|item| {
                item.metadata
                    .get(TARGET_SEALED_DESCRIPTOR_DIAGNOSTIC_METADATA_KEY)
                    .cloned()
            }),
            "match_reasons": candidate.reasons,
            "connection_summary": profile
                .map(target_connection_summary_from_profile)
                .or_else(|| descriptor.as_ref().map(|item| item.connection_summary.clone()))
                .unwrap_or_else(|| "unknown".to_string()),
            "diagnostic_summary": toolchain_summary,
            "session_summary": {
                "active_logical_sessions": active_logical_sessions,
                "active_transport_sessions": active_transport_sessions,
                "active_channels": active_channels,
            }
        })
    }

    fn confirmation_payload_json(
        &mut self,
        confirmation: &TargetResolutionConfirmation,
        tool_name: &str,
    ) -> Value {
        let exact_match = confirmation
            .exact_match
            .as_ref()
            .map(|candidate| self.target_candidate_summary_json(candidate))
            .unwrap_or(Value::Null);
        let related_candidates = confirmation
            .related_candidates
            .iter()
            .map(|candidate| self.target_candidate_summary_json(candidate))
            .collect::<Vec<_>>();
        json!({
            "tool_name": tool_name,
            "resolution_state": "confirmation_required",
            "requested_target_ref": confirmation.requested_target_ref,
            "policy": confirmation.policy.as_str(),
            "policy_reason": confirmation.policy_reason,
            "exact_match": exact_match,
            "related_candidates": related_candidates,
        })
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
        let target = self.resolve_target_profile_for_execution_by_ref(target_ref)?;
        self.execute_target_command_on_profile(
            target_ref.to_string(),
            target,
            context,
            command,
            artifact_id,
        )
    }

    fn execute_target_command_on_profile(
        &mut self,
        requested_target_ref: String,
        target: TargetProfile,
        context: ToolRequestContext,
        command: &str,
        artifact_id: Option<String>,
    ) -> Result<TargetCommandExecution, CoreRuntimeError> {
        let mut invocation = self.resolve_structured_exec_invocation(&target, command)?;
        let artifact_id = artifact_id.unwrap_or_else(|| self.next_internal_artifact_hint());
        let one_shot_attach_scope = format!("one-shot:{artifact_id}");
        let mut ssh_broker_session_id = None;
        if let Some(invocation_ref) = invocation.as_mut() {
            ssh_broker_session_id = self.prepare_ssh_secret_delivery_for_invocation(
                &target,
                &context,
                invocation_ref,
                None,
            )?;
            if let Some(session_id) = ssh_broker_session_id.as_deref() {
                self.vault_router
                    .attach_ssh_agent_broker_session(session_id, &one_shot_attach_scope)
                    .map_err(vault_error_to_runtime)?;
            }
        }
        let executed_command = invocation
            .as_ref()
            .map(CommandInvocation::to_host_shell_command)
            .unwrap_or_else(|| command.to_string());
        let command_preview = self.tool_handler.terminal_provider.command_preview(command);
        let executed_command_preview = self
            .tool_handler
            .terminal_provider
            .command_preview(&executed_command);
        let source_group = Self::timeline_source_from_tool_context(&context);
        self.host_platform_adapter.runtime_logger().log(
            RuntimeLogLevel::Debug,
            RuntimeLogCategory::Terminal,
            &format!(
                "executing target command target_id={} kind={} command_preview={}",
                target.id,
                target_kind_label(&target.kind),
                command_preview
            ),
        );
        let result = self.tool_handler.handle(ToolRequest::TerminalExec {
            target_id: target.id.clone(),
            target_kind: target.kind.clone(),
            context,
            command: command.to_string(),
            artifact_id: artifact_id.clone(),
            invocation: invocation.clone(),
        });
        if let Some(session_id) = ssh_broker_session_id.as_deref() {
            let _ = self
                .vault_router
                .detach_ssh_agent_broker_session(session_id, &one_shot_attach_scope);
            let cleanup_reason = if matches!(result, ToolResult::Execution { .. }) {
                "one-shot execution completed"
            } else {
                "one-shot execution ended with error"
            };
            let _ = self
                .vault_router
                .close_ssh_agent_broker_session(session_id, cleanup_reason);
        }

        let (artifact_id, logical_session_id, channel_id) = match result {
            ToolResult::Execution {
                artifact_id,
                logical_session_id,
                channel_id,
            } => (artifact_id, logical_session_id, channel_id),
            ToolResult::ApprovalRequired { reason } => {
                self.host_platform_adapter.runtime_logger().log(
                    RuntimeLogLevel::Warn,
                    RuntimeLogCategory::Policy,
                    &format!(
                        "execution blocked by policy target_id={} reason={}",
                        target.id, reason
                    ),
                );
                return Err(CoreRuntimeError::Config(format!(
                    "command requires approval: {reason}"
                )));
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
            command_preview.clone(),
            "success",
            source_group,
            Some(artifact_id.clone()),
        );

        Ok(TargetCommandExecution {
            requested_target_ref,
            resolved_target_id: target.id.clone(),
            target_kind: target_kind_label(&target.kind),
            command: command_preview,
            executed_command: executed_command_preview,
            invocation,
            artifact_id,
            logical_session_id,
            channel_id,
            output: output_chunks.join("\n"),
        })
    }

    fn inspect_target_basic_with_profile(
        &mut self,
        requested_target_ref: String,
        target: TargetProfile,
        context: ToolRequestContext,
    ) -> Result<TargetInspectionResult, CoreRuntimeError> {
        let kernel = self.execute_target_command_on_profile(
            requested_target_ref.clone(),
            target.clone(),
            context.clone(),
            "uname -r",
            None,
        )?;
        let user = self.execute_target_command_on_profile(
            requested_target_ref.clone(),
            target,
            context,
            "whoami",
            None,
        )?;

        Ok(TargetInspectionResult {
            requested_target_ref,
            resolved_target_id: kernel.resolved_target_id.clone(),
            target_kind: kernel.target_kind.clone(),
            kernel_version: first_data_line(&kernel.output),
            username: first_data_line(&user.output),
            artifacts: vec![kernel.artifact_id, user.artifact_id],
            logical_session_id: user.logical_session_id,
            invocation: kernel.invocation.or(user.invocation),
        })
    }

    #[allow(dead_code)]
    fn open_interactive_shell(
        &mut self,
        target_ref: &str,
        context: ToolRequestContext,
    ) -> Result<InteractiveShellHandle, CoreRuntimeError> {
        let target = self.resolve_target_profile_for_execution_by_ref(target_ref)?;
        self.open_interactive_shell_with_profile(target_ref.to_string(), target, context)
    }

    fn open_interactive_shell_with_profile(
        &mut self,
        requested_target_ref: String,
        target: TargetProfile,
        context: ToolRequestContext,
    ) -> Result<InteractiveShellHandle, CoreRuntimeError> {
        let now = SystemTime::now();
        let scope = build_scope(&context, now);
        let logical = self.tool_handler.metadata_mut().resolve_logical_session(
            &scope,
            &target.id,
            context.reuse_policy.clone(),
            now,
        );
        let mut invocation = self.resolve_structured_interactive_invocation(&target)?;
        let (resolved_path, resolved_source) = if let Some(invocation) = invocation.as_ref() {
            (
                invocation.resolution.effective_path.clone(),
                invocation.resolution.effective_source.clone(),
            )
        } else {
            self.resolve_transport_executable_for_target(&target)
        };
        let transport = self
            .tool_handler
            .metadata_mut()
            .open_transport_session(
                &logical.logical_session_id,
                &target.id,
                target.kind.clone(),
                resolved_path,
                resolved_source,
                now,
            )
            .map_err(|err| CoreRuntimeError::Config(err.message()))?;
        let channel = self.tool_handler.metadata_mut().open_channel(
            &logical.logical_session_id,
            &transport.transport_session_id,
            &target.id,
            ChannelKind::InteractiveShell,
            Some("terminal.interactive".into()),
            now,
        );
        let mut ssh_broker_session_id = None;
        if let Some(invocation_ref) = invocation.as_mut() {
            ssh_broker_session_id = self.prepare_ssh_secret_delivery_for_invocation(
                &target,
                &context,
                invocation_ref,
                Some(&channel.channel_id),
            )?;
        }
        let launch_command = invocation
            .as_ref()
            .map(CommandInvocation::to_host_shell_command);
        let shell = match self
            .tool_handler
            .terminal_provider
            .open_interactive_shell_with_command(
                &logical.logical_session_id,
                &channel.channel_id,
                Some(&transport.transport_session_id),
                &target_kind_label(&target.kind),
                launch_command.as_deref(),
            ) {
            Ok(shell) => shell,
            Err(err) => {
                let failed_at = SystemTime::now();
                let _ = self.tool_handler.metadata_mut().update_channel_status(
                    &channel.channel_id,
                    ChannelStatus::Failed,
                    Some(format!("interactive shell open failed: {}", err.message)),
                    failed_at,
                );
                let _ = self.tool_handler.metadata_mut().close_transport_session(
                    &transport.transport_session_id,
                    "interactive shell open failed",
                    failed_at,
                );
                if let Some(session_id) = ssh_broker_session_id.as_deref() {
                    let _ = self
                        .vault_router
                        .detach_ssh_agent_broker_session(session_id, &channel.channel_id);
                    let _ = self.vault_router.close_ssh_agent_broker_session(
                        session_id,
                        "interactive shell open failed",
                    );
                }
                return Err(CoreRuntimeError::Config(err.message));
            }
        };
        if shell.launch_fallback_applied {
            if let Some(invocation_ref) = invocation.as_mut() {
                invocation_ref
                    .resolution
                    .warnings
                    .extend(shell.launch_diagnostics.clone());
            }
        }
        self.interactive_shell_owners.insert(
            shell.shell_id.clone(),
            InteractiveShellOwner {
                scope_id: scope.scope_id,
                logical_session_id: logical.logical_session_id.clone(),
                channel_id: channel.channel_id.clone(),
                transport_session_id: transport.transport_session_id.clone(),
                target_id: target.id.clone(),
                ssh_broker_session_id,
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
            invocation,
            launch_strategy: shell.launch_strategy,
            launch_fallback_applied: shell.launch_fallback_applied,
            launch_diagnostics: shell.launch_diagnostics,
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
        if state.closed {
            self.release_interactive_ssh_broker(
                shell_id,
                &owner,
                "interactive shell ended and broker cleanup was triggered",
            );
        }

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
        let mut state = self
            .tool_handler
            .terminal_provider
            .get_interactive_shell(shell_id)
            .map_err(|err| CoreRuntimeError::Config(err.message))?;
        state.running = false;
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
        let _ = self.tool_handler.metadata_mut().close_transport_session(
            &owner.transport_session_id,
            "interactive shell closed",
            now,
        );
        self.release_interactive_ssh_broker(shell_id, &owner, "interactive shell closed");
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

    fn prepare_ssh_secret_delivery_for_invocation(
        &mut self,
        target: &TargetProfile,
        context: &ToolRequestContext,
        invocation: &mut CommandInvocation,
        attach_channel_id: Option<&str>,
    ) -> Result<Option<String>, CoreRuntimeError> {
        if !matches!(target.kind, TargetKind::Ssh) {
            return Ok(None);
        }
        if target
            .metadata
            .get(TARGET_SSH_DELIVERY_MODE_METADATA_KEY)
            .map(|raw| !raw.trim().eq_ignore_ascii_case("ssh-agent-broker"))
            .unwrap_or(true)
        {
            return Ok(None);
        }
        let Some(credential_ref) = target.credential_ref.as_ref() else {
            return Ok(None);
        };

        let host_key_policy = ssh_host_key_policy_for_target(target);
        let allow_identity_fallback = target
            .metadata
            .get(TARGET_SSH_ALLOW_IDENTITY_FALLBACK_METADATA_KEY)
            .map(|raw| parse_bool_metadata(raw, true))
            .unwrap_or(true);
        let runtime_passphrase_requested = target
            .metadata
            .get(TARGET_SSH_RUNTIME_PASSPHRASE_PROMPT_METADATA_KEY)
            .map(|raw| parse_bool_metadata(raw, false))
            .unwrap_or(false);
        let prepared = self
            .vault_router
            .prepare_ssh_agent_broker_session(SshAgentBrokerPrepareRequest {
                target_id: target.id.clone(),
                credential_ref: credential_ref.id.clone(),
                principal_id: context.agent_id.clone(),
                logical_session_id: None,
                host_platform: runtime_host_platform_label(self.host_platform_adapter.as_ref()),
                allow_identity_fallback,
                host_key_policy,
                key_passphrase_handling: SshKeyPassphraseHandling::RuntimePromptForbidden,
                runtime_passphrase_requested,
                session_ttl: Some(Duration::from_secs(120)),
            })
            .map_err(vault_error_to_runtime)?;

        apply_ssh_delivery_args(invocation, &prepared.ssh_option_args);
        invocation
            .resolution
            .warnings
            .extend(prepared.diagnostics.clone());
        invocation.resolution.warnings.push(format!(
            "ssh broker session {} endpoint_kind={} degraded={}",
            prepared.session.broker_session_id,
            prepared.session.endpoint_kind.as_str(),
            prepared.session.degraded
        ));

        if let Some(channel_id) = attach_channel_id {
            self.vault_router
                .attach_ssh_agent_broker_session(&prepared.session.broker_session_id, channel_id)
                .map_err(vault_error_to_runtime)?;
        }

        Ok(Some(prepared.session.broker_session_id))
    }

    fn release_interactive_ssh_broker(
        &mut self,
        shell_id: &str,
        owner: &InteractiveShellOwner,
        reason: &str,
    ) {
        let Some(state) = self.interactive_shell_owners.get_mut(shell_id) else {
            return;
        };
        let Some(session_id) = state.ssh_broker_session_id.take() else {
            return;
        };
        let _ = self
            .vault_router
            .detach_ssh_agent_broker_session(&session_id, &owner.channel_id);
        let _ = self
            .vault_router
            .close_ssh_agent_broker_session(&session_id, reason);
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

    fn resolve_structured_exec_invocation(
        &self,
        target: &TargetProfile,
        command: &str,
    ) -> Result<Option<CommandInvocation>, CoreRuntimeError> {
        let diagnostic = self.toolchain_diagnostic_for_target(target);
        let dialect = target_shell_dialect_for(target);
        let mut invocation =
            if let Some(connector) = terminal_connector_for_target(target, &diagnostic) {
                Some(
                    connector
                        .build_exec_invocation(target, command)
                        .map_err(|err| CoreRuntimeError::Config(format!("{err:?}")))?,
                )
            } else {
                None
            };

        if let Some(invocation_ref) = invocation.as_mut() {
            invocation_ref.resolution = invocation_resolution_from_toolchain(
                diagnostic.as_ref(),
                dialect.support_level() == "deferred",
                dialect.as_str(),
            );
        }
        Ok(invocation)
    }

    fn resolve_structured_interactive_invocation(
        &self,
        target: &TargetProfile,
    ) -> Result<Option<CommandInvocation>, CoreRuntimeError> {
        let diagnostic = self.toolchain_diagnostic_for_target(target);
        let dialect = target_shell_dialect_for(target);
        let mut invocation =
            if let Some(connector) = terminal_connector_for_target(target, &diagnostic) {
                Some(
                    connector
                        .build_interactive_invocation(target)
                        .map_err(|err| CoreRuntimeError::Config(format!("{err:?}")))?,
                )
            } else {
                None
            };

        if let Some(invocation_ref) = invocation.as_mut() {
            invocation_ref.resolution = invocation_resolution_from_toolchain(
                diagnostic.as_ref(),
                dialect.support_level() == "deferred",
                dialect.as_str(),
            );
        }
        Ok(invocation)
    }

    fn toolchain_diagnostic_for_target(
        &self,
        target: &TargetProfile,
    ) -> Option<ToolchainDiagnosticView> {
        let command = toolchain_command_for_kind(&target.kind)?;
        let key = toolchain_cache_key(&target.id, command);
        self.toolchain_diagnostics_by_target
            .get(&key)
            .cloned()
            .or_else(|| Some(self.resolve_toolchain_diagnostic_for_target(target, command)))
    }

    fn refresh_toolchain_diagnostics(&mut self) {
        let mut target_ids = self.profiles.keys().cloned().collect::<Vec<_>>();
        target_ids.sort();
        let mut diagnostics = Vec::new();
        let mut diagnostics_by_target = HashMap::new();
        for target_id in target_ids {
            let Some(profile) = self.profiles.get(&target_id) else {
                continue;
            };
            let Some(command) = toolchain_command_for_kind(&profile.kind) else {
                continue;
            };
            let resolved = self.resolve_toolchain_diagnostic_for_target(profile, command);
            diagnostics_by_target
                .insert(toolchain_cache_key(&profile.id, command), resolved.clone());
            diagnostics.push(resolved);
        }
        self.toolchain_diagnostics = diagnostics;
        self.toolchain_diagnostics_by_target = diagnostics_by_target;
    }

    fn resolve_toolchain_diagnostic_for_target(
        &self,
        profile: &TargetProfile,
        command: &str,
    ) -> ToolchainDiagnosticView {
        let resolution_order = self
            .host_platform_adapter
            .toolchain_locator()
            .resolution_order();
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
        let target_override_allowed = resolution_order.contains(&"target_override");
        let global_override_allowed = resolution_order.contains(&"global_override");
        let target_override = if target_override_allowed {
            target_override_path.as_deref()
        } else {
            None
        };
        let global_override = if global_override_allowed {
            global_override_path.as_deref()
        } else {
            None
        };

        let (resolution, diagnostics, override_scope) = if let Some(path) = target_override {
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
                        .resolve_with_diagnostics(command, global_override.map(Path::new));
                    let scope = match &fallback_resolution {
                        Ok(selected)
                            if selected.source == ExecutableSource::UserOverride
                                && global_override.is_some() =>
                        {
                            Some("global_override")
                        }
                        _ => None,
                    };
                    (fallback_resolution, fallback_diag, scope)
                }
            }
        } else {
            let (fallback_resolution, fallback_diag) = self
                .toolchain_resolver
                .resolve_with_diagnostics(command, global_override.map(Path::new));
            let scope = match &fallback_resolution {
                Ok(selected)
                    if selected.source == ExecutableSource::UserOverride
                        && global_override.is_some() =>
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
        self.host_platform_adapter.runtime_logger().log(
            RuntimeLogLevel::Debug,
            RuntimeLogCategory::Toolchain,
            &format!(
                "resolved toolchain command={} target_id={} scope={:?} source={:?} path={:?}",
                command, profile.id, effective_scope, effective_source, effective_path
            ),
        );

        ToolchainDiagnosticView {
            command: command.to_string(),
            target_id: Some(profile.id.clone()),
            target_name: Some(profile.name.clone()),
            target_terminal_family: Some(terminal_target_family_for(profile).as_str().to_string()),
            target_shell_dialect: Some(target_shell_dialect_for(profile).as_str().to_string()),
            target_terminal_concurrency_policy: Some(
                terminal_concurrency_policy_for(profile)
                    .as_str()
                    .to_string(),
            ),
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
        let platform_snapshot = self.host_platform_adapter.snapshot();
        let vault_status = self
            .vault_router
            .active_backend_diagnostics()
            .ok()
            .map(|diag| vault_readiness_to_capability_status(&diag.status))
            .unwrap_or(platform_snapshot.native_vault_status);
        let runtime_paths = self.host_platform_adapter.runtime_paths().runtime_paths(
            &self.settings_store.settings.core.instance_name,
            Path::new(&self.settings_store.settings.core.data_dir),
        );
        let logs_root = runtime_paths.logs_dir.to_string_lossy().to_string();
        let logs_used_bytes = directory_size_bytes(&runtime_paths.logs_dir);
        let mut toolchain_entries = self
            .settings_store
            .settings
            .toolchains
            .iter()
            .map(
                |(command, section)| bridgingio_app_api::ToolchainSettingsView {
                    command: command.clone(),
                    path_override: section.path_override.clone(),
                    prefer_builtin_fallback: section.prefer_builtin_fallback,
                },
            )
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
            runtime_logs: RuntimeLogSettingsView {
                level: self.settings_store.settings.core.log_level.clone(),
                root: logs_root,
                used_bytes: logs_used_bytes,
            },
            vault: VaultStatusView {
                status: vault_status.as_str().to_string(),
                configured_backend: self.settings_store.settings.vault.backend.clone(),
                binding_backend: self
                    .host_platform_adapter
                    .native_vault_binding()
                    .backend_label()
                    .to_string(),
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
        let (profiles, target_refs, target_descriptors) =
            build_target_runtime_indexes(&self.settings_store.settings)?;
        self.tool_handler.metadata_mut().profiles.clear();
        for profile in profiles.values().cloned() {
            self.tool_handler.metadata_mut().upsert_profile(profile);
        }
        self.profiles = profiles;
        self.target_refs = target_refs;
        self.target_descriptors = target_descriptors;
        self.target_resolution_policy = McpTargetResolutionPolicy::parse(
            &self
                .settings_store
                .settings
                .policies
                .mcp_target_resolution_policy,
        )?;
        Ok(())
    }

    fn upsert_profile_and_persist(
        &mut self,
        profile: TargetProfile,
    ) -> Result<String, CoreRuntimeError> {
        let profile = normalize_profile_credential_ref(profile)?;
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

        if let Some(log_level) = update.core_log_level {
            settings.core.log_level = log_level;
            restart_required = true;
        }
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
            self.host_platform_adapter.as_ref(),
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
                    "launch_strategy": state.launch_strategy,
                    "launch_fallback_applied": state.launch_fallback_applied,
                    "launch_diagnostics": state.launch_diagnostics,
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

        let mut projected_profiles = self.catalog_projected_profiles();
        projected_profiles.sort_by(|a, b| a.id.cmp(&b.id));
        let targets_json = projected_profiles
            .iter()
            .map(|target| {
                let descriptor = self.target_descriptors.get(&target.id);
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
                    "storage_class": descriptor
                        .map(|item| item.storage_class.as_str())
                        .unwrap_or("plain"),
                    "access_class": descriptor
                        .map(|item| item.access_class.as_str())
                        .unwrap_or("anonymous-local"),
                    "sealed_profile_ref": descriptor
                        .and_then(|item| item.sealed_profile_ref.clone()),
                    "catalog_projection_state": target
                        .metadata
                        .get(TARGET_CATALOG_PROJECTION_STATE_METADATA_KEY)
                        .cloned(),
                    "sealed_descriptor_diagnostic": target
                        .metadata
                        .get(TARGET_SEALED_DESCRIPTOR_DIAGNOSTIC_METADATA_KEY)
                        .cloned(),
                    "session_id": session.as_ref().map(|s| s.id.clone()),
                    "session_state": session.as_ref().map(|s| session_state_label(&s.state)),
                    "capability_ids": capabilities,
                })
            })
            .collect::<Vec<_>>();
        let profiles_json = projected_profiles
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
            "host_instance": self.host_instance_probe_payload(),
        });
        let platform_snapshot = self.host_platform_adapter.snapshot();
        let runtime_paths = self.host_platform_adapter.runtime_paths().runtime_paths(
            &self.settings_store.settings.core.instance_name,
            Path::new(&self.settings_store.settings.core.data_dir),
        );
        let control_plane_transport = self.host_platform_adapter.control_plane_transport();
        let control_plane_endpoint_semantics = control_plane_transport.endpoint_semantics(
            &self.settings_store.settings.core.instance_name,
            &runtime_paths,
        );
        let control_plane_lifecycle = control_plane_transport.lifecycle_semantics();
        let control_plane_diagnostics = control_plane_transport
            .diagnostics(
                &self.settings_store.settings.core.instance_name,
                &runtime_paths,
            )
            .into_iter()
            .map(|item| {
                json!({
                    "code": item.code,
                    "status": item.status.as_str(),
                    "message": item.message,
                    "recovery_hint": item.recovery_hint,
                })
            })
            .collect::<Vec<_>>();
        let platform_diagnostics = platform_snapshot
            .diagnostics
            .iter()
            .map(|item| {
                json!({
                    "capability": item.capability,
                    "status": item.status.as_str(),
                    "message": item.message,
                })
            })
            .collect::<Vec<_>>();
        let capability_health = self.capability_health_snapshot_json();

        json!({
            "host_mode": self.host_mode.as_label(),
            "readiness_state": self.readiness_state_label(),
            "model_plane_ready": self.model_plane_ready(),
            "management_plane": {
                "bridge": "trusted_local_control_plane",
                "requires_management_address_handoff": false,
                "managed_restart_states": ["saving", "restart_required", "restarting", "connected", "failed"],
            },
            "capability_health": capability_health,
            "host_platform_adapter": {
                "host_platform": platform_snapshot.host_platform.as_str(),
                "local_shell_runtime": platform_snapshot.local_shell_runtime_status.as_str(),
                "control_plane_transport": platform_snapshot.control_plane_transport_status.as_str(),
                "runtime_paths": platform_snapshot.runtime_paths_status.as_str(),
                "toolchain_locator": platform_snapshot.toolchain_locator_status.as_str(),
                "native_vault_binding": platform_snapshot.native_vault_status.as_str(),
                "runtime_logger": platform_snapshot.runtime_logger_status.as_str(),
                "output_decoder": platform_snapshot.output_decoder_status.as_str(),
                "control_plane_transport_detail": {
                    "kind": control_plane_transport.transport_kind(),
                    "endpoint": control_plane_endpoint_semantics.endpoint,
                    "naming_rule": control_plane_endpoint_semantics.naming_rule,
                    "local_only": control_plane_endpoint_semantics.local_only,
                    "attach_semantics": control_plane_lifecycle.attach,
                    "request_response_semantics": control_plane_lifecycle.request_response,
                    "lifecycle_semantics": control_plane_lifecycle.lifecycle,
                    "diagnostics": control_plane_diagnostics,
                },
                "diagnostics": platform_diagnostics,
            },
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
                    "source_group": {
                        "group_key": entry.source_group.group_key,
                        "group_kind": entry.source_group.group_kind,
                        "group_label": entry.source_group.group_label,
                        "principal_summary": entry.source_group.principal_summary,
                        "user_agent_summary": entry.source_group.user_agent_summary,
                    },
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
                host_id,
                ui_session_id,
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
                    if existing.host_id != host_id {
                        return ApiResponse::OwnershipConflict {
                            request_id: request.request_id,
                            payload_json: self.ownership_conflict_payload_json(
                                &host_id,
                                &ui_session_id,
                                &ui_kind,
                            ),
                        };
                    }
                }

                self.attached_ui = Some(UiAttachment {
                    host_id,
                    ui_session_id,
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
            AppCommand::ProbeHostInstance => ApiResponse::HostInstanceProbe {
                request_id: request.request_id,
                payload_json: self.host_instance_probe_payload().to_string(),
            },
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
                    principal_id: None,
                    agent_id: request.context.agent_id,
                    run_id: request.context.run_id,
                    client_session_id: request.context.client_session_id,
                    reuse_policy: request.context.reuse_policy,
                    timeline_source: None,
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
                let mut items = self.catalog_projected_profiles();
                items.sort_by(|a, b| a.id.cmp(&b.id));
                ApiResponse::Targets {
                    request_id: request.request_id,
                    items,
                }
            }
            AppCommand::ListProfiles => {
                let mut items = self.catalog_projected_profiles();
                items.sort_by(|a, b| a.id.cmp(&b.id));
                ApiResponse::Profiles {
                    request_id: request.request_id,
                    items,
                }
            }
            AppCommand::GetProfile { target_id } => {
                if !self.profiles.contains_key(&target_id) {
                    return error_response(
                        request.request_id,
                        ApiErrorCode::NotFound,
                        "target profile not found",
                    );
                }
                let profile = match self.project_catalog_profile_by_id(&target_id) {
                    Ok(profile) => profile,
                    Err(CoreRuntimeError::Config(message)) => {
                        return error_response(
                            request.request_id,
                            ApiErrorCode::ValidationFailed,
                            &message,
                        )
                    }
                    Err(err) => {
                        return error_response(
                            request.request_id,
                            ApiErrorCode::Internal,
                            &format!("{err:?}"),
                        )
                    }
                };
                ApiResponse::Profile {
                    request_id: request.request_id,
                    payload_json: target_profile_payload_json(&profile),
                }
            }
            AppCommand::UpsertProfile { profile } => {
                match self.upsert_profile_and_persist(profile) {
                    Ok(apply_strategy) => ApiResponse::Accepted {
                        request_id: request.request_id,
                        apply_strategy: Some(apply_strategy),
                    },
                    Err(CoreRuntimeError::Config(message)) => {
                        error_response(request.request_id, ApiErrorCode::ValidationFailed, &message)
                    }
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
                let update = SettingsUpdateRequest {
                    core_log_level,
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
                    Err(CoreRuntimeError::Config(message)) => {
                        error_response(request.request_id, ApiErrorCode::ValidationFailed, &message)
                    }
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
                Err(CoreRuntimeError::Config(message)) => {
                    error_response(request.request_id, ApiErrorCode::ValidationFailed, &message)
                }
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
            AppCommand::CreateAgentToken {
                label,
                expires_in_seconds,
                scope,
                attestation_id,
            } => {
                let actor = control_plane_request_actor(&request.context);
                match self
                    .vault_router
                    .create_agent_token(CreateAgentTokenRequest {
                        label,
                        created_by: actor,
                        expires_in: expires_in_seconds.map(Duration::from_secs),
                        idle_timeout_sec: None,
                        scope: token_scope_input_from_view(scope),
                        attestation_id,
                    }) {
                    Ok(result) => ApiResponse::AgentTokenCreated {
                        request_id: request.request_id,
                        result: ApiCreateAgentTokenResult {
                            plaintext_token: result.plaintext_token,
                            summary: app_agent_token_summary(result.summary),
                        },
                    },
                    Err(err) => token_vault_error_response(request.request_id, err),
                }
            }
            AppCommand::ListAgentTokens => ApiResponse::AgentTokens {
                request_id: request.request_id,
                items: self
                    .vault_router
                    .list_agent_tokens()
                    .into_iter()
                    .map(app_agent_token_summary)
                    .collect(),
            },
            AppCommand::RevokeAgentToken { token_id, reason } => {
                match self.vault_router.revoke_agent_token(&token_id, reason) {
                    Ok(summary) => ApiResponse::AgentTokenRevoked {
                        request_id: request.request_id,
                        summary: app_agent_token_summary(summary),
                    },
                    Err(err) => token_vault_error_response(request.request_id, err),
                }
            }
            AppCommand::UpdateAgentTokenScope {
                token_id,
                scope,
                reason,
                attestation_id,
            } => {
                let actor = control_plane_request_actor(&request.context);
                match self
                    .vault_router
                    .update_agent_token_scope(UpdateAgentTokenScopeRequest {
                        token_id,
                        changed_by: actor,
                        scope: token_scope_input_from_view(scope),
                        reason,
                        attestation_id,
                    }) {
                    Ok(_) => ApiResponse::Accepted {
                        request_id: request.request_id,
                        apply_strategy: Some("live_applied".to_string()),
                    },
                    Err(err) => token_vault_error_response(request.request_id, err),
                }
            }
            AppCommand::CreateLocalAdminIntent {
                action_kind,
                target_object_ref,
                requested_payload_digest,
                ttl_seconds,
            } => {
                let actor = control_plane_request_actor(&request.context);
                let action_kind = match LocalAdminActionKind::parse(&action_kind) {
                    Some(value) => value,
                    None => {
                        return error_response(
                            request.request_id,
                            ApiErrorCode::ValidationFailed,
                            "unsupported local_admin action_kind",
                        )
                    }
                };
                let ttl = Duration::from_secs(ttl_seconds.unwrap_or(300).max(1));
                let created = if let Some(payload_digest) = requested_payload_digest.as_deref() {
                    self.vault_router.create_local_admin_intent_with_digest(
                        action_kind,
                        &target_object_ref,
                        payload_digest,
                        &actor,
                        ttl,
                    )
                } else {
                    self.vault_router.create_local_admin_intent(
                        action_kind,
                        &target_object_ref,
                        &actor,
                        ttl,
                    )
                };
                match created {
                    Ok(intent) => ApiResponse::LocalAdminIntentCreated {
                        request_id: request.request_id,
                        intent: app_local_admin_intent(intent),
                    },
                    Err(err) => token_vault_error_response(request.request_id, err),
                }
            }
            AppCommand::CompleteLocalAdminAttestation {
                intent_id,
                verification_method,
                ttl_seconds,
            } => {
                let actor = control_plane_request_actor(&request.context);
                let ttl = Duration::from_secs(ttl_seconds.unwrap_or(120).max(1));
                match self.vault_router.complete_local_admin_attestation(
                    &intent_id,
                    &actor,
                    &verification_method,
                    ttl,
                ) {
                    Ok(attestation) => ApiResponse::LocalAdminAttestationCompleted {
                        request_id: request.request_id,
                        attestation: app_local_admin_attestation(attestation),
                    },
                    Err(err) => token_vault_error_response(request.request_id, err),
                }
            }
            AppCommand::GetVaultState => match app_vault_state_projection(self) {
                Ok(state) => ApiResponse::VaultState {
                    request_id: request.request_id,
                    state,
                },
                Err(err) => token_vault_error_response(request.request_id, err),
            },
            AppCommand::UnlockVault {
                method,
                passphrase,
                attestation_id,
            } => {
                let actor = control_plane_request_actor(&request.context);
                match self.vault_router.unlock_vault_with_attestation(UnlockVaultRequest {
                    method,
                    passphrase,
                    requested_by: actor,
                    attestation_id,
                }) {
                    Ok(()) => ApiResponse::VaultUnlocked {
                        request_id: request.request_id,
                        lock_state: self.vault_router.vault_lock_state().as_str().to_string(),
                    },
                    Err(err) => token_vault_error_response(request.request_id, err),
                }
            }
            AppCommand::LockVault { reason } => {
                let lock_reason = reason.unwrap_or_else(|| "manual lock".to_string());
                match self.vault_router.lock_vault(&lock_reason) {
                    Ok(()) => ApiResponse::VaultLocked {
                        request_id: request.request_id,
                        lock_state: self.vault_router.vault_lock_state().as_str().to_string(),
                    },
                    Err(err) => token_vault_error_response(request.request_id, err),
                }
            }
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
                let command_preview = self
                    .tool_handler
                    .terminal_provider
                    .command_preview(&command);
                let source_group = Self::local_ui_timeline_source(&request.context);
                let result = self.tool_handler.handle(ToolRequest::TerminalExec {
                    target_id: target.id.clone(),
                    target_kind: target.kind.clone(),
                    context: ToolRequestContext {
                        principal_id: None,
                        agent_id: request.context.agent_id,
                        run_id: request.context.run_id,
                        client_session_id: request.context.client_session_id,
                        reuse_policy: request.context.reuse_policy,
                        timeline_source: Some(source_group.clone()),
                    },
                    command,
                    artifact_id,
                    invocation: None,
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
                            source_group.clone(),
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
                            source_group.clone(),
                            None,
                        );
                        error_response(request.request_id, ApiErrorCode::PermissionDenied, &reason)
                    }
                    ToolResult::Error { message } => {
                        self.push_timeline_entry(
                            session_id,
                            command_preview,
                            "failed",
                            source_group,
                            None,
                        );
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

fn control_plane_request_actor(context: &bridgingio_app_api::ApiRequestContext) -> String {
    format!(
        "control-plane:{}:{}:{}",
        context.agent_id, context.run_id, context.client_session_id
    )
}

fn directory_size_bytes(path: &Path) -> u64 {
    let read_dir = match std::fs::read_dir(path) {
        Ok(value) => value,
        Err(_) => return 0,
    };
    let mut total = 0u64;
    for entry in read_dir.flatten() {
        let file_type = match entry.file_type() {
            Ok(value) => value,
            Err(_) => continue,
        };
        if file_type.is_file() {
            if let Ok(metadata) = entry.metadata() {
                total = total.saturating_add(metadata.len());
            }
            continue;
        }
        if file_type.is_dir() {
            total = total.saturating_add(directory_size_bytes(&entry.path()));
        }
    }
    total
}

fn token_scope_input_from_view(view: AgentTokenScopeView) -> TokenScopeInput {
    TokenScopeInput {
        scope_profile: view.scope_profile,
        target_ids: view.target_ids,
        tool_ids: view.tool_ids,
        max_risk_envelope: view.max_risk_envelope,
        allow_open_shell: view.allow_open_shell,
        allow_write_shell_input: view.allow_write_shell_input,
        allow_artifact_cross_principal: view.allow_artifact_cross_principal,
        allow_delegation: view.allow_delegation,
        allow_admin_actions: view.allow_admin_actions,
    }
}

fn app_agent_token_summary(summary: AgentTokenSummary) -> AgentTokenSummaryView {
    AgentTokenSummaryView {
        token_id: summary.token_id,
        label: summary.label,
        principal_summary: summary.principal_summary,
        status: summary.status.as_str().to_string(),
        scope_profile: summary.scope_profile,
        target_scope_summary: summary.target_scope_summary,
        active_scope_version: summary.active_scope_version,
        created_at: summary.created_at,
        last_used_at: summary.last_used_at,
        expires_at: summary.expires_at,
        revoked_at: summary.revoked_at,
        revoke_reason: summary.revoke_reason,
    }
}

fn app_local_admin_intent(intent: LocalAdminActionIntent) -> LocalAdminActionIntentView {
    LocalAdminActionIntentView {
        intent_id: intent.intent_id,
        action_kind: intent.action_kind.as_str().to_string(),
        target_object_ref: intent.target_object_ref,
        requested_by_principal: intent.requested_by_principal,
        requested_payload_digest: intent.requested_payload_digest,
        created_at: intent.created_at,
        expires_at: intent.expires_at,
        status: format!("{:?}", intent.status).to_ascii_lowercase(),
    }
}

fn app_local_admin_attestation(
    attestation: LocalAdminAttestationRecord,
) -> LocalAdminAttestationView {
    LocalAdminAttestationView {
        attestation_id: attestation.attestation_id,
        intent_id: attestation.intent_id,
        verified_principal: attestation.verified_principal,
        verification_method: attestation.verification_method,
        issued_at: attestation.issued_at,
        expires_at: attestation.expires_at,
        consumed_at: attestation.consumed_at,
        status: format!("{:?}", attestation.status).to_ascii_lowercase(),
    }
}

fn unlock_trigger_policy_label(policy: &VaultUnlockTriggerPolicy) -> &'static str {
    match policy {
        VaultUnlockTriggerPolicy::OnCoreStart => "on-core-start",
        VaultUnlockTriggerPolicy::OnFirstSecretAccess => "on-first-secret-access",
        VaultUnlockTriggerPolicy::OnEverySecretAccess => "on-every-secret-access",
        VaultUnlockTriggerPolicy::ManualOnly => "manual-only",
    }
}

fn app_vault_state_projection(
    runtime: &StandaloneCoreRuntime,
) -> Result<VaultStateProjectionView, VaultError> {
    let protector = runtime.vault_router.protector_assembly()?;
    let unlock_policy = runtime.vault_router.unlock_policy().clone();
    let secrets = runtime
        .vault_router
        .list_secret_summaries()
        .into_iter()
        .map(|item| VaultSecretSummaryView {
            reference: item.reference,
            kind: item.kind,
            label: item.label,
            status: item.status.as_str().to_string(),
            active_version_id: item.active_version_id,
        })
        .collect::<Vec<_>>();
    Ok(VaultStateProjectionView {
        lock_state: runtime.vault_router.vault_lock_state().as_str().to_string(),
        configured_backend: runtime.settings_store.settings.vault.backend.clone(),
        active_backend: runtime.vault_router.active_backend().to_string(),
        protector_summary: VaultProtectorSummaryView {
            primary: protector.primary,
            recovery: protector.recovery,
            retired: protector.retired,
            fail_closed: protector.fail_closed,
        },
        unlock_policy_summary: VaultUnlockPolicySummaryView {
            trigger_policy: unlock_trigger_policy_label(&unlock_policy.trigger_policy).to_string(),
            allowed_methods: unlock_policy.allowed_methods,
            preferred_method: unlock_policy.preferred_method,
            cache_ttl_sec: unlock_policy.cache_ttl_sec,
            require_fresh_user_verification: unlock_policy.require_fresh_user_verification,
        },
        secrets,
    })
}

fn token_vault_error_response(request_id: String, err: VaultError) -> ApiResponse {
    shared_error_response(request_id, err.shared_error())
}

fn error_response(request_id: String, code: ApiErrorCode, message: &str) -> ApiResponse {
    let status = match code {
        CommonErrorCode::MethodNotImplemented => ContractStatus::MethodNotImplemented,
        CommonErrorCode::NotReady => ContractStatus::NotReady,
        CommonErrorCode::Unsupported => ContractStatus::Unsupported,
        CommonErrorCode::Degraded => ContractStatus::Degraded,
        CommonErrorCode::Locked | CommonErrorCode::VerificationRequired => ContractStatus::Locked,
        CommonErrorCode::DependencyUnavailable => ContractStatus::NotReady,
        _ => ContractStatus::Failed,
    };
    shared_error_response(
        request_id,
        SharedError::new(status, ErrorDomain::ControlPlane, code, message),
    )
}

fn shared_error_response(request_id: String, error: SharedError) -> ApiResponse {
    ApiResponse::Error {
        request_id,
        error: ApiError::from_shared(error),
    }
}

fn normalize_target_ref(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_target_ref_relaxed(value: &str) -> String {
    normalize_target_ref(value)
        .chars()
        .filter(|ch| !matches!(*ch, '-' | '_') && !ch.is_whitespace())
        .collect::<String>()
}

fn is_family_related_reference(input_normalized: &str, reference: &str) -> bool {
    if input_normalized.is_empty() {
        return false;
    }
    let normalized_reference = normalize_target_ref(reference);
    if normalized_reference.len() <= input_normalized.len() {
        return false;
    }
    if !normalized_reference.starts_with(input_normalized) {
        return false;
    }
    let mut tail = normalized_reference[input_normalized.len()..].chars();
    let Some(boundary) = tail.next() else {
        return false;
    };
    if !matches!(boundary, '-' | '_' | ' ') {
        return false;
    }
    tail.any(|ch| !matches!(ch, '-' | '_' | ' '))
}

fn is_typo_related_reference(
    input_normalized: &str,
    input_relaxed: &str,
    reference: &str,
    is_exact_match: bool,
) -> bool {
    if is_exact_match {
        return false;
    }
    let normalized_reference = normalize_target_ref(reference);
    if normalized_reference.is_empty() || normalized_reference == input_normalized {
        return false;
    }
    let relaxed_reference = normalize_target_ref_relaxed(reference);
    if relaxed_reference.is_empty() || relaxed_reference == input_relaxed {
        return false;
    }
    let max_distance = if input_relaxed.len() <= 4 { 1 } else { 2 };
    levenshtein_with_limit(input_relaxed, &relaxed_reference, max_distance).is_some()
}

fn levenshtein_with_limit(left: &str, right: &str, max_distance: usize) -> Option<usize> {
    if left == right {
        return Some(0);
    }
    if left.is_empty() {
        return (right.chars().count() <= max_distance).then_some(right.chars().count());
    }
    if right.is_empty() {
        return (left.chars().count() <= max_distance).then_some(left.chars().count());
    }

    let left_chars = left.chars().collect::<Vec<_>>();
    let right_chars = right.chars().collect::<Vec<_>>();
    if left_chars.len().abs_diff(right_chars.len()) > max_distance {
        return None;
    }

    let mut prev_row = (0..=right_chars.len()).collect::<Vec<_>>();
    let mut current_row = vec![0usize; right_chars.len() + 1];

    for (i, left_char) in left_chars.iter().enumerate() {
        current_row[0] = i + 1;
        let mut row_min = current_row[0];
        for (j, right_char) in right_chars.iter().enumerate() {
            let substitution_cost = if left_char == right_char { 0 } else { 1 };
            let deletion = prev_row[j + 1] + 1;
            let insertion = current_row[j] + 1;
            let substitution = prev_row[j] + substitution_cost;
            let cost = deletion.min(insertion).min(substitution);
            current_row[j + 1] = cost;
            row_min = row_min.min(cost);
        }
        if row_min > max_distance {
            return None;
        }
        std::mem::swap(&mut prev_row, &mut current_row);
    }

    let distance = prev_row[right_chars.len()];
    (distance <= max_distance).then_some(distance)
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

fn target_connection_summary(configured: &StandaloneTargetProfile) -> String {
    match configured.kind {
        TargetKind::Ssh => {
            let host = configured
                .connection
                .host
                .as_deref()
                .unwrap_or("<missing-host>");
            let port = configured.connection.port.unwrap_or(22);
            let username = configured
                .connection
                .username
                .as_deref()
                .unwrap_or("<missing-user>");
            format!("ssh://{username}@{host}:{port}")
        }
        TargetKind::Adb => {
            let serial = configured
                .connection
                .selector_value
                .as_deref()
                .unwrap_or("<auto>");
            let selector_kind = configured
                .connection
                .selector_kind
                .as_deref()
                .unwrap_or("serial");
            format!("adb({selector_kind}={serial})")
        }
        TargetKind::Serial => {
            let device = configured
                .connection
                .selector_value
                .as_deref()
                .unwrap_or("/dev/tty.usbmodem0");
            let baud_rate = configured
                .connection
                .selector_kind
                .as_deref()
                .unwrap_or("115200");
            format!("serial({device}@{baud_rate})")
        }
        TargetKind::Docker => {
            let container = configured
                .connection
                .selector_value
                .as_deref()
                .unwrap_or("container");
            let context = configured
                .connection
                .selector_kind
                .as_deref()
                .unwrap_or("default");
            format!("docker({container}, context={context})")
        }
        TargetKind::Other(ref value) => format!("custom({value})"),
    }
}

fn target_connection_summary_from_profile(profile: &TargetProfile) -> String {
    match &profile.connection {
        ConnectionConfig::Ssh {
            host,
            port,
            username,
        } => format!("ssh://{username}@{host}:{port}"),
        ConnectionConfig::Adb { serial, transport } => {
            let serial = serial.as_deref().unwrap_or("<auto>");
            let transport = transport.as_deref().unwrap_or("serial");
            format!("adb({transport}={serial})")
        }
        ConnectionConfig::Serial { device, baud_rate } => format!("serial({device}@{baud_rate})"),
        ConnectionConfig::Docker { container, context } => format!(
            "docker({container}, context={})",
            context.as_deref().unwrap_or("default")
        ),
        ConnectionConfig::Custom { description } => format!("custom({description})"),
    }
}

fn apply_target_catalog_projection_metadata(
    profile: &mut TargetProfile,
    projection_state: &str,
    diagnostic: Option<&str>,
) {
    profile.metadata.insert(
        TARGET_CATALOG_PROJECTION_STATE_METADATA_KEY.to_string(),
        projection_state.to_string(),
    );
    match diagnostic.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => {
            profile.metadata.insert(
                TARGET_SEALED_DESCRIPTOR_DIAGNOSTIC_METADATA_KEY.to_string(),
                value.to_string(),
            );
        }
        None => {
            profile
                .metadata
                .remove(TARGET_SEALED_DESCRIPTOR_DIAGNOSTIC_METADATA_KEY);
        }
    }
}

fn redacted_sealed_catalog_profile(
    profile: &TargetProfile,
    projection_state: &str,
    diagnostic: Option<&str>,
) -> TargetProfile {
    let mut redacted = profile.clone();
    redacted.notes = None;
    redacted.credential_ref = None;
    redacted.connection = match &profile.connection {
        ConnectionConfig::Ssh { .. } => ConnectionConfig::Ssh {
            host: "<sealed-redacted>".to_string(),
            port: 0,
            username: "<sealed-redacted>".to_string(),
        },
        ConnectionConfig::Adb { .. } => ConnectionConfig::Adb {
            serial: None,
            transport: None,
        },
        ConnectionConfig::Serial { .. } => ConnectionConfig::Serial {
            device: "<sealed-redacted>".to_string(),
            baud_rate: 0,
        },
        ConnectionConfig::Docker { .. } => ConnectionConfig::Docker {
            container: "<sealed-redacted>".to_string(),
            context: None,
        },
        ConnectionConfig::Custom { .. } => ConnectionConfig::Custom {
            description: "sealed target descriptor redacted".to_string(),
        },
    };
    apply_target_catalog_projection_metadata(&mut redacted, projection_state, diagnostic);
    redacted
}

fn apply_sealed_overlay_to_target_profile(
    profile: &mut TargetProfile,
    overlay: &Value,
) -> Result<(), CoreRuntimeError> {
    let map = overlay.as_object().ok_or_else(|| {
        CoreRuntimeError::Config("sealed overlay payload must be a JSON object".into())
    })?;

    if let Some(notes_value) = map.get("notes") {
        profile.notes = match notes_value {
            Value::Null => None,
            Value::String(value) => Some(value.clone()),
            _ => {
                return Err(CoreRuntimeError::Config(
                    "sealed overlay field `notes` must be string or null".into(),
                ));
            }
        };
    }

    if let Some(credential_value) = map.get("credential_ref") {
        profile.credential_ref = match credential_value {
            Value::Null => None,
            Value::String(raw_ref) => {
                let normalized = normalize_credential_ref(raw_ref).map_err(vault_error_to_runtime)?;
                Some(CredentialRef {
                    id: normalized,
                    provider: "vault".to_string(),
                })
            }
            Value::Object(obj) => {
                let id = obj
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        CoreRuntimeError::Config(
                            "sealed overlay credential_ref object requires non-empty `id`".into(),
                        )
                    })?;
                let provider = obj
                    .get("provider")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("vault")
                    .to_string();
                let normalized = normalize_credential_ref(id).map_err(vault_error_to_runtime)?;
                Some(CredentialRef {
                    id: normalized,
                    provider,
                })
            }
            _ => {
                return Err(CoreRuntimeError::Config(
                    "sealed overlay field `credential_ref` must be string, object, or null".into(),
                ));
            }
        };
    }

    if let Some(connection_value) = map.get("connection") {
        let connection = connection_value.as_object().ok_or_else(|| {
            CoreRuntimeError::Config("sealed overlay field `connection` must be a JSON object".into())
        })?;
        match &mut profile.connection {
            ConnectionConfig::Ssh {
                host,
                port,
                username,
            } => {
                if let Some(value) = connection.get("host") {
                    let host_value = value.as_str().ok_or_else(|| {
                        CoreRuntimeError::Config(
                            "sealed overlay ssh connection.host must be string".into(),
                        )
                    })?;
                    *host = host_value.to_string();
                }
                if let Some(value) = connection.get("port") {
                    let port_value = value.as_u64().ok_or_else(|| {
                        CoreRuntimeError::Config(
                            "sealed overlay ssh connection.port must be unsigned integer".into(),
                        )
                    })?;
                    if port_value > u16::MAX as u64 {
                        return Err(CoreRuntimeError::Config(
                            "sealed overlay ssh connection.port exceeds u16 range".into(),
                        ));
                    }
                    *port = port_value as u16;
                }
                if let Some(value) = connection.get("username") {
                    let username_value = value.as_str().ok_or_else(|| {
                        CoreRuntimeError::Config(
                            "sealed overlay ssh connection.username must be string".into(),
                        )
                    })?;
                    *username = username_value.to_string();
                }
            }
            ConnectionConfig::Adb { serial, transport } => {
                if let Some(value) = connection.get("serial") {
                    *serial = match value {
                        Value::Null => None,
                        Value::String(raw) => Some(raw.clone()),
                        _ => {
                            return Err(CoreRuntimeError::Config(
                                "sealed overlay adb connection.serial must be string or null"
                                    .into(),
                            ));
                        }
                    };
                }
                if let Some(value) = connection.get("transport") {
                    *transport = match value {
                        Value::Null => None,
                        Value::String(raw) => Some(raw.clone()),
                        _ => {
                            return Err(CoreRuntimeError::Config(
                                "sealed overlay adb connection.transport must be string or null"
                                    .into(),
                            ));
                        }
                    };
                }
            }
            ConnectionConfig::Serial { device, baud_rate } => {
                if let Some(value) = connection.get("device") {
                    let device_value = value.as_str().ok_or_else(|| {
                        CoreRuntimeError::Config(
                            "sealed overlay serial connection.device must be string".into(),
                        )
                    })?;
                    *device = device_value.to_string();
                }
                if let Some(value) = connection.get("baud_rate") {
                    let baud_rate_value = value.as_u64().ok_or_else(|| {
                        CoreRuntimeError::Config(
                            "sealed overlay serial connection.baud_rate must be unsigned integer"
                                .into(),
                        )
                    })?;
                    if baud_rate_value > u32::MAX as u64 {
                        return Err(CoreRuntimeError::Config(
                            "sealed overlay serial connection.baud_rate exceeds u32 range".into(),
                        ));
                    }
                    *baud_rate = baud_rate_value as u32;
                }
            }
            ConnectionConfig::Docker { container, context } => {
                if let Some(value) = connection.get("container") {
                    let container_value = value.as_str().ok_or_else(|| {
                        CoreRuntimeError::Config(
                            "sealed overlay docker connection.container must be string".into(),
                        )
                    })?;
                    *container = container_value.to_string();
                }
                if let Some(value) = connection.get("context") {
                    *context = match value {
                        Value::Null => None,
                        Value::String(raw) => Some(raw.clone()),
                        _ => {
                            return Err(CoreRuntimeError::Config(
                                "sealed overlay docker connection.context must be string or null"
                                    .into(),
                            ));
                        }
                    };
                }
            }
            ConnectionConfig::Custom { description } => {
                if let Some(value) = connection.get("description") {
                    let description_value = value.as_str().ok_or_else(|| {
                        CoreRuntimeError::Config(
                            "sealed overlay custom connection.description must be string".into(),
                        )
                    })?;
                    *description = description_value.to_string();
                }
            }
        }
    }

    if let Some(toolchains_value) = map.get("toolchains") {
        let toolchains = toolchains_value.as_object().ok_or_else(|| {
            CoreRuntimeError::Config("sealed overlay field `toolchains` must be a JSON object".into())
        })?;
        for (command, value) in toolchains {
            match value {
                Value::Null => {
                    profile.toolchains.remove(command);
                }
                Value::String(path_override) => {
                    if path_override.trim().is_empty() {
                        profile.toolchains.remove(command);
                    } else {
                        profile
                            .toolchains
                            .insert(command.clone(), path_override.clone());
                    }
                }
                _ => {
                    return Err(CoreRuntimeError::Config(
                        "sealed overlay toolchains values must be string or null".into(),
                    ));
                }
            }
        }
    }

    Ok(())
}

fn public_descriptor_digest_for_target_descriptor(descriptor: &McpTargetDescriptor) -> String {
    let payload = json!({
        "target_id": descriptor.target_id,
        "enabled": descriptor.enabled,
        "display_name": descriptor.display_name,
        "kind": target_kind_label(&descriptor.kind),
        "aliases": descriptor.aliases,
        "notes": descriptor.notes,
        "connection_summary": descriptor.connection_summary,
        "storage_class": descriptor.storage_class.as_str(),
        "access_class": descriptor.access_class.as_str(),
        "sealed_profile_ref": descriptor.sealed_profile_ref,
    });
    let mut hasher = Sha256::new();
    hasher.update(payload.to_string().as_bytes());
    hex_encode_lower(&hasher.finalize())
}

fn hex_encode_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for value in bytes {
        out.push(HEX[(value >> 4) as usize] as char);
        out.push(HEX[(value & 0x0f) as usize] as char);
    }
    out
}

fn parse_target_storage_class_or_default(raw: &str) -> TargetStorageClass {
    TargetStorageClass::parse(raw).unwrap_or(TargetStorageClass::Plain)
}

fn parse_target_access_class_or_default(raw: &str) -> TargetAccessClass {
    TargetAccessClass::parse(raw).unwrap_or(TargetAccessClass::AnonymousLocal)
}

fn mcp_target_descriptor_from_config(configured: &StandaloneTargetProfile) -> McpTargetDescriptor {
    let storage_class = parse_target_storage_class_or_default(&configured.storage_class);
    let access_class = parse_target_access_class_or_default(&configured.access_class);
    McpTargetDescriptor {
        target_id: configured.id.clone(),
        enabled: configured.enabled,
        display_name: configured.display_name.clone(),
        kind: configured.kind.clone(),
        aliases: configured.aliases.clone(),
        notes: configured.notes.clone(),
        connection_summary: target_connection_summary(configured),
        storage_class,
        access_class,
        sealed_profile_ref: configured
            .sealed_profile_ref
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
    }
}

fn build_target_runtime_indexes(
    settings: &CoreSettings,
) -> Result<
    (
        HashMap<String, TargetProfile>,
        HashMap<String, String>,
        HashMap<String, McpTargetDescriptor>,
    ),
    CoreRuntimeError,
> {
    let mut profiles = HashMap::new();
    let mut target_refs = HashMap::new();
    let mut target_descriptors = HashMap::new();
    for configured in &settings.targets {
        let profile = to_target_profile(configured)?;
        let target_id = profile.id.clone();
        let descriptor = mcp_target_descriptor_from_config(configured);
        if descriptor.enabled {
            register_target_ref(&mut target_refs, &target_id, &target_id)?;
            register_target_ref(&mut target_refs, &descriptor.display_name, &target_id)?;
            for alias in &descriptor.aliases {
                register_target_ref(&mut target_refs, alias, &target_id)?;
            }
        }
        profiles.insert(target_id.clone(), profile);
        target_descriptors.insert(target_id, descriptor);
    }
    Ok((profiles, target_refs, target_descriptors))
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

fn terminal_connector_for_target(
    target: &TargetProfile,
    diagnostic: &Option<ToolchainDiagnosticView>,
) -> Option<Box<dyn TerminalConnector>> {
    match &target.kind {
        TargetKind::Ssh => {
            let executable = diagnostic
                .as_ref()
                .and_then(|value| value.effective_path.as_deref())
                .unwrap_or("ssh");
            Some(Box::new(SshConnector::new(PathBuf::from(executable))))
        }
        TargetKind::Adb => {
            let executable = diagnostic
                .as_ref()
                .and_then(|value| value.effective_path.as_deref())
                .unwrap_or("adb");
            Some(Box::new(AdbConnector::new(PathBuf::from(executable))))
        }
        _ => None,
    }
}

fn toolchain_cache_key(target_id: &str, command: &str) -> String {
    format!("{target_id}:{command}")
}

fn vault_readiness_to_capability_status(status: &VaultReadinessState) -> CapabilityStatus {
    match status {
        VaultReadinessState::Ready => CapabilityStatus::Ready,
        VaultReadinessState::Degraded => CapabilityStatus::Degraded,
        VaultReadinessState::Fallback => CapabilityStatus::Fallback,
        VaultReadinessState::Unsupported => CapabilityStatus::Unsupported,
    }
}

fn vault_unlock_policy_from_settings(settings: &CoreSettings) -> Result<VaultUnlockPolicy, CoreRuntimeError> {
    let trigger_policy = match settings
        .vault
        .unlock
        .trigger_policy
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "on-core-start" => VaultUnlockTriggerPolicy::OnCoreStart,
        "on-first-secret-access" => VaultUnlockTriggerPolicy::OnFirstSecretAccess,
        "on-every-secret-access" => VaultUnlockTriggerPolicy::OnEverySecretAccess,
        "manual-only" => VaultUnlockTriggerPolicy::ManualOnly,
        other => {
            return Err(CoreRuntimeError::Config(format!(
                "unsupported vault unlock trigger policy: {other}"
            )))
        }
    };
    Ok(VaultUnlockPolicy {
        trigger_policy,
        allowed_methods: settings.vault.unlock.allowed_methods.clone(),
        preferred_method: settings.vault.unlock.preferred_method.clone(),
        cache_ttl_sec: settings.vault.unlock.cache_ttl_sec,
        require_fresh_user_verification: settings.vault.unlock.require_fresh_user_verification,
    })
}

fn aggregate_health_state(statuses: &[CapabilityStatus]) -> &'static str {
    if statuses
        .iter()
        .any(|status| *status == CapabilityStatus::Unsupported)
    {
        "unsupported"
    } else if statuses.iter().any(|status| {
        *status == CapabilityStatus::Degraded || *status == CapabilityStatus::Fallback
    }) {
        "degraded"
    } else {
        "ready"
    }
}

fn runtime_log_category_labels() -> Vec<&'static str> {
    vec![
        RuntimeLogCategory::Startup.as_str(),
        RuntimeLogCategory::Transport.as_str(),
        RuntimeLogCategory::Terminal.as_str(),
        RuntimeLogCategory::Toolchain.as_str(),
        RuntimeLogCategory::Vault.as_str(),
        RuntimeLogCategory::Decode.as_str(),
        RuntimeLogCategory::Policy.as_str(),
    ]
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

fn invocation_resolution_from_toolchain(
    diagnostic: Option<&ToolchainDiagnosticView>,
    dialect_deferred: bool,
    dialect: &str,
) -> InvocationResolution {
    let mut warnings = diagnostic
        .map(|value| {
            if value.effective_path.is_none() {
                vec!["no resolved executable path; connector default is used".to_string()]
            } else {
                Vec::new()
            }
        })
        .unwrap_or_default();
    if dialect_deferred {
        warnings.push(format!(
            "target shell dialect `{dialect}` is deferred; connector invocation remains baseline-compatible"
        ));
    }

    InvocationResolution {
        target_override_path: diagnostic.and_then(|value| value.target_override_path.clone()),
        global_override_path: diagnostic.and_then(|value| value.global_override_path.clone()),
        effective_scope: diagnostic.and_then(|value| value.effective_scope.clone()),
        effective_source: diagnostic.and_then(|value| value.effective_source.clone()),
        effective_path: diagnostic.and_then(|value| value.effective_path.clone()),
        warnings,
    }
}

fn apply_ssh_delivery_args(invocation: &mut CommandInvocation, option_args: &[String]) {
    if option_args.is_empty() {
        return;
    }
    let insertion_index = match invocation.invocation_kind {
        InvocationKind::OneShot if invocation.args.len() >= 2 => invocation.args.len() - 2,
        InvocationKind::Interactive if !invocation.args.is_empty() => invocation.args.len() - 1,
        _ => invocation.args.len(),
    };
    let mut index = insertion_index;
    for value in option_args {
        invocation.args.insert(index, value.clone());
        index += 1;
    }
}

fn parse_bool_metadata(raw: &str, default: bool) -> bool {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        _ => default,
    }
}

fn ssh_host_key_policy_for_target(target: &TargetProfile) -> SshHostKeyPolicy {
    let Some(raw) = target.metadata.get(TARGET_SSH_HOST_KEY_POLICY_METADATA_KEY) else {
        return SshHostKeyPolicy::Strict;
    };
    match raw.trim().to_ascii_lowercase().as_str() {
        "accept-new" | "accept_new" => SshHostKeyPolicy::AcceptNew,
        "insecure-no-check" | "insecure_no_check" | "no-check" => SshHostKeyPolicy::InsecureNoCheck,
        _ => SshHostKeyPolicy::Strict,
    }
}

fn runtime_host_platform_label(adapter: &dyn HostPlatformAdapter) -> String {
    match adapter.host_platform().as_str() {
        "windows" => "windows".into(),
        "unix" => std::env::consts::OS.to_string(),
        _ => "unknown".into(),
    }
}

fn invocation_json(invocation: Option<&CommandInvocation>) -> Value {
    let Some(invocation) = invocation else {
        return Value::Null;
    };
    let view = invocation.diagnostics_view();
    json!({
        "mode": view.mode,
        "program": view.program,
        "args": view.args,
        "target_terminal": {
            "family": view.target_terminal_family,
            "concurrency_policy": view.target_terminal_concurrency_policy
        },
        "target_shell_dialect": {
            "name": view.target_shell_dialect,
            "support": view.dialect_support
        },
        "toolchain_resolution": {
            "target_override_path": view.target_override_path,
            "global_override_path": view.global_override_path,
            "effective_scope": view.effective_scope,
            "effective_source": view.effective_source,
            "effective_path": view.effective_path
        },
        "warnings": view.warnings,
        "quoting_boundary": {
            "host_shell_runtime": view.quoting_host_shell_runtime,
            "target_shell_dialect": view.quoting_target_shell_dialect
        }
    })
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
    host_platform_adapter: &dyn HostPlatformAdapter,
) -> Result<ArtifactStoreConfig, CoreRuntimeError> {
    let backend = ArtifactCacheBackend::parse(&settings.storage.artifacts.backend)
        .map_err(|err| CoreRuntimeError::Config(err.message))?;
    let eviction_policy =
        ArtifactEvictionPolicy::parse(&settings.storage.artifacts.eviction_policy)
            .map_err(|err| CoreRuntimeError::Config(err.message))?;
    Ok(ArtifactStoreConfig {
        backend,
        root: host_platform_adapter
            .runtime_paths()
            .expand_user_path(&settings.storage.artifacts.root),
        max_bytes: settings.storage.artifacts.max_bytes,
        eviction_policy,
    })
}

fn resolve_runtime_path_defaults(
    settings: &mut CoreSettings,
    host_platform_adapter: &dyn HostPlatformAdapter,
) -> Result<(), CoreRuntimeError> {
    let runtime_paths_adapter = host_platform_adapter.runtime_paths();
    let data_dir = if is_auto_value(&settings.core.data_dir) {
        runtime_paths_adapter.default_data_dir(&settings.core.instance_name)
    } else {
        runtime_paths_adapter.expand_user_path(&settings.core.data_dir)
    };
    if data_dir.as_os_str().is_empty() {
        return Err(CoreRuntimeError::Config(
            "core.data_dir resolved to an empty path".into(),
        ));
    }

    let runtime_paths =
        runtime_paths_adapter.runtime_paths(&settings.core.instance_name, &data_dir);
    settings.core.data_dir = data_dir.to_string_lossy().to_string();
    settings.storage.metadata_path = resolve_path_with_host_default(
        &settings.storage.metadata_path,
        &runtime_paths.metadata_path,
        runtime_paths_adapter,
    );
    settings.storage.artifacts.root = resolve_path_with_host_default(
        &settings.storage.artifacts.root,
        &runtime_paths.artifact_root,
        runtime_paths_adapter,
    );
    if settings.control_plane.transport == "platform-ipc" {
        settings.control_plane.endpoint = if is_auto_value(&settings.control_plane.endpoint) {
            host_platform_adapter
                .control_plane_transport()
                .endpoint(&settings.core.instance_name, &runtime_paths)
        } else if looks_like_named_pipe_endpoint(&settings.control_plane.endpoint) {
            settings.control_plane.endpoint.trim().to_string()
        } else {
            runtime_paths_adapter
                .expand_user_path(&settings.control_plane.endpoint)
                .to_string_lossy()
                .to_string()
        };
    }
    Ok(())
}

fn resolve_path_with_host_default(
    raw: &str,
    host_default: &Path,
    runtime_paths_adapter: &dyn bridgingio_platform::RuntimePathsAdapter,
) -> String {
    if is_auto_value(raw) {
        host_default.to_string_lossy().to_string()
    } else {
        runtime_paths_adapter
            .expand_user_path(raw)
            .to_string_lossy()
            .to_string()
    }
}

fn is_auto_value(raw: &str) -> bool {
    raw.trim().eq_ignore_ascii_case("auto")
}

fn looks_like_named_pipe_endpoint(endpoint: &str) -> bool {
    endpoint.trim().starts_with(r"\\.\pipe\")
}

fn normalize_profile_credential_ref(
    mut profile: TargetProfile,
) -> Result<TargetProfile, CoreRuntimeError> {
    if let Some(credential_ref) = profile.credential_ref.as_mut() {
        credential_ref.id =
            normalize_credential_ref(&credential_ref.id).map_err(vault_error_to_runtime)?;
        credential_ref.provider = "vault".into();
    }
    Ok(profile)
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
    if let Some(shell) = configured
        .terminal_provider
        .shell
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        metadata.insert(
            TARGET_TERMINAL_SHELL_METADATA_KEY.to_string(),
            shell.to_string(),
        );
    }
    if let Some(family) = configured
        .terminal
        .family
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        metadata.insert(
            TARGET_TERMINAL_FAMILY_METADATA_KEY.to_string(),
            family.to_string(),
        );
    }
    if let Some(policy) = configured
        .terminal
        .concurrency_policy
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        metadata.insert(
            TARGET_TERMINAL_CONCURRENCY_METADATA_KEY.to_string(),
            policy.to_string(),
        );
    }
    let storage_class = parse_target_storage_class_or_default(&configured.storage_class);
    metadata.insert(
        TARGET_STORAGE_CLASS_METADATA_KEY.to_string(),
        storage_class.as_str().to_string(),
    );
    let access_class = parse_target_access_class_or_default(&configured.access_class);
    metadata.insert(
        TARGET_ACCESS_CLASS_METADATA_KEY.to_string(),
        access_class.as_str().to_string(),
    );
    if let Some(sealed_profile_ref) = configured
        .sealed_profile_ref
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        metadata.insert(
            TARGET_SEALED_PROFILE_REF_METADATA_KEY.to_string(),
            sealed_profile_ref.to_string(),
        );
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
    let credential_ref = configured
        .credential_ref
        .as_ref()
        .map(|reference| {
            normalize_credential_ref(reference).map(|normalized| bridgingio_domain::CredentialRef {
                id: normalized,
                provider: "vault".into(),
            })
        })
        .transpose()
        .map_err(vault_error_to_runtime)?;
    Ok(TargetProfile {
        id: configured.id.clone(),
        name: configured.display_name.clone(),
        kind: configured.kind.clone(),
        connection,
        credential_ref,
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

fn to_serial_connection(
    connection: &StandaloneConnectionSection,
) -> bridgingio_domain::ConnectionConfig {
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

fn to_docker_connection(
    connection: &StandaloneConnectionSection,
) -> bridgingio_domain::ConnectionConfig {
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
        storage_class: profile
            .metadata
            .get(TARGET_STORAGE_CLASS_METADATA_KEY)
            .and_then(|value| TargetStorageClass::parse(value))
            .unwrap_or(TargetStorageClass::Plain)
            .as_str()
            .to_string(),
        access_class: profile
            .metadata
            .get(TARGET_ACCESS_CLASS_METADATA_KEY)
            .and_then(|value| TargetAccessClass::parse(value))
            .unwrap_or(TargetAccessClass::AnonymousLocal)
            .as_str()
            .to_string(),
        sealed_profile_ref: profile
            .metadata
            .get(TARGET_SEALED_PROFILE_REF_METADATA_KEY)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        credential_ref: profile
            .credential_ref
            .as_ref()
            .map(|value| normalize_credential_ref(&value.id))
            .transpose()
            .map_err(vault_error_to_runtime)?,
        notes: profile.notes.clone(),
        connection,
        terminal: StandaloneTerminalSection {
            family: Some(terminal_target_family_for(profile).as_str().to_string()),
            concurrency_policy: Some(
                terminal_concurrency_policy_for(profile)
                    .as_str()
                    .to_string(),
            ),
        },
        toolchains,
        terminal_provider: bridgingio_engine::TerminalProviderSection {
            enabled: false,
            shell: profile
                .metadata
                .get(TARGET_TERMINAL_SHELL_METADATA_KEY)
                .map(|value| value.to_string()),
        },
        git_repositories: Vec::new(),
    })
}

fn target_profile_payload_json(profile: &TargetProfile) -> String {
    target_profile_json_value(profile).to_string()
}

fn target_profile_json_value(profile: &TargetProfile) -> Value {
    let alias = profile.metadata.get("alias").cloned().unwrap_or_default();
    let target_shell = profile
        .metadata
        .get(TARGET_TERMINAL_SHELL_METADATA_KEY)
        .cloned();
    let target_family = terminal_target_family_for(profile).as_str().to_string();
    let target_concurrency = terminal_concurrency_policy_for(profile)
        .as_str()
        .to_string();
    let target_dialect = target_shell_dialect_for(profile);
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
        "storage_class": profile
            .metadata
            .get(TARGET_STORAGE_CLASS_METADATA_KEY)
            .and_then(|value| TargetStorageClass::parse(value))
            .unwrap_or(TargetStorageClass::Plain)
            .as_str(),
        "access_class": profile
            .metadata
            .get(TARGET_ACCESS_CLASS_METADATA_KEY)
            .and_then(|value| TargetAccessClass::parse(value))
            .unwrap_or(TargetAccessClass::AnonymousLocal)
            .as_str(),
        "sealed_profile_ref": profile
            .metadata
            .get(TARGET_SEALED_PROFILE_REF_METADATA_KEY)
            .map(|value| value.to_string()),
        "catalog_projection_state": profile
            .metadata
            .get(TARGET_CATALOG_PROJECTION_STATE_METADATA_KEY)
            .cloned(),
        "sealed_descriptor_diagnostic": profile
            .metadata
            .get(TARGET_SEALED_DESCRIPTOR_DIAGNOSTIC_METADATA_KEY)
            .cloned(),
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
        "terminal_provider": {
            "shell": target_shell,
            "target_terminal_family": target_family,
            "target_terminal_concurrency_policy": target_concurrency,
            "target_shell_dialect": target_dialect.as_str(),
            "dialect_support": target_dialect.support_level()
        },
    })
}

fn vault_error_to_runtime(err: VaultError) -> CoreRuntimeError {
    CoreRuntimeError::Config(format!("{err:?}"))
}

pub fn control_plane_socket_path(settings: &CoreSettings) -> PathBuf {
    let host_platform_adapter = detect_host_platform_adapter(&settings.core.log_level);
    let mut resolved = settings.clone();
    if resolve_runtime_path_defaults(&mut resolved, host_platform_adapter.as_ref()).is_ok() {
        return PathBuf::from(resolved.control_plane.endpoint);
    }

    if looks_like_named_pipe_endpoint(&settings.control_plane.endpoint) {
        PathBuf::from(settings.control_plane.endpoint.trim())
    } else if is_auto_value(&settings.control_plane.endpoint) {
        let runtime_paths = host_platform_adapter.runtime_paths().runtime_paths(
            &settings.core.instance_name,
            Path::new(&settings.core.data_dir),
        );
        PathBuf::from(
            host_platform_adapter
                .control_plane_transport()
                .endpoint(&settings.core.instance_name, &runtime_paths),
        )
    } else {
        host_platform_adapter
            .runtime_paths()
            .expand_user_path(&settings.control_plane.endpoint)
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
        let (mut stream, remote_addr) = self
            .listener
            .accept()
            .map_err(|err| CoreRuntimeError::Io(format!("accept http: {err}")))?;
        handle_http_connection(&mut stream, &self.runtime, Some(remote_addr))
    }
}

fn handle_http_connection(
    stream: &mut TcpStream,
    runtime: &SharedRuntime,
    remote_addr: Option<SocketAddr>,
) -> Result<(), CoreRuntimeError> {
    let (method, path, body, headers) = read_http_request(stream, Some(runtime))?;
    trace_mcp(
        Some(runtime),
        format!(
            "http request method={} path={} body_len={}",
            method,
            path,
            body.len()
        ),
    );
    let (status, content_type, response_body) = match (method.as_str(), path.as_str()) {
        ("GET", "/health") => {
            let runtime = runtime.lock().map_err(|_| CoreRuntimeError::LockPoisoned)?;
            (
                200,
                "application/json",
                json!({
                    "readiness_state": runtime.readiness_state_label(),
                    "model_plane_ready": runtime.model_plane_ready(),
                    "capability_health": runtime.capability_health_snapshot_json(),
                })
                .to_string(),
            )
        }
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
                let user_agent_summary = summarize_user_agent_header(&headers);
                let mut context = ToolRequestContext {
                    principal_id: None,
                    agent_id: required_param(&params, "agent_id")?.to_string(),
                    run_id: required_param(&params, "run_id")?.to_string(),
                    client_session_id: required_param(&params, "client_session_id")?.to_string(),
                    reuse_policy: parse_reuse_policy(required_param(&params, "reuse_policy")?)?,
                    timeline_source: Some(timeline_source_from_http_fingerprint(
                        remote_addr,
                        user_agent_summary.clone(),
                    )),
                };

                let mut auth_error = None::<&str>;
                if let Some(token) = bearer_token_from_headers(&headers) {
                    if let Some(authenticated) =
                        runtime.vault_router.authenticate_agent_token(&token)
                    {
                        let resolved_target_id = runtime
                            .resolve_target_profile_by_ref(target_ref)
                            .map(|profile| profile.id.to_ascii_lowercase());
                        let allowed = resolved_target_id
                            .as_ref()
                            .map(|target_id| authenticated.target_ids.contains(target_id))
                            .unwrap_or(false);
                        if !allowed {
                            auth_error = Some("token_scope_denied_for_target");
                        } else if authenticated.tool_ids.is_empty()
                            || !authenticated
                                .tool_ids
                                .iter()
                                .any(|tool_id| tool_id == "terminal.exec")
                        {
                            auth_error = Some("token_scope_denied_for_tool");
                        } else if !authenticated.allow_open_shell
                            || !authenticated.allow_write_shell_input
                        {
                            auth_error = Some("token_scope_denied_for_shell_capability");
                        } else if authenticated.max_risk_envelope == "deny-all" {
                            auth_error = Some("token_scope_denied_by_risk_envelope");
                        } else {
                            context.principal_id = Some(authenticated.principal_id);
                            context.timeline_source = Some(timeline_source_from_token_label(
                                &authenticated.label,
                                context.principal_id.as_deref().unwrap_or("local-operator"),
                                user_agent_summary.clone(),
                            ));
                        }
                    } else {
                        auth_error = Some("invalid_or_expired_token");
                    }
                } else if let Some(descriptor) = runtime
                    .resolve_target_descriptor_by_ref(target_ref)
                    .cloned()
                {
                    if !runtime.anonymous_loopback_compat_enabled() {
                        auth_error = Some("anonymous_compat_disabled");
                    } else if !remote_addr
                        .map(|value| value.ip().is_loopback())
                        .unwrap_or(false)
                    {
                        auth_error = Some("anonymous_compat_non_loopback_rejected");
                    } else if !descriptor.allows_anonymous_execution() {
                        auth_error = Some("anonymous_compat_denied_for_target");
                    } else {
                        context.principal_id = Some("anonymous-local".to_string());
                        context.timeline_source = Some(timeline_source_for_anonymous_loopback(
                            remote_addr,
                            user_agent_summary.clone(),
                        ));
                    }
                }

                if let Some(message) = auth_error {
                    (403, "text/plain", format!("result=error|message={message}"))
                } else {
                    match runtime.execute_target_command(
                        target_ref,
                        context,
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
        }
        _ => (404, "text/plain", "not found".to_string()),
    };
    trace_mcp(
        Some(runtime),
        format!(
            "http response method={} path={} status={} content_type={} body_len={}",
            method,
            path,
            status,
            content_type,
            response_body.len()
        ),
    );
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
        trace_mcp(
            Some(runtime),
            format!(
                "mcp request id={} method={} tool={}",
                id_brief, method, tool_name
            ),
        );
    } else {
        trace_mcp(
            Some(runtime),
            format!("mcp request id={} method={}", id_brief, method),
        );
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
    trace_mcp(
        Some(runtime),
        format!(
            "mcp response id={} method={} body_len={}",
            id_brief,
            method,
            response_body.len()
        ),
    );
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
            match runtime.resolve_target_for_mcp(target) {
                TargetResolutionResult::Resolved(resolved) => {
                    let profile = runtime
                        .resolve_target_profile_for_execution_by_ref(
                            resolved.resolved_target_id.as_str(),
                        )
                        .map_err(|err| jsonrpc_error(Value::Null, -32603, &format!("{err:?}")))?;
                    let outcome = runtime
                        .execute_target_command_on_profile(
                            resolved.requested_target_ref,
                            profile,
                            context,
                            command,
                            None,
                        )
                        .map_err(|err| jsonrpc_error(Value::Null, -32603, &format!("{err:?}")))?;
                    let structured = json!({
                        "resolution_state": "resolved",
                        "resolution_policy": runtime.target_resolution_policy.as_str(),
                        "resolution_matched_via": resolved.matched_via,
                        "requested_target_ref": outcome.requested_target_ref,
                        "resolved_target_id": outcome.resolved_target_id,
                        "target_kind": outcome.target_kind,
                        "artifact_id": outcome.artifact_id,
                        "logical_session_id": outcome.logical_session_id,
                        "channel_id": outcome.channel_id,
                        "command": outcome.command,
                        "executed_command": outcome.executed_command,
                        "invocation": invocation_json(outcome.invocation.as_ref()),
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
                TargetResolutionResult::ConfirmationRequired(confirmation) => {
                    let structured = runtime
                        .confirmation_payload_json(&confirmation, "bridgingio.terminal.exec");
                    Ok(json!({
                        "content": [
                            {
                                "type": "text",
                                "text": format!(
                                    "target confirmation required for input={}",
                                    structured["requested_target_ref"].as_str().unwrap_or("unknown")
                                )
                            }
                        ],
                        "structuredContent": structured,
                        "isError": false
                    }))
                }
                TargetResolutionResult::NotFound(not_found) => Err(jsonrpc_error(
                    Value::Null,
                    -32603,
                    &format!("target not found: {}", not_found.requested_target_ref),
                )),
            }
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
            match runtime.resolve_target_for_mcp(target) {
                TargetResolutionResult::Resolved(resolved) => {
                    let profile = runtime
                        .resolve_target_profile_for_execution_by_ref(
                            resolved.resolved_target_id.as_str(),
                        )
                        .map_err(|err| jsonrpc_error(Value::Null, -32603, &format!("{err:?}")))?;
                    let handle = runtime
                        .open_interactive_shell_with_profile(
                            resolved.requested_target_ref,
                            profile,
                            context,
                        )
                        .map_err(|err| jsonrpc_error(Value::Null, -32603, &format!("{err:?}")))?;
                    let structured = json!({
                        "resolution_state": "resolved",
                        "resolution_policy": runtime.target_resolution_policy.as_str(),
                        "resolution_matched_via": resolved.matched_via,
                        "mode": "interactive_shell",
                        "shell_id": handle.shell_id,
                        "requested_target_ref": handle.requested_target_ref,
                        "resolved_target_id": handle.resolved_target_id,
                        "target_kind": handle.target_kind,
                        "logical_session_id": handle.logical_session_id,
                        "channel_id": handle.channel_id,
                        "prompt": handle.prompt,
                        "cwd": handle.cwd,
                        "launch_strategy": handle.launch_strategy,
                        "launch_fallback_applied": handle.launch_fallback_applied,
                        "launch_diagnostics": handle.launch_diagnostics,
                        "invocation": invocation_json(handle.invocation.as_ref())
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
                TargetResolutionResult::ConfirmationRequired(confirmation) => {
                    let structured = runtime
                        .confirmation_payload_json(&confirmation, "bridgingio.terminal.shell.open");
                    Ok(json!({
                        "content": [
                            {
                                "type": "text",
                                "text": format!(
                                    "target confirmation required for input={}",
                                    structured["requested_target_ref"].as_str().unwrap_or("unknown")
                                )
                            }
                        ],
                        "structuredContent": structured,
                        "isError": false
                    }))
                }
                TargetResolutionResult::NotFound(not_found) => Err(jsonrpc_error(
                    Value::Null,
                    -32603,
                    &format!("target not found: {}", not_found.requested_target_ref),
                )),
            }
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
            match runtime.resolve_target_for_mcp(target) {
                TargetResolutionResult::Resolved(resolved) => {
                    let profile = runtime
                        .resolve_target_profile_for_execution_by_ref(
                            resolved.resolved_target_id.as_str(),
                        )
                        .map_err(|err| jsonrpc_error(Value::Null, -32603, &format!("{err:?}")))?;
                    let result = runtime
                        .inspect_target_basic_with_profile(
                            resolved.requested_target_ref,
                            profile,
                            context,
                        )
                        .map_err(|err| jsonrpc_error(Value::Null, -32603, &format!("{err:?}")))?;
                    let structured = json!({
                        "resolution_state": "resolved",
                        "resolution_policy": runtime.target_resolution_policy.as_str(),
                        "resolution_matched_via": resolved.matched_via,
                        "requested_target_ref": result.requested_target_ref,
                        "resolved_target_id": result.resolved_target_id,
                        "target_kind": result.target_kind,
                        "kernel_version": result.kernel_version,
                        "username": result.username,
                        "logical_session_id": result.logical_session_id,
                        "artifacts": result.artifacts,
                        "invocation": invocation_json(result.invocation.as_ref())
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
                TargetResolutionResult::ConfirmationRequired(confirmation) => {
                    let structured = runtime.confirmation_payload_json(
                        &confirmation,
                        "bridgingio.target.inspect_basic",
                    );
                    Ok(json!({
                        "content": [
                            {
                                "type": "text",
                                "text": format!(
                                    "target confirmation required for input={}",
                                    structured["requested_target_ref"].as_str().unwrap_or("unknown")
                                )
                            }
                        ],
                        "structuredContent": structured,
                        "isError": false
                    }))
                }
                TargetResolutionResult::NotFound(not_found) => Err(jsonrpc_error(
                    Value::Null,
                    -32603,
                    &format!("target not found: {}", not_found.requested_target_ref),
                )),
            }
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
        principal_id: optional_json_string(args, "principal_id"),
        agent_id: optional_json_string(args, "agent_id").unwrap_or_else(|| "agent-mcp".to_string()),
        run_id: optional_json_string(args, "run_id").unwrap_or_else(|| "run-mcp".to_string()),
        client_session_id: optional_json_string(args, "client_session_id")
            .unwrap_or_else(|| "client-mcp".to_string()),
        reuse_policy,
        timeline_source: None,
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

fn trace_mcp(runtime: Option<&SharedRuntime>, message: impl AsRef<str>) {
    if !mcp_trace_enabled() {
        return;
    }
    if let Some(runtime) = runtime {
        if let Ok(locked) = runtime.lock() {
            locked.host_platform_adapter.runtime_logger().log(
                RuntimeLogLevel::Trace,
                RuntimeLogCategory::Mcp,
                message.as_ref(),
            );
            return;
        }
    }
    detect_host_platform_adapter("trace").runtime_logger().log(
        RuntimeLogLevel::Trace,
        RuntimeLogCategory::Mcp,
        message.as_ref(),
    );
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

fn read_http_request(
    stream: &mut TcpStream,
    runtime: Option<&SharedRuntime>,
) -> Result<(String, String, String, HashMap<String, String>), CoreRuntimeError> {
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
    let header_text = decode_network_text(runtime, &buffer[..header_end], "http_headers");
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
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((raw_key, raw_value)) = line.split_once(':') {
            headers.insert(
                raw_key.trim().to_ascii_lowercase(),
                raw_value.trim().to_string(),
            );
        }
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
    let body = decode_network_text(runtime, &body_bytes, "http_body");
    Ok((method, path, body, headers))
}

fn bearer_token_from_headers(headers: &HashMap<String, String>) -> Option<String> {
    let value = headers.get("authorization")?;
    let trimmed = value.trim();
    let (scheme, token) = trimmed.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim();
    if token.is_empty() {
        return None;
    }
    Some(token.to_string())
}

fn summarize_user_agent_header(headers: &HashMap<String, String>) -> Option<String> {
    let raw = headers.get("user-agent")?.trim();
    if raw.is_empty() {
        return None;
    }
    let summary = raw.split_whitespace().next().unwrap_or(raw);
    Some(summary.chars().take(64).collect::<String>())
}

fn timeline_source_from_token_label(
    token_label: &str,
    principal_summary: &str,
    user_agent_summary: Option<String>,
) -> TimelineSourceGroupSummaryView {
    let normalized_label = token_label.trim();
    TimelineSourceGroupSummaryView {
        group_key: format!("token-label:{}", normalized_label.to_ascii_lowercase()),
        group_kind: "token_label".to_string(),
        group_label: if normalized_label.is_empty() {
            "token:<unlabeled>".to_string()
        } else {
            normalized_label.to_string()
        },
        principal_summary: principal_summary.to_string(),
        user_agent_summary,
    }
}

fn timeline_source_from_http_fingerprint(
    remote_addr: Option<SocketAddr>,
    user_agent_summary: Option<String>,
) -> TimelineSourceGroupSummaryView {
    let addr_seed = remote_addr
        .map(|value| value.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let ua_seed = user_agent_summary
        .as_deref()
        .unwrap_or("unknown-agent")
        .to_ascii_lowercase();
    let fingerprint = stable_fingerprint_id(&format!("{addr_seed}|{ua_seed}"));
    TimelineSourceGroupSummaryView {
        group_key: format!("http-fingerprint:{fingerprint}"),
        group_kind: "http_fingerprint".to_string(),
        group_label: format!("HTTP {fingerprint}"),
        principal_summary: format!("http:{fingerprint}"),
        user_agent_summary,
    }
}

fn timeline_source_for_anonymous_loopback(
    remote_addr: Option<SocketAddr>,
    user_agent_summary: Option<String>,
) -> TimelineSourceGroupSummaryView {
    let mut source = timeline_source_from_http_fingerprint(remote_addr, user_agent_summary);
    source.group_key = format!("anonymous-loopback:{}", source.group_key);
    source.group_kind = "anonymous_loopback".to_string();
    source.group_label = "Anonymous Loopback".to_string();
    source.principal_summary = "anonymous-local".to_string();
    source
}

fn stable_fingerprint_id(seed: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in seed.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn decode_network_text(runtime: Option<&SharedRuntime>, bytes: &[u8], surface: &str) -> String {
    if let Some(runtime) = runtime {
        if let Ok(locked) = runtime.lock() {
            let decoded = locked.host_platform_adapter.output_decoder().decode(bytes);
            if decoded.used_fallback || decoded.had_replacement_char || decoded.normalized_newlines
            {
                locked.host_platform_adapter.runtime_logger().log(
                    RuntimeLogLevel::Warn,
                    RuntimeLogCategory::Decode,
                    &format!(
                        "decode diagnostics surface={} fallback={} replacement_char={} normalized_newlines={}",
                        surface,
                        decoded.used_fallback,
                        decoded.had_replacement_char,
                        decoded.normalized_newlines
                    ),
                );
            }
            return decoded.text;
        }
    }
    let adapter = detect_host_platform_adapter("info");
    let decoded = adapter.output_decoder().decode(bytes);
    if decoded.used_fallback || decoded.had_replacement_char || decoded.normalized_newlines {
        adapter.runtime_logger().log(
            RuntimeLogLevel::Warn,
            RuntimeLogCategory::Decode,
            &format!(
                "decode diagnostics surface={} fallback={} replacement_char={} normalized_newlines={}",
                surface, decoded.used_fallback, decoded.had_replacement_char, decoded.normalized_newlines
            ),
        );
    }
    decoded.text
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
    use std::io::{Read, Write};
    use std::net::TcpStream;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use bridgingio_app_api::{
        AgentTokenScopeView, ApiRequest, ApiRequestContext, ApiResponse, AppCommand,
    };
    use bridgingio_connectors::{BuiltInBinarySpec, BuiltInDistributionKind, ExecutableResolver};
    use bridgingio_domain::{ConnectionConfig, PolicyProfile, TargetKind, TargetProfile};
    use bridgingio_secrets::{
        local_admin_create_token_target, local_admin_payload_digest_for_create_agent_token,
        local_admin_payload_digest_for_unlock_vault,
        local_admin_payload_digest_for_update_agent_token_scope, local_admin_unlock_vault_target,
        CreateAgentTokenRequest, TokenScopeInput, UpdateAgentTokenScopeRequest, VaultLockState,
        VaultUnlockPolicy, VaultUnlockTriggerPolicy,
    };

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
            principal_id: None,
            agent_id: agent.into(),
            run_id: run.into(),
            client_session_id: client.into(),
            reuse_policy,
            timeline_source: None,
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

    fn create_token_attestation_id(
        runtime: &mut StandaloneCoreRuntime,
        label: &str,
        expires_in_seconds: Option<u64>,
        scope: AgentTokenScopeView,
    ) -> String {
        let request = CreateAgentTokenRequest {
            label: label.to_string(),
            created_by: "control-plane:ui-agent:ui-run:ui-client".into(),
            expires_in: expires_in_seconds.map(Duration::from_secs),
            idle_timeout_sec: None,
            scope: TokenScopeInput {
                scope_profile: scope.scope_profile.clone(),
                target_ids: scope.target_ids.clone(),
                tool_ids: scope.tool_ids.clone(),
                max_risk_envelope: scope.max_risk_envelope.clone(),
                allow_open_shell: scope.allow_open_shell,
                allow_write_shell_input: scope.allow_write_shell_input,
                allow_artifact_cross_principal: scope.allow_artifact_cross_principal,
                allow_delegation: scope.allow_delegation,
                allow_admin_actions: scope.allow_admin_actions,
            },
            attestation_id: None,
        };
        let payload_digest = local_admin_payload_digest_for_create_agent_token(&request);
        let intent_response = runtime.handle_app_request(ApiRequest {
            request_id: "intent-create-token".into(),
            context: request_context(),
            command: AppCommand::CreateLocalAdminIntent {
                action_kind: "create-agent-token".into(),
                target_object_ref: local_admin_create_token_target().into(),
                requested_payload_digest: Some(payload_digest),
                ttl_seconds: Some(300),
            },
        });
        let intent_id = match intent_response {
            ApiResponse::LocalAdminIntentCreated { intent, .. } => intent.intent_id,
            other => panic!("expected local admin intent response, got {other:?}"),
        };
        let attestation_response = runtime.handle_app_request(ApiRequest {
            request_id: "intent-complete-token".into(),
            context: request_context(),
            command: AppCommand::CompleteLocalAdminAttestation {
                intent_id,
                verification_method: "passkey".into(),
                ttl_seconds: Some(120),
            },
        });
        match attestation_response {
            ApiResponse::LocalAdminAttestationCompleted { attestation, .. } => {
                attestation.attestation_id
            }
            other => panic!("expected local admin attestation response, got {other:?}"),
        }
    }

    fn unlock_attestation_id(runtime: &mut StandaloneCoreRuntime, method: &str) -> String {
        let payload_digest = local_admin_payload_digest_for_unlock_vault(method);
        let intent_response = runtime.handle_app_request(ApiRequest {
            request_id: "intent-create-unlock".into(),
            context: request_context(),
            command: AppCommand::CreateLocalAdminIntent {
                action_kind: "unlock-vault".into(),
                target_object_ref: local_admin_unlock_vault_target().into(),
                requested_payload_digest: Some(payload_digest),
                ttl_seconds: Some(300),
            },
        });
        let intent_id = match intent_response {
            ApiResponse::LocalAdminIntentCreated { intent, .. } => intent.intent_id,
            other => panic!("expected local admin intent response, got {other:?}"),
        };
        let attestation_response = runtime.handle_app_request(ApiRequest {
            request_id: "intent-complete-unlock".into(),
            context: request_context(),
            command: AppCommand::CompleteLocalAdminAttestation {
                intent_id,
                verification_method: "passkey".into(),
                ttl_seconds: Some(120),
            },
        });
        match attestation_response {
            ApiResponse::LocalAdminAttestationCompleted { attestation, .. } => {
                attestation.attestation_id
            }
            other => panic!("expected local admin attestation response, got {other:?}"),
        }
    }

    fn scope_update_attestation_id(
        runtime: &mut StandaloneCoreRuntime,
        token_id: &str,
        scope: AgentTokenScopeView,
        reason: Option<String>,
    ) -> String {
        let request = UpdateAgentTokenScopeRequest {
            token_id: token_id.to_string(),
            changed_by: "control-plane:ui-agent:ui-run:ui-client".into(),
            scope: TokenScopeInput {
                scope_profile: scope.scope_profile.clone(),
                target_ids: scope.target_ids.clone(),
                tool_ids: scope.tool_ids.clone(),
                max_risk_envelope: scope.max_risk_envelope.clone(),
                allow_open_shell: scope.allow_open_shell,
                allow_write_shell_input: scope.allow_write_shell_input,
                allow_artifact_cross_principal: scope.allow_artifact_cross_principal,
                allow_delegation: scope.allow_delegation,
                allow_admin_actions: scope.allow_admin_actions,
            },
            reason: reason.clone(),
            attestation_id: None,
        };
        let payload_digest = local_admin_payload_digest_for_update_agent_token_scope(&request);
        let intent_response = runtime.handle_app_request(ApiRequest {
            request_id: "intent-create-scope".into(),
            context: request_context(),
            command: AppCommand::CreateLocalAdminIntent {
                action_kind: "update-agent-token-scope".into(),
                target_object_ref: token_id.into(),
                requested_payload_digest: Some(payload_digest),
                ttl_seconds: Some(300),
            },
        });
        let intent_id = match intent_response {
            ApiResponse::LocalAdminIntentCreated { intent, .. } => intent.intent_id,
            other => panic!("expected local admin intent response, got {other:?}"),
        };
        let attestation_response = runtime.handle_app_request(ApiRequest {
            request_id: "intent-complete-scope".into(),
            context: request_context(),
            command: AppCommand::CompleteLocalAdminAttestation {
                intent_id,
                verification_method: "passkey".into(),
                ttl_seconds: Some(120),
            },
        });
        match attestation_response {
            ApiResponse::LocalAdminAttestationCompleted { attestation, .. } => {
                attestation.attestation_id
            }
            other => panic!("expected local admin attestation response, got {other:?}"),
        }
    }

    fn settings_with_extra_targets(
        policy: &str,
        extra_targets: &str,
    ) -> bridgingio_engine::CoreSettings {
        let base = bridgingio_engine::CoreSettings::minimal_example().replace(
            "mcp_target_resolution_policy = \"confirm_if_family\"",
            &format!("mcp_target_resolution_policy = \"{policy}\""),
        );
        let merged = format!("{base}\n{extra_targets}");
        bridgingio_engine::CoreSettings::from_toml_str(&merged)
            .expect("parse settings with extra targets")
    }

    fn sealed_overlay_runtime(prefix: &str) -> StandaloneCoreRuntime {
        let root = temp_dir(prefix);
        let settings = settings_with_extra_targets(
            "confirm_if_family",
            r#"
[[targets]]
id = "sealed-shell"
display_name = "Sealed Shell"
kind = "localshell"
enabled = true
aliases = ["sealed-shell"]
storage_class = "sealed-overlay"
access_class = "token-scoped"
sealed_profile_ref = "vault://bridgingio/targets/sealed-shell"

[targets.connection]

[targets.providers.terminal]
enabled = true
"#,
        );
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(Vec::new()),
            &root,
            Vec::new(),
        );
        StandaloneCoreRuntime::from_settings(settings, resolver).expect("runtime")
    }

    fn install_sealed_overlay(
        runtime: &mut StandaloneCoreRuntime,
        target_ref: &str,
        digest_override: Option<&str>,
        notes: Option<&str>,
    ) {
        let descriptor = runtime
            .resolve_target_descriptor_by_ref(target_ref)
            .cloned()
            .expect("sealed target descriptor");
        let digest = digest_override
            .map(ToString::to_string)
            .unwrap_or_else(|| super::public_descriptor_digest_for_target_descriptor(&descriptor));
        let overlay_ref = descriptor
            .sealed_profile_ref
            .clone()
            .expect("sealed profile ref");

        runtime
            .vault_router
            .set_active_backend("builtin-encrypted")
            .expect("switch vault backend");
        let overlay_payload = serde_json::json!({
            "public_descriptor_digest": digest,
            "notes": notes,
            "credential_ref": "vault:ssh-key:sealed_overlay",
            "connection": {
                "description": "sealed-overlay-connection"
            }
        })
        .to_string();
        runtime
            .vault_router
            .put(&overlay_ref, &overlay_payload, "sealed overlay")
            .expect("store sealed overlay");
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
    fn terminal_exec_returns_busy_error_for_exclusive_target_conflict() {
        let mut handler = McpToolHandler::default();
        handler.metadata.upsert_profile(TargetProfile {
            id: "serial-console-01".into(),
            name: "serial console".into(),
            kind: TargetKind::Serial,
            connection: ConnectionConfig::Serial {
                device: "/dev/tty.usbmodem01".into(),
                baud_rate: 115200,
            },
            credential_ref: None,
            default_policy: PolicyProfile::default(),
            notes: None,
            metadata: BTreeMap::new(),
            toolchains: BTreeMap::new(),
        });

        let now = SystemTime::now();
        let holder = context(
            "agent-holder",
            "run-holder",
            "client-holder",
            bridgingio_domain::SessionReusePolicy::ReuseIfAlive,
        );
        let holder_scope = super::build_scope(&holder, now);
        let logical = handler.metadata.resolve_logical_session(
            &holder_scope,
            "serial-console-01",
            holder.reuse_policy.clone(),
            now,
        );
        let held_transport = handler
            .metadata
            .open_transport_session(
                &logical.logical_session_id,
                "serial-console-01",
                TargetKind::Serial,
                None,
                None,
                now,
            )
            .expect("hold exclusive transport");

        let contender = context(
            "agent-contender",
            "run-contender",
            "client-contender",
            bridgingio_domain::SessionReusePolicy::ReuseIfAlive,
        );
        let result = handler.run_terminal(
            "serial-console-01".into(),
            TargetKind::Serial,
            contender,
            "echo ping".into(),
            "artifact-contender".into(),
            None,
        );
        match result {
            ToolResult::Error { message } => {
                assert!(message.contains("busy"));
                assert!(message.contains("serial-console-01"));
                assert!(message.contains(&held_transport.transport_session_id));
            }
            other => panic!("expected busy error result, got {other:?}"),
        }
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

    #[test]
    fn resolver_supports_multi_alias_relaxed_normalization_and_disabled_filtering() {
        let root = temp_dir("resolver-multi-alias");
        let settings = settings_with_extra_targets(
            "confirm_if_family",
            r#"
[[targets]]
id = "test-device"
display_name = "Test Device"
kind = "ssh"
enabled = true
aliases = ["test_alias", "device-a"]

[targets.connection]
host = "127.0.0.1"
port = 22
username = "dev"

[targets.providers.terminal]
enabled = true

[[targets]]
id = "legacy-device"
display_name = "Legacy Device"
kind = "ssh"
enabled = false
aliases = ["legacy"]

[targets.connection]
host = "127.0.0.1"
port = 22
username = "dev"

[targets.providers.terminal]
enabled = true
"#,
        );
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(Vec::new()),
            &root,
            Vec::new(),
        );
        let runtime = StandaloneCoreRuntime::from_settings(settings, resolver).expect("runtime");

        let alias_hit = runtime.resolve_target_for_mcp("test_alias");
        assert!(matches!(
            alias_hit,
            super::TargetResolutionResult::Resolved(super::TargetResolutionResolved { ref resolved_target_id, .. })
                if resolved_target_id == "test-device"
        ));

        let relaxed_hit = runtime.resolve_target_for_mcp("TEST DEVICE");
        assert!(matches!(
            relaxed_hit,
            super::TargetResolutionResult::Resolved(super::TargetResolutionResolved { ref resolved_target_id, .. })
                if resolved_target_id == "test-device"
        ));

        let disabled_hit = runtime.resolve_target_for_mcp("legacy");
        assert!(matches!(
            disabled_hit,
            super::TargetResolutionResult::NotFound(_)
        ));
    }

    #[test]
    fn resolver_family_boundary_and_policy_branches_are_respected() {
        let root = temp_dir("resolver-family-policy");
        let settings = settings_with_extra_targets(
            "confirm_if_family",
            r#"
[[targets]]
id = "test"
display_name = "Test"
kind = "ssh"
enabled = true
aliases = []

[targets.connection]
host = "127.0.0.1"
port = 22
username = "dev"

[targets.providers.terminal]
enabled = true

[[targets]]
id = "test-1"
display_name = "Test 1"
kind = "ssh"
enabled = true
aliases = []

[targets.connection]
host = "127.0.0.1"
port = 22
username = "dev"

[targets.providers.terminal]
enabled = true

[[targets]]
id = "testlab"
display_name = "Test Lab"
kind = "ssh"
enabled = true
aliases = []

[targets.connection]
host = "127.0.0.1"
port = 22
username = "dev"

[targets.providers.terminal]
enabled = true
"#,
        );
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(Vec::new()),
            &root,
            Vec::new(),
        );
        let runtime = StandaloneCoreRuntime::from_settings(settings, resolver).expect("runtime");
        let confirmation = runtime.resolve_target_for_mcp("test");
        match confirmation {
            super::TargetResolutionResult::ConfirmationRequired(value) => {
                assert_eq!(value.policy.as_str(), "confirm_if_family");
                assert_eq!(
                    value
                        .exact_match
                        .as_ref()
                        .map(|item| item.target_id.as_str()),
                    Some("test")
                );
                let candidate_ids = value
                    .related_candidates
                    .iter()
                    .map(|item| item.target_id.as_str())
                    .collect::<Vec<_>>();
                assert!(candidate_ids.contains(&"test-1"));
                assert!(!candidate_ids.contains(&"testlab"));
            }
            other => panic!("expected confirmation, got {other:?}"),
        }

        let settings = settings_with_extra_targets(
            "auto_execute",
            r#"
[[targets]]
id = "test"
display_name = "Test"
kind = "ssh"
enabled = true
aliases = []

[targets.connection]
host = "127.0.0.1"
port = 22
username = "dev"

[targets.providers.terminal]
enabled = true

[[targets]]
id = "test-1"
display_name = "Test 1"
kind = "ssh"
enabled = true
aliases = []

[targets.connection]
host = "127.0.0.1"
port = 22
username = "dev"

[targets.providers.terminal]
enabled = true
"#,
        );
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(Vec::new()),
            &root,
            Vec::new(),
        );
        let runtime = StandaloneCoreRuntime::from_settings(settings, resolver).expect("runtime");
        let resolved = runtime.resolve_target_for_mcp("test");
        assert!(matches!(
            resolved,
            super::TargetResolutionResult::Resolved(super::TargetResolutionResolved { ref resolved_target_id, .. })
                if resolved_target_id == "test"
        ));
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
        fs::write(
            &config_path,
            bridgingio_engine::CoreSettings::minimal_example(),
        )
        .expect("write config");

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
                core_log_level: Some("debug".into()),
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
        assert!(
            persisted.contains("port = 19719"),
            "persisted config: {persisted}"
        );
        assert!(
            persisted.contains("log_level = \"debug\""),
            "persisted config: {persisted}"
        );
        let reloaded =
            bridgingio_engine::CoreSettings::load_from_file(&config_path).expect("reload");
        assert_eq!(reloaded.model_plane.http.port, 19719);
        assert_eq!(reloaded.core.log_level, "debug");

        let bootstrap = runtime.handle_app_request(ApiRequest {
            request_id: "req-bootstrap".into(),
            context: request_context(),
            command: AppCommand::GetBootstrapState {
                timeline_limit: 10,
                artifact_limit: 10,
                transcript_limit: 10,
            },
        });
        let bootstrap_payload = match bootstrap {
            ApiResponse::Bootstrap { payload_json, .. } => payload_json,
            other => panic!("expected bootstrap response, got {other:?}"),
        };
        let bootstrap_json: serde_json::Value =
            serde_json::from_str(&bootstrap_payload).expect("bootstrap json");
        assert_eq!(
            bootstrap_json["management_plane"]["requires_management_address_handoff"].as_bool(),
            Some(false)
        );
    }

    #[test]
    fn settings_view_exposes_runtime_logs_and_vault_status() {
        let root = temp_dir("settings-view-runtime-vault");
        let settings = bridgingio_engine::CoreSettings::from_toml_str(
            &bridgingio_engine::CoreSettings::minimal_example(),
        )
        .expect("parse settings");
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(Vec::new()),
            &root,
            Vec::new(),
        );
        let runtime = StandaloneCoreRuntime::from_settings(settings, resolver).expect("runtime");

        let view = runtime.settings_view();
        assert!(view.runtime_logs.root.ends_with("/logs"));
        assert_eq!(
            view.runtime_logs.level,
            runtime.settings_store.settings.core.log_level
        );
        assert!(!view.vault.status.is_empty());
        assert_eq!(
            view.vault.configured_backend,
            runtime.settings_store.settings.vault.backend
        );
    }

    #[test]
    fn capability_health_uses_runtime_vault_readiness_projection() {
        let root = temp_dir("capability-health-vault-readiness");
        let settings = bridgingio_engine::CoreSettings::from_toml_str(
            &bridgingio_engine::CoreSettings::minimal_example(),
        )
        .expect("parse settings");
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(Vec::new()),
            &root,
            Vec::new(),
        );
        let runtime = StandaloneCoreRuntime::from_settings(settings, resolver).expect("runtime");
        let vault_diag = runtime
            .vault_router
            .active_backend_diagnostics()
            .expect("vault diagnostics");
        let expected_status =
            super::vault_readiness_to_capability_status(&vault_diag.status).as_str();

        let health = runtime.capability_health_snapshot_json();
        assert_eq!(health["vault"]["status"].as_str(), Some(expected_status));
        assert!(health["vault"]["readiness_message"]
            .as_str()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false));
    }

    #[test]
    fn control_plane_get_settings_includes_vault_status_projection() {
        let root = temp_dir("control-plane-settings-vault-status");
        let settings = bridgingio_engine::CoreSettings::from_toml_str(
            &bridgingio_engine::CoreSettings::minimal_example(),
        )
        .expect("parse settings");
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(Vec::new()),
            &root,
            Vec::new(),
        );
        let mut runtime =
            StandaloneCoreRuntime::from_settings(settings, resolver).expect("runtime");

        let response = runtime.handle_app_request(ApiRequest {
            request_id: "settings".into(),
            context: request_context(),
            command: AppCommand::GetSettings,
        });
        match response {
            ApiResponse::Settings { settings, .. } => {
                assert!(!settings.vault.status.is_empty());
                assert!(!settings.vault.binding_backend.is_empty());
                assert_eq!(
                    settings.vault.configured_backend,
                    runtime.settings_store.settings.vault.backend
                );
            }
            other => panic!("expected settings response, got {other:?}"),
        }
    }

    #[test]
    fn control_plane_vault_state_unlock_and_lock_contract() {
        let root = temp_dir("control-plane-vault-state-unlock-lock");
        let settings = bridgingio_engine::CoreSettings::from_toml_str(
            &bridgingio_engine::CoreSettings::minimal_example(),
        )
        .expect("parse settings");
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(Vec::new()),
            &root,
            Vec::new(),
        );
        let mut runtime =
            StandaloneCoreRuntime::from_settings(settings, resolver).expect("runtime");
        runtime
            .vault_router
            .put("vault:ssh-key:contract", "CONTRACT-KEY", "contract key")
            .expect("put test secret");

        let state = runtime.handle_app_request(ApiRequest {
            request_id: "vault-state".into(),
            context: request_context(),
            command: AppCommand::GetVaultState,
        });
        match state {
            ApiResponse::VaultState { state, .. } => {
                assert!(!state.lock_state.is_empty());
                assert!(!state.protector_summary.primary.is_empty());
                assert!(!state.unlock_policy_summary.trigger_policy.is_empty());
                assert!(state
                    .secrets
                    .iter()
                    .any(|item| item.reference == "vault://bridgingio/ssh-private-key/contract"));
            }
            other => panic!("expected vault state response, got {other:?}"),
        }

        let locked = runtime.handle_app_request(ApiRequest {
            request_id: "vault-lock".into(),
            context: request_context(),
            command: AppCommand::LockVault {
                reason: Some("contract-test".into()),
            },
        });
        match locked {
            ApiResponse::VaultLocked { lock_state, .. } => {
                assert_eq!(lock_state, "locked");
            }
            other => panic!("expected vault locked response, got {other:?}"),
        }

        let attestation_id = unlock_attestation_id(&mut runtime, "os-native");
        let unlocked = runtime.handle_app_request(ApiRequest {
            request_id: "vault-unlock".into(),
            context: request_context(),
            command: AppCommand::UnlockVault {
                method: "os-native".into(),
                passphrase: None,
                attestation_id,
            },
        });
        match unlocked {
            ApiResponse::VaultUnlocked { lock_state, .. } => {
                assert_eq!(lock_state, "unlocked");
            }
            other => panic!("expected vault unlocked response, got {other:?}"),
        }
    }

    #[test]
    fn timeline_payload_and_bootstrap_include_source_group_summary() {
        let root = temp_dir("timeline-source-group-summary");
        let settings = bridgingio_engine::CoreSettings::from_toml_str(
            &bridgingio_engine::CoreSettings::minimal_example(),
        )
        .expect("parse settings");
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(Vec::new()),
            &root,
            Vec::new(),
        );
        let mut runtime =
            StandaloneCoreRuntime::from_settings(settings, resolver).expect("runtime");
        runtime.push_timeline_entry(
            "logical-session-1".into(),
            "echo hello".into(),
            "success",
            super::timeline_source_from_token_label(
                "nightly-runner",
                "principal-000001",
                Some("curl/8.0".into()),
            ),
            Some("artifact-1".into()),
        );
        runtime.push_timeline_entry(
            "logical-session-2".into(),
            "uname -a".into(),
            "success",
            super::timeline_source_from_http_fingerprint(
                Some("127.0.0.1:9022".parse().expect("socket")),
                Some("curl/8.0".into()),
            ),
            Some("artifact-2".into()),
        );

        let timeline = runtime.handle_app_request(ApiRequest {
            request_id: "timeline".into(),
            context: request_context(),
            command: AppCommand::GetTimeline { limit: 10 },
        });
        let timeline_payload = match timeline {
            ApiResponse::Timeline { payload_json, .. } => payload_json,
            other => panic!("unexpected timeline response: {other:?}"),
        };
        let timeline_json: serde_json::Value =
            serde_json::from_str(&timeline_payload).expect("timeline json");
        let timeline_items = timeline_json["items"]
            .as_array()
            .expect("timeline items array");
        let timeline_group_kinds: Vec<&str> = timeline_items
            .iter()
            .filter_map(|item| item["source_group"]["group_kind"].as_str())
            .collect();
        assert!(timeline_group_kinds.contains(&"token_label"));
        assert!(timeline_group_kinds.contains(&"http_fingerprint"));
        let has_token_label = timeline_items.iter().any(|item| {
            item["source_group"]["group_kind"].as_str() == Some("token_label")
                && item["source_group"]["group_label"].as_str() == Some("nightly-runner")
        });
        assert!(has_token_label);

        let bootstrap = runtime.handle_app_request(ApiRequest {
            request_id: "bootstrap".into(),
            context: request_context(),
            command: AppCommand::GetBootstrapState {
                timeline_limit: 10,
                artifact_limit: 10,
                transcript_limit: 10,
            },
        });
        let bootstrap_payload = match bootstrap {
            ApiResponse::Bootstrap { payload_json, .. } => payload_json,
            other => panic!("unexpected bootstrap response: {other:?}"),
        };
        let bootstrap_json: serde_json::Value =
            serde_json::from_str(&bootstrap_payload).expect("bootstrap json");
        let bootstrap_timeline = bootstrap_json["timeline"]
            .as_array()
            .expect("bootstrap timeline array");
        let bootstrap_group_kinds: Vec<&str> = bootstrap_timeline
            .iter()
            .filter_map(|item| item["source_group"]["group_kind"].as_str())
            .collect();
        assert!(bootstrap_group_kinds.contains(&"token_label"));
        assert!(bootstrap_group_kinds.contains(&"http_fingerprint"));
    }

    #[test]
    fn request_attribution_projection_prefers_token_label_then_fingerprint() {
        let token_source = super::timeline_source_from_token_label(
            "release-bot",
            "principal-000007",
            Some("curl/8.0".into()),
        );
        assert_eq!(token_source.group_kind, "token_label");
        assert_eq!(token_source.group_label, "release-bot");
        assert_eq!(token_source.principal_summary, "principal-000007");

        let fallback_source = super::timeline_source_from_http_fingerprint(
            Some("127.0.0.1:9022".parse().expect("socket")),
            Some("curl/8.0".into()),
        );
        assert_eq!(fallback_source.group_kind, "http_fingerprint");
        assert!(fallback_source.group_key.starts_with("http-fingerprint:"));
        assert_ne!(token_source.group_key, fallback_source.group_key);
    }

    #[test]
    fn control_plane_create_list_revoke_agent_token_contract() {
        let root = temp_dir("control-plane-agent-token");
        let settings = bridgingio_engine::CoreSettings::from_toml_str(
            &bridgingio_engine::CoreSettings::minimal_example(),
        )
        .expect("parse settings");
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(Vec::new()),
            &root,
            Vec::new(),
        );
        let mut runtime =
            StandaloneCoreRuntime::from_settings(settings, resolver).expect("runtime");

        let create_scope = AgentTokenScopeView {
            scope_profile: Some("strict-default".into()),
            target_ids: vec!["LOCAL-SSH".into(), "local-ssh".into()],
            tool_ids: Vec::new(),
            max_risk_envelope: Some("deny-all".into()),
            allow_open_shell: Some(false),
            allow_write_shell_input: Some(false),
            allow_artifact_cross_principal: Some(false),
            allow_delegation: Some(false),
            allow_admin_actions: Some(false),
        };
        let attestation_id = create_token_attestation_id(
            &mut runtime,
            "nightly-runner",
            Some(1800),
            create_scope.clone(),
        );
        let create_response = runtime.handle_app_request(ApiRequest {
            request_id: "token-create".into(),
            context: request_context(),
            command: AppCommand::CreateAgentToken {
                label: "nightly-runner".into(),
                expires_in_seconds: Some(1800),
                scope: create_scope,
                attestation_id: Some(attestation_id),
            },
        });
        let (token_id, plaintext_token) = match create_response {
            ApiResponse::AgentTokenCreated { result, .. } => {
                assert!(result.plaintext_token.starts_with("agt_"));
                assert_eq!(result.summary.status, "active");
                assert_eq!(result.summary.target_scope_summary, "targets:1");
                (result.summary.token_id, result.plaintext_token)
            }
            other => panic!("expected token created response, got {other:?}"),
        };

        let list_response = runtime.handle_app_request(ApiRequest {
            request_id: "token-list".into(),
            context: request_context(),
            command: AppCommand::ListAgentTokens,
        });
        match list_response {
            ApiResponse::AgentTokens { items, .. } => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].token_id, token_id);
                assert_ne!(items[0].token_id, plaintext_token);
            }
            other => panic!("expected token list response, got {other:?}"),
        }

        let revoke_response = runtime.handle_app_request(ApiRequest {
            request_id: "token-revoke".into(),
            context: request_context(),
            command: AppCommand::RevokeAgentToken {
                token_id,
                reason: Some("user delete".into()),
            },
        });
        match revoke_response {
            ApiResponse::AgentTokenRevoked { summary, .. } => {
                assert_eq!(summary.status, "revoked");
                assert_eq!(summary.revoke_reason.as_deref(), Some("user delete"));
            }
            other => panic!("expected token revoke response, got {other:?}"),
        }

        let list_after_revoke = runtime.handle_app_request(ApiRequest {
            request_id: "token-list-after-revoke".into(),
            context: request_context(),
            command: AppCommand::ListAgentTokens,
        });
        match list_after_revoke {
            ApiResponse::AgentTokens { items, .. } => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].status, "revoked");
                assert_eq!(items[0].revoke_reason.as_deref(), Some("user delete"));
            }
            other => panic!("expected token list response, got {other:?}"),
        }
    }

    #[test]
    fn control_plane_update_agent_token_scope_advances_version() {
        let root = temp_dir("control-plane-agent-token-scope-update");
        let settings = bridgingio_engine::CoreSettings::from_toml_str(
            &bridgingio_engine::CoreSettings::minimal_example(),
        )
        .expect("parse settings");
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(Vec::new()),
            &root,
            Vec::new(),
        );
        let mut runtime =
            StandaloneCoreRuntime::from_settings(settings, resolver).expect("runtime");

        let create_scope = AgentTokenScopeView {
            scope_profile: Some("strict-default".into()),
            target_ids: vec!["target-a".into()],
            tool_ids: Vec::new(),
            max_risk_envelope: None,
            allow_open_shell: None,
            allow_write_shell_input: None,
            allow_artifact_cross_principal: None,
            allow_delegation: None,
            allow_admin_actions: None,
        };
        let create_attestation_id =
            create_token_attestation_id(&mut runtime, "scope-token", None, create_scope.clone());
        let created = runtime.handle_app_request(ApiRequest {
            request_id: "token-create".into(),
            context: request_context(),
            command: AppCommand::CreateAgentToken {
                label: "scope-token".into(),
                expires_in_seconds: None,
                scope: create_scope,
                attestation_id: Some(create_attestation_id),
            },
        });
        let token_id = match created {
            ApiResponse::AgentTokenCreated { result, .. } => result.summary.token_id,
            other => panic!("unexpected token create response: {other:?}"),
        };

        let update_scope = AgentTokenScopeView {
            scope_profile: Some("strict-default".into()),
            target_ids: vec!["target-a".into(), "target-b".into()],
            tool_ids: vec!["terminal.exec".into()],
            max_risk_envelope: Some("deny-all".into()),
            allow_open_shell: Some(false),
            allow_write_shell_input: Some(false),
            allow_artifact_cross_principal: Some(false),
            allow_delegation: Some(false),
            allow_admin_actions: Some(false),
        };
        let update_attestation_id = scope_update_attestation_id(
            &mut runtime,
            &token_id,
            update_scope.clone(),
            Some("expand target scope".into()),
        );
        let update = runtime.handle_app_request(ApiRequest {
            request_id: "token-scope-update".into(),
            context: request_context(),
            command: AppCommand::UpdateAgentTokenScope {
                token_id,
                scope: update_scope,
                reason: Some("expand target scope".into()),
                attestation_id: Some(update_attestation_id),
            },
        });
        match update {
            ApiResponse::Accepted { apply_strategy, .. } => {
                assert_eq!(apply_strategy.as_deref(), Some("live_applied"));
            }
            other => panic!("expected accepted update response, got {other:?}"),
        }

        let list = runtime.handle_app_request(ApiRequest {
            request_id: "token-list".into(),
            context: request_context(),
            command: AppCommand::ListAgentTokens,
        });
        match list {
            ApiResponse::AgentTokens { items, .. } => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].active_scope_version, 2);
                assert_eq!(items[0].target_scope_summary, "targets:2");
            }
            other => panic!("unexpected list response: {other:?}"),
        }
    }

    #[test]
    fn build_scope_uses_principal_id_while_treating_agent_fields_as_labels() {
        let context = ToolRequestContext {
            principal_id: Some("principal-000001".into()),
            agent_id: "agent-label".into(),
            run_id: "run-label".into(),
            client_session_id: "client-label".into(),
            reuse_policy: bridgingio_domain::SessionReusePolicy::ReuseIfAlive,
            timeline_source: None,
        };
        let scope = super::build_scope(&context, SystemTime::now());
        assert_eq!(scope.principal_id, "principal-000001");
        assert_eq!(scope.agent_id, "agent-label");
        assert_eq!(scope.run_id, "run-label");
        assert_eq!(scope.client_session_id, "client-label");
    }

    #[test]
    fn model_plane_terminal_exec_allows_loopback_anonymous_for_plain_target_when_enabled() {
        let root = temp_dir("model-plane-anon-plain-allow");
        let settings = settings_with_extra_targets(
            "confirm_if_family",
            r#"
[[targets]]
id = "local-shell"
display_name = "Local Shell"
kind = "localshell"
enabled = true
aliases = ["local-shell"]
storage_class = "plain"
access_class = "anonymous-local"

[targets.connection]

[targets.providers.terminal]
enabled = true
"#,
        );
        let mut settings = settings;
        settings.model_plane.http.port = 0;
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(Vec::new()),
            &root,
            Vec::new(),
        );
        let runtime =
            StandaloneCoreRuntime::from_settings(settings.clone(), resolver).expect("runtime");
        let shared = runtime.shared();
        let server = ModelPlaneHttpServer::bind(shared, &settings).expect("bind model-plane");
        let addr = server.local_addr().expect("local addr");
        let serve_thread = thread::spawn(move || server.serve_once().expect("serve once"));

        let body = "agent_id=agent-mcp|run_id=run-mcp|client_session_id=client-mcp|reuse_policy=reuse_if_alive|target_id=local-shell|command=echo anonymous-ok";
        let request = format!(
            "POST /tool/terminal.exec HTTP/1.1\r\nHost: {addr}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let mut stream = TcpStream::connect(addr).expect("connect model-plane");
        stream.write_all(request.as_bytes()).expect("write request");
        let mut response = String::new();
        stream.read_to_string(&mut response).expect("read response");
        serve_thread.join().expect("join serve thread");

        assert!(response.starts_with("HTTP/1.1 200"), "response={response}");
        assert!(response.contains("result=execution"));
    }

    #[test]
    fn model_plane_terminal_exec_rejects_missing_token_when_anonymous_compat_disabled() {
        let root = temp_dir("model-plane-anon-disabled");
        let settings = settings_with_extra_targets(
            "confirm_if_family",
            r#"
[[targets]]
id = "local-shell"
display_name = "Local Shell"
kind = "localshell"
enabled = true
aliases = ["local-shell"]
storage_class = "plain"
access_class = "anonymous-local"

[targets.connection]

[targets.providers.terminal]
enabled = true
"#,
        );
        let mut settings = settings;
        settings.model_plane.http.port = 0;
        settings
            .model_plane
            .http
            .auth
            .allow_loopback_anonymous_compat = false;
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(Vec::new()),
            &root,
            Vec::new(),
        );
        let runtime =
            StandaloneCoreRuntime::from_settings(settings.clone(), resolver).expect("runtime");
        let shared = runtime.shared();
        let server = ModelPlaneHttpServer::bind(shared, &settings).expect("bind model-plane");
        let addr = server.local_addr().expect("local addr");
        let serve_thread = thread::spawn(move || server.serve_once().expect("serve once"));

        let body = "agent_id=agent-mcp|run_id=run-mcp|client_session_id=client-mcp|reuse_policy=reuse_if_alive|target_id=local-shell|command=echo anonymous-ok";
        let request = format!(
            "POST /tool/terminal.exec HTTP/1.1\r\nHost: {addr}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let mut stream = TcpStream::connect(addr).expect("connect model-plane");
        stream.write_all(request.as_bytes()).expect("write request");
        let mut response = String::new();
        stream.read_to_string(&mut response).expect("read response");
        serve_thread.join().expect("join serve thread");

        assert!(response.starts_with("HTTP/1.1 403"), "response={response}");
        assert!(response.contains("anonymous_compat_disabled"));
    }

    #[test]
    fn model_plane_terminal_exec_rejects_missing_token_for_sealed_target() {
        let root = temp_dir("model-plane-anon-sealed-deny");
        let settings = settings_with_extra_targets(
            "confirm_if_family",
            r#"
[[targets]]
id = "sealed-shell"
display_name = "Sealed Shell"
kind = "localshell"
enabled = true
aliases = ["sealed-shell"]
storage_class = "sealed-overlay"
access_class = "token-scoped"
sealed_profile_ref = "vault://bridgingio/targets/sealed-shell"

[targets.connection]

[targets.providers.terminal]
enabled = true
"#,
        );
        let mut settings = settings;
        settings.model_plane.http.port = 0;
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(Vec::new()),
            &root,
            Vec::new(),
        );
        let runtime =
            StandaloneCoreRuntime::from_settings(settings.clone(), resolver).expect("runtime");
        let shared = runtime.shared();
        let server = ModelPlaneHttpServer::bind(shared, &settings).expect("bind model-plane");
        let addr = server.local_addr().expect("local addr");
        let serve_thread = thread::spawn(move || server.serve_once().expect("serve once"));

        let body = "agent_id=agent-mcp|run_id=run-mcp|client_session_id=client-mcp|reuse_policy=reuse_if_alive|target_id=sealed-shell|command=echo should-not-run";
        let request = format!(
            "POST /tool/terminal.exec HTTP/1.1\r\nHost: {addr}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let mut stream = TcpStream::connect(addr).expect("connect model-plane");
        stream.write_all(request.as_bytes()).expect("write request");
        let mut response = String::new();
        stream.read_to_string(&mut response).expect("read response");
        serve_thread.join().expect("join serve thread");

        assert!(response.starts_with("HTTP/1.1 403"), "response={response}");
        assert!(response.contains("anonymous_compat_denied_for_target"));
    }

    #[test]
    fn model_plane_terminal_exec_invalid_token_does_not_fallback_to_anonymous() {
        let root = temp_dir("model-plane-invalid-token-no-fallback");
        let settings = settings_with_extra_targets(
            "confirm_if_family",
            r#"
[[targets]]
id = "local-shell"
display_name = "Local Shell"
kind = "localshell"
enabled = true
aliases = ["local-shell"]
storage_class = "plain"
access_class = "anonymous-local"

[targets.connection]

[targets.providers.terminal]
enabled = true
"#,
        );
        let mut settings = settings;
        settings.model_plane.http.port = 0;
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(Vec::new()),
            &root,
            Vec::new(),
        );
        let runtime =
            StandaloneCoreRuntime::from_settings(settings.clone(), resolver).expect("runtime");
        let shared = runtime.shared();
        let server = ModelPlaneHttpServer::bind(shared, &settings).expect("bind model-plane");
        let addr = server.local_addr().expect("local addr");
        let serve_thread = thread::spawn(move || server.serve_once().expect("serve once"));

        let body = "agent_id=agent-mcp|run_id=run-mcp|client_session_id=client-mcp|reuse_policy=reuse_if_alive|target_id=local-shell|command=echo no-fallback";
        let request = format!(
            "POST /tool/terminal.exec HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer invalid-token\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let mut stream = TcpStream::connect(addr).expect("connect model-plane");
        stream.write_all(request.as_bytes()).expect("write request");
        let mut response = String::new();
        stream.read_to_string(&mut response).expect("read response");
        serve_thread.join().expect("join serve thread");

        assert!(response.starts_with("HTTP/1.1 403"), "response={response}");
        assert!(response.contains("invalid_or_expired_token"));
        assert!(!response.contains("anonymous_compat"));
    }

    #[test]
    fn list_profiles_redacts_sealed_target_while_vault_locked() {
        let mut runtime = sealed_overlay_runtime("sealed-catalog-redacted-locked");
        install_sealed_overlay(
            &mut runtime,
            "sealed-shell",
            None,
            Some("sealed notes should not be visible while locked"),
        );
        runtime
            .vault_router
            .set_unlock_policy(VaultUnlockPolicy {
                trigger_policy: VaultUnlockTriggerPolicy::ManualOnly,
                allowed_methods: vec!["os-native".into(), "passphrase".into()],
                preferred_method: "os-native".into(),
                cache_ttl_sec: 600,
                require_fresh_user_verification: true,
            })
            .expect("manual unlock policy");
        runtime.vault_router.lock_vault("test lock").expect("lock vault");

        let response = runtime.handle_app_request(ApiRequest {
            request_id: "list-profiles-locked".into(),
            context: request_context(),
            command: AppCommand::ListProfiles,
        });
        let items = match response {
            ApiResponse::Profiles { items, .. } => items,
            other => panic!("unexpected list profiles response: {other:?}"),
        };
        let sealed = items
            .into_iter()
            .find(|item| item.id == "sealed-shell")
            .expect("sealed profile");

        assert_eq!(
            sealed
                .metadata
                .get(super::TARGET_CATALOG_PROJECTION_STATE_METADATA_KEY)
                .map(String::as_str),
            Some("sealed-redacted-locked")
        );
        assert!(sealed.notes.is_none());
        assert!(sealed.credential_ref.is_none());
        assert_eq!(
            sealed
                .metadata
                .get(super::TARGET_SEALED_DESCRIPTOR_DIAGNOSTIC_METADATA_KEY)
                .map(String::as_str),
            Some("sealed target descriptor is redacted while vault is locked")
        );
    }

    #[test]
    fn list_profiles_resolves_sealed_target_after_unlock_with_valid_digest() {
        let mut runtime = sealed_overlay_runtime("sealed-catalog-resolved-unlocked");
        install_sealed_overlay(
            &mut runtime,
            "sealed-shell",
            None,
            Some("sealed notes resolved"),
        );

        let response = runtime.handle_app_request(ApiRequest {
            request_id: "list-profiles-unlocked".into(),
            context: request_context(),
            command: AppCommand::ListProfiles,
        });
        let items = match response {
            ApiResponse::Profiles { items, .. } => items,
            other => panic!("unexpected list profiles response: {other:?}"),
        };
        let sealed = items
            .into_iter()
            .find(|item| item.id == "sealed-shell")
            .expect("sealed profile");

        assert_eq!(
            sealed
                .metadata
                .get(super::TARGET_CATALOG_PROJECTION_STATE_METADATA_KEY)
                .map(String::as_str),
            Some("sealed-resolved")
        );
        assert_eq!(sealed.notes.as_deref(), Some("sealed notes resolved"));
        assert_eq!(
            sealed.credential_ref.as_ref().map(|item| item.id.as_str()),
            Some("vault://bridgingio/ssh-private-key/sealed-overlay")
        );
        assert!(sealed
            .metadata
            .get(super::TARGET_SEALED_DESCRIPTOR_DIAGNOSTIC_METADATA_KEY)
            .is_none());
    }

    #[test]
    fn list_profiles_reports_descriptor_tamper_diagnostic_for_sealed_overlay() {
        let mut runtime = sealed_overlay_runtime("sealed-catalog-tamper");
        install_sealed_overlay(
            &mut runtime,
            "sealed-shell",
            Some("deadbeef"),
            Some("sealed notes should not be visible after tamper"),
        );

        let response = runtime.handle_app_request(ApiRequest {
            request_id: "list-profiles-tamper".into(),
            context: request_context(),
            command: AppCommand::ListProfiles,
        });
        let items = match response {
            ApiResponse::Profiles { items, .. } => items,
            other => panic!("unexpected list profiles response: {other:?}"),
        };
        let sealed = items
            .into_iter()
            .find(|item| item.id == "sealed-shell")
            .expect("sealed profile");

        assert_eq!(
            sealed
                .metadata
                .get(super::TARGET_CATALOG_PROJECTION_STATE_METADATA_KEY)
                .map(String::as_str),
            Some("sealed-redacted-tamper")
        );
        assert!(sealed.notes.is_none());
        assert!(sealed.credential_ref.is_none());
        assert!(sealed
            .metadata
            .get(super::TARGET_SEALED_DESCRIPTOR_DIAGNOSTIC_METADATA_KEY)
            .map(String::as_str)
            .unwrap_or_default()
            .contains("descriptor tamper detected"));
    }

    #[test]
    fn execution_resolve_rejects_sealed_target_when_vault_locked() {
        let mut runtime = sealed_overlay_runtime("sealed-execution-locked-reject");
        install_sealed_overlay(&mut runtime, "sealed-shell", None, Some("sealed notes"));
        runtime
            .vault_router
            .set_unlock_policy(VaultUnlockPolicy {
                trigger_policy: VaultUnlockTriggerPolicy::ManualOnly,
                allowed_methods: vec!["os-native".into(), "passphrase".into()],
                preferred_method: "os-native".into(),
                cache_ttl_sec: 600,
                require_fresh_user_verification: true,
            })
            .expect("manual unlock policy");
        runtime.vault_router.lock_vault("test lock").expect("lock vault");

        let err = runtime
            .resolve_target_profile_for_execution_by_ref("sealed-shell")
            .expect_err("locked sealed target should be rejected");
        match err {
            CoreRuntimeError::Config(message) => {
                assert!(message.contains("requires unlocked vault before execution"));
            }
            other => panic!("expected config error, got {other:?}"),
        }
    }

    #[test]
    fn model_plane_terminal_exec_rejects_bearer_token_outside_target_scope() {
        let root = temp_dir("model-plane-bearer-scope-deny");
        let settings = bridgingio_engine::CoreSettings::from_toml_str(
            &bridgingio_engine::CoreSettings::minimal_example().replace("port = 19718", "port = 0"),
        )
        .expect("parse settings");
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(Vec::new()),
            &root,
            Vec::new(),
        );
        let mut runtime =
            StandaloneCoreRuntime::from_settings(settings.clone(), resolver).expect("runtime");
        let scope = AgentTokenScopeView {
            scope_profile: Some("strict-default".into()),
            target_ids: vec!["another-target".into()],
            tool_ids: Vec::new(),
            max_risk_envelope: Some("deny-all".into()),
            allow_open_shell: Some(false),
            allow_write_shell_input: Some(false),
            allow_artifact_cross_principal: Some(false),
            allow_delegation: Some(false),
            allow_admin_actions: Some(false),
        };
        let attestation_id =
            create_token_attestation_id(&mut runtime, "limited-token", None, scope.clone());
        let created = runtime.handle_app_request(ApiRequest {
            request_id: "token-create".into(),
            context: request_context(),
            command: AppCommand::CreateAgentToken {
                label: "limited-token".into(),
                expires_in_seconds: None,
                scope,
                attestation_id: Some(attestation_id),
            },
        });
        let token = match created {
            ApiResponse::AgentTokenCreated { result, .. } => result.plaintext_token,
            other => panic!("unexpected token create response: {other:?}"),
        };

        let shared = runtime.shared();
        let server = ModelPlaneHttpServer::bind(shared, &settings).expect("bind model-plane");
        let addr = server.local_addr().expect("local addr");
        let serve_thread = thread::spawn(move || server.serve_once().expect("serve once"));

        let body = "agent_id=agent-mcp|run_id=run-mcp|client_session_id=client-mcp|reuse_policy=reuse_if_alive|target_id=local-ssh|command=echo";
        let request = format!(
            "POST /tool/terminal.exec HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {token}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let mut stream = TcpStream::connect(addr).expect("connect model-plane");
        stream.write_all(request.as_bytes()).expect("write request");
        let mut response = String::new();
        stream.read_to_string(&mut response).expect("read response");
        serve_thread.join().expect("join serve thread");

        assert!(response.starts_with("HTTP/1.1 403"), "response={response}");
        assert!(response.contains("token_scope_denied_for_target"));
    }

    #[test]
    fn model_plane_terminal_exec_rejects_bearer_token_without_tool_scope() {
        let root = temp_dir("model-plane-bearer-tool-scope-deny");
        let settings = bridgingio_engine::CoreSettings::from_toml_str(
            &bridgingio_engine::CoreSettings::minimal_example().replace("port = 19718", "port = 0"),
        )
        .expect("parse settings");
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(Vec::new()),
            &root,
            Vec::new(),
        );
        let mut runtime =
            StandaloneCoreRuntime::from_settings(settings.clone(), resolver).expect("runtime");
        let scope = AgentTokenScopeView {
            scope_profile: Some("strict-default".into()),
            target_ids: vec!["local-ssh".into()],
            tool_ids: Vec::new(),
            max_risk_envelope: Some("allow-low".into()),
            allow_open_shell: Some(true),
            allow_write_shell_input: Some(true),
            allow_artifact_cross_principal: Some(false),
            allow_delegation: Some(false),
            allow_admin_actions: Some(false),
        };
        let attestation_id =
            create_token_attestation_id(&mut runtime, "limited-token-tool", None, scope.clone());
        let created = runtime.handle_app_request(ApiRequest {
            request_id: "token-create".into(),
            context: request_context(),
            command: AppCommand::CreateAgentToken {
                label: "limited-token-tool".into(),
                expires_in_seconds: None,
                scope,
                attestation_id: Some(attestation_id),
            },
        });
        let token = match created {
            ApiResponse::AgentTokenCreated { result, .. } => result.plaintext_token,
            other => panic!("unexpected token create response: {other:?}"),
        };

        let shared = runtime.shared();
        let server = ModelPlaneHttpServer::bind(shared, &settings).expect("bind model-plane");
        let addr = server.local_addr().expect("local addr");
        let serve_thread = thread::spawn(move || server.serve_once().expect("serve once"));

        let body = "agent_id=agent-mcp|run_id=run-mcp|client_session_id=client-mcp|reuse_policy=reuse_if_alive|target_id=local-ssh|command=echo";
        let request = format!(
            "POST /tool/terminal.exec HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {token}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let mut stream = TcpStream::connect(addr).expect("connect model-plane");
        stream.write_all(request.as_bytes()).expect("write request");
        let mut response = String::new();
        stream.read_to_string(&mut response).expect("read response");
        serve_thread.join().expect("join serve thread");

        assert!(response.starts_with("HTTP/1.1 403"), "response={response}");
        assert!(response.contains("token_scope_denied_for_tool"));
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
    fn structured_exec_invocation_uses_unified_pipeline() {
        let root = temp_dir("structured-exec-invocation");
        let settings = bridgingio_engine::CoreSettings::from_toml_str(
            &bridgingio_engine::CoreSettings::minimal_example(),
        )
        .expect("parse settings");
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(Vec::new()),
            &root,
            Vec::new(),
        );
        let runtime = StandaloneCoreRuntime::from_settings(settings, resolver).expect("runtime");
        let target = runtime
            .resolve_target_profile_by_ref("local")
            .expect("local target");
        let invocation = runtime
            .resolve_structured_exec_invocation(&target, "uname -r")
            .expect("resolve invocation")
            .expect("ssh invocation");
        assert_eq!(invocation.target_shell_dialect.as_str(), "ssh-posix");
        let view = super::invocation_json(Some(&invocation));
        assert_eq!(view["mode"].as_str(), Some("one_shot"));
        assert_eq!(
            view["target_shell_dialect"]["name"].as_str(),
            Some("ssh-posix")
        );
        assert!(view["quoting_boundary"]["host_shell_runtime"]
            .as_str()
            .unwrap_or_default()
            .contains("host shell"));
    }

    #[test]
    fn secret_backed_ssh_delivery_injects_broker_args_and_tracks_lifecycle() {
        let root = temp_dir("secret-backed-ssh-delivery");
        let settings = bridgingio_engine::CoreSettings::from_toml_str(
            &bridgingio_engine::CoreSettings::minimal_example(),
        )
        .expect("parse settings");
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(Vec::new()),
            &root,
            Vec::new(),
        );
        let mut runtime =
            StandaloneCoreRuntime::from_settings(settings, resolver).expect("runtime");
        runtime
            .vault_router
            .set_active_backend("builtin-encrypted")
            .expect("switch vault backend");
        runtime
            .vault_router
            .put("vault:ssh-key:ops", "OPS-KEY", "ops key")
            .expect("store key");

        let mut target = runtime
            .resolve_target_profile_by_ref("local")
            .expect("local target");
        target.metadata.insert(
            "ssh.delivery_mode".to_string(),
            "ssh-agent-broker".to_string(),
        );
        target
            .metadata
            .insert("ssh.host_key_policy".to_string(), "accept-new".to_string());
        target.credential_ref = Some(bridgingio_domain::CredentialRef {
            id: "vault:ssh-key:ops".into(),
            provider: "vault".into(),
        });

        let mut invocation = runtime
            .resolve_structured_exec_invocation(&target, "whoami")
            .expect("resolve invocation")
            .expect("invocation");
        let session_id = runtime
            .prepare_ssh_secret_delivery_for_invocation(
                &target,
                &context(
                    "agent-ssh-broker",
                    "run-ssh-broker",
                    "client-ssh-broker",
                    bridgingio_domain::SessionReusePolicy::ReuseIfAlive,
                ),
                &mut invocation,
                Some("channel-ssh-1"),
            )
            .expect("prepare delivery")
            .expect("broker session id");

        assert!(invocation
            .args
            .iter()
            .any(|arg| arg.contains("IdentityAgent=")));
        assert!(invocation
            .args
            .iter()
            .any(|arg| arg.contains("StrictHostKeyChecking=accept-new")));
        assert!(invocation
            .resolution
            .warnings
            .iter()
            .any(|line| line.contains("ssh broker session")));

        let summary = runtime
            .vault_router
            .ssh_agent_broker_session_summary(&session_id)
            .expect("summary");
        assert_eq!(summary.state.as_str(), "attached");
        assert_eq!(summary.attached_channel_count, 1);

        runtime
            .vault_router
            .detach_ssh_agent_broker_session(&session_id, "channel-ssh-1")
            .expect("detach");
        let closed = runtime
            .vault_router
            .close_ssh_agent_broker_session(&session_id, "test cleanup")
            .expect("close");
        assert_eq!(closed.state.as_str(), "closed");
    }

    #[test]
    fn secret_backed_ssh_delivery_rejects_when_vault_not_unlocked() {
        let root = temp_dir("secret-backed-ssh-locked-gate");
        let settings = bridgingio_engine::CoreSettings::from_toml_str(
            &bridgingio_engine::CoreSettings::minimal_example(),
        )
        .expect("parse settings");
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(Vec::new()),
            &root,
            Vec::new(),
        );
        let mut runtime =
            StandaloneCoreRuntime::from_settings(settings, resolver).expect("runtime");
        runtime
            .vault_router
            .set_active_backend("builtin-encrypted")
            .expect("switch vault backend");
        runtime
            .vault_router
            .put("vault:ssh-key:ops", "OPS-KEY", "ops key")
            .expect("store key");

        let mut target = runtime
            .resolve_target_profile_by_ref("local")
            .expect("local target");
        target.metadata.insert(
            "ssh.delivery_mode".to_string(),
            "ssh-agent-broker".to_string(),
        );
        target.credential_ref = Some(bridgingio_domain::CredentialRef {
            id: "vault:ssh-key:ops".into(),
            provider: "vault".into(),
        });

        runtime
            .vault_router
            .set_unlock_policy(VaultUnlockPolicy {
                trigger_policy: VaultUnlockTriggerPolicy::ManualOnly,
                allowed_methods: vec!["os-native".into(), "passphrase".into()],
                preferred_method: "os-native".into(),
                cache_ttl_sec: 600,
                require_fresh_user_verification: true,
            })
            .expect("manual policy");
        runtime.vault_router.lock_vault("test lock").expect("lock vault");

        let mut locked_invocation = runtime
            .resolve_structured_exec_invocation(&target, "whoami")
            .expect("resolve invocation")
            .expect("invocation");
        let locked_err = runtime
            .prepare_ssh_secret_delivery_for_invocation(
                &target,
                &context(
                    "agent-ssh-locked",
                    "run-ssh-locked",
                    "client-ssh-locked",
                    bridgingio_domain::SessionReusePolicy::ReuseIfAlive,
                ),
                &mut locked_invocation,
                Some("channel-ssh-locked"),
            )
            .expect_err("locked vault must block secret-backed ssh delivery");
        match locked_err {
            CoreRuntimeError::Config(message) => {
                assert!(message.contains("VaultLocked"));
            }
            other => panic!("expected config error, got {other:?}"),
        }

        runtime
            .vault_router
            .set_unlock_policy(VaultUnlockPolicy {
                trigger_policy: VaultUnlockTriggerPolicy::OnCoreStart,
                allowed_methods: vec!["passphrase".into()],
                preferred_method: "passphrase".into(),
                cache_ttl_sec: 600,
                require_fresh_user_verification: true,
            })
            .expect("on-core-start policy");

        let mut unavailable_invocation = runtime
            .resolve_structured_exec_invocation(&target, "whoami")
            .expect("resolve invocation")
            .expect("invocation");
        let unavailable_err = runtime
            .prepare_ssh_secret_delivery_for_invocation(
                &target,
                &context(
                    "agent-ssh-unavailable",
                    "run-ssh-unavailable",
                    "client-ssh-unavailable",
                    bridgingio_domain::SessionReusePolicy::ReuseIfAlive,
                ),
                &mut unavailable_invocation,
                Some("channel-ssh-unavailable"),
            )
            .expect_err("unavailable vault must block secret-backed ssh delivery");
        match unavailable_err {
            CoreRuntimeError::Config(message) => {
                assert!(
                    message.contains("VaultUnavailable") || message.contains("VaultLocked")
                );
            }
            other => panic!("expected config error, got {other:?}"),
        }
    }

    #[test]
    fn startup_unlock_requires_secret_and_unlocks_with_passphrase() {
        let root = temp_dir("startup-unlock-passphrase");
        let settings = bridgingio_engine::CoreSettings::from_toml_str(
            &bridgingio_engine::CoreSettings::minimal_example(),
        )
        .expect("parse settings");
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(Vec::new()),
            &root,
            Vec::new(),
        );
        let mut runtime =
            StandaloneCoreRuntime::from_settings(settings, resolver).expect("runtime");
        runtime
            .vault_router
            .set_active_backend("builtin-encrypted")
            .expect("switch vault backend");
        runtime
            .vault_router
            .unlock_with_os_native()
            .expect("unlock with os-native");
        runtime
            .vault_router
            .configure_passphrase_protector("correct horse battery staple")
            .expect("configure passphrase protector");
        runtime.vault_router.lock_vault("test lock").expect("lock vault");
        runtime
            .vault_router
            .set_unlock_policy(VaultUnlockPolicy {
                trigger_policy: VaultUnlockTriggerPolicy::OnCoreStart,
                allowed_methods: vec!["passphrase".into()],
                preferred_method: "passphrase".into(),
                cache_ttl_sec: 600,
                require_fresh_user_verification: true,
            })
            .expect("on-core-start policy");

        assert!(runtime.requires_startup_secret_input());
        assert!(runtime.complete_startup_unlock_with_secret(None).is_err());
        runtime
            .complete_startup_unlock_with_secret(Some("correct horse battery staple"))
            .expect("startup unlock with passphrase");
        assert_eq!(runtime.vault_router.vault_lock_state(), VaultLockState::Unlocked);
    }

    #[test]
    fn invocation_json_separates_host_runtime_and_target_terminal_dimensions() {
        let root = temp_dir("invocation-dimension-separation");
        let settings = bridgingio_engine::CoreSettings::from_toml_str(
            &bridgingio_engine::CoreSettings::minimal_example(),
        )
        .expect("parse settings");
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(Vec::new()),
            &root,
            Vec::new(),
        );
        let runtime = StandaloneCoreRuntime::from_settings(settings, resolver).expect("runtime");
        let target = runtime
            .resolve_target_profile_by_ref("local")
            .expect("local target");
        let invocation = runtime
            .resolve_structured_exec_invocation(&target, "pwd")
            .expect("resolve invocation")
            .expect("ssh invocation");
        let view = super::invocation_json(Some(&invocation));

        assert!(view["quoting_boundary"]["host_shell_runtime"]
            .as_str()
            .unwrap_or_default()
            .contains("host shell"));
        assert_eq!(view["target_terminal"]["family"].as_str(), Some("terminal"));
        assert_eq!(
            view["target_terminal"]["concurrency_policy"].as_str(),
            Some("multiplexed")
        );
        assert_eq!(
            view["target_shell_dialect"]["name"].as_str(),
            Some("ssh-posix")
        );
    }

    #[test]
    fn target_terminal_shell_config_selects_future_dialect_with_deferred_semantics() {
        let root = temp_dir("target-shell-override");
        let mut settings = bridgingio_engine::CoreSettings::from_toml_str(
            &bridgingio_engine::CoreSettings::minimal_example(),
        )
        .expect("parse settings");
        let target = settings
            .targets
            .iter_mut()
            .find(|entry| entry.id == "local-ssh")
            .expect("local-ssh target");
        target.terminal_provider.shell = Some("powershell".to_string());
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(Vec::new()),
            &root,
            Vec::new(),
        );
        let runtime = StandaloneCoreRuntime::from_settings(settings, resolver).expect("runtime");
        let target = runtime
            .resolve_target_profile_by_ref("local")
            .expect("local target");
        let invocation = runtime
            .resolve_structured_exec_invocation(&target, "whoami")
            .expect("resolve invocation")
            .expect("ssh invocation");
        let view = invocation.diagnostics_view();
        assert_eq!(view.target_shell_dialect, "ssh-powershell");
        assert_eq!(view.dialect_support, "deferred");
        assert!(view
            .warnings
            .iter()
            .any(|warning| warning.contains("deferred")));
    }

    #[test]
    fn interactive_shell_open_carries_structured_invocation() {
        let root = temp_dir("interactive-invocation");
        #[cfg(windows)]
        let ssh_mock = root.join("ssh.bat");
        #[cfg(not(windows))]
        let ssh_mock = root.join("ssh");
        #[cfg(windows)]
        fs::write(
            &ssh_mock,
            r#"@echo off
if "%~1"=="-p" (
  shift
  shift
  shift
)
cmd /Q
"#,
        )
        .expect("write ssh mock");
        #[cfg(not(windows))]
        fs::write(
            &ssh_mock,
            r#"#!/bin/sh
if [ "${1:-}" = "-p" ]; then
  shift
  shift
  shift
fi
exec /bin/sh -s
"#,
        )
        .expect("write ssh mock");
        #[cfg(unix)]
        {
            let mut perms = fs::metadata(&ssh_mock).expect("ssh metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&ssh_mock, perms).expect("chmod ssh");
        }
        let settings = bridgingio_engine::CoreSettings::from_toml_str(
            &bridgingio_engine::CoreSettings::minimal_example(),
        )
        .expect("parse settings");
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(vec![root.clone()]),
            &root,
            Vec::new(),
        );
        let mut runtime =
            StandaloneCoreRuntime::from_settings(settings, resolver).expect("runtime");
        let handle = runtime
            .open_interactive_shell(
                "local",
                context(
                    "agent-int",
                    "run-int",
                    "client-int",
                    bridgingio_domain::SessionReusePolicy::ReuseIfAlive,
                ),
            )
            .expect("open interactive shell");
        assert!(matches!(
            handle.launch_strategy.as_str(),
            "structured_interactive_invocation"
                | "structured_interactive_invocation_with_host_baseline_fallback"
        ));
        if handle.launch_fallback_applied {
            assert_eq!(
                handle.launch_strategy,
                "structured_interactive_invocation_with_host_baseline_fallback"
            );
        } else {
            assert_eq!(handle.launch_strategy, "structured_interactive_invocation");
            assert!(handle.launch_diagnostics.is_empty());
        }
        let invocation = handle.invocation.expect("invocation");
        assert_eq!(invocation.invocation_kind.as_str(), "interactive");
        assert_eq!(invocation.target_shell_dialect.as_str(), "ssh-posix");
    }

    #[test]
    fn interactive_shell_structured_launch_fallback_keeps_lifecycle_state_deterministic() {
        let root = temp_dir("interactive-fallback-lifecycle");
        #[cfg(windows)]
        let failing_ssh = root.join("ssh.bat");
        #[cfg(not(windows))]
        let failing_ssh = root.join("ssh");
        #[cfg(windows)]
        fs::write(
            &failing_ssh,
            r#"@echo off
echo forced ssh failure>&2
exit /b 1
"#,
        )
        .expect("write failing ssh");
        #[cfg(not(windows))]
        fs::write(
            &failing_ssh,
            r#"#!/bin/sh
echo "forced ssh failure" >&2
exit 1
"#,
        )
        .expect("write failing ssh");
        #[cfg(unix)]
        {
            let mut perms = fs::metadata(&failing_ssh)
                .expect("ssh metadata")
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&failing_ssh, perms).expect("chmod ssh");
        }
        let settings = bridgingio_engine::CoreSettings::from_toml_str(
            &bridgingio_engine::CoreSettings::minimal_example(),
        )
        .expect("parse settings");
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(vec![root.clone()]),
            &root,
            Vec::new(),
        );
        let mut runtime =
            StandaloneCoreRuntime::from_settings(settings, resolver).expect("runtime");
        let open_context = context(
            "agent-fb",
            "run-fb",
            "client-fb",
            bridgingio_domain::SessionReusePolicy::ReuseIfAlive,
        );
        let handle = runtime
            .open_interactive_shell("local", open_context.clone())
            .expect("open interactive shell");
        assert_eq!(
            handle.launch_strategy,
            "structured_interactive_invocation_with_host_baseline_fallback"
        );
        assert!(handle.launch_fallback_applied);
        assert!(handle
            .launch_diagnostics
            .iter()
            .any(|line| line.contains("structured interactive launch failed")));
        let invocation = handle.invocation.expect("invocation");
        assert!(invocation
            .resolution
            .warnings
            .iter()
            .any(|line| line.contains("structured interactive launch failed")));

        let read = runtime
            .read_interactive_shell(&handle.shell_id, 0, 20, open_context.clone())
            .expect("read interactive");
        assert!(!read.running);
        assert!(!read.closed);

        let interrupted = runtime
            .interrupt_interactive_shell(&handle.shell_id, open_context.clone())
            .expect("interrupt interactive");
        assert!(interrupted.interrupted);
        assert!(!interrupted.running);
        assert!(!interrupted.closed);

        let closed = runtime
            .close_interactive_shell(&handle.shell_id, open_context)
            .expect("close interactive");
        assert!(closed.closed);
        assert!(!closed.running);
    }

    #[test]
    fn target_terminal_shell_setting_roundtrips_via_profile_metadata() {
        let mut target = TargetProfile {
            id: "t-ssh".into(),
            name: "ssh-host".into(),
            kind: TargetKind::Ssh,
            connection: ConnectionConfig::Ssh {
                host: "10.1.1.8".into(),
                port: 22,
                username: "root".into(),
            },
            credential_ref: None,
            default_policy: PolicyProfile::default(),
            notes: None,
            metadata: BTreeMap::new(),
            toolchains: BTreeMap::new(),
        };
        target.metadata.insert(
            super::TARGET_TERMINAL_SHELL_METADATA_KEY.to_string(),
            "powershell".to_string(),
        );

        let standalone = super::to_standalone_target_profile(&target).expect("standalone profile");
        assert_eq!(
            standalone.terminal_provider.shell.as_deref(),
            Some("powershell")
        );
    }

    #[test]
    fn upsert_profile_normalizes_legacy_credential_ref_before_persisting() {
        let root = temp_dir("legacy-credential-ref");
        let settings = bridgingio_engine::CoreSettings::from_toml_str(
            &bridgingio_engine::CoreSettings::minimal_example(),
        )
        .expect("parse settings");
        let resolver = super::ToolchainResolver::new(
            ExecutableResolver::with_search_paths(Vec::new()),
            &root,
            Vec::new(),
        );
        let mut runtime =
            StandaloneCoreRuntime::from_settings(settings, resolver).expect("runtime");
        let config_path = root.join("managed-core.toml");
        fs::write(
            &config_path,
            bridgingio_engine::CoreSettings::minimal_example(),
        )
        .expect("write config");
        runtime.settings_store.runtime_metadata.config_path =
            Some(config_path.to_string_lossy().to_string());

        let response = runtime.handle_app_request(ApiRequest {
            request_id: "upsert-legacy-ref".into(),
            context: request_context(),
            command: AppCommand::UpsertProfile {
                profile: TargetProfile {
                    id: "legacy-ref-target".into(),
                    name: "Legacy Ref Target".into(),
                    kind: TargetKind::Ssh,
                    connection: ConnectionConfig::Ssh {
                        host: "127.0.0.1".into(),
                        port: 22,
                        username: "dev".into(),
                    },
                    credential_ref: Some(bridgingio_domain::CredentialRef {
                        id: "vault:ssh-key:ops_prod".into(),
                        provider: "legacy".into(),
                    }),
                    default_policy: PolicyProfile::default(),
                    notes: None,
                    metadata: BTreeMap::new(),
                    toolchains: BTreeMap::new(),
                },
            },
        });
        match response {
            ApiResponse::Accepted { apply_strategy, .. } => {
                assert_eq!(apply_strategy.as_deref(), Some("live_applied"));
            }
            other => panic!("unexpected upsert response: {other:?}"),
        }

        let profile_response = runtime.handle_app_request(ApiRequest {
            request_id: "get-legacy-ref".into(),
            context: request_context(),
            command: AppCommand::GetProfile {
                target_id: "legacy-ref-target".into(),
            },
        });
        let payload_json = match profile_response {
            ApiResponse::Profile { payload_json, .. } => payload_json,
            other => panic!("unexpected profile response: {other:?}"),
        };
        let payload: serde_json::Value =
            serde_json::from_str(&payload_json).expect("profile payload json");
        assert_eq!(
            payload["credential_ref"].as_str(),
            Some("vault://bridgingio/ssh-private-key/ops-prod")
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
        text = text.replace("__GLOBAL_ADB_OVERRIDE__", &global_adb.to_string_lossy());
        text = text.replace("__TARGET_ADB_OVERRIDE__", &target_adb.to_string_lossy());
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
                    item.target_id.as_deref() == Some("android-emulator") && item.command == "adb"
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
                    item.target_id.as_deref() == Some("android-emulator") && item.command == "adb"
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
                core_log_level: None,
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
                    item.target_id.as_deref() == Some("android-emulator") && item.command == "adb"
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
        text = text.replace("__GLOBAL_ADB_OVERRIDE__", "");
        text = text.replace("__TARGET_ADB_OVERRIDE__", "");
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
                    item.target_id.as_deref() == Some("android-emulator") && item.command == "adb"
                })
                .expect("adb diagnostics for android-emulator"),
            other => panic!("unexpected diagnostics response: {other:?}"),
        };
        assert_eq!(
            adb_diag.effective_scope.as_deref(),
            Some("builtin_fallback")
        );
        assert_eq!(
            adb_diag.effective_path.as_deref(),
            Some(bundled_adb.to_string_lossy().as_ref())
        );
    }
}
