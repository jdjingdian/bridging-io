use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::time::SystemTime;

use bridgingio_artifacts::{ArtifactRefineMode, InMemoryArtifactStore};
use bridgingio_connectors::CommandInvocation;
use bridgingio_domain::{ArtifactRecord, CapabilitySummary, PolicyProfile};
use bridgingio_platform::{
    detect_host_platform_adapter, HostPlatformAdapter, InteractiveShellDiagnostics,
    LocalShellRuntimeSnapshot, RuntimeLogCategory, RuntimeLogLevel,
};
use bridgingio_policy::{evaluate, OperationKind, PolicyDecision};
use bridgingio_secrets::{command_audit_preview, RuntimeRedactionRegistry};

const INTERACTIVE_LAUNCH_STRUCTURED: &str = "structured_interactive_invocation";
const INTERACTIVE_LAUNCH_HOST_BASELINE: &str = "host_baseline";
const INTERACTIVE_LAUNCH_STRUCTURED_FALLBACK: &str =
    "structured_interactive_invocation_with_host_baseline_fallback";
const INTERACTIVE_LAUNCH_NOTE_PREFIX: &str = "[runtime] interactive launch:";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderError {
    pub message: String,
}

pub fn terminal_provider_capability() -> CapabilitySummary {
    CapabilitySummary {
        id: "terminal.exec".into(),
        label: "terminal command execution".into(),
        supports_streaming: true,
        supports_file_transfer: false,
        requires_approval: true,
        typed_entrypoints: vec![
            "terminal.exec".into(),
            "artifacts.read".into(),
            "artifacts.refine".into(),
        ],
        raw_fallback: true,
    }
}

