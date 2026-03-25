use std::path::PathBuf;
use std::process::Command;
use std::time::SystemTime;

use bridgingio_artifacts::InMemoryArtifactStore;
use bridgingio_domain::{ArtifactRecord, CapabilitySummary, PolicyProfile};
use bridgingio_policy::{evaluate, OperationKind, PolicyDecision};

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

#[derive(Default)]
pub struct TerminalProvider {
    pub artifacts: InMemoryArtifactStore,
}

impl TerminalProvider {
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

        let output = Command::new("/bin/sh")
            .arg("-lc")
            .arg(command)
            .output()
            .map_err(|e| ProviderError {
                message: format!("failed to run shell command: {e}"),
            })?;

        let created = self.artifacts.create_raw(
            artifact_id.to_string(),
            logical_session_id.to_string(),
            channel_id.map(|v| v.to_string()),
            transport_session_id.map(|v| v.to_string()),
            command.to_string(),
            "terminal command output",
            SystemTime::now(),
        );

        for line in String::from_utf8_lossy(&output.stdout).lines() {
            self.artifacts.append_chunk(&created.id, line.to_string());
        }
        for line in String::from_utf8_lossy(&output.stderr).lines() {
            self.artifacts
                .append_chunk(&created.id, format!("stderr: {line}"));
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
                message: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
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
}
