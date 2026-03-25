use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use bridgingio_domain::{SessionReusePolicy, TargetKind};
use bridgingio_mcp::{McpToolHandler, ToolRequest, ToolRequestContext, ToolResult};
use bridgingio_policy::OperationKind;

fn context(agent: &str, run: &str, client: &str, reuse_policy: SessionReusePolicy) -> ToolRequestContext {
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
    let path = std::env::temp_dir().join(format!("bridgingio-mcp-{prefix}-{stamp}"));
    fs::create_dir_all(&path).expect("create temp dir");
    path
}

#[test]
fn validates_ssh_adb_git_flow_with_artifacts_and_approval() {
    let mut handler = McpToolHandler::default();
    let ssh_context = context(
        "agent-ssh",
        "run-ssh-1",
        "client-ssh",
        SessionReusePolicy::ReuseIfAlive,
    );

    let ssh_exec = handler.handle(ToolRequest::TerminalExec {
        target_id: "target-ssh".into(),
        target_kind: TargetKind::Ssh,
        context: ssh_context.clone(),
        command: "printf 'INFO boot\\nERROR panic\\n'".into(),
        artifact_id: "artifact-ssh-raw".into(),
    });
    let ssh_artifact = match ssh_exec {
        ToolResult::Execution { artifact_id, .. } => artifact_id,
        other => panic!("expected execution for ssh, got {other:?}"),
    };

    let chunks = handler.handle(ToolRequest::ArtifactsRead {
        artifact_id: ssh_artifact.clone(),
        offset: 0,
        limit: 20,
    });
    let output = match chunks {
        ToolResult::ArtifactChunks { chunks, .. } => chunks.join("\n"),
        other => panic!("expected artifact chunks, got {other:?}"),
    };
    assert!(output.contains("INFO boot"));
    assert!(output.contains("ERROR panic"));

    let derived = handler.handle(ToolRequest::ArtifactsRefine {
        source_artifact_id: ssh_artifact,
        derived_artifact_id: "artifact-ssh-derived".into(),
        keyword: "ERROR".into(),
    });
    assert!(matches!(derived, ToolResult::Execution { .. }));

    let approval = handler.handle(ToolRequest::TerminalExecRaw {
        target_id: "target-ssh".into(),
        target_kind: TargetKind::Ssh,
        context: ssh_context,
        command: "rm -rf /tmp/demo".into(),
        artifact_id: "artifact-ssh-delete".into(),
        operation: OperationKind::Delete,
    });
    assert!(matches!(approval, ToolResult::ApprovalRequired { .. }));

    let adb_exec = handler.handle(ToolRequest::TerminalExec {
        target_id: "target-adb".into(),
        target_kind: TargetKind::Adb,
        context: context(
            "agent-adb",
            "run-adb-1",
            "client-adb",
            SessionReusePolicy::ReuseIfAlive,
        ),
        command: "printf 'shell-ok\\n'".into(),
        artifact_id: "artifact-adb-raw".into(),
    });
    assert!(matches!(adb_exec, ToolResult::Execution { .. }));

    let repo = temp_dir("git-status");
    let init = Command::new("git")
        .arg("init")
        .current_dir(&repo)
        .output()
        .expect("init repo");
    assert!(init.status.success(), "git init failed: {:?}", init);
    fs::write(repo.join("demo.txt"), "hello\n").expect("write file");

    let git_status = handler.handle(ToolRequest::GitStatus {
        repo_path: repo.to_string_lossy().to_string(),
    });
    let status_output = match git_status {
        ToolResult::GitOutput { output } => output,
        other => panic!("expected git output, got {other:?}"),
    };
    assert!(status_output.contains("demo.txt"));
}

#[test]
fn validates_multi_agent_isolation_resume_and_multi_channel() {
    let mut handler = McpToolHandler::default();
    let exec_req = |agent: &str, reuse_policy: SessionReusePolicy, artifact_id: &str| {
        ToolRequest::TerminalExec {
            target_id: "target-shared".into(),
            target_kind: TargetKind::Ssh,
            context: context(agent, "run-1", "client-shared", reuse_policy),
            command: "printf 'ok\\n'".into(),
            artifact_id: artifact_id.into(),
        }
    };

    let first = handler.handle(exec_req("agent-a", SessionReusePolicy::ReuseIfAlive, "a1"));
    let logical_id = match first {
        ToolResult::Execution {
            logical_session_id, ..
        } => logical_session_id,
        other => panic!("expected execution, got {other:?}"),
    };

    let second = handler.handle(exec_req("agent-a", SessionReusePolicy::ReuseIfAlive, "a2"));
    let second_logical_id = match second {
        ToolResult::Execution {
            logical_session_id, ..
        } => logical_session_id,
        other => panic!("expected execution, got {other:?}"),
    };
    assert_eq!(logical_id, second_logical_id);

    handler
        .metadata_mut()
        .close_logical_session(&logical_id, "simulate disconnect", SystemTime::now())
        .expect("close logical session");

    let resumed = handler.handle(exec_req(
        "agent-a",
        SessionReusePolicy::ResumeOrCreate,
        "a3",
    ));
    let resumed_logical_id = match resumed {
        ToolResult::Execution {
            logical_session_id, ..
        } => logical_session_id,
        other => panic!("expected execution, got {other:?}"),
    };
    assert_eq!(logical_id, resumed_logical_id);

    let isolated = handler.handle(exec_req("agent-b", SessionReusePolicy::ReuseIfAlive, "b1"));
    let isolated_logical_id = match isolated {
        ToolResult::Execution {
            logical_session_id, ..
        } => logical_session_id,
        other => panic!("expected execution, got {other:?}"),
    };
    assert_ne!(logical_id, isolated_logical_id);

    let channels = handler.metadata().channels_for_logical_session(&logical_id);
    assert!(channels.len() >= 3);
}
