use std::fs;
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use bridgingio_app_api::{ApiRequest, ApiRequestContext, ApiResponse, AppCommand};
use bridgingio_connectors::{
    BuiltInBinarySpec, BuiltInDistributionKind, ExecutableResolver, ToolchainResolver,
};
use bridgingio_domain::{SessionReusePolicy, TargetKind};
use bridgingio_mcp::{
    CoreHostMode, CoreRuntimeError, McpToolHandler, ModelPlaneHttpServer, StandaloneCoreRuntime,
    ToolRequest, ToolRequestContext, ToolResult,
};
#[cfg(unix)]
use bridgingio_mcp::{ControlPlaneIpcClient, ControlPlaneIpcServer};
use bridgingio_platform::{detect_host_platform_adapter, HostPlatform};
use bridgingio_policy::OperationKind;
use serde_json::{json, Value};

fn context(
    agent: &str,
    run: &str,
    client: &str,
    reuse_policy: SessionReusePolicy,
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
    let path = std::env::temp_dir().join(format!("bridgingio-mcp-{prefix}-{stamp}"));
    fs::create_dir_all(&path).expect("create temp dir");
    path
}

#[cfg(unix)]
fn short_unix_socket_path(prefix: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    PathBuf::from("/tmp").join(format!("bridgingio-{prefix}-{stamp}.sock"))
}

fn toolchain_resolver(root: &PathBuf) -> ToolchainResolver {
    let ssh = root.join("ssh");
    let adb = root.join("adb");
    #[cfg(windows)]
    {
        fs::write(
            &ssh,
            r#"@echo off
if "%~1"=="-V" (
  >&2 echo OpenSSH_mock
  exit /b 0
)
if "%~1"=="-p" if not "%~4"=="" (
  cmd /C "%~4"
  exit /b %ERRORLEVEL%
)
cmd /Q
"#,
        )
        .expect("write ssh");
        fs::write(
            &adb,
            r#"@echo off
set "arg1=%~1"
if /I "%arg1%"=="-s" (
  shift
  shift
  set "arg1=%~1"
)
if /I "%arg1%"=="-e" (
  shift
  set "arg1=%~1"
)
if /I "%arg1%"=="-d" (
  shift
  set "arg1=%~1"
)
if /I not "%arg1%"=="shell" (
  >&2 echo unsupported adb mock invocation: %*
  exit /b 1
)
shift
if "%~1"=="" (
  cmd /Q
  exit /b %ERRORLEVEL%
)
cmd /C "%~1"
exit /b %ERRORLEVEL%
"#,
        )
        .expect("write adb");
    }
    #[cfg(not(windows))]
    {
        fs::write(
            &ssh,
            r#"#!/bin/sh
if [ "${1:-}" = "-V" ]; then
  echo "OpenSSH_mock" >&2
  exit 0
fi
if [ "${1:-}" = "-p" ] && [ "$#" -ge 4 ]; then
  remote_cmd="$4"
  /bin/sh -lc "$remote_cmd"
  exit $?
fi
exec /bin/sh -s
"#,
        )
        .expect("write ssh");
        fs::write(
            &adb,
            r#"#!/bin/sh
if [ "${1:-}" = "-s" ] || [ "${1:-}" = "-t" ]; then
  shift 2
fi
if [ "${1:-}" = "-e" ] || [ "${1:-}" = "-d" ]; then
  shift
fi
if [ "${1:-}" != "shell" ]; then
  echo "unsupported adb mock invocation: $*" >&2
  exit 1
fi
shift
if [ "$#" -gt 0 ]; then
  /bin/sh -lc "$1"
  exit $?
fi
exec /bin/sh -s
"#,
        )
        .expect("write adb");
    }
    #[cfg(unix)]
    {
        let mut ssh_perms = fs::metadata(&ssh).expect("ssh metadata").permissions();
        ssh_perms.set_mode(0o755);
        fs::set_permissions(&ssh, ssh_perms).expect("chmod ssh");
        let mut adb_perms = fs::metadata(&adb).expect("adb metadata").permissions();
        adb_perms.set_mode(0o755);
        fs::set_permissions(&adb, adb_perms).expect("chmod adb");
    }
    ToolchainResolver::new(
        ExecutableResolver::with_search_paths(vec![root.clone()]),
        root,
        vec![
            BuiltInBinarySpec {
                command: "ssh".into(),
                relative_path: PathBuf::from("embedded/ssh"),
                distribution: BuiltInDistributionKind::StandalonePackage,
            },
            BuiltInBinarySpec {
                command: "adb".into(),
                relative_path: PathBuf::from("embedded/adb"),
                distribution: BuiltInDistributionKind::StandalonePackage,
            },
        ],
    )
}

fn read_http_response(stream: &mut std::net::TcpStream) -> String {
    let mut buf = String::new();
    stream.read_to_string(&mut buf).expect("read http response");
    buf
}

fn parse_http_response(raw: &str) -> (u16, String) {
    let status = raw
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .expect("status code");
    let body = raw
        .split_once("\r\n\r\n")
        .map(|(_, tail)| tail.to_string())
        .unwrap_or_default();
    (status, body)
}

fn post_json(addr: std::net::SocketAddr, path: &str, payload: &Value) -> String {
    let body = payload.to_string();
    let mut stream = std::net::TcpStream::connect(addr).expect("connect http");
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(request.as_bytes()).expect("write request");
    read_http_response(&mut stream)
}

#[cfg(windows)]
fn quote_shell_path(path: &Path) -> String {
    format!("\"{}\"", path.to_string_lossy().replace('"', "\"\""))
}

