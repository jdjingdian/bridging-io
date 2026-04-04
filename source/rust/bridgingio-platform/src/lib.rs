use std::collections::HashMap;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use bridgingio_domain::ContractStatus;
use serde::Serialize;

mod local_shell_runtime;

use local_shell_runtime::{BaselineLocalShellRuntime, InteractiveShellRuntimeSnapshot};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostPlatform {
    Unix,
    Windows,
    Unknown,
}

impl HostPlatform {
    pub fn current() -> Self {
        #[cfg(windows)]
        {
            return HostPlatform::Windows;
        }
        #[cfg(unix)]
        {
            return HostPlatform::Unix;
        }
        #[allow(unreachable_code)]
        HostPlatform::Unknown
    }

    pub fn as_str(self) -> &'static str {
        match self {
            HostPlatform::Unix => "unix",
            HostPlatform::Windows => "windows",
            HostPlatform::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityStatus {
    Ready,
    Degraded,
    Fallback,
    Unsupported,
}

impl CapabilityStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            CapabilityStatus::Ready => "ready",
            CapabilityStatus::Degraded => "degraded",
            CapabilityStatus::Fallback => "fallback",
            CapabilityStatus::Unsupported => "unsupported",
        }
    }

    pub fn contract_status(self) -> ContractStatus {
        match self {
            Self::Ready => ContractStatus::Ready,
            Self::Degraded => ContractStatus::Degraded,
            Self::Fallback => ContractStatus::Fallback,
            Self::Unsupported => ContractStatus::Unsupported,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostCapabilityDiagnostic {
    pub capability: &'static str,
    pub status: CapabilityStatus,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellLaunchMode {
    OneShot,
    Interactive,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellLaunchSpec {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalShellRuntimeError {
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalShellOneShotOutput {
    pub stdout_lines: Vec<String>,
    pub stderr_lines: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InteractiveShellSemantics {
    pub host_shell_default: &'static str,
    pub cwd_semantics: &'static str,
    pub env_semantics: &'static str,
    pub interrupt_semantics: &'static str,
    pub close_semantics: &'static str,
    pub prompt_semantics: &'static str,
    pub degraded_mode_semantics: &'static str,
    pub supports_tty_backend: bool,
    pub supports_pipe_backend: bool,
    pub default_completion_marker: &'static str,
    pub host_shell_label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InteractiveShellApiLayering {
    pub startup: &'static str,
    pub write: &'static str,
    pub read: &'static str,
    pub state_query: &'static str,
    pub interrupt: &'static str,
    pub close: &'static str,
    pub diagnostics: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InteractiveShellDiagnostics {
    pub shell_id: String,
    pub backend: String,
    pub degraded_mode: bool,
    pub running: bool,
    pub closed: bool,
    pub interrupted: bool,
    pub completion_marker: Option<String>,
    pub required_visible_states: Vec<String>,
    pub allowed_degraded_capabilities: Vec<String>,
    pub semantics: InteractiveShellSemantics,
    pub api_layering: InteractiveShellApiLayering,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalShellRuntimeSnapshot {
    pub shell_id: String,
    pub target_kind: String,
    pub prompt: String,
    pub cwd: String,
    pub env: HashMap<String, String>,
    pub transcript: Vec<String>,
    pub interrupted: bool,
    pub closed: bool,
    pub running: bool,
    pub completion_marker: Option<String>,
    pub backend: String,
    pub degraded_mode: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalShellRuntimeWriteOutcome {
    pub output_lines: Vec<String>,
    pub snapshot: LocalShellRuntimeSnapshot,
}

pub trait LocalShellRuntime: Send + Sync {
    fn status(&self) -> CapabilityStatus;
    fn default_shell_label(&self) -> &'static str;
    fn launch_spec(&self, mode: ShellLaunchMode, command: Option<&str>) -> ShellLaunchSpec;
    fn run_one_shot(
        &self,
        command: &str,
    ) -> Result<LocalShellOneShotOutput, LocalShellRuntimeError>;
    fn open_interactive_shell(
        &self,
        shell_id: &str,
        target_kind: &str,
        launch_command: Option<&str>,
    ) -> Result<LocalShellRuntimeSnapshot, LocalShellRuntimeError>;
    fn write_interactive_shell(
        &self,
        shell_id: &str,
        command: &str,
        completion_marker: String,
    ) -> Result<LocalShellRuntimeWriteOutcome, LocalShellRuntimeError>;
    fn read_interactive_transcript(
        &self,
        shell_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<String>, LocalShellRuntimeError>;
    fn interactive_shell_state(
        &self,
        shell_id: &str,
    ) -> Result<LocalShellRuntimeSnapshot, LocalShellRuntimeError>;
    fn interrupt_interactive_shell(
        &self,
        shell_id: &str,
    ) -> Result<LocalShellRuntimeSnapshot, LocalShellRuntimeError>;
    fn close_interactive_shell(
        &self,
        shell_id: &str,
    ) -> Result<LocalShellRuntimeSnapshot, LocalShellRuntimeError>;
    fn interactive_shell_diagnostics(
        &self,
        shell_id: &str,
    ) -> Result<InteractiveShellDiagnostics, LocalShellRuntimeError>;
    fn semantics(&self) -> InteractiveShellSemantics;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlPlaneLifecycleSemantics {
    pub attach: &'static str,
    pub request_response: &'static str,
    pub lifecycle: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlPlaneEndpointSemantics {
    pub endpoint: String,
    pub naming_rule: String,
    pub local_only: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlPlaneTransportDiagnostic {
    pub code: &'static str,
    pub status: CapabilityStatus,
    pub message: String,
    pub recovery_hint: String,
}

pub trait ControlPlaneTransport: Send + Sync {
    fn status(&self) -> CapabilityStatus;
    fn transport_kind(&self) -> &'static str;
    fn endpoint(&self, instance_name: &str, paths: &RuntimePaths) -> String;
    fn endpoint_semantics(
        &self,
        instance_name: &str,
        paths: &RuntimePaths,
    ) -> ControlPlaneEndpointSemantics;
    fn lifecycle_semantics(&self) -> ControlPlaneLifecycleSemantics;
    fn diagnostics(
        &self,
        instance_name: &str,
        paths: &RuntimePaths,
    ) -> Vec<ControlPlaneTransportDiagnostic>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimePaths {
    pub data_dir: PathBuf,
    pub state_dir: PathBuf,
    pub metadata_path: PathBuf,
    pub artifact_root: PathBuf,
    pub temp_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub control_plane_endpoint: String,
}

pub trait RuntimePathsAdapter: Send + Sync {
    fn status(&self) -> CapabilityStatus;
    fn default_data_dir(&self, instance_name: &str) -> PathBuf;
    fn expand_user_path(&self, raw: &str) -> PathBuf;
    fn runtime_paths(&self, instance_name: &str, data_dir: &Path) -> RuntimePaths;
}

pub trait ToolchainLocator: Send + Sync {
    fn status(&self) -> CapabilityStatus;
    fn source_label(&self) -> &'static str;
    fn resolution_order(&self) -> &'static [&'static str];
}

pub trait NativeVaultBinding: Send + Sync {
    fn status(&self) -> CapabilityStatus;
    fn backend_label(&self) -> &'static str;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RuntimeLogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl RuntimeLogLevel {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "error" => RuntimeLogLevel::Error,
            "warn" | "warning" => RuntimeLogLevel::Warn,
            "debug" => RuntimeLogLevel::Debug,
            "trace" => RuntimeLogLevel::Trace,
            _ => RuntimeLogLevel::Info,
        }
    }

    fn rank(self) -> u8 {
        match self {
            RuntimeLogLevel::Error => 1,
            RuntimeLogLevel::Warn => 2,
            RuntimeLogLevel::Info => 3,
            RuntimeLogLevel::Debug => 4,
            RuntimeLogLevel::Trace => 5,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            RuntimeLogLevel::Error => "error",
            RuntimeLogLevel::Warn => "warn",
            RuntimeLogLevel::Info => "info",
            RuntimeLogLevel::Debug => "debug",
            RuntimeLogLevel::Trace => "trace",
        }
    }

    fn enabled(self, level: RuntimeLogLevel) -> bool {
        level.rank() <= self.rank()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeLogCategory {
    Startup,
    Transport,
    Terminal,
    Toolchain,
    Vault,
    Decode,
    Policy,
    Mcp,
}

impl RuntimeLogCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            RuntimeLogCategory::Startup => "startup",
            RuntimeLogCategory::Transport => "transport",
            RuntimeLogCategory::Terminal => "terminal",
            RuntimeLogCategory::Toolchain => "toolchain",
            RuntimeLogCategory::Vault => "vault",
            RuntimeLogCategory::Decode => "decode",
            RuntimeLogCategory::Policy => "policy",
            RuntimeLogCategory::Mcp => "mcp",
        }
    }
}

pub trait RuntimeLogger: Send + Sync {
    fn status(&self) -> CapabilityStatus;
    fn level(&self) -> RuntimeLogLevel;
    fn log(&self, level: RuntimeLogLevel, category: RuntimeLogCategory, message: &str);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LocalOperatorSurface {
    Menuconfig,
    StandaloneCli,
}

impl LocalOperatorSurface {
    pub fn as_str(self) -> &'static str {
        match self {
            LocalOperatorSurface::Menuconfig => "menuconfig",
            LocalOperatorSurface::StandaloneCli => "standalone-cli",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalAuthorizationLogStream {
    Authorization,
    MenuconfigSession,
}

impl LocalAuthorizationLogStream {
    pub fn relative_path(self) -> &'static str {
        match self {
            LocalAuthorizationLogStream::Authorization => "logs/local-authorization.jsonl",
            LocalAuthorizationLogStream::MenuconfigSession => "logs/menuconfig-session.jsonl",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LocalAuthorizationEvent {
    pub timestamp_unix_ms: u64,
    pub flow_id: String,
    pub surface: LocalOperatorSurface,
    pub screen: String,
    pub action: String,
    pub operation: String,
    pub phase: String,
    pub result: String,
    pub dedupe_state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator_principal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_locator_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub byte_length: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_index: Option<usize>,
}

impl LocalAuthorizationEvent {
    pub fn new(
        flow_id: impl Into<String>,
        surface: LocalOperatorSurface,
        screen: impl Into<String>,
        action: impl Into<String>,
        operation: impl Into<String>,
        phase: impl Into<String>,
        result: impl Into<String>,
        dedupe_state: impl Into<String>,
    ) -> Self {
        Self {
            timestamp_unix_ms: unix_time_ms_now(),
            flow_id: flow_id.into(),
            surface,
            screen: screen.into(),
            action: action.into(),
            operation: operation.into(),
            phase: phase.into(),
            result: result.into(),
            dedupe_state: dedupe_state.into(),
            operator_principal: None,
            error_code: None,
            intent_id: None,
            source_kind: None,
            source_summary: None,
            source_locator_digest: None,
            byte_length: None,
            field_path: None,
            token_id: None,
            credential_ref: None,
            target_index: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalAuthorizationRecorder {
    logs_dir: PathBuf,
    level: RuntimeLogLevel,
}

impl LocalAuthorizationRecorder {
    pub fn for_runtime_root(runtime_root: &Path, level: RuntimeLogLevel) -> Self {
        Self {
            logs_dir: runtime_root.join("logs"),
            level,
        }
    }

    pub fn for_logs_dir(logs_dir: &Path, level: RuntimeLogLevel) -> Self {
        Self {
            logs_dir: logs_dir.to_path_buf(),
            level,
        }
    }

    pub fn append_event(
        &self,
        stream: LocalAuthorizationLogStream,
        level: RuntimeLogLevel,
        event: &LocalAuthorizationEvent,
    ) -> Result<(), String> {
        if !self.level.enabled(level) {
            return Ok(());
        }
        fs::create_dir_all(&self.logs_dir).map_err(|err| {
            format!(
                "create local authorization logs dir failed: {} ({err})",
                self.logs_dir.display()
            )
        })?;
        let path = self.stream_path(stream);
        let line = serde_json::to_string(event)
            .map_err(|err| format!("encode local authorization event failed: {err}"))?;
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&path)
            .map_err(|err| format!("open local authorization log failed: {} ({err})", path.display()))?;
        writeln!(file, "{line}")
            .map_err(|err| format!("append local authorization log failed: {} ({err})", path.display()))?;
        Ok(())
    }

    pub fn append_event_with_fallback(
        &self,
        stream: LocalAuthorizationLogStream,
        level: RuntimeLogLevel,
        event: &LocalAuthorizationEvent,
    ) {
        if let Err(err) = self.append_event(stream, level, event) {
            eprintln!("[bridgingio:authorization:warn] {err}");
        }
    }

    fn stream_path(&self, stream: LocalAuthorizationLogStream) -> PathBuf {
        let file = match stream {
            LocalAuthorizationLogStream::Authorization => "local-authorization.jsonl",
            LocalAuthorizationLogStream::MenuconfigSession => "menuconfig-session.jsonl",
        };
        self.logs_dir.join(file)
    }
}

static LOCAL_AUTH_FLOW_COUNTER: AtomicU64 = AtomicU64::new(1);

pub fn next_local_authorization_flow_id(operation: &str) -> String {
    let op = operation
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    let stamp = unix_time_ms_now();
    let seq = LOCAL_AUTH_FLOW_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("flow-{op}-{stamp}-{seq:08x}")
}

fn unix_time_ms_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedOutput {
    pub text: String,
    pub used_fallback: bool,
    pub normalized_newlines: bool,
    pub had_replacement_char: bool,
}

pub trait OutputDecoder: Send + Sync {
    fn status(&self) -> CapabilityStatus;
    fn decode(&self, bytes: &[u8]) -> DecodedOutput;

    fn decode_lines(&self, bytes: &[u8]) -> Vec<String> {
        self.decode(bytes)
            .text
            .lines()
            .map(ToString::to_string)
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostPlatformSnapshot {
    pub host_platform: HostPlatform,
    pub local_shell_runtime_status: CapabilityStatus,
    pub control_plane_transport_status: CapabilityStatus,
    pub runtime_paths_status: CapabilityStatus,
    pub toolchain_locator_status: CapabilityStatus,
    pub native_vault_status: CapabilityStatus,
    pub runtime_logger_status: CapabilityStatus,
    pub output_decoder_status: CapabilityStatus,
    pub diagnostics: Vec<HostCapabilityDiagnostic>,
}

pub trait HostPlatformAdapter: Send + Sync {
    fn host_platform(&self) -> HostPlatform;
    fn local_shell_runtime(&self) -> &dyn LocalShellRuntime;
    fn control_plane_transport(&self) -> &dyn ControlPlaneTransport;
    fn runtime_paths(&self) -> &dyn RuntimePathsAdapter;
    fn toolchain_locator(&self) -> &dyn ToolchainLocator;
    fn native_vault_binding(&self) -> &dyn NativeVaultBinding;
    fn runtime_logger(&self) -> &dyn RuntimeLogger;
    fn output_decoder(&self) -> &dyn OutputDecoder;
    fn diagnostics(&self) -> Vec<HostCapabilityDiagnostic>;

    fn snapshot(&self) -> HostPlatformSnapshot {
        HostPlatformSnapshot {
            host_platform: self.host_platform(),
            local_shell_runtime_status: self.local_shell_runtime().status(),
            control_plane_transport_status: self.control_plane_transport().status(),
            runtime_paths_status: self.runtime_paths().status(),
            toolchain_locator_status: self.toolchain_locator().status(),
            native_vault_status: self.native_vault_binding().status(),
            runtime_logger_status: self.runtime_logger().status(),
            output_decoder_status: self.output_decoder().status(),
            diagnostics: self.diagnostics(),
        }
    }
}

pub fn detect_host_platform_adapter(log_level: &str) -> Box<dyn HostPlatformAdapter> {
    match HostPlatform::current() {
        HostPlatform::Unix => Box::new(unix_baseline_adapter(log_level)),
        HostPlatform::Windows => Box::new(windows_baseline_adapter(log_level)),
        HostPlatform::Unknown => Box::new(unknown_baseline_adapter(log_level)),
    }
}

struct BaselineHostPlatformAdapter {
    host_platform: HostPlatform,
    local_shell_runtime: Box<dyn LocalShellRuntime>,
    control_plane_transport: Box<dyn ControlPlaneTransport>,
    runtime_paths: PlatformRuntimePathsAdapter,
    toolchain_locator: BaselineToolchainLocator,
    native_vault_binding: BaselineNativeVaultBinding,
    runtime_logger: StderrRuntimeLogger,
    output_decoder: PlatformOutputDecoder,
    diagnostics: Vec<HostCapabilityDiagnostic>,
}

impl HostPlatformAdapter for BaselineHostPlatformAdapter {
    fn host_platform(&self) -> HostPlatform {
        self.host_platform
    }

    fn local_shell_runtime(&self) -> &dyn LocalShellRuntime {
        self.local_shell_runtime.as_ref()
    }

    fn control_plane_transport(&self) -> &dyn ControlPlaneTransport {
        self.control_plane_transport.as_ref()
    }

    fn runtime_paths(&self) -> &dyn RuntimePathsAdapter {
        &self.runtime_paths
    }

    fn toolchain_locator(&self) -> &dyn ToolchainLocator {
        &self.toolchain_locator
    }

    fn native_vault_binding(&self) -> &dyn NativeVaultBinding {
        &self.native_vault_binding
    }

    fn runtime_logger(&self) -> &dyn RuntimeLogger {
        &self.runtime_logger
    }

    fn output_decoder(&self) -> &dyn OutputDecoder {
        &self.output_decoder
    }

    fn diagnostics(&self) -> Vec<HostCapabilityDiagnostic> {
        self.diagnostics.clone()
    }
}

struct UnixLocalShellRuntime {
    inner: BaselineLocalShellRuntime,
}

impl UnixLocalShellRuntime {
    fn new() -> Self {
        Self {
            inner: BaselineLocalShellRuntime::new(HostPlatform::Unix, "sh"),
        }
    }
}

impl LocalShellRuntime for UnixLocalShellRuntime {
    fn status(&self) -> CapabilityStatus {
        self.inner.status()
    }

    fn default_shell_label(&self) -> &'static str {
        self.inner.default_shell_label()
    }

    fn launch_spec(&self, mode: ShellLaunchMode, command: Option<&str>) -> ShellLaunchSpec {
        self.inner.launch_spec(mode, command)
    }

    fn run_one_shot(
        &self,
        command: &str,
    ) -> Result<LocalShellOneShotOutput, LocalShellRuntimeError> {
        self.inner.run_one_shot(command)
    }

    fn open_interactive_shell(
        &self,
        shell_id: &str,
        target_kind: &str,
        launch_command: Option<&str>,
    ) -> Result<LocalShellRuntimeSnapshot, LocalShellRuntimeError> {
        self.inner
            .open_interactive_shell(shell_id, target_kind, launch_command)
            .map(runtime_snapshot_to_public)
    }

    fn write_interactive_shell(
        &self,
        shell_id: &str,
        command: &str,
        completion_marker: String,
    ) -> Result<LocalShellRuntimeWriteOutcome, LocalShellRuntimeError> {
        self.inner
            .write_interactive_shell(shell_id, command, completion_marker)
            .map(|outcome| LocalShellRuntimeWriteOutcome {
                output_lines: outcome.output_lines,
                snapshot: outcome.snapshot,
            })
    }

    fn read_interactive_transcript(
        &self,
        shell_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<String>, LocalShellRuntimeError> {
        self.inner
            .read_interactive_transcript(shell_id, offset, limit)
    }

    fn interactive_shell_state(
        &self,
        shell_id: &str,
    ) -> Result<LocalShellRuntimeSnapshot, LocalShellRuntimeError> {
        self.inner
            .interactive_shell_state(shell_id)
            .map(runtime_snapshot_to_public)
    }

    fn interrupt_interactive_shell(
        &self,
        shell_id: &str,
    ) -> Result<LocalShellRuntimeSnapshot, LocalShellRuntimeError> {
        self.inner
            .interrupt_interactive_shell(shell_id)
            .map(runtime_snapshot_to_public)
    }

    fn close_interactive_shell(
        &self,
        shell_id: &str,
    ) -> Result<LocalShellRuntimeSnapshot, LocalShellRuntimeError> {
        self.inner
            .close_interactive_shell(shell_id)
            .map(runtime_snapshot_to_public)
    }

    fn interactive_shell_diagnostics(
        &self,
        shell_id: &str,
    ) -> Result<InteractiveShellDiagnostics, LocalShellRuntimeError> {
        self.inner.interactive_shell_diagnostics(shell_id)
    }

    fn semantics(&self) -> InteractiveShellSemantics {
        self.inner.semantics()
    }
}

struct WindowsLocalShellRuntime {
    inner: BaselineLocalShellRuntime,
}

impl WindowsLocalShellRuntime {
    fn new() -> Self {
        Self {
            inner: BaselineLocalShellRuntime::new(HostPlatform::Windows, "cmd"),
        }
    }
}

impl LocalShellRuntime for WindowsLocalShellRuntime {
    fn status(&self) -> CapabilityStatus {
        self.inner.status()
    }

    fn default_shell_label(&self) -> &'static str {
        self.inner.default_shell_label()
    }

    fn launch_spec(&self, mode: ShellLaunchMode, command: Option<&str>) -> ShellLaunchSpec {
        self.inner.launch_spec(mode, command)
    }

    fn run_one_shot(
        &self,
        command: &str,
    ) -> Result<LocalShellOneShotOutput, LocalShellRuntimeError> {
        self.inner.run_one_shot(command)
    }

    fn open_interactive_shell(
        &self,
        shell_id: &str,
        target_kind: &str,
        launch_command: Option<&str>,
    ) -> Result<LocalShellRuntimeSnapshot, LocalShellRuntimeError> {
        self.inner
            .open_interactive_shell(shell_id, target_kind, launch_command)
            .map(runtime_snapshot_to_public)
    }

    fn write_interactive_shell(
        &self,
        shell_id: &str,
        command: &str,
        completion_marker: String,
    ) -> Result<LocalShellRuntimeWriteOutcome, LocalShellRuntimeError> {
        self.inner
            .write_interactive_shell(shell_id, command, completion_marker)
            .map(|outcome| LocalShellRuntimeWriteOutcome {
                output_lines: outcome.output_lines,
                snapshot: outcome.snapshot,
            })
    }

    fn read_interactive_transcript(
        &self,
        shell_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<String>, LocalShellRuntimeError> {
        self.inner
            .read_interactive_transcript(shell_id, offset, limit)
    }

    fn interactive_shell_state(
        &self,
        shell_id: &str,
    ) -> Result<LocalShellRuntimeSnapshot, LocalShellRuntimeError> {
        self.inner
            .interactive_shell_state(shell_id)
            .map(runtime_snapshot_to_public)
    }

    fn interrupt_interactive_shell(
        &self,
        shell_id: &str,
    ) -> Result<LocalShellRuntimeSnapshot, LocalShellRuntimeError> {
        self.inner
            .interrupt_interactive_shell(shell_id)
            .map(runtime_snapshot_to_public)
    }

    fn close_interactive_shell(
        &self,
        shell_id: &str,
    ) -> Result<LocalShellRuntimeSnapshot, LocalShellRuntimeError> {
        self.inner
            .close_interactive_shell(shell_id)
            .map(runtime_snapshot_to_public)
    }

    fn interactive_shell_diagnostics(
        &self,
        shell_id: &str,
    ) -> Result<InteractiveShellDiagnostics, LocalShellRuntimeError> {
        self.inner.interactive_shell_diagnostics(shell_id)
    }

    fn semantics(&self) -> InteractiveShellSemantics {
        self.inner.semantics()
    }
}

struct UnsupportedLocalShellRuntime {
    inner: BaselineLocalShellRuntime,
}

impl UnsupportedLocalShellRuntime {
    fn new() -> Self {
        Self {
            inner: BaselineLocalShellRuntime::new(HostPlatform::Unknown, "sh"),
        }
    }
}

impl LocalShellRuntime for UnsupportedLocalShellRuntime {
    fn status(&self) -> CapabilityStatus {
        CapabilityStatus::Unsupported
    }

    fn default_shell_label(&self) -> &'static str {
        self.inner.default_shell_label()
    }

    fn launch_spec(&self, mode: ShellLaunchMode, command: Option<&str>) -> ShellLaunchSpec {
        self.inner.launch_spec(mode, command)
    }

    fn run_one_shot(
        &self,
        command: &str,
    ) -> Result<LocalShellOneShotOutput, LocalShellRuntimeError> {
        self.inner.run_one_shot(command)
    }

    fn open_interactive_shell(
        &self,
        shell_id: &str,
        target_kind: &str,
        launch_command: Option<&str>,
    ) -> Result<LocalShellRuntimeSnapshot, LocalShellRuntimeError> {
        self.inner
            .open_interactive_shell(shell_id, target_kind, launch_command)
            .map(runtime_snapshot_to_public)
    }

    fn write_interactive_shell(
        &self,
        shell_id: &str,
        command: &str,
        completion_marker: String,
    ) -> Result<LocalShellRuntimeWriteOutcome, LocalShellRuntimeError> {
        self.inner
            .write_interactive_shell(shell_id, command, completion_marker)
            .map(|outcome| LocalShellRuntimeWriteOutcome {
                output_lines: outcome.output_lines,
                snapshot: outcome.snapshot,
            })
    }

    fn read_interactive_transcript(
        &self,
        shell_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<String>, LocalShellRuntimeError> {
        self.inner
            .read_interactive_transcript(shell_id, offset, limit)
    }

    fn interactive_shell_state(
        &self,
        shell_id: &str,
    ) -> Result<LocalShellRuntimeSnapshot, LocalShellRuntimeError> {
        self.inner
            .interactive_shell_state(shell_id)
            .map(runtime_snapshot_to_public)
    }

    fn interrupt_interactive_shell(
        &self,
        shell_id: &str,
    ) -> Result<LocalShellRuntimeSnapshot, LocalShellRuntimeError> {
        self.inner
            .interrupt_interactive_shell(shell_id)
            .map(runtime_snapshot_to_public)
    }

    fn close_interactive_shell(
        &self,
        shell_id: &str,
    ) -> Result<LocalShellRuntimeSnapshot, LocalShellRuntimeError> {
        self.inner
            .close_interactive_shell(shell_id)
            .map(runtime_snapshot_to_public)
    }

    fn interactive_shell_diagnostics(
        &self,
        shell_id: &str,
    ) -> Result<InteractiveShellDiagnostics, LocalShellRuntimeError> {
        self.inner.interactive_shell_diagnostics(shell_id)
    }

    fn semantics(&self) -> InteractiveShellSemantics {
        self.inner.semantics()
    }
}

fn runtime_snapshot_to_public(
    snapshot: InteractiveShellRuntimeSnapshot,
) -> LocalShellRuntimeSnapshot {
    LocalShellRuntimeSnapshot {
        shell_id: snapshot.shell_id,
        target_kind: snapshot.target_kind,
        prompt: snapshot.prompt,
        cwd: snapshot.cwd,
        env: snapshot.env,
        transcript: snapshot.transcript,
        interrupted: snapshot.interrupted,
        closed: snapshot.closed,
        running: snapshot.running,
        completion_marker: snapshot.inflight_marker,
        backend: snapshot.backend.as_str().to_string(),
        degraded_mode: snapshot.backend.as_str() == "pipe",
    }
}

struct UnixControlPlaneTransport;

impl ControlPlaneTransport for UnixControlPlaneTransport {
    fn status(&self) -> CapabilityStatus {
        CapabilityStatus::Ready
    }

    fn transport_kind(&self) -> &'static str {
        "unix-socket"
    }

    fn endpoint(&self, _instance_name: &str, paths: &RuntimePaths) -> String {
        paths
            .state_dir
            .join("control-plane.sock")
            .to_string_lossy()
            .to_string()
    }

    fn endpoint_semantics(
        &self,
        instance_name: &str,
        paths: &RuntimePaths,
    ) -> ControlPlaneEndpointSemantics {
        ControlPlaneEndpointSemantics {
            endpoint: self.endpoint(instance_name, paths),
            naming_rule: "state_dir/control-plane.sock".to_string(),
            local_only: true,
        }
    }

    fn lifecycle_semantics(&self) -> ControlPlaneLifecycleSemantics {
        ControlPlaneLifecycleSemantics {
            attach: "ui/host attach opens a local unix domain socket session",
            request_response:
                "line-oriented request/response over unix domain socket until client closes",
            lifecycle: "socket file is created on bind and removed during shutdown",
        }
    }

    fn diagnostics(
        &self,
        instance_name: &str,
        paths: &RuntimePaths,
    ) -> Vec<ControlPlaneTransportDiagnostic> {
        vec![ControlPlaneTransportDiagnostic {
            code: "transport.unix_socket.ready",
            status: CapabilityStatus::Ready,
            message: format!(
                "unix control-plane transport is ready at {}",
                self.endpoint(instance_name, paths)
            ),
            recovery_hint: "if attach fails, verify the socket parent directory is writable"
                .to_string(),
        }]
    }
}

struct WindowsControlPlaneTransport;

impl ControlPlaneTransport for WindowsControlPlaneTransport {
    fn status(&self) -> CapabilityStatus {
        CapabilityStatus::Degraded
    }

    fn transport_kind(&self) -> &'static str {
        "named-pipe"
    }

    fn endpoint(&self, instance_name: &str, _paths: &RuntimePaths) -> String {
        named_pipe_endpoint(instance_name)
    }

    fn endpoint_semantics(
        &self,
        instance_name: &str,
        paths: &RuntimePaths,
    ) -> ControlPlaneEndpointSemantics {
        ControlPlaneEndpointSemantics {
            endpoint: self.endpoint(instance_name, paths),
            naming_rule:
                r"\\.\pipe\bridgingio-<sanitized-instance>-control-plane (sanitized instance: [A-Za-z0-9_-], others -> '-')"
                    .to_string(),
            local_only: true,
        }
    }

    fn lifecycle_semantics(&self) -> ControlPlaneLifecycleSemantics {
        ControlPlaneLifecycleSemantics {
            attach: "ui/host attach is expected to open a local named pipe handle",
            request_response:
                "request/response over named pipe is deferred until windows server wiring lands",
            lifecycle:
                "pipe endpoint naming is stable; full server lifecycle is deferred in this baseline",
        }
    }

    fn diagnostics(
        &self,
        instance_name: &str,
        paths: &RuntimePaths,
    ) -> Vec<ControlPlaneTransportDiagnostic> {
        vec![
            ControlPlaneTransportDiagnostic {
                code: "transport.windows_named_pipe.endpoint_defined",
                status: CapabilityStatus::Ready,
                message: format!(
                    "windows named pipe endpoint naming contract is active: {}",
                    self.endpoint(instance_name, paths)
                ),
                recovery_hint: "keep instance names stable so UI/core can resolve the same pipe endpoint"
                    .to_string(),
            },
            ControlPlaneTransportDiagnostic {
                code: "transport.windows_named_pipe.deferred",
                status: CapabilityStatus::Degraded,
                message: "windows named pipe server/client runtime wiring is deferred to the platform-ui increment".to_string(),
                recovery_hint:
                    "for now use unix host for local ipc flows, or run standalone with model-plane only on windows"
                        .to_string(),
            },
        ]
    }
}

struct UnsupportedControlPlaneTransport;

impl ControlPlaneTransport for UnsupportedControlPlaneTransport {
    fn status(&self) -> CapabilityStatus {
        CapabilityStatus::Unsupported
    }

    fn transport_kind(&self) -> &'static str {
        "unsupported"
    }

    fn endpoint(&self, _instance_name: &str, paths: &RuntimePaths) -> String {
        paths
            .state_dir
            .join("control-plane.sock")
            .to_string_lossy()
            .to_string()
    }

    fn endpoint_semantics(
        &self,
        instance_name: &str,
        paths: &RuntimePaths,
    ) -> ControlPlaneEndpointSemantics {
        ControlPlaneEndpointSemantics {
            endpoint: self.endpoint(instance_name, paths),
            naming_rule: "unsupported host fallback uses unix-style socket path".to_string(),
            local_only: true,
        }
    }

    fn lifecycle_semantics(&self) -> ControlPlaneLifecycleSemantics {
        ControlPlaneLifecycleSemantics {
            attach: "attach is unsupported on unknown host platform",
            request_response: "request/response is unsupported on unknown host platform",
            lifecycle: "no lifecycle is guaranteed for unsupported host platform",
        }
    }

    fn diagnostics(
        &self,
        _instance_name: &str,
        _paths: &RuntimePaths,
    ) -> Vec<ControlPlaneTransportDiagnostic> {
        vec![ControlPlaneTransportDiagnostic {
            code: "transport.unsupported_host",
            status: CapabilityStatus::Unsupported,
            message: "control-plane transport is unsupported on unknown host platform".to_string(),
            recovery_hint:
                "run on unix or windows host, or disable control-plane in standalone mode"
                    .to_string(),
        }]
    }
}

struct PlatformRuntimePathsAdapter {
    platform: HostPlatform,
}

impl RuntimePathsAdapter for PlatformRuntimePathsAdapter {
    fn status(&self) -> CapabilityStatus {
        CapabilityStatus::Ready
    }

    fn default_data_dir(&self, instance_name: &str) -> PathBuf {
        let home = home_dir().unwrap_or_else(|| PathBuf::from("."));
        let _ = instance_name;
        home.join(".bridgingio")
    }

    fn expand_user_path(&self, raw: &str) -> PathBuf {
        if raw == "~" {
            return home_dir().unwrap_or_else(|| PathBuf::from(raw));
        }
        if let Some(rest) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
            if let Some(home) = home_dir() {
                return home.join(rest);
            }
        }
        PathBuf::from(raw)
    }

    fn runtime_paths(&self, instance_name: &str, data_dir: &Path) -> RuntimePaths {
        let state_dir = data_dir.join("state");
        let paths = RuntimePaths {
            data_dir: data_dir.to_path_buf(),
            state_dir: state_dir.clone(),
            metadata_path: state_dir.join("metadata.sqlite3"),
            artifact_root: data_dir.join("artifacts"),
            temp_dir: env::temp_dir(),
            logs_dir: data_dir.join("logs"),
            control_plane_endpoint: String::new(),
        };
        let endpoint = match self.platform {
            HostPlatform::Windows => named_pipe_endpoint(instance_name),
            _ => paths
                .state_dir
                .join("control-plane.sock")
                .to_string_lossy()
                .to_string(),
        };
        RuntimePaths {
            control_plane_endpoint: endpoint,
            ..paths
        }
    }
}

struct BaselineToolchainLocator;

impl ToolchainLocator for BaselineToolchainLocator {
    fn status(&self) -> CapabilityStatus {
        CapabilityStatus::Ready
    }

    fn source_label(&self) -> &'static str {
        "host-platform-toolchain-locator"
    }

    fn resolution_order(&self) -> &'static [&'static str] {
        &[
            "target_override",
            "global_override",
            "system_path",
            "builtin_fallback",
        ]
    }
}

struct BaselineNativeVaultBinding {
    platform: HostPlatform,
}

impl NativeVaultBinding for BaselineNativeVaultBinding {
    fn status(&self) -> CapabilityStatus {
        match self.platform {
            HostPlatform::Unknown => CapabilityStatus::Unsupported,
            _ => CapabilityStatus::Degraded,
        }
    }

    fn backend_label(&self) -> &'static str {
        "os-native"
    }
}

struct StderrRuntimeLogger {
    level: RuntimeLogLevel,
}

impl RuntimeLogger for StderrRuntimeLogger {
    fn status(&self) -> CapabilityStatus {
        CapabilityStatus::Ready
    }

    fn level(&self) -> RuntimeLogLevel {
        self.level
    }

    fn log(&self, level: RuntimeLogLevel, category: RuntimeLogCategory, message: &str) {
        if !self.level.enabled(level) {
            return;
        }
        eprintln!(
            "[bridgingio:{}:{}] {}",
            category.as_str(),
            level.as_str(),
            message
        );
    }
}

struct PlatformOutputDecoder {
    _platform: HostPlatform,
}

impl OutputDecoder for PlatformOutputDecoder {
    fn status(&self) -> CapabilityStatus {
        CapabilityStatus::Ready
    }

    fn decode(&self, bytes: &[u8]) -> DecodedOutput {
        decode_with_platform_defaults(bytes)
    }
}

pub(crate) fn decode_with_platform_defaults(bytes: &[u8]) -> DecodedOutput {
    let decoded = match String::from_utf8(bytes.to_vec()) {
        Ok(text) => DecodedOutput {
            text,
            used_fallback: false,
            normalized_newlines: false,
            had_replacement_char: false,
        },
        Err(_) => {
            let lossy = String::from_utf8_lossy(bytes).to_string();
            DecodedOutput {
                had_replacement_char: lossy.contains('\u{fffd}'),
                text: lossy,
                used_fallback: true,
                normalized_newlines: false,
            }
        }
    };

    let normalized = normalize_newlines(&decoded.text);
    DecodedOutput {
        text: normalized.clone(),
        normalized_newlines: decoded.text != normalized,
        ..decoded
    }
}

fn normalize_newlines(input: &str) -> String {
    input.replace("\r\n", "\n").replace('\r', "\n")
}

fn named_pipe_endpoint(instance_name: &str) -> String {
    format!(
        r"\\.\pipe\bridgingio-{}-control-plane",
        sanitize_instance_name(instance_name)
    )
}

fn sanitize_instance_name(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return "default".to_string();
    }
    trimmed
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn home_dir() -> Option<PathBuf> {
    if let Ok(home) = env::var("HOME") {
        if !home.trim().is_empty() {
            return Some(PathBuf::from(home));
        }
    }
    if let Ok(profile) = env::var("USERPROFILE") {
        if !profile.trim().is_empty() {
            return Some(PathBuf::from(profile));
        }
    }
    let drive = env::var("HOMEDRIVE").ok();
    let path = env::var("HOMEPATH").ok();
    match (drive, path) {
        (Some(drive), Some(path)) if !drive.trim().is_empty() && !path.trim().is_empty() => {
            Some(PathBuf::from(format!("{drive}{path}")))
        }
        _ => None,
    }
}

fn unix_baseline_adapter(log_level: &str) -> BaselineHostPlatformAdapter {
    let diagnostics = vec![
        HostCapabilityDiagnostic {
            capability: "local_shell_runtime",
            status: CapabilityStatus::Ready,
            message: "unix baseline shell runtime is active".to_string(),
        },
        HostCapabilityDiagnostic {
            capability: "control_plane_transport",
            status: CapabilityStatus::Ready,
            message: "unix domain socket transport is active".to_string(),
        },
        HostCapabilityDiagnostic {
            capability: "runtime_paths",
            status: CapabilityStatus::Ready,
            message: "runtime path defaults are resolved through RuntimePathsAdapter".to_string(),
        },
        HostCapabilityDiagnostic {
            capability: "toolchain_locator",
            status: CapabilityStatus::Ready,
            message:
                "ToolchainLocator is the single resolution entrypoint (target/global/PATH/built-in)"
                    .to_string(),
        },
        HostCapabilityDiagnostic {
            capability: "native_vault_binding",
            status: CapabilityStatus::Degraded,
            message:
                "os-native vault binding is currently a controlled degraded mode (memory shim)"
                    .to_string(),
        },
        HostCapabilityDiagnostic {
            capability: "runtime_logger",
            status: CapabilityStatus::Ready,
            message: "RuntimeLogger baseline is active".to_string(),
        },
        HostCapabilityDiagnostic {
            capability: "output_decoder",
            status: CapabilityStatus::Ready,
            message: "OutputDecoder baseline uses platform text decoding with UTF-8 fallback"
                .to_string(),
        },
    ];

    BaselineHostPlatformAdapter {
        host_platform: HostPlatform::Unix,
        local_shell_runtime: Box::new(UnixLocalShellRuntime::new()),
        control_plane_transport: Box::new(UnixControlPlaneTransport),
        runtime_paths: PlatformRuntimePathsAdapter {
            platform: HostPlatform::Unix,
        },
        toolchain_locator: BaselineToolchainLocator,
        native_vault_binding: BaselineNativeVaultBinding {
            platform: HostPlatform::Unix,
        },
        runtime_logger: StderrRuntimeLogger {
            level: RuntimeLogLevel::parse(log_level),
        },
        output_decoder: PlatformOutputDecoder {
            _platform: HostPlatform::Unix,
        },
        diagnostics,
    }
}

fn windows_baseline_adapter(log_level: &str) -> BaselineHostPlatformAdapter {
    let diagnostics = vec![
        HostCapabilityDiagnostic {
            capability: "local_shell_runtime",
            status: CapabilityStatus::Ready,
            message: "windows baseline shell runtime defaults to cmd".to_string(),
        },
        HostCapabilityDiagnostic {
            capability: "control_plane_transport",
            status: CapabilityStatus::Degraded,
            message:
                "named pipe endpoint semantics are defined; full transport server wiring is pending"
                    .to_string(),
        },
        HostCapabilityDiagnostic {
            capability: "runtime_paths",
            status: CapabilityStatus::Ready,
            message: "RuntimePathsAdapter resolves platform-aware defaults, including named pipe endpoint"
                .to_string(),
        },
        HostCapabilityDiagnostic {
            capability: "toolchain_locator",
            status: CapabilityStatus::Ready,
            message:
                "ToolchainLocator is the single resolution entrypoint (target/global/PATH/built-in)"
                    .to_string(),
        },
        HostCapabilityDiagnostic {
            capability: "native_vault_binding",
            status: CapabilityStatus::Degraded,
            message:
                "os-native vault binding is currently a controlled degraded mode (memory shim)"
                    .to_string(),
        },
        HostCapabilityDiagnostic {
            capability: "runtime_logger",
            status: CapabilityStatus::Ready,
            message: "RuntimeLogger baseline is active".to_string(),
        },
        HostCapabilityDiagnostic {
            capability: "output_decoder",
            status: CapabilityStatus::Ready,
            message: "OutputDecoder baseline uses platform text decoding with UTF-8 fallback"
                .to_string(),
        },
    ];

    BaselineHostPlatformAdapter {
        host_platform: HostPlatform::Windows,
        local_shell_runtime: Box::new(WindowsLocalShellRuntime::new()),
        control_plane_transport: Box::new(WindowsControlPlaneTransport),
        runtime_paths: PlatformRuntimePathsAdapter {
            platform: HostPlatform::Windows,
        },
        toolchain_locator: BaselineToolchainLocator,
        native_vault_binding: BaselineNativeVaultBinding {
            platform: HostPlatform::Windows,
        },
        runtime_logger: StderrRuntimeLogger {
            level: RuntimeLogLevel::parse(log_level),
        },
        output_decoder: PlatformOutputDecoder {
            _platform: HostPlatform::Windows,
        },
        diagnostics,
    }
}

fn unknown_baseline_adapter(log_level: &str) -> BaselineHostPlatformAdapter {
    let diagnostics = vec![HostCapabilityDiagnostic {
        capability: "host_platform",
        status: CapabilityStatus::Unsupported,
        message: "host platform is unknown; adapter is running in unsupported baseline mode"
            .to_string(),
    }];

    BaselineHostPlatformAdapter {
        host_platform: HostPlatform::Unknown,
        local_shell_runtime: Box::new(UnsupportedLocalShellRuntime::new()),
        control_plane_transport: Box::new(UnsupportedControlPlaneTransport),
        runtime_paths: PlatformRuntimePathsAdapter {
            platform: HostPlatform::Unknown,
        },
        toolchain_locator: BaselineToolchainLocator,
        native_vault_binding: BaselineNativeVaultBinding {
            platform: HostPlatform::Unknown,
        },
        runtime_logger: StderrRuntimeLogger {
            level: RuntimeLogLevel::parse(log_level),
        },
        output_decoder: PlatformOutputDecoder {
            _platform: HostPlatform::Unknown,
        },
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        detect_host_platform_adapter, named_pipe_endpoint, next_local_authorization_flow_id,
        normalize_newlines, unix_baseline_adapter, windows_baseline_adapter, CapabilityStatus,
        HostPlatform, HostPlatformAdapter, LocalAuthorizationEvent, LocalAuthorizationLogStream,
        LocalAuthorizationRecorder, LocalOperatorSurface, RuntimeLogCategory, RuntimeLogLevel,
    };
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn normalizes_newlines_consistently() {
        let normalized = normalize_newlines("a\r\nb\rc\n");
        assert_eq!(normalized, "a\nb\nc\n");
    }

    #[test]
    fn named_pipe_endpoint_sanitizes_instance_name() {
        let endpoint = named_pipe_endpoint("bridgingio ui");
        assert_eq!(endpoint, r"\\.\pipe\bridgingio-bridgingio-ui-control-plane");
    }

    #[test]
    fn adapter_snapshot_reports_core_capabilities() {
        let adapter = detect_host_platform_adapter("info");
        let snapshot = adapter.snapshot();
        assert_ne!(snapshot.host_platform, HostPlatform::Unknown);
        assert!(
            snapshot.runtime_logger_status == CapabilityStatus::Ready
                || snapshot.runtime_logger_status == CapabilityStatus::Fallback
        );
        assert!(!snapshot.diagnostics.is_empty());

        adapter
            .runtime_logger()
            .log(RuntimeLogLevel::Info, RuntimeLogCategory::Startup, "test");
    }

    #[test]
    fn unix_baseline_adapter_reports_ready_transport_and_shell() {
        let adapter = unix_baseline_adapter("info");
        let snapshot = adapter.snapshot();
        assert_eq!(snapshot.host_platform, HostPlatform::Unix);
        assert_eq!(snapshot.local_shell_runtime_status, CapabilityStatus::Ready);
        assert_eq!(
            snapshot.control_plane_transport_status,
            CapabilityStatus::Ready
        );
        assert_eq!(adapter.local_shell_runtime().default_shell_label(), "sh");
        let runtime_paths = adapter
            .runtime_paths()
            .runtime_paths("test-instance", Path::new("/tmp/bridgingio-platform-test"));
        assert!(
            runtime_paths
                .control_plane_endpoint
                .ends_with("control-plane.sock"),
            "unexpected endpoint: {}",
            runtime_paths.control_plane_endpoint
        );
    }

    #[test]
    fn windows_baseline_adapter_reports_named_pipe_degraded_mode() {
        let adapter = windows_baseline_adapter("info");
        let snapshot = adapter.snapshot();
        assert_eq!(snapshot.host_platform, HostPlatform::Windows);
        assert_eq!(snapshot.local_shell_runtime_status, CapabilityStatus::Ready);
        assert_eq!(
            snapshot.control_plane_transport_status,
            CapabilityStatus::Degraded
        );
        assert_eq!(adapter.local_shell_runtime().default_shell_label(), "cmd");
        assert_eq!(snapshot.native_vault_status, CapabilityStatus::Degraded);
        assert_eq!(
            adapter.control_plane_transport().transport_kind(),
            "named-pipe"
        );
        let runtime_paths = adapter
            .runtime_paths()
            .runtime_paths("bridgingio ui", Path::new("C:\\bridgingio"));
        assert!(
            runtime_paths
                .control_plane_endpoint
                .starts_with(r"\\.\pipe\bridgingio-"),
            "unexpected endpoint: {}",
            runtime_paths.control_plane_endpoint
        );
    }

    #[test]
    fn detect_adapter_uses_current_host_branch() {
        let adapter = detect_host_platform_adapter("info");
        #[cfg(unix)]
        assert_eq!(adapter.host_platform(), HostPlatform::Unix);
        #[cfg(windows)]
        assert_eq!(adapter.host_platform(), HostPlatform::Windows);
    }

    #[test]
    fn local_authorization_recorder_writes_jsonl() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = Path::new("/tmp").join(format!("bridgingio-local-auth-{stamp}"));
        let recorder = LocalAuthorizationRecorder::for_runtime_root(&root, RuntimeLogLevel::Info);
        let event = LocalAuthorizationEvent::new(
            "flow-test",
            LocalOperatorSurface::StandaloneCli,
            "Security",
            "vault.unlock",
            "vault.unlock",
            "succeeded",
            "ok",
            "leader",
        );
        recorder
            .append_event(
                LocalAuthorizationLogStream::Authorization,
                RuntimeLogLevel::Info,
                &event,
            )
            .expect("append auth event");
        let written = fs::read_to_string(root.join("logs/local-authorization.jsonl"))
            .expect("read authorization log");
        assert!(written.contains("\"flow_id\":\"flow-test\""));
        assert!(written.contains("\"surface\":\"standalone-cli\""));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_authorization_recorder_filters_debug_when_level_is_info() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = Path::new("/tmp").join(format!("bridgingio-local-auth-debug-{stamp}"));
        let recorder = LocalAuthorizationRecorder::for_runtime_root(&root, RuntimeLogLevel::Info);
        let event = LocalAuthorizationEvent::new(
            "flow-debug",
            LocalOperatorSurface::Menuconfig,
            "Security",
            "screen.change",
            "menuconfig.session",
            "breadcrumb",
            "ok",
            "not-applicable",
        );
        recorder
            .append_event(
                LocalAuthorizationLogStream::MenuconfigSession,
                RuntimeLogLevel::Debug,
                &event,
            )
            .expect("append debug breadcrumb should be a no-op");
        assert!(!root.join("logs/menuconfig-session.jsonl").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_authorization_flow_id_is_stable_and_scoped() {
        let flow = next_local_authorization_flow_id("vault.unlock");
        assert!(flow.starts_with("flow-vault.unlock-"));
    }
}
