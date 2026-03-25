use std::time::SystemTime;

use bridgingio_domain::{
    AccessScope, CapabilitySummary, ChannelKind, SessionRecord, SessionReusePolicy, TargetKind,
    TargetProfile,
};
use bridgingio_engine::InMemoryMetadataStore;
use bridgingio_policy::{evaluate, OperationKind, PolicyDecision};
use bridgingio_providers::{GitProvider, TerminalProvider};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityEnvelope {
    pub target_id: String,
    pub session_id: Option<String>,
    pub captured_at: SystemTime,
    pub capabilities: Vec<CapabilitySummary>,
}

pub trait CapabilityDiscovery {
    fn describe_target(&self, target: &TargetProfile, session: Option<&SessionRecord>) -> CapabilityEnvelope;
}

#[derive(Default)]
pub struct DefaultCapabilityDiscovery;

impl CapabilityDiscovery for DefaultCapabilityDiscovery {
    fn describe_target(&self, target: &TargetProfile, session: Option<&SessionRecord>) -> CapabilityEnvelope {
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
    match kind {
        TargetKind::Ssh => vec![
            CapabilitySummary {
                id: "terminal.exec".into(),
                label: "execute shell command".into(),
                supports_streaming: true,
                supports_file_transfer: true,
                requires_approval: true,
                typed_entrypoints: vec!["terminal.exec".into(), "artifacts.read".into()],
                raw_fallback: true,
            },
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
        TargetKind::Adb => vec![CapabilitySummary {
            id: "terminal.exec".into(),
            label: "execute adb shell command".into(),
            supports_streaming: true,
            supports_file_transfer: true,
            requires_approval: true,
            typed_entrypoints: vec!["terminal.exec".into(), "artifacts.read".into()],
            raw_fallback: true,
        }],
        _ => vec![CapabilitySummary {
            id: "terminal.exec".into(),
            label: "execute command".into(),
            supports_streaming: false,
            supports_file_transfer: false,
            requires_approval: true,
            typed_entrypoints: vec!["terminal.exec".into()],
            raw_fallback: true,
        }],
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
        derived_artifact_id: String,
        keyword: String,
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
    ArtifactChunks {
        artifact_id: String,
        chunks: Vec<String>,
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

pub struct McpToolHandler {
    discovery: DefaultCapabilityDiscovery,
    terminal_provider: TerminalProvider,
    metadata: InMemoryMetadataStore,
}

impl Default for McpToolHandler {
    fn default() -> Self {
        Self {
            discovery: DefaultCapabilityDiscovery,
            terminal_provider: TerminalProvider::default(),
            metadata: InMemoryMetadataStore::default(),
        }
    }
}

impl McpToolHandler {
    pub fn metadata(&self) -> &InMemoryMetadataStore {
        &self.metadata
    }

    pub fn metadata_mut(&mut self) -> &mut InMemoryMetadataStore {
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
            } => ToolResult::ArtifactChunks {
                chunks: self
                    .terminal_provider
                    .artifacts
                    .read_chunks(&artifact_id, offset, limit),
                artifact_id,
            },
            ToolRequest::ArtifactsRefine {
                source_artifact_id,
                derived_artifact_id,
                keyword,
            } => match self.terminal_provider.refine_keyword(
                &source_artifact_id,
                &derived_artifact_id,
                &keyword,
            ) {
                Some(record) => ToolResult::Execution {
                    artifact_id: record.id,
                    logical_session_id: record.logical_session_id,
                    channel_id: record
                        .channel_id
                        .unwrap_or_else(|| "unknown-channel".to_string()),
                },
                None => ToolResult::Error {
                    message: "source artifact not found".into(),
                },
            },
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
                if matches!(evaluate(&policy, operation), PolicyDecision::RequireApproval) {
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
                self.metadata.upsert_artifact(record.clone());
                ToolResult::Execution {
                    artifact_id: record.id,
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
        scope_id: format!(
            "scope:{}:{}:{}",
            context.agent_id, context.run_id, context.client_session_id
        ),
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

#[cfg(test)]
mod tests {
    use bridgingio_domain::{ConnectionConfig, PolicyProfile, SessionReusePolicy, TargetProfile};
    use bridgingio_policy::OperationKind;

    use super::{
        CapabilityDiscovery, DefaultCapabilityDiscovery, McpToolHandler, ToolRequest,
        ToolRequestContext, ToolResult,
    };

    fn context(agent: &str, run: &str, client: &str, reuse_policy: SessionReusePolicy) -> ToolRequestContext {
        ToolRequestContext {
            agent_id: agent.into(),
            run_id: run.into(),
            client_session_id: client.into(),
            reuse_policy,
        }
    }

    #[test]
    fn returns_structured_capabilities_for_ssh_target() {
        let target = TargetProfile {
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
        };

        let discovery = DefaultCapabilityDiscovery;
        let envelope = discovery.describe_target(&target, None);
        assert!(!envelope.capabilities.is_empty());
        assert!(
            envelope
                .capabilities
                .iter()
                .any(|c| c.id == "terminal.exec")
        );
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
                SessionReusePolicy::ReuseIfAlive,
            ),
            command: "rm -rf /tmp/demo".into(),
            artifact_id: "artifact-1".into(),
            operation: OperationKind::Delete,
        });
        assert!(matches!(result, ToolResult::ApprovalRequired { .. }));
    }

    #[test]
    fn terminal_exec_reuses_logical_session_when_alive() {
        let mut handler = McpToolHandler::default();
        let req = |artifact_id: &str| ToolRequest::TerminalExec {
            target_id: "target-ssh".into(),
            target_kind: bridgingio_domain::TargetKind::Ssh,
            context: context(
                "agent-a",
                "run-1",
                "client-1",
                SessionReusePolicy::ReuseIfAlive,
            ),
            command: "printf 'ok'".into(),
            artifact_id: artifact_id.into(),
        };

        let first = handler.handle(req("artifact-1"));
        let second = handler.handle(req("artifact-2"));

        let first_ls = match first {
            ToolResult::Execution {
                logical_session_id, ..
            } => logical_session_id,
            other => panic!("expected execution, got {other:?}"),
        };
        let second_ls = match second {
            ToolResult::Execution {
                logical_session_id, ..
            } => logical_session_id,
            other => panic!("expected execution, got {other:?}"),
        };
        assert_eq!(first_ls, second_ls);
    }

    #[test]
    fn terminal_exec_isolates_different_agents() {
        let mut handler = McpToolHandler::default();
        let first = handler.handle(ToolRequest::TerminalExec {
            target_id: "target-ssh".into(),
            target_kind: bridgingio_domain::TargetKind::Ssh,
            context: context(
                "agent-a",
                "run-1",
                "client-1",
                SessionReusePolicy::ReuseIfAlive,
            ),
            command: "echo one".into(),
            artifact_id: "artifact-a".into(),
        });
        let second = handler.handle(ToolRequest::TerminalExec {
            target_id: "target-ssh".into(),
            target_kind: bridgingio_domain::TargetKind::Ssh,
            context: context(
                "agent-b",
                "run-1",
                "client-1",
                SessionReusePolicy::ReuseIfAlive,
            ),
            command: "echo two".into(),
            artifact_id: "artifact-b".into(),
        });

        let first_ls = match first {
            ToolResult::Execution {
                logical_session_id, ..
            } => logical_session_id,
            other => panic!("expected execution, got {other:?}"),
        };
        let second_ls = match second {
            ToolResult::Execution {
                logical_session_id, ..
            } => logical_session_id,
            other => panic!("expected execution, got {other:?}"),
        };
        assert_ne!(first_ls, second_ls);
    }
}