#[cfg(not(windows))]
fn quote_shell_path(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
}

#[cfg(windows)]
fn command_info_boot_error_panic() -> &'static str {
    "echo INFO boot&& echo ERROR panic"
}

#[cfg(not(windows))]
fn command_info_boot_error_panic() -> &'static str {
    "printf 'INFO boot\\nERROR panic\\n'"
}

fn command_shell_ok() -> &'static str {
    "echo shell-ok"
}

#[cfg(windows)]
fn command_system_server_netd() -> &'static str {
    "echo system_server init&& echo netd ready"
}

#[cfg(not(windows))]
fn command_system_server_netd() -> &'static str {
    "printf 'system_server init\\nnetd ready\\n'"
}

fn command_ok() -> &'static str {
    "echo ok"
}

fn command_alias_ok() -> &'static str {
    "echo alias-ok"
}

fn command_read_cwd() -> &'static str {
    #[cfg(windows)]
    {
        return "cd";
    }
    #[cfg(not(windows))]
    {
        "pwd"
    }
}

fn command_change_cwd(path: &Path) -> String {
    #[cfg(windows)]
    {
        return format!("cd /d {}", quote_shell_path(path));
    }
    #[cfg(not(windows))]
    {
        format!("cd {}", quote_shell_path(path))
    }
}