pub struct TerminalProvider {
    pub artifacts: InMemoryArtifactStore,
    host_platform_adapter: Box<dyn HostPlatformAdapter>,
    redaction_registry: RuntimeRedactionRegistry,
    interactive_contexts: HashMap<String, InteractiveShellContext>,
    next_shell_seq: u64,
    next_command_seq: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InteractiveShellContext {
    logical_session_id: String,
    channel_id: String,
    transport_session_id: Option<String>,
    target_kind: String,
    launch_strategy: String,
    launch_fallback_applied: bool,
    launch_diagnostics: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InteractiveShellState {
    pub shell_id: String,
    pub logical_session_id: String,
    pub channel_id: String,
    pub transport_session_id: Option<String>,
    pub target_kind: String,
    pub prompt: String,
    pub cwd: String,
    pub env: HashMap<String, String>,
    pub transcript: Vec<String>,
    pub interrupted: bool,
    pub closed: bool,
    pub running: bool,
    pub inflight_marker: Option<String>,
    pub launch_strategy: String,
    pub launch_fallback_applied: bool,
    pub launch_diagnostics: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InteractiveShellWriteResult {
    pub shell_id: String,
    pub output: String,
    pub prompt: String,
    pub cwd: String,
    pub artifact_id: String,
    pub running: bool,
}

impl Default for TerminalProvider {
    fn default() -> Self {
        Self {
            artifacts: InMemoryArtifactStore::default(),
            host_platform_adapter: detect_host_platform_adapter("info"),
            redaction_registry: RuntimeRedactionRegistry::default(),
            interactive_contexts: HashMap::new(),
            next_shell_seq: 0,
            next_command_seq: 0,
        }
    }
}

impl TerminalProvider {
    pub fn register_sensitive_value(&mut self, value: &str) {
        self.redaction_registry.register(value);
    }

    pub fn redact_for_display(&self, text: &str) -> String {
        self.redaction_registry.redact(text)
    }

    pub fn command_preview(&self, command: &str) -> String {
        command_audit_preview(&self.redact_for_display(command))
    }

    pub fn exec_local(
        &mut self,
        logical_session_id: &str,
        channel_id: Option<&str>,
        transport_session_id: Option<&str>,
        command: &str,
        artifact_id: &str,
        policy: &PolicyProfile,
    ) -> Result<ArtifactRecord, ProviderError> {
        if matches!(
            evaluate(policy, OperationKind::Write),
            PolicyDecision::RequireApproval
        ) && command.contains("rm ")
        {
            return Err(ProviderError {
                message: "command requires approval".into(),
            });
        }

        let output = self
            .host_platform_adapter
            .local_shell_runtime()
            .run_one_shot(command)
            .map_err(runtime_error_to_provider_error)?;

        let created = self.artifacts.create_raw(
            artifact_id.to_string(),
            logical_session_id.to_string(),
            channel_id.map(ToString::to_string),
            transport_session_id.map(ToString::to_string),
            self.command_preview(command),
            "terminal command output",
            SystemTime::now(),
        );

        for line in output.stdout_lines {
            self.artifacts
                .append_chunk(&created.id, self.redact_for_display(&line));
        }
        for line in output.stderr_lines {
            self.artifacts.append_chunk(
                &created.id,
                self.redact_for_display(&format!("stderr: {line}")),
            );
        }

        Ok(created)
    }

    pub fn exec_structured_invocation(
        &mut self,
        logical_session_id: &str,
        channel_id: Option<&str>,
        transport_session_id: Option<&str>,
        invocation: &CommandInvocation,
        source_command: &str,
        artifact_id: &str,
        policy: &PolicyProfile,
    ) -> Result<ArtifactRecord, ProviderError> {
        if matches!(
            evaluate(policy, OperationKind::Write),
            PolicyDecision::RequireApproval
        ) && source_command.contains("rm ")
        {
            return Err(ProviderError {
                message: "command requires approval".into(),
            });
        }

        let output = Command::new(&invocation.program)
            .args(&invocation.args)
            .output()
            .map_err(|err| ProviderError {
                message: format!("failed to execute structured invocation: {err}"),
            })?;

        let created = self.artifacts.create_raw(
            artifact_id.to_string(),
            logical_session_id.to_string(),
            channel_id.map(ToString::to_string),
            transport_session_id.map(ToString::to_string),
            self.command_preview(source_command),
            "terminal command output",
            SystemTime::now(),
        );

        for line in decode_command_output_lines(&output.stdout) {
            self.artifacts
                .append_chunk(&created.id, self.redact_for_display(&line));
        }
        for line in decode_command_output_lines(&output.stderr) {
            self.artifacts.append_chunk(
                &created.id,
                self.redact_for_display(&format!("stderr: {line}")),
            );
        }

        Ok(created)
    }

    pub fn refine_keyword(
        &mut self,
        source_artifact_id: &str,
        derived_artifact_id: &str,
        keyword: &str,
    ) -> Option<ArtifactRecord> {
        self.artifacts.derive_with_keyword(
            derived_artifact_id.to_string(),
            source_artifact_id.to_string(),
            keyword.to_string(),
            SystemTime::now(),
        )
    }

    pub fn refine_with_filter(
        &mut self,
        source_artifact_id: &str,
        derived_artifact_id: &str,
        pattern: &str,
        mode_label: &str,
        ignore_case: bool,
    ) -> Result<ArtifactRecord, ProviderError> {
        let mode = parse_artifact_refine_mode(mode_label)?;
        let record = self
            .artifacts
            .derive_with_pattern(
                derived_artifact_id.to_string(),
                source_artifact_id.to_string(),
                pattern.to_string(),
                mode,
                ignore_case,
                SystemTime::now(),
            )
            .map_err(|message| ProviderError { message })?
            .ok_or_else(|| ProviderError {
                message: "source artifact not found".into(),
            })?;
        Ok(record)
    }

    pub fn open_interactive_shell(
        &mut self,
        logical_session_id: &str,
        channel_id: &str,
        transport_session_id: Option<&str>,
        target_kind: &str,
    ) -> Result<InteractiveShellState, ProviderError> {
        self.open_interactive_shell_with_command(
            logical_session_id,
            channel_id,
            transport_session_id,
            target_kind,
            None,
        )
    }

    pub fn open_interactive_shell_with_command(
        &mut self,
        logical_session_id: &str,
        channel_id: &str,
        transport_session_id: Option<&str>,
        target_kind: &str,
        launch_command: Option<&str>,
    ) -> Result<InteractiveShellState, ProviderError> {
        self.next_shell_seq += 1;
        let shell_id = format!("shell-{:06}", self.next_shell_seq);
        let (snapshot, launch_strategy, launch_fallback_applied, launch_diagnostics) =
            open_runtime_snapshot_with_launch_fallback(
                self.host_platform_adapter.local_shell_runtime(),
                &shell_id,
                target_kind,
                launch_command,
            )?;
        let context = InteractiveShellContext {
            logical_session_id: logical_session_id.to_string(),
            channel_id: channel_id.to_string(),
            transport_session_id: transport_session_id.map(ToString::to_string),
            target_kind: target_kind.to_string(),
            launch_strategy,
            launch_fallback_applied,
            launch_diagnostics,
        };

        self.interactive_contexts
            .insert(shell_id.clone(), context.clone());
        Ok(runtime_state_to_provider_state(context, snapshot))
    }

    pub fn write_interactive_shell(
        &mut self,
        shell_id: &str,
        command: &str,
        artifact_id: &str,
        _policy: &PolicyProfile,
    ) -> Result<InteractiveShellWriteResult, ProviderError> {
        let context = self.ensure_shell_context(shell_id)?.clone();

        self.next_command_seq += 1;
        let marker = format!("__BRIDGINGIO_DONE_{}_{}__", shell_id, self.next_command_seq);
        let outcome = self
            .host_platform_adapter
            .local_shell_runtime()
            .write_interactive_shell(shell_id, command, marker)
            .map_err(runtime_error_to_provider_error)?;

        let created = self.artifacts.create_raw(
            artifact_id.to_string(),
            context.logical_session_id.clone(),
            Some(context.channel_id.clone()),
            context.transport_session_id.clone(),
            self.command_preview(command.trim()),
            "interactive shell output",
            SystemTime::now(),
        );

        for line in &outcome.output_lines {
            self.artifacts
                .append_chunk(&created.id, self.redact_for_display(line));
        }
        let redacted_output_lines = outcome
            .output_lines
            .iter()
            .map(|line| self.redact_for_display(line))
            .collect::<Vec<_>>();

        Ok(InteractiveShellWriteResult {
            shell_id: shell_id.to_string(),
            output: redacted_output_lines.join("\n"),
            prompt: outcome.snapshot.prompt,
            cwd: outcome.snapshot.cwd,
            artifact_id: created.id,
            running: outcome.snapshot.running,
        })
    }

    pub fn read_interactive_transcript(
        &mut self,
        shell_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<String>, ProviderError> {
        self.ensure_shell_context(shell_id)?;
        self.host_platform_adapter
            .local_shell_runtime()
            .read_interactive_transcript(shell_id, offset, limit)
            .map_err(runtime_error_to_provider_error)
            .map(|lines| {
                lines
                    .into_iter()
                    .map(|line| self.redact_for_display(&line))
                    .collect()
            })
    }

    pub fn get_interactive_shell(
        &self,
        shell_id: &str,
    ) -> Result<InteractiveShellState, ProviderError> {
        let context = self
            .interactive_contexts
            .get(shell_id)
            .cloned()
            .ok_or_else(|| ProviderError {
                message: format!("interactive shell not found: {shell_id}"),
            })?;

        let snapshot = self
            .host_platform_adapter
            .local_shell_runtime()
            .interactive_shell_state(shell_id)
            .map_err(runtime_error_to_provider_error)?;
        let mut state = runtime_state_to_provider_state(context, snapshot);
        state.transcript = state
            .transcript
            .into_iter()
            .map(|line| self.redact_for_display(&line))
            .collect();
        Ok(state)
    }

    pub fn interrupt_interactive_shell(&mut self, shell_id: &str) -> Result<(), ProviderError> {
        self.ensure_shell_context(shell_id)?;
        self.host_platform_adapter
            .local_shell_runtime()
            .interrupt_interactive_shell(shell_id)
            .map_err(runtime_error_to_provider_error)
            .map(|_| ())
    }

    pub fn close_interactive_shell(&mut self, shell_id: &str) -> Result<(), ProviderError> {
        self.ensure_shell_context(shell_id)?;
        self.host_platform_adapter
            .local_shell_runtime()
            .close_interactive_shell(shell_id)
            .map_err(runtime_error_to_provider_error)
            .map(|_| ())
    }

    pub fn interactive_shell_diagnostics(
        &self,
        shell_id: &str,
    ) -> Result<InteractiveShellDiagnostics, ProviderError> {
        self.interactive_contexts
            .get(shell_id)
            .ok_or_else(|| ProviderError {
                message: format!("interactive shell not found: {shell_id}"),
            })?;
        self.host_platform_adapter
            .local_shell_runtime()
            .interactive_shell_diagnostics(shell_id)
            .map_err(runtime_error_to_provider_error)
    }

    fn ensure_shell_context(
        &self,
        shell_id: &str,
    ) -> Result<&InteractiveShellContext, ProviderError> {
        self.interactive_contexts
            .get(shell_id)
            .ok_or_else(|| ProviderError {
                message: format!("interactive shell not found: {shell_id}"),
            })
    }
}

fn runtime_state_to_provider_state(
    context: InteractiveShellContext,
    snapshot: LocalShellRuntimeSnapshot,
) -> InteractiveShellState {
    InteractiveShellState {
        shell_id: snapshot.shell_id,
        logical_session_id: context.logical_session_id,
        channel_id: context.channel_id,
        transport_session_id: context.transport_session_id,
        target_kind: context.target_kind,
        prompt: snapshot.prompt,
        cwd: snapshot.cwd,
        env: snapshot.env,
        transcript: snapshot.transcript,
        interrupted: snapshot.interrupted,
        closed: snapshot.closed,
        running: snapshot.running,
        inflight_marker: snapshot.completion_marker,
        launch_strategy: context.launch_strategy,
        launch_fallback_applied: context.launch_fallback_applied,
        launch_diagnostics: context.launch_diagnostics,
    }
}

fn open_runtime_snapshot_with_launch_fallback(
    runtime: &dyn bridgingio_platform::LocalShellRuntime,
    shell_id: &str,
    target_kind: &str,
    launch_command: Option<&str>,
) -> Result<(LocalShellRuntimeSnapshot, String, bool, Vec<String>), ProviderError> {
    let Some(command) = launch_command else {
        let snapshot = runtime
            .open_interactive_shell(shell_id, target_kind, None)
            .map_err(runtime_error_to_provider_error)?;
        return Ok((
            snapshot,
            INTERACTIVE_LAUNCH_HOST_BASELINE.to_string(),
            false,
            Vec::new(),
        ));
    };

    match runtime.open_interactive_shell(shell_id, target_kind, Some(command)) {
        Ok(snapshot) => Ok((
            snapshot,
            INTERACTIVE_LAUNCH_STRUCTURED.to_string(),
            false,
            Vec::new(),
        )),
        Err(structured_err) => {
            let mut diagnostics = vec![format!(
                "structured interactive launch failed: {}",
                structured_err.message
            )];
            let mut fallback_snapshot = runtime
                .open_interactive_shell(shell_id, target_kind, None)
                .map_err(|fallback_err| ProviderError {
                    message: format!(
                        "interactive launch failed; structured invocation error: {}; host baseline fallback error: {}",
                        structured_err.message, fallback_err.message
                    ),
                })?;
            diagnostics.push("host baseline fallback launch succeeded".to_string());
            fallback_snapshot.transcript.push(format!(
                "{INTERACTIVE_LAUNCH_NOTE_PREFIX} structured invocation failed, fallback to host baseline: {}",
                structured_err.message
            ));
            Ok((
                fallback_snapshot,
                INTERACTIVE_LAUNCH_STRUCTURED_FALLBACK.to_string(),
                true,
                diagnostics,
            ))
        }
    }
}

fn runtime_error_to_provider_error(
    err: bridgingio_platform::LocalShellRuntimeError,
) -> ProviderError {
    ProviderError {
        message: err.message,
    }
}

fn parse_artifact_refine_mode(mode_label: &str) -> Result<ArtifactRefineMode, ProviderError> {
    match mode_label.trim().to_ascii_lowercase().as_str() {
        "keyword" => Ok(ArtifactRefineMode::Keyword),
        "regex" => Ok(ArtifactRefineMode::Regex),
        "auto" | "" => Ok(ArtifactRefineMode::Auto),
        other => Err(ProviderError {
            message: format!("invalid artifact refine mode: {other}"),
        }),
    }
}

fn decode_command_output_lines(bytes: &[u8]) -> Vec<String> {
    let normalized = String::from_utf8_lossy(bytes).replace("\r\n", "\n");
    normalized
        .split('\n')
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect()
}

pub struct GitProvider {
    repo_path: PathBuf,
}

impl GitProvider {
    pub fn new(repo_path: impl Into<PathBuf>) -> Self {
        Self {
            repo_path: repo_path.into(),
        }
    }

    pub fn status(&self) -> Result<String, ProviderError> {
        self.run_git(["status", "--short"])
    }

    pub fn diff(&self, reference: &str) -> Result<String, ProviderError> {
        self.run_git(["diff", reference])
    }

    pub fn log(&self, max_count: usize) -> Result<String, ProviderError> {
        self.run_git(["log", "--oneline", "--max-count", &max_count.to_string()])
    }

    fn run_git<const N: usize>(&self, args: [&str; N]) -> Result<String, ProviderError> {
        let output = Command::new("git")
            .current_dir(&self.repo_path)
            .args(args)
            .output()
            .map_err(|e| ProviderError {
                message: format!("failed to run git: {e}"),
            })?;

        if !output.status.success() {
            return Err(ProviderError {
                message: decode_output_text(&output.stderr),
            });
        }

        Ok(decode_output_text(&output.stdout))
    }
}

fn decode_output_text(bytes: &[u8]) -> String {
    let adapter = detect_host_platform_adapter("info");
    let decoded = adapter.output_decoder().decode(bytes);
    if decoded.used_fallback || decoded.had_replacement_char || decoded.normalized_newlines {
        adapter.runtime_logger().log(
            RuntimeLogLevel::Warn,
            RuntimeLogCategory::Decode,
            &format!(
                "decode diagnostics surface=provider_command_output fallback={} replacement_char={} normalized_newlines={}",
                decoded.used_fallback,
                decoded.had_replacement_char,
                decoded.normalized_newlines
            ),
        );
    }
    decoded.text
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use bridgingio_domain::PolicyProfile;

    use super::TerminalProvider;

    #[test]
    fn refines_artifact_with_keyword() {
        let mut provider = TerminalProvider::default();
        let now = SystemTime::now();
        let raw = provider
            .artifacts
            .create_raw("a1", "s1", None, None, "echo test", "raw", now);
        provider.artifacts.append_chunk(&raw.id, "alpha");
        provider.artifacts.append_chunk(&raw.id, "beta");
        provider.artifacts.append_chunk(&raw.id, "alpha gamma");

        let derived = provider
            .refine_keyword(&raw.id, "a2", "alpha")
            .expect("derive");
        let chunks = provider.artifacts.read_chunks(&derived.id, 0, 10);
        assert_eq!(chunks.len(), 2);
    }

    #[test]
    fn refines_artifact_with_regex_like_filter() {
        let mut provider = TerminalProvider::default();
        let now = SystemTime::now();
        let raw =
            provider
                .artifacts
                .create_raw("a1", "s1", None, None, "logcat -b all", "raw", now);
        provider
            .artifacts
            .append_chunk(&raw.id, "system_server: ok");
        provider.artifacts.append_chunk(&raw.id, "Netd: ready");
        provider
            .artifacts
            .append_chunk(&raw.id, "surfaceflinger: frame");

        let derived = provider
            .refine_with_filter(&raw.id, "a2", "system_server|netd", "regex", true)
            .expect("derive");
        let chunks = provider.artifacts.read_chunks(&derived.id, 0, 10);
        assert_eq!(chunks.len(), 2);
    }

    #[test]
    fn blocks_dangerous_command_when_policy_requires_approval() {
        let mut provider = TerminalProvider::default();
        let err = provider
            .exec_local(
                "session",
                Some("ch-1"),
                Some("ts-1"),
                "rm -rf /tmp/x",
                "art",
                &PolicyProfile::default(),
            )
            .expect_err("must reject command");
        assert!(err.message.contains("requires approval"));
    }

    #[test]
    fn exec_local_records_basic_command_output() {
        let mut provider = TerminalProvider::default();
        let policy = PolicyProfile::default();
        #[cfg(windows)]
        let command = "echo provider-ok";
        #[cfg(not(windows))]
        let command = "printf 'provider-ok\\n'";

        let artifact = provider
            .exec_local(
                "session",
                Some("ch-1"),
                Some("ts-1"),
                command,
                "art",
                &policy,
            )
            .expect("exec command");
        let chunks = provider.artifacts.read_chunks(&artifact.id, 0, 20);
        assert!(
            chunks.iter().any(|line| line.contains("provider-ok")),
            "artifact chunks: {chunks:?}"
        );
    }

    #[test]
    fn exec_local_redacts_sensitive_output_and_uses_command_preview() {
        let mut provider = TerminalProvider::default();
        provider.register_sensitive_value("super-secret-token");
        let policy = PolicyProfile::default();
        #[cfg(windows)]
        let command = "echo super-secret-token";
        #[cfg(not(windows))]
        let command = "printf 'super-secret-token\\n'";

        let artifact = provider
            .exec_local(
                "session",
                Some("ch-redact"),
                Some("ts-redact"),
                command,
                "art-redact",
                &policy,
            )
            .expect("exec command");
        let stored = provider
            .artifacts
            .artifacts
            .get(&artifact.id)
            .expect("stored artifact");
        let source_command = stored.source_command.clone().unwrap_or_default();
        assert!(source_command.contains("cmd#"));
        assert!(!source_command.contains("super-secret-token"));

        let chunks = provider.artifacts.read_chunks(&artifact.id, 0, 20);
        assert!(chunks.iter().any(|line| line.contains("[REDACTED]")));
        assert!(!chunks
            .iter()
            .any(|line| line.contains("super-secret-token")));
    }

    #[test]
    fn exec_structured_invocation_runs_program_args_directly() {
        let mut provider = TerminalProvider::default();
        let policy = PolicyProfile::default();
        #[cfg(windows)]
        let invocation = bridgingio_connectors::CommandInvocation {
            program: "cmd".into(),
            args: vec!["/C".into(), "echo structured-direct-ok".into()],
            invocation_kind: bridgingio_connectors::InvocationKind::OneShot,
            target_terminal_family: bridgingio_domain::TerminalTargetFamily::Terminal,
            target_terminal_concurrency_policy:
                bridgingio_domain::TerminalConcurrencyPolicy::Multiplexed,
            target_shell_dialect: bridgingio_connectors::TargetShellDialect::SshPosix,
            resolution: bridgingio_connectors::InvocationResolution::default(),
            quoting_boundary: bridgingio_connectors::InvocationQuotingBoundary::default(),
        };
        #[cfg(not(windows))]
        let invocation = bridgingio_connectors::CommandInvocation {
            program: "/bin/sh".into(),
            args: vec!["-lc".into(), "printf 'structured-direct-ok\\n'".into()],
            invocation_kind: bridgingio_connectors::InvocationKind::OneShot,
            target_terminal_family: bridgingio_domain::TerminalTargetFamily::Terminal,
            target_terminal_concurrency_policy:
                bridgingio_domain::TerminalConcurrencyPolicy::Multiplexed,
            target_shell_dialect: bridgingio_connectors::TargetShellDialect::SshPosix,
            resolution: bridgingio_connectors::InvocationResolution::default(),
            quoting_boundary: bridgingio_connectors::InvocationQuotingBoundary::default(),
        };

        let artifact = provider
            .exec_structured_invocation(
                "session",
                Some("ch-structured"),
                Some("ts-structured"),
                &invocation,
                "__do_not_execute_this_via_shell__",
                "art-structured",
                &policy,
            )
            .expect("exec structured invocation");
        let chunks = provider.artifacts.read_chunks(&artifact.id, 0, 20);
        assert!(
            chunks
                .iter()
                .any(|line| line.contains("structured-direct-ok")),
            "artifact chunks: {chunks:?}"
        );
    }

    #[test]
    fn interactive_shell_preserves_env_and_cwd_per_channel() {
        let mut provider = TerminalProvider::default();
        let policy = PolicyProfile::default();
        let shell_a = provider
            .open_interactive_shell("ls-1", "ch-a", Some("ts-1"), "ssh")
            .expect("open shell a");
        let shell_b = provider
            .open_interactive_shell("ls-1", "ch-b", Some("ts-1"), "ssh")
            .expect("open shell b");

        provider
            .write_interactive_shell(&shell_a.shell_id, "export DEMO=hello", "a1", &policy)
            .expect("export");
        let output = provider
            .write_interactive_shell(&shell_a.shell_id, "printf \"$DEMO\\n\"", "a2", &policy)
            .expect("print env");
        assert!(output.output.contains("hello"));

        provider
            .write_interactive_shell(&shell_a.shell_id, "cd /tmp", "a3", &policy)
            .expect("cd");
        let pwd = provider
            .write_interactive_shell(&shell_a.shell_id, "pwd", "a4", &policy)
            .expect("pwd");
        assert!(pwd.output.trim().ends_with("/tmp"));

        let output_b = provider
            .write_interactive_shell(&shell_b.shell_id, "printf \"$DEMO\\n\"", "b1", &policy)
            .expect("print env b");
        assert!(!output_b.output.contains("hello"));
    }

    #[test]
    fn interactive_shell_supports_interrupt_close_and_read_transcript() {
        let mut provider = TerminalProvider::default();
        let policy = PolicyProfile::default();
        let shell = provider
            .open_interactive_shell("ls-2", "ch-1", Some("ts-1"), "adb")
            .expect("open shell");
        provider
            .write_interactive_shell(&shell.shell_id, "echo hello", "s1", &policy)
            .expect("write");
        provider
            .interrupt_interactive_shell(&shell.shell_id)
            .expect("interrupt");
        provider
            .close_interactive_shell(&shell.shell_id)
            .expect("close");

        let transcript = provider
            .read_interactive_transcript(&shell.shell_id, 0, 50)
            .expect("read");
        assert!(transcript.iter().any(|line| line.contains("hello")));
        assert!(transcript
            .iter()
            .any(|line| line.contains("interrupt requested")));
        assert!(transcript.iter().any(|line| line.contains("closed")));

        let err = provider
            .write_interactive_shell(&shell.shell_id, "echo after-close", "s2", &policy)
            .expect_err("must reject");
        assert!(err.message.contains("closed"));
    }

    #[test]
    fn interactive_transcript_is_redacted_via_shared_registry() {
        let mut provider = TerminalProvider::default();
        provider.register_sensitive_value("transcript-secret");
        let policy = PolicyProfile::default();
        let shell = provider
            .open_interactive_shell("ls-redact", "ch-1", Some("ts-1"), "ssh")
            .expect("open shell");
        #[cfg(windows)]
        let write_command = "echo transcript-secret";
        #[cfg(not(windows))]
        let write_command = "printf 'transcript-secret\\n'";
        provider
            .write_interactive_shell(&shell.shell_id, write_command, "a-redact", &policy)
            .expect("write secret");

        let transcript = provider
            .read_interactive_transcript(&shell.shell_id, 0, 200)
            .expect("read transcript");
        assert!(transcript.iter().any(|line| line.contains("[REDACTED]")));
        assert!(!transcript
            .iter()
            .any(|line| line.contains("transcript-secret")));
    }

    #[test]
    fn interactive_shell_diagnostics_expose_api_layering() {
        let mut provider = TerminalProvider::default();
        let shell = provider
            .open_interactive_shell("ls-3", "ch-1", Some("ts-1"), "ssh")
            .expect("open shell");
        let diagnostics = provider
            .interactive_shell_diagnostics(&shell.shell_id)
            .expect("diagnostics");

        assert_eq!(diagnostics.api_layering.startup, "open_interactive_shell");
        assert_eq!(diagnostics.api_layering.write, "write_interactive_shell");
        assert_eq!(diagnostics.api_layering.read, "read_interactive_transcript");
        assert_eq!(
            diagnostics.api_layering.state_query,
            "interactive_shell_state"
        );
        assert_eq!(
            diagnostics.api_layering.interrupt,
            "interrupt_interactive_shell"
        );
        assert_eq!(diagnostics.api_layering.close, "close_interactive_shell");
        assert_eq!(
            diagnostics.api_layering.diagnostics,
            "interactive_shell_diagnostics"
        );
    }

    #[cfg(unix)]
    #[test]
    fn interactive_shell_exposes_tty_for_terminal_programs() {
        let mut provider = TerminalProvider::default();
        let policy = PolicyProfile::default();
        let shell = provider
            .open_interactive_shell("ls-4", "ch-1", Some("ts-1"), "adb")
            .expect("open shell");
        let tty = provider
            .write_interactive_shell(&shell.shell_id, "tty", "tty-1", &policy)
            .expect("tty");
        let output = tty.output.to_lowercase();
        assert!(!output.contains("not a tty"));
        assert!(!output.contains("inappropriate ioctl"));
    }

    #[test]
    fn interactive_shell_launch_fallback_is_observable_when_structured_launch_fails() {
        let mut provider = TerminalProvider::default();
        let shell = provider
            .open_interactive_shell_with_command(
                "ls-5",
                "ch-1",
                Some("ts-1"),
                "ssh",
                Some("__definitely_missing_command__ --interactive"),
            )
            .expect("open shell with fallback");
        assert_eq!(
            shell.launch_strategy,
            "structured_interactive_invocation_with_host_baseline_fallback"
        );
        assert!(shell.launch_fallback_applied);
        assert!(shell
            .launch_diagnostics
            .iter()
            .any(|line| line.contains("structured interactive launch failed")));
        assert!(shell
            .transcript
            .iter()
            .any(|line| line.contains("fallback to host baseline")));
    }
}