fn long_running_interrupt_command() -> &'static str {
    #[cfg(windows)]
    {
        return "ping -n 3 127.0.0.1 >NUL && echo done";
    }
    #[cfg(not(windows))]
    {
        "i=0; while [ $i -lt 500000 ]; do i=$((i+1)); done; echo done"
    }
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
        command: command_info_boot_error_panic().into(),
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
        ToolResult::ArtifactRead { view } => view.chunks.join("\n"),
        other => panic!("expected artifact chunks, got {other:?}"),
    };
    assert!(output.contains("INFO boot"));
    assert!(output.contains("ERROR panic"));

    let derived = handler.handle(ToolRequest::ArtifactsRefine {
        source_artifact_id: ssh_artifact,
        pattern: "ERROR".into(),
        mode: "keyword".into(),
        ignore_case: false,
        processing_mode: "bridgingio".into(),
    });
    assert!(matches!(derived, ToolResult::ArtifactRefined { .. }));

    let approval = handler.handle(ToolRequest::TerminalExecRaw {
        target_id: "target-ssh".into(),
        target_kind: TargetKind::Ssh,
        context: ssh_context,
        command: "echo dangerous-delete".into(),
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
        command: command_shell_ok().into(),
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
fn validates_cross_target_artifact_reanalysis_without_ssh_adb_semantics() {
    let mut handler = McpToolHandler::default();
    let exec = handler.handle(ToolRequest::TerminalExec {
        target_id: "target-http-debug".into(),
        target_kind: TargetKind::Other("http-debug".into()),
        context: context(
            "agent-http",
            "run-http-1",
            "client-http",
            SessionReusePolicy::ReuseIfAlive,
        ),
        command: command_system_server_netd().into(),
        artifact_id: "artifact-http-raw".into(),
    });
    let source_artifact_id = match exec {
        ToolResult::Execution { artifact_id, .. } => artifact_id,
        other => panic!("expected execution, got {other:?}"),
    };

    let refined = handler.handle(ToolRequest::ArtifactsRefine {
        source_artifact_id,
        pattern: "netd".into(),
        mode: "keyword".into(),
        ignore_case: false,
        processing_mode: "auto".into(),
    });
    let derived_artifact_id = match refined {
        ToolResult::ArtifactRefined {
            artifact,
            requested_processing_mode,
            resolved_processing_mode,
            ..
        } => {
            assert_eq!(requested_processing_mode, "auto");
            assert_eq!(resolved_processing_mode, "bridgingio");
            artifact.id
        }
        other => panic!("expected refined artifact, got {other:?}"),
    };

    let chunks = handler.handle(ToolRequest::ArtifactsRead {
        artifact_id: derived_artifact_id,
        offset: 0,
        limit: 20,
    });
    let output = match chunks {
        ToolResult::ArtifactRead { view } => view.chunks.join("\n"),
        other => panic!("expected artifact chunks, got {other:?}"),
    };
    assert!(output.contains("netd ready"));
}

#[test]
fn validates_multi_agent_isolation_resume_and_multi_channel() {
    let mut handler = McpToolHandler::default();
    let exec_req = |agent: &str, reuse_policy: SessionReusePolicy, artifact_id: &str| {
        ToolRequest::TerminalExec {
            target_id: "target-shared".into(),
            target_kind: TargetKind::Ssh,
            context: context(agent, "run-1", "client-shared", reuse_policy),
            command: command_ok().into(),
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

#[cfg(unix)]
#[test]
fn validates_shared_state_between_ipc_control_plane_and_http_model_plane() {
    let root = temp_dir("shared-state");
    let config_text =
        bridgingio_engine::CoreSettings::minimal_example().replace("port = 19718", "port = 0");
    let settings =
        bridgingio_engine::CoreSettings::from_toml_str(&config_text).expect("parse config");
    let runtime = StandaloneCoreRuntime::from_settings(settings.clone(), toolchain_resolver(&root))
        .expect("runtime")
        .shared();

    let socket_path = short_unix_socket_path("shared-state");
    let ipc_server = match ControlPlaneIpcServer::bind(runtime.clone(), &socket_path) {
        Ok(server) => server,
        Err(CoreRuntimeError::Io(message)) if message.contains("Operation not permitted") => {
            return;
        }
        Err(err) => panic!("bind ipc failed: {err:?}"),
    };
    let http_server = ModelPlaneHttpServer::bind(runtime.clone(), &settings).expect("bind http");

    let ipc_thread = thread::spawn(move || ipc_server.serve_once().expect("serve ipc"));
    let ipc_client = ControlPlaneIpcClient::new(&socket_path);
    let open_request = ApiRequest {
        request_id: "req-open".into(),
        context: ApiRequestContext {
            agent_id: "agent-control".into(),
            run_id: "run-control-1".into(),
            client_session_id: "client-control".into(),
            reuse_policy: SessionReusePolicy::ReuseIfAlive,
        },
        command: AppCommand::OpenSession {
            target_id: "local-ssh".into(),
        },
    };
    let open_response = ipc_client.send(&open_request).expect("ipc send");
    assert!(matches!(open_response, ApiResponse::Accepted { .. }));
    ipc_thread.join().expect("ipc join");

    let http_addr = http_server.local_addr().expect("http addr");
    let http_thread = thread::spawn(move || http_server.serve_once().expect("serve http"));
    let mut stream = std::net::TcpStream::connect(http_addr).expect("connect model plane");
    stream
        .write_all(b"GET /state/sessions HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .expect("write http");
    let response = read_http_response(&mut stream);
    assert!(
        response.contains("sessions=1"),
        "unexpected response: {response}"
    );
    http_thread.join().expect("http join");
}

#[cfg(unix)]
#[test]
fn validates_headless_smoke_with_standalone_sample_config() {
    let root = temp_dir("headless-smoke");
    let config_path = root.join("standalone.toml");
    let adb_override = root.join("adb");
    fs::write(&adb_override, "binary").expect("write adb");

    let config_text = bridgingio_engine::CoreSettings::complete_example()
        .replace("__GLOBAL_ADB_OVERRIDE__", &adb_override.to_string_lossy())
        .replace("port = 19718", "port = 0");
    fs::write(&config_path, config_text).expect("write config");

    let resolver = toolchain_resolver(&root);
    let mut runtime =
        StandaloneCoreRuntime::from_config_file(&config_path, resolver).expect("load runtime");
    assert_eq!(runtime.settings_store.settings.targets.len(), 2);

    let open_response = runtime.handle_app_request(ApiRequest {
        request_id: "open-1".into(),
        context: ApiRequestContext {
            agent_id: "agent-smoke".into(),
            run_id: "run-smoke".into(),
            client_session_id: "client-smoke".into(),
            reuse_policy: SessionReusePolicy::ReuseIfAlive,
        },
        command: AppCommand::OpenSession {
            target_id: "lab-ssh-01".into(),
        },
    });
    let session_id = match open_response {
        ApiResponse::Session { session, .. } => session.id,
        other => panic!("expected session response, got {other:?}"),
    };

    let exec_response = runtime.handle_app_request(ApiRequest {
        request_id: "exec-1".into(),
        context: ApiRequestContext {
            agent_id: "agent-smoke".into(),
            run_id: "run-smoke".into(),
            client_session_id: "client-smoke".into(),
            reuse_policy: SessionReusePolicy::ReuseIfAlive,
        },
        command: AppCommand::Execute {
            session_id,
            command: command_shell_ok().into(),
            stream: false,
        },
    });
    assert!(matches!(exec_response, ApiResponse::Execution { .. }));
}

#[cfg(unix)]
#[test]
fn validates_ui_managed_mode_gates_mcp_until_attach() {
    let root = temp_dir("ui-managed-gating");
    let config_text =
        bridgingio_engine::CoreSettings::minimal_example().replace("port = 19718", "port = 0");
    let settings =
        bridgingio_engine::CoreSettings::from_toml_str(&config_text).expect("parse config");
    let runtime = StandaloneCoreRuntime::from_settings_with_mode(
        settings.clone(),
        toolchain_resolver(&root),
        CoreHostMode::UiManagedEphemeral,
    )
    .expect("runtime")
    .shared();

    let socket_path = short_unix_socket_path("ui-managed");
    let ipc_server = match ControlPlaneIpcServer::bind(runtime.clone(), &socket_path) {
        Ok(server) => server,
        Err(CoreRuntimeError::Io(message)) if message.contains("Operation not permitted") => {
            return;
        }
        Err(err) => panic!("bind ipc failed: {err:?}"),
    };
    let http_server = ModelPlaneHttpServer::bind(runtime, &settings).expect("bind http");
    let addr = http_server.local_addr().expect("http addr");

    let initialize = json!({
        "jsonrpc": "2.0",
        "id": "pre-attach",
        "method": "initialize",
        "params": {}
    });
    let pre_client = thread::spawn(move || post_json(addr, "/mcp", &initialize));
    http_server.serve_once().expect("serve pre-attach");
    let (status, body) = parse_http_response(&pre_client.join().expect("join pre-attach"));
    assert_eq!(status, 503, "expected not-ready status before attach");
    assert!(
        body.contains("not ready"),
        "expected not-ready message before attach, got {body}"
    );

    let ipc_client = ControlPlaneIpcClient::new(&socket_path);
    let attach_thread = thread::spawn(move || ipc_server.serve_once().expect("serve attach ipc"));
    let attach_response = ipc_client
        .send(&ApiRequest {
            request_id: "attach-1".into(),
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
        })
        .expect("attach send");
    assert!(matches!(attach_response, ApiResponse::Attached { .. }));
    attach_thread.join().expect("join attach thread");

    let addr = http_server.local_addr().expect("http addr");
    let initialize_after = json!({
        "jsonrpc": "2.0",
        "id": "post-attach",
        "method": "initialize",
        "params": {}
    });
    let post_client = thread::spawn(move || post_json(addr, "/mcp", &initialize_after));
    http_server.serve_once().expect("serve post-attach");
    let (status, body) = parse_http_response(&post_client.join().expect("join post-attach"));
    assert_eq!(status, 200, "expected ready after attach");
    let payload: Value = serde_json::from_str(&body).expect("json payload");
    assert_eq!(
        payload["result"]["serverInfo"]["name"].as_str(),
        Some("bridgingio-core")
    );
}

#[test]
fn validates_mcp_http_jsonrpc_entry_and_alias_exec() {
    let root = temp_dir("mcp-jsonrpc");
    let config_text =
        bridgingio_engine::CoreSettings::minimal_example().replace("port = 19718", "port = 0");
    let settings =
        bridgingio_engine::CoreSettings::from_toml_str(&config_text).expect("parse config");
    let runtime = StandaloneCoreRuntime::from_settings(settings.clone(), toolchain_resolver(&root))
        .expect("runtime")
        .shared();
    let http_server = ModelPlaneHttpServer::bind(runtime, &settings).expect("bind http");
    let addr = http_server.local_addr().expect("http addr");

    let initialize = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {}
    });
    let initialize_client = thread::spawn(move || post_json(addr, "/mcp", &initialize));
    http_server.serve_once().expect("serve initialize");
    let (status, body) = parse_http_response(&initialize_client.join().expect("join initialize"));
    assert_eq!(status, 200);
    let payload: Value = serde_json::from_str(&body).expect("initialize json");
    assert_eq!(
        payload["result"]["serverInfo"]["name"].as_str(),
        Some("bridgingio-core")
    );

    let addr = http_server.local_addr().expect("http addr");
    let list_resources = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "resources/list",
        "params": {}
    });
    let resources_client = thread::spawn(move || post_json(addr, "/mcp", &list_resources));
    http_server.serve_once().expect("serve resources/list");
    let (status, body) =
        parse_http_response(&resources_client.join().expect("join resources/list"));
    assert_eq!(status, 200);
    let payload: Value = serde_json::from_str(&body).expect("resources list json");
    let resources = payload["result"]["resources"]
        .as_array()
        .expect("resources array");
    assert!(!resources.is_empty());
    assert!(resources.iter().any(|item| {
        item["uri"]
            .as_str()
            .unwrap_or_default()
            .starts_with("bridgingio://capability/")
    }));

    let addr = http_server.local_addr().expect("http addr");
    let list_resource_templates = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "resources/templates/list",
        "params": {}
    });
    let templates_client = thread::spawn(move || post_json(addr, "/mcp", &list_resource_templates));
    http_server
        .serve_once()
        .expect("serve resources/templates/list");
    let (status, body) = parse_http_response(
        &templates_client
            .join()
            .expect("join resources/templates/list"),
    );
    assert_eq!(status, 200);
    let payload: Value = serde_json::from_str(&body).expect("resource templates json");
    assert!(payload["result"]["resourceTemplates"]
        .as_array()
        .map(|items| !items.is_empty())
        .unwrap_or(false));

    let addr = http_server.local_addr().expect("http addr");
    let list_tools = json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/list",
        "params": {}
    });
    let list_client = thread::spawn(move || post_json(addr, "/mcp", &list_tools));
    http_server.serve_once().expect("serve tools/list");
    let (status, body) = parse_http_response(&list_client.join().expect("join tools/list"));
    assert_eq!(status, 200);
    let payload: Value = serde_json::from_str(&body).expect("list json");
    let tool_names = payload["result"]["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect::<Vec<_>>();
    assert!(payload["result"]["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .all(|tool| !tool["description"].as_str().unwrap_or_default().is_empty()));
    assert!(tool_names.contains(&"bridgingio.terminal.exec"));
    assert!(tool_names.contains(&"bridgingio.terminal.shell.open"));
    assert!(tool_names.contains(&"bridgingio.terminal.shell.write"));
    assert!(tool_names.contains(&"bridgingio.terminal.shell.read"));
    assert!(tool_names.contains(&"bridgingio.terminal.shell.interrupt"));
    assert!(tool_names.contains(&"bridgingio.terminal.shell.close"));
    assert!(tool_names.contains(&"bridgingio.artifacts.read"));
    assert!(tool_names.contains(&"bridgingio.artifacts.refine"));
    assert!(tool_names.contains(&"bridgingio.target.inspect_basic"));
    assert!(tool_names.contains(&"bridgingio.capability.describe"));

    let addr = http_server.local_addr().expect("http addr");
    let call_exec = json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
                "name": "bridgingio.terminal.exec",
                "arguments": {
                    "target": "local",
                    "command": command_alias_ok(),
                    "agent_id": "agent-mcp",
                    "run_id": "run-mcp-1",
                    "client_session_id": "client-mcp",
                    "reuse_policy": "reuse_if_alive"
                }
        }
    });
    let call_client = thread::spawn(move || post_json(addr, "/mcp", &call_exec));
    http_server.serve_once().expect("serve tools/call");
    let (status, body) = parse_http_response(&call_client.join().expect("join tools/call"));
    assert_eq!(status, 200);
    let payload: Value = serde_json::from_str(&body).expect("call json");
    assert_eq!(
        payload["result"]["structuredContent"]["resolved_target_id"].as_str(),
        Some("local-ssh")
    );
    assert_eq!(
        payload["result"]["structuredContent"]["requested_target_ref"].as_str(),
        Some("local")
    );
    assert!(payload["result"]["structuredContent"]["output"]
        .as_str()
        .unwrap_or_default()
        .contains("alias-ok"));

    let source_artifact_id = payload["result"]["structuredContent"]["artifact_id"]
        .as_str()
        .expect("source artifact id")
        .to_string();

    let addr = http_server.local_addr().expect("http addr");
    let refine = json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": {
            "name": "bridgingio.artifacts.refine",
            "arguments": {
                "source_artifact_id": source_artifact_id,
                "pattern": "ALIAS|alias",
                "grep_flags": "Ei",
                "processing_mode": "source"
            }
        }
    });
    let refine_client = thread::spawn(move || post_json(addr, "/mcp", &refine));
    http_server.serve_once().expect("serve artifacts.refine");
    let (status, body) = parse_http_response(&refine_client.join().expect("join refine"));
    assert_eq!(status, 200);
    let payload: Value = serde_json::from_str(&body).expect("refine json");
    assert_eq!(
        payload["result"]["isError"].as_bool(),
        Some(false),
        "payload={payload}"
    );
    assert_eq!(
        payload["result"]["structuredContent"]["requested_processing_mode"].as_str(),
        Some("source")
    );
    assert_eq!(
        payload["result"]["structuredContent"]["resolved_processing_mode"].as_str(),
        Some("bridgingio")
    );
    assert!(
        payload["result"]["structuredContent"]["line_count"]
            .as_u64()
            .unwrap_or_default()
            >= 1
    );

    let addr = http_server.local_addr().expect("http addr");
    let describe = json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": {
            "name": "bridgingio.capability.describe",
            "arguments": {
                "id": "artifact.reanalysis",
                "kind": "capability"
            }
        }
    });
    let describe_client = thread::spawn(move || post_json(addr, "/mcp", &describe));
    http_server.serve_once().expect("serve capability.describe");
    let (status, body) = parse_http_response(&describe_client.join().expect("join describe"));
    assert_eq!(status, 200);
    let payload: Value = serde_json::from_str(&body).expect("describe json");
    assert_eq!(
        payload["result"]["structuredContent"]["kind"].as_str(),
        Some("capability")
    );
    assert_eq!(
        payload["result"]["structuredContent"]["id"].as_str(),
        Some("artifact.reanalysis")
    );
    assert!(
        payload["result"]["structuredContent"]["detailed_description"]
            .as_str()
            .unwrap_or_default()
            .contains("first-class capability")
    );

    let addr = http_server.local_addr().expect("http addr");
    let describe_tool_alias = json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": {
            "name": "bridgingio.capability.describe",
            "arguments": {
                "id": "bridgingio_terminal_exec",
                "kind": "tool"
            }
        }
    });
    let describe_tool_alias_client =
        thread::spawn(move || post_json(addr, "/mcp", &describe_tool_alias));
    http_server
        .serve_once()
        .expect("serve capability.describe alias tool");
    let (status, body) = parse_http_response(
        &describe_tool_alias_client
            .join()
            .expect("join describe alias"),
    );
    assert_eq!(status, 200);
    let payload: Value = serde_json::from_str(&body).expect("describe alias json");
    assert_eq!(
        payload["result"]["structuredContent"]["id"].as_str(),
        Some("bridgingio.terminal.exec")
    );

    let addr = http_server.local_addr().expect("http addr");
    let read_resource = json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "resources/read",
        "params": {
            "uri": "bridgingio://capability/artifact.reanalysis"
        }
    });
    let read_resource_client = thread::spawn(move || post_json(addr, "/mcp", &read_resource));
    http_server.serve_once().expect("serve resources/read");
    let (status, body) =
        parse_http_response(&read_resource_client.join().expect("join resources/read"));
    assert_eq!(status, 200);
    let payload: Value = serde_json::from_str(&body).expect("resources/read json");
    assert!(payload["result"]["contents"]
        .as_array()
        .map(|items| !items.is_empty())
        .unwrap_or(false));

    let addr = http_server.local_addr().expect("http addr");
    let missing_tool = json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": {
            "name": "bridgingio.not.exists",
            "arguments": {}
        }
    });
    let missing_client = thread::spawn(move || post_json(addr, "/mcp", &missing_tool));
    http_server.serve_once().expect("serve missing tool");
    let (status, body) = parse_http_response(&missing_client.join().expect("join missing"));
    assert_eq!(status, 200);
    let payload: Value = serde_json::from_str(&body).expect("missing tool json");
    assert_eq!(payload["id"].as_u64(), Some(10));
    assert!(payload.get("error").is_some());
}

#[test]
fn validates_interactive_shell_mode_context_isolation_and_lifecycle() {
    let root = temp_dir("mcp-interactive");
    let config_text =
        bridgingio_engine::CoreSettings::minimal_example().replace("port = 19718", "port = 0");
    let settings =
        bridgingio_engine::CoreSettings::from_toml_str(&config_text).expect("parse config");
    let runtime = StandaloneCoreRuntime::from_settings(settings.clone(), toolchain_resolver(&root))
        .expect("runtime")
        .shared();
    let http_server = ModelPlaneHttpServer::bind(runtime, &settings).expect("bind http");
    let interactive_cwd = temp_dir("interactive-shell-cwd");
    let change_cwd_command = command_change_cwd(&interactive_cwd);
    let read_cwd_command = command_read_cwd();
    let cwd_hint = interactive_cwd
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_else(|| interactive_cwd.to_string_lossy().to_ascii_lowercase());

    let addr = http_server.local_addr().expect("http addr");
    let open = json!({
        "jsonrpc": "2.0",
        "id": 11,
        "method": "tools/call",
        "params": {
            "name": "bridgingio.terminal.shell.open",
            "arguments": {
                "target": "local",
                "agent_id": "agent-a",
                "run_id": "run-1",
                "client_session_id": "client-1",
                "reuse_policy": "reuse_if_alive"
            }
        }
    });
    let open_client = thread::spawn(move || post_json(addr, "/mcp", &open));
    http_server.serve_once().expect("serve open");
    let (_, body) = parse_http_response(&open_client.join().expect("join open"));
    let payload: Value = serde_json::from_str(&body).expect("open json");
    let shell_id = payload["result"]["structuredContent"]["shell_id"]
        .as_str()
        .expect("shell id")
        .to_string();
    assert_eq!(
        payload["result"]["structuredContent"]["launch_strategy"].as_str(),
        Some("structured_interactive_invocation")
    );
    assert_eq!(
        payload["result"]["structuredContent"]["launch_fallback_applied"].as_bool(),
        Some(false)
    );

    let addr = http_server.local_addr().expect("http addr");
    let write_export = json!({
        "jsonrpc": "2.0",
        "id": 12,
        "method": "tools/call",
        "params": {
            "name": "bridgingio.terminal.shell.write",
            "arguments": {
                "shell_id": shell_id.clone(),
                "input": change_cwd_command,
                "agent_id": "agent-a",
                "run_id": "run-1",
                "client_session_id": "client-1",
                "reuse_policy": "reuse_if_alive"
            }
        }
    });
    let export_client = thread::spawn(move || post_json(addr, "/mcp", &write_export));
    http_server.serve_once().expect("serve write export");
    let (_, body) = parse_http_response(&export_client.join().expect("join write export"));
    let payload: Value = serde_json::from_str(&body).expect("export json");
    assert_eq!(payload["result"]["isError"].as_bool(), Some(false));

    let addr = http_server.local_addr().expect("http addr");
    let write_print = json!({
        "jsonrpc": "2.0",
        "id": 13,
        "method": "tools/call",
        "params": {
            "name": "bridgingio.terminal.shell.write",
            "arguments": {
                "shell_id": shell_id.clone(),
                "input": read_cwd_command,
                "agent_id": "agent-a",
                "run_id": "run-1",
                "client_session_id": "client-1",
                "reuse_policy": "reuse_if_alive"
            }
        }
    });
    let print_client = thread::spawn(move || post_json(addr, "/mcp", &write_print));
    http_server.serve_once().expect("serve write print");
    let (_, body) = parse_http_response(&print_client.join().expect("join write print"));
    let payload: Value = serde_json::from_str(&body).expect("print json");
    assert!(payload["result"]["structuredContent"]["output"]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .contains(&cwd_hint));

    let addr = http_server.local_addr().expect("http addr");
    let open_second = json!({
        "jsonrpc": "2.0",
        "id": 14,
        "method": "tools/call",
        "params": {
            "name": "bridgingio.terminal.shell.open",
            "arguments": {
                "target": "local",
                "agent_id": "agent-a",
                "run_id": "run-1",
                "client_session_id": "client-1",
                "reuse_policy": "reuse_if_alive"
            }
        }
    });
    let open_second_client = thread::spawn(move || post_json(addr, "/mcp", &open_second));
    http_server.serve_once().expect("serve open second");
    let (_, body) = parse_http_response(&open_second_client.join().expect("join open second"));
    let payload: Value = serde_json::from_str(&body).expect("open second json");
    let shell_2 = payload["result"]["structuredContent"]["shell_id"]
        .as_str()
        .expect("shell2")
        .to_string();
    assert_ne!(shell_2, shell_id);

    let addr = http_server.local_addr().expect("http addr");
    let write_second = json!({
        "jsonrpc": "2.0",
        "id": 15,
        "method": "tools/call",
        "params": {
            "name": "bridgingio.terminal.shell.write",
            "arguments": {
                "shell_id": shell_2.clone(),
                "input": read_cwd_command,
                "agent_id": "agent-a",
                "run_id": "run-1",
                "client_session_id": "client-1",
                "reuse_policy": "reuse_if_alive"
            }
        }
    });
    let write_second_client = thread::spawn(move || post_json(addr, "/mcp", &write_second));
    http_server.serve_once().expect("serve write second");
    let (_, body) = parse_http_response(&write_second_client.join().expect("join write second"));
    let payload: Value = serde_json::from_str(&body).expect("write second json");
    assert!(!payload["result"]["structuredContent"]["output"]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .contains(&cwd_hint));

    let addr = http_server.local_addr().expect("http addr");
    let read = json!({
        "jsonrpc": "2.0",
        "id": 16,
        "method": "tools/call",
        "params": {
            "name": "bridgingio.terminal.shell.read",
            "arguments": {
                "shell_id": shell_id.clone(),
                "offset": 0,
                "limit": 50,
                "agent_id": "agent-a",
                "run_id": "run-1",
                "client_session_id": "client-1",
                "reuse_policy": "reuse_if_alive"
            }
        }
    });
    let read_client = thread::spawn(move || post_json(addr, "/mcp", &read));
    http_server.serve_once().expect("serve read");
    let (_, body) = parse_http_response(&read_client.join().expect("join read"));
    let payload: Value = serde_json::from_str(&body).expect("read json");
    let lines = payload["result"]["structuredContent"]["lines"]
        .as_array()
        .expect("lines array");
    assert_eq!(
        payload["result"]["structuredContent"]["running"].as_bool(),
        Some(false)
    );
    assert_eq!(
        payload["result"]["structuredContent"]["closed"].as_bool(),
        Some(false)
    );
    assert!(lines
        .iter()
        .any(|line| {
            line.as_str()
                .unwrap_or_default()
                .to_ascii_lowercase()
                .contains(&cwd_hint)
        }));

    let addr = http_server.local_addr().expect("http addr");
    let cross_scope_read = json!({
        "jsonrpc": "2.0",
        "id": 17,
        "method": "tools/call",
        "params": {
            "name": "bridgingio.terminal.shell.read",
            "arguments": {
                "shell_id": shell_id.clone(),
                "offset": 0,
                "limit": 5,
                "agent_id": "agent-b",
                "run_id": "run-1",
                "client_session_id": "client-1",
                "reuse_policy": "reuse_if_alive"
            }
        }
    });
    let cross_scope_client = thread::spawn(move || post_json(addr, "/mcp", &cross_scope_read));
    http_server.serve_once().expect("serve cross-scope read");
    let (_, body) = parse_http_response(&cross_scope_client.join().expect("join cross-scope"));
    let payload: Value = serde_json::from_str(&body).expect("cross-scope json");
    assert!(payload.get("error").is_some());
    assert!(payload["error"]["message"]
        .as_str()
        .unwrap_or_default()
        .contains("access denied"));

    let addr = http_server.local_addr().expect("http addr");
    let interrupt = json!({
        "jsonrpc": "2.0",
        "id": 18,
        "method": "tools/call",
        "params": {
            "name": "bridgingio.terminal.shell.interrupt",
            "arguments": {
                "shell_id": shell_id.clone(),
                "agent_id": "agent-a",
                "run_id": "run-1",
                "client_session_id": "client-1",
                "reuse_policy": "reuse_if_alive"
            }
        }
    });
    let interrupt_client = thread::spawn(move || post_json(addr, "/mcp", &interrupt));
    http_server.serve_once().expect("serve interrupt");
    let (_, body) = parse_http_response(&interrupt_client.join().expect("join interrupt"));
    let payload: Value = serde_json::from_str(&body).expect("interrupt json");
    assert_eq!(
        payload["result"]["structuredContent"]["interrupted"].as_bool(),
        Some(true)
    );
    assert_eq!(
        payload["result"]["structuredContent"]["running"].as_bool(),
        Some(false)
    );
    assert_eq!(
        payload["result"]["structuredContent"]["closed"].as_bool(),
        Some(false)
    );

    let addr = http_server.local_addr().expect("http addr");
    let close = json!({
        "jsonrpc": "2.0",
        "id": 19,
        "method": "tools/call",
        "params": {
            "name": "bridgingio.terminal.shell.close",
            "arguments": {
                "shell_id": shell_id.clone(),
                "agent_id": "agent-a",
                "run_id": "run-1",
                "client_session_id": "client-1",
                "reuse_policy": "reuse_if_alive"
            }
        }
    });
    let close_client = thread::spawn(move || post_json(addr, "/mcp", &close));
    http_server.serve_once().expect("serve close");
    let (_, body) = parse_http_response(&close_client.join().expect("join close"));
    let payload: Value = serde_json::from_str(&body).expect("close json");
    assert_eq!(
        payload["result"]["structuredContent"]["closed"].as_bool(),
        Some(true)
    );
    assert_eq!(
        payload["result"]["structuredContent"]["running"].as_bool(),
        Some(false)
    );

    let addr = http_server.local_addr().expect("http addr");
    let write_after_close = json!({
        "jsonrpc": "2.0",
        "id": 20,
        "method": "tools/call",
        "params": {
            "name": "bridgingio.terminal.shell.write",
            "arguments": {
                "shell_id": shell_id.clone(),
                "input": "echo should-fail",
                "agent_id": "agent-a",
                "run_id": "run-1",
                "client_session_id": "client-1",
                "reuse_policy": "reuse_if_alive"
            }
        }
    });
    let write_after_close_client =
        thread::spawn(move || post_json(addr, "/mcp", &write_after_close));
    http_server.serve_once().expect("serve write after close");
    let (_, body) = parse_http_response(
        &write_after_close_client
            .join()
            .expect("join write after close"),
    );
    let payload: Value = serde_json::from_str(&body).expect("after close json");
    assert!(payload.get("error").is_some());
    assert!(payload["error"]["message"]
        .as_str()
        .unwrap_or_default()
        .contains("closed"));
}

#[test]
fn validates_interactive_shell_long_running_interrupt_flow() {
    let root = temp_dir("mcp-interrupt");
    let config_text =
        bridgingio_engine::CoreSettings::minimal_example().replace("port = 19718", "port = 0");
    let settings =
        bridgingio_engine::CoreSettings::from_toml_str(&config_text).expect("parse config");
    let runtime = StandaloneCoreRuntime::from_settings(settings.clone(), toolchain_resolver(&root))
        .expect("runtime")
        .shared();
    let http_server = ModelPlaneHttpServer::bind(runtime, &settings).expect("bind http");

    let addr = http_server.local_addr().expect("http addr");
    let open = json!({
        "jsonrpc": "2.0",
        "id": 31,
        "method": "tools/call",
        "params": {
            "name": "bridgingio.terminal.shell.open",
            "arguments": {
                "target": "local",
                "agent_id": "agent-i",
                "run_id": "run-1",
                "client_session_id": "client-1",
                "reuse_policy": "reuse_if_alive"
            }
        }
    });
    let open_client = thread::spawn(move || post_json(addr, "/mcp", &open));
    http_server.serve_once().expect("serve open");
    let (_, body) = parse_http_response(&open_client.join().expect("join open"));
    let payload: Value = serde_json::from_str(&body).expect("open json");
    let shell_id = payload["result"]["structuredContent"]["shell_id"]
        .as_str()
        .expect("shell id")
        .to_string();
    assert_eq!(
        payload["result"]["structuredContent"]["launch_strategy"].as_str(),
        Some("structured_interactive_invocation")
    );
    assert_eq!(
        payload["result"]["structuredContent"]["launch_fallback_applied"].as_bool(),
        Some(false)
    );

    let addr = http_server.local_addr().expect("http addr");
    let write = json!({
        "jsonrpc": "2.0",
        "id": 32,
        "method": "tools/call",
        "params": {
            "name": "bridgingio.terminal.shell.write",
            "arguments": {
                "shell_id": shell_id.clone(),
                "input": long_running_interrupt_command(),
                "agent_id": "agent-i",
                "run_id": "run-1",
                "client_session_id": "client-1",
                "reuse_policy": "reuse_if_alive"
            }
        }
    });
    let write_client = thread::spawn(move || post_json(addr, "/mcp", &write));
    http_server.serve_once().expect("serve write");
    let (_, body) = parse_http_response(&write_client.join().expect("join write"));
    let payload: Value = serde_json::from_str(&body).expect("write json");

    if payload["result"]["structuredContent"]["running"].as_bool() == Some(true) {
        let addr = http_server.local_addr().expect("http addr");
        let interrupt = json!({
            "jsonrpc": "2.0",
            "id": 33,
            "method": "tools/call",
            "params": {
                "name": "bridgingio.terminal.shell.interrupt",
                "arguments": {
                    "shell_id": shell_id,
                    "agent_id": "agent-i",
                    "run_id": "run-1",
                    "client_session_id": "client-1",
                    "reuse_policy": "reuse_if_alive"
                }
            }
        });
        let interrupt_client = thread::spawn(move || post_json(addr, "/mcp", &interrupt));
        http_server.serve_once().expect("serve interrupt");
        let (_, body) = parse_http_response(&interrupt_client.join().expect("join interrupt"));
        let payload: Value = serde_json::from_str(&body).expect("interrupt json");
        assert_eq!(
            payload["result"]["structuredContent"]["interrupted"].as_bool(),
            Some(true)
        );
        assert_eq!(
            payload["result"]["structuredContent"]["running"].as_bool(),
            Some(false)
        );
    } else {
        assert!(
            payload["result"]["structuredContent"]["output"]
                .as_str()
                .unwrap_or_default()
                .contains("done"),
            "expected completed output to contain done marker"
        );
    }
}

#[test]
fn validates_platform_matrix_contract_for_transport_paths_and_toolchain_fallback() {
    let root = temp_dir("platform-matrix-contract");
    let bundled_root = root.join("bundled");
    fs::create_dir_all(&bundled_root).expect("create bundled root");
    let _ = toolchain_resolver(&bundled_root);

    let empty_system_path = root.join("system-empty");
    fs::create_dir_all(&empty_system_path).expect("create empty system path");

    let mut config_text = bridgingio_engine::CoreSettings::complete_example().to_string();
    config_text = config_text.replace("__GLOBAL_ADB_OVERRIDE__", "");
    config_text = config_text.replace("__TARGET_ADB_OVERRIDE__", "");
    let config_path = root.join("matrix.toml");
    fs::write(&config_path, config_text).expect("write matrix config");

    let resolver = ToolchainResolver::new(
        ExecutableResolver::with_search_paths(vec![empty_system_path]),
        &bundled_root,
        vec![
            BuiltInBinarySpec {
                command: "ssh".into(),
                relative_path: PathBuf::from("ssh"),
                distribution: BuiltInDistributionKind::StandalonePackage,
            },
            BuiltInBinarySpec {
                command: "adb".into(),
                relative_path: PathBuf::from("adb"),
                distribution: BuiltInDistributionKind::StandalonePackage,
            },
        ],
    );
    let mut runtime =
        StandaloneCoreRuntime::from_config_file(&config_path, resolver).expect("runtime");
    let diagnostics = runtime.handle_app_request(ApiRequest {
        request_id: "diag-matrix".into(),
        context: ApiRequestContext {
            agent_id: "agent-matrix".into(),
            run_id: "run-matrix-1".into(),
            client_session_id: "client-matrix".into(),
            reuse_policy: SessionReusePolicy::ReuseIfAlive,
        },
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
    assert_eq!(adb_diag.effective_scope.as_deref(), Some("builtin_fallback"));
    assert_eq!(adb_diag.effective_source.as_deref(), Some("builtin_fallback"));

    let adapter = detect_host_platform_adapter("info");
    let runtime_root = root.join("runtime-root");
    fs::create_dir_all(&runtime_root).expect("create runtime root");
    let runtime_paths = adapter
        .runtime_paths()
        .runtime_paths("bridgingio-matrix", &runtime_root);
    let endpoint = adapter
        .control_plane_transport()
        .endpoint_semantics("bridgingio-matrix", &runtime_paths)
        .endpoint;
    assert!(!endpoint.is_empty(), "transport endpoint should not be empty");
    match adapter.host_platform() {
        HostPlatform::Windows => assert!(
            endpoint.starts_with(r"\\.\pipe\bridgingio-"),
            "windows endpoint should use named-pipe contract, got {endpoint}"
        ),
        HostPlatform::Unix | HostPlatform::Unknown => assert!(
            endpoint.ends_with("control-plane.sock"),
            "unix/unknown endpoint should end with control-plane.sock, got {endpoint}"
        ),
    }
}
