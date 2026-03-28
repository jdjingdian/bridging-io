use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bridgingio_app_api::{ApiRequest, ApiRequestContext, ApiResponse, AppCommand};
use bridgingio_connectors::{
    BuiltInBinarySpec, BuiltInDistributionKind, ExecutableResolver, ToolchainResolver,
};
use bridgingio_domain::{PolicyProfile, SessionReusePolicy, TargetKind};
use bridgingio_engine::{
    CoreSettings, StandaloneConnectionSection, StandaloneTargetProfile, StandaloneTerminalSection,
    TerminalProviderSection,
};
use bridgingio_mcp::{
    control_plane_socket_path, CoreHostMode, CoreRuntimeError, ModelPlaneHttpServer,
    StandaloneCoreRuntime,
};
use bridgingio_platform::{
    detect_host_platform_adapter, CapabilityStatus, HostPlatform, RuntimeLogCategory,
    RuntimeLogLevel,
};
use bridgingio_providers::TerminalProvider;

#[cfg(unix)]
use bridgingio_mcp::ControlPlaneIpcServer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LaunchMode {
    SelfTest,
    UiManagedEphemeral,
    StandaloneRun,
    StandaloneDetachedLauncher,
    StandaloneDetachedChild,
}

#[derive(Debug)]
struct CliArgs {
    config_path: Option<PathBuf>,
    runtime_root: Option<PathBuf>,
    control_plane_socket_override: Option<PathBuf>,
    mode: LaunchMode,
}

fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            print_usage();
            std::process::exit(2);
        }
    };

    if let Err(err) = run(args) {
        eprintln!("failed to start bridgingio-core: {err}");
        std::process::exit(1);
    }
}

fn run(args: CliArgs) -> Result<(), String> {
    if args.mode == LaunchMode::SelfTest {
        return run_self_test();
    }
    let config_path = resolve_config_path(&args)?;
    match args.mode {
        LaunchMode::StandaloneDetachedLauncher => {
            spawn_detached_child(&config_path, args.control_plane_socket_override.as_deref())
        }
        LaunchMode::UiManagedEphemeral => run_core(
            config_path,
            args.mode,
            CoreHostMode::UiManagedEphemeral,
            args.control_plane_socket_override.as_deref(),
        ),
        LaunchMode::StandaloneRun => run_core(
            config_path,
            args.mode,
            CoreHostMode::StandaloneRun,
            args.control_plane_socket_override.as_deref(),
        ),
        LaunchMode::StandaloneDetachedChild => run_core(
            config_path,
            args.mode,
            CoreHostMode::StandaloneDetached,
            args.control_plane_socket_override.as_deref(),
        ),
        LaunchMode::SelfTest => unreachable!("self-test handled before config resolution"),
    }
}

fn run_self_test() -> Result<(), String> {
    println!("running bridgingio-core self-test...");

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| format!("clock error: {err}"))?
        .as_nanos();
    let self_test_root = env::temp_dir().join(format!("bridgingio-self-test-{stamp}"));
    fs::create_dir_all(&self_test_root)
        .map_err(|err| format!("create self-test root failed: {err}"))?;

    let marker_one_shot = "BRIDGINGIO_SELFTEST_ONE_SHOT_OK";
    let marker_interactive = "BRIDGINGIO_SELFTEST_INTERACTIVE_OK";
    let marker_env = "BRIDGINGIO_SELFTEST_ENV_OK";
    let marker_space = "BRIDGINGIO_SELFTEST_SPACE_OK";
    let marker_arg = "BRIDGINGIO SELFTEST ARG OK";
    let marker_runtime = "BRIDGINGIO_SELFTEST_RUNTIME_OK";
    let cwd_hint = "bridgingio selftest cwd";

    println!("self-test [1/6] validating terminal provider one-shot execution...");
    let mut provider = TerminalProvider::default();
    let one_shot_artifact = provider
        .exec_local(
            "self-test-session",
            Some("self-test-channel"),
            Some("self-test-transport"),
            &format!("echo {marker_one_shot}"),
            "self-test-artifact-one-shot",
            &PolicyProfile::default(),
        )
        .map_err(|err| format!("one-shot exec failed: {}", err.message))?;
    let one_shot_chunks = provider.artifacts.read_chunks(&one_shot_artifact.id, 0, 20);
    if !one_shot_chunks
        .iter()
        .any(|line| line.contains(marker_one_shot))
    {
        return Err(format!(
            "one-shot exec output missing marker {marker_one_shot}: {one_shot_chunks:?}"
        ));
    }
    println!("self-test [ok] terminal provider one-shot execution");

    println!("self-test [2/6] validating interactive shell open/write/read/interrupt/close...");
    let shell = provider
        .open_interactive_shell(
            "self-test-session",
            "self-test-channel-interactive",
            Some("self-test-transport-interactive"),
            "self-test",
        )
        .map_err(|err| format!("interactive open failed: {}", err.message))?;
    let write = provider
        .write_interactive_shell(
            &shell.shell_id,
            &format!("echo {marker_interactive}"),
            "self-test-artifact-interactive",
            &PolicyProfile::default(),
        )
        .map_err(|err| format!("interactive write failed: {}", err.message))?;
    if !write.output.contains(marker_interactive) {
        return Err(format!(
            "interactive output missing marker {marker_interactive}: {}",
            write.output
        ));
    }
    println!("self-test [ok] interactive shell lifecycle");

    println!("self-test [3/6] validating interactive cwd/env semantics...");
    let interactive_cwd = self_test_root.join(cwd_hint);
    fs::create_dir_all(&interactive_cwd)
        .map_err(|err| format!("create self-test cwd dir failed: {err}"))?;

    #[cfg(windows)]
    {
        let change_dir_command = format!("cd /d {}", shell_quote_path(&interactive_cwd));
        provider
            .write_interactive_shell(
                &shell.shell_id,
                &change_dir_command,
                "self-test-artifact-cwd-change",
                &PolicyProfile::default(),
            )
            .map_err(|err| format!("interactive cd /d failed: {}", err.message))?;

        let cwd_output = provider
            .write_interactive_shell(
                &shell.shell_id,
                "cd",
                "self-test-artifact-cwd-read",
                &PolicyProfile::default(),
            )
            .map_err(|err| format!("interactive cwd read failed: {}", err.message))?;
        if !cwd_output.output.to_ascii_lowercase().contains(cwd_hint) {
            return Err(format!(
                "interactive cwd output missing expected folder hint ({cwd_hint}): {}",
                cwd_output.output
            ));
        }

        provider
            .write_interactive_shell(
                &shell.shell_id,
                &format!("set BRIDGINGIO_SELFTEST_ENV={marker_env}"),
                "self-test-artifact-env-set",
                &PolicyProfile::default(),
            )
            .map_err(|err| format!("interactive env set failed: {}", err.message))?;
        let env_output = provider
            .write_interactive_shell(
                &shell.shell_id,
                "echo %BRIDGINGIO_SELFTEST_ENV%",
                "self-test-artifact-env-read",
                &PolicyProfile::default(),
            )
            .map_err(|err| format!("interactive env read failed: {}", err.message))?;
        if !env_output.output.contains(marker_env) {
            return Err(format!(
                "interactive env output missing marker {marker_env}: {}",
                env_output.output
            ));
        }
    }

    #[cfg(not(windows))]
    {
        let change_dir_command = format!("cd {}", shell_quote_path(&interactive_cwd));
        provider
            .write_interactive_shell(
                &shell.shell_id,
                &change_dir_command,
                "self-test-artifact-cwd-change",
                &PolicyProfile::default(),
            )
            .map_err(|err| format!("interactive cd failed: {}", err.message))?;

        let cwd_output = provider
            .write_interactive_shell(
                &shell.shell_id,
                "pwd",
                "self-test-artifact-cwd-read",
                &PolicyProfile::default(),
            )
            .map_err(|err| format!("interactive cwd read failed: {}", err.message))?;
        if !cwd_output.output.contains(cwd_hint) {
            return Err(format!(
                "interactive cwd output missing expected folder hint ({cwd_hint}): {}",
                cwd_output.output
            ));
        }

        provider
            .write_interactive_shell(
                &shell.shell_id,
                &format!("export BRIDGINGIO_SELFTEST_ENV={marker_env}"),
                "self-test-artifact-env-set",
                &PolicyProfile::default(),
            )
            .map_err(|err| format!("interactive env set failed: {}", err.message))?;
        let env_output = provider
            .write_interactive_shell(
                &shell.shell_id,
                "printf \"%s\\n\" \"$BRIDGINGIO_SELFTEST_ENV\"",
                "self-test-artifact-env-read",
                &PolicyProfile::default(),
            )
            .map_err(|err| format!("interactive env read failed: {}", err.message))?;
        if !env_output.output.contains(marker_env) {
            return Err(format!(
                "interactive env output missing marker {marker_env}: {}",
                env_output.output
            ));
        }
    }
    println!("self-test [ok] interactive cwd/env semantics");

    println!("self-test [4/6] validating space-containing path/argument handling...");
    let spaced_file = interactive_cwd.join("file with space.txt");
    fs::write(&spaced_file, format!("{marker_space}\n"))
        .map_err(|err| format!("write self-test spaced file failed: {err}"))?;

    #[cfg(windows)]
    {
        let read_command = format!("type {}", shell_quote_path(&spaced_file));
        let read_output = provider
            .write_interactive_shell(
                &shell.shell_id,
                &read_command,
                "self-test-artifact-space-read",
                &PolicyProfile::default(),
            )
            .map_err(|err| format!("interactive spaced file read failed: {}", err.message))?;
        if !read_output.output.contains(marker_space) {
            return Err(format!(
                "interactive spaced-path output missing marker {marker_space}: {}",
                read_output.output
            ));
        }

        let arg_output = provider
            .write_interactive_shell(
                &shell.shell_id,
                &format!("echo \"{marker_arg}\""),
                "self-test-artifact-space-arg",
                &PolicyProfile::default(),
            )
            .map_err(|err| format!("interactive spaced-arg echo failed: {}", err.message))?;
        if !arg_output.output.contains(marker_arg) {
            return Err(format!(
                "interactive spaced-arg output missing marker {marker_arg}: {}",
                arg_output.output
            ));
        }
    }

    #[cfg(not(windows))]
    {
        let read_command = format!("cat {}", shell_quote_path(&spaced_file));
        let read_output = provider
            .write_interactive_shell(
                &shell.shell_id,
                &read_command,
                "self-test-artifact-space-read",
                &PolicyProfile::default(),
            )
            .map_err(|err| format!("interactive spaced file read failed: {}", err.message))?;
        if !read_output.output.contains(marker_space) {
            return Err(format!(
                "interactive spaced-path output missing marker {marker_space}: {}",
                read_output.output
            ));
        }

        let arg_output = provider
            .write_interactive_shell(
                &shell.shell_id,
                &format!("printf \"%s\\n\" \"{marker_arg}\""),
                "self-test-artifact-space-arg",
                &PolicyProfile::default(),
            )
            .map_err(|err| format!("interactive spaced-arg echo failed: {}", err.message))?;
        if !arg_output.output.contains(marker_arg) {
            return Err(format!(
                "interactive spaced-arg output missing marker {marker_arg}: {}",
                arg_output.output
            ));
        }
    }
    println!("self-test [ok] space-containing path/argument handling");

    provider
        .interrupt_interactive_shell(&shell.shell_id)
        .map_err(|err| format!("interactive interrupt failed: {}", err.message))?;
    provider
        .close_interactive_shell(&shell.shell_id)
        .map_err(|err| format!("interactive close failed: {}", err.message))?;
    let transcript = provider
        .read_interactive_transcript(&shell.shell_id, 0, 50)
        .map_err(|err| format!("interactive transcript read failed: {}", err.message))?;
    if !transcript
        .iter()
        .any(|line| line.contains(marker_interactive))
    {
        return Err(format!(
            "interactive transcript missing marker {marker_interactive}: {transcript:?}"
        ));
    }
    println!("self-test [ok] interactive transcript/checkpoint");

    println!("self-test [5/6] validating standalone runtime execute path without config file...");
    let runtime_root = self_test_root.join("runtime");
    let state_dir = runtime_root.join("state");
    let artifacts_dir = runtime_root.join("artifacts");
    fs::create_dir_all(&state_dir)
        .map_err(|err| format!("create self-test state dir failed: {err}"))?;
    fs::create_dir_all(&artifacts_dir)
        .map_err(|err| format!("create self-test artifacts dir failed: {err}"))?;

    let mut settings = CoreSettings::from_toml_str(CoreSettings::minimal_example())
        .map_err(|err| format!("parse minimal settings failed: {err:?}"))?;
    settings.core.instance_name = "bridgingio-self-test".into();
    settings.core.data_dir = runtime_root.to_string_lossy().to_string();
    settings.storage.metadata_path = state_dir
        .join("metadata.sqlite3")
        .to_string_lossy()
        .to_string();
    settings.storage.artifacts.backend = "memory".into();
    settings.storage.artifacts.root = artifacts_dir.to_string_lossy().to_string();
    settings.model_plane.http.enabled = false;
    settings.control_plane.enabled = false;
    settings.targets = vec![StandaloneTargetProfile {
        id: "self-test-local".into(),
        display_name: "Self-Test Local".into(),
        kind: TargetKind::Other("self-test".into()),
        enabled: true,
        aliases: vec!["local".into()],
        credential_ref: None,
        notes: Some("self-test synthetic local target".into()),
        connection: StandaloneConnectionSection::default(),
        terminal: StandaloneTerminalSection::default(),
        toolchains: HashMap::new(),
        terminal_provider: TerminalProviderSection {
            enabled: true,
            shell: None,
        },
        git_repositories: Vec::new(),
    }];

    let config_hint = runtime_root.join("self-test.toml");
    let resolver = default_toolchain_resolver(&settings, &config_hint);
    let mut runtime = StandaloneCoreRuntime::from_settings_with_mode(
        settings,
        resolver,
        CoreHostMode::StandaloneRun,
    )
    .map_err(|err| format!("create runtime for self-test failed: {err:?}"))?;

    let context = ApiRequestContext {
        agent_id: "self-test-agent".into(),
        run_id: "self-test-run".into(),
        client_session_id: "self-test-client".into(),
        reuse_policy: SessionReusePolicy::ReuseIfAlive,
    };

    let targets = runtime.handle_app_request(ApiRequest {
        request_id: "self-test-list-targets".into(),
        context: context.clone(),
        command: AppCommand::ListTargets,
    });
    let target_id = match targets {
        ApiResponse::Targets { items, .. } => items
            .into_iter()
            .find(|item| item.id == "self-test-local")
            .map(|item| item.id)
            .ok_or_else(|| "self-test target not found in runtime".to_string())?,
        other => return Err(format!("unexpected response for list targets: {other:?}")),
    };

    let open_session = runtime.handle_app_request(ApiRequest {
        request_id: "self-test-open-session".into(),
        context: context.clone(),
        command: AppCommand::OpenSession { target_id },
    });
    let session_id = match open_session {
        ApiResponse::Session { session, .. } => session.id,
        ApiResponse::Error { error, .. } => {
            return Err(format!("open session failed: {}", error.message))
        }
        other => return Err(format!("unexpected response for open session: {other:?}")),
    };

    let execution = runtime.handle_app_request(ApiRequest {
        request_id: "self-test-exec".into(),
        context: context.clone(),
        command: AppCommand::Execute {
            session_id,
            command: format!("echo {marker_runtime}"),
            stream: false,
        },
    });
    let artifact_id = match execution {
        ApiResponse::Execution { artifact, .. } => artifact.id,
        ApiResponse::Error { error, .. } => {
            return Err(format!("execute failed: {}", error.message))
        }
        other => return Err(format!("unexpected response for execute: {other:?}")),
    };

    let artifact = runtime.handle_app_request(ApiRequest {
        request_id: "self-test-read-artifact".into(),
        context,
        command: AppCommand::ReadArtifact {
            artifact_id,
            offset: 0,
            limit: 50,
        },
    });
    match artifact {
        ApiResponse::Artifact { artifact, .. } => {
            if !artifact
                .chunks
                .iter()
                .any(|line| line.contains(marker_runtime))
            {
                return Err(format!(
                    "runtime execute output missing marker {marker_runtime}: {:?}",
                    artifact.chunks
                ));
            }
        }
        ApiResponse::Error { error, .. } => {
            return Err(format!("read artifact failed: {}", error.message))
        }
        other => return Err(format!("unexpected response for read artifact: {other:?}")),
    }

    println!("self-test [ok] standalone runtime execute path");

    println!("self-test [6/6] validating host platform contract snapshot...");
    let host_platform_adapter = detect_host_platform_adapter("info");
    let snapshot = host_platform_adapter.snapshot();
    if snapshot.host_platform == HostPlatform::Unknown {
        return Err("host platform is unknown in self-test contract snapshot".to_string());
    }
    if snapshot.control_plane_transport_status == CapabilityStatus::Unsupported
        && snapshot.host_platform != HostPlatform::Unknown
    {
        return Err(format!(
            "unexpected unsupported control-plane transport status on host platform {:?}",
            snapshot.host_platform
        ));
    }
    let contract_paths = host_platform_adapter
        .runtime_paths()
        .runtime_paths("bridgingio-self-test", &runtime_root);
    match snapshot.host_platform {
        HostPlatform::Windows => {
            if !contract_paths
                .control_plane_endpoint
                .starts_with(r"\\.\pipe\bridgingio-")
            {
                return Err(format!(
                    "windows control-plane endpoint contract mismatch: {}",
                    contract_paths.control_plane_endpoint
                ));
            }
        }
        HostPlatform::Unix => {
            if !contract_paths
                .control_plane_endpoint
                .ends_with("control-plane.sock")
            {
                return Err(format!(
                    "unix control-plane endpoint contract mismatch: {}",
                    contract_paths.control_plane_endpoint
                ));
            }
        }
        HostPlatform::Unknown => {}
    }
    if host_platform_adapter.native_vault_binding().backend_label() != "os-native" {
        return Err("native vault backend label must remain os-native".to_string());
    }
    if !host_platform_adapter
        .toolchain_locator()
        .resolution_order()
        .contains(&"builtin_fallback")
    {
        return Err("toolchain resolution order missing builtin_fallback".to_string());
    }
    host_platform_adapter.runtime_logger().log(
        RuntimeLogLevel::Info,
        RuntimeLogCategory::Decode,
        "self-test validating platform output decoder",
    );
    let decoded = host_platform_adapter
        .output_decoder()
        .decode(b"line-1\r\nline-2\r");
    if decoded.text != "line-1\nline-2\n" || !decoded.normalized_newlines {
        return Err(format!(
            "platform output decoder contract mismatch: {:?}",
            decoded
        ));
    }
    println!("self-test [ok] host platform contract snapshot");

    let _ = fs::remove_dir_all(&self_test_root);
    println!("self-test passed");
    Ok(())
}

#[cfg(windows)]
fn shell_quote_path(path: &Path) -> String {
    format!("\"{}\"", path.to_string_lossy().replace('"', "\"\""))
}

#[cfg(not(windows))]
fn shell_quote_path(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
}

fn run_core(
    config_path: PathBuf,
    mode: LaunchMode,
    host_mode: CoreHostMode,
    control_plane_socket_override: Option<&Path>,
) -> Result<(), String> {
    let mut settings = CoreSettings::load_from_file(&config_path)
        .map_err(|err| format!("invalid config: {err:?}"))?;
    let host_platform_adapter = detect_host_platform_adapter(&settings.core.log_level);
    host_platform_adapter.runtime_logger().log(
        RuntimeLogLevel::Info,
        RuntimeLogCategory::Startup,
        &format!(
            "starting bridgingio-core (mode={mode:?}, host_mode={host_mode:?}, config={})",
            config_path.to_string_lossy()
        ),
    );
    apply_control_plane_socket_override(&mut settings, control_plane_socket_override)?;
    let toolchain_resolver = default_toolchain_resolver(&settings, &config_path);
    let runtime = StandaloneCoreRuntime::from_settings_with_mode(
        settings.clone(),
        toolchain_resolver,
        host_mode,
    )
    .map_err(|err| format!("{err:?}"))?
    .shared();
    {
        let mut locked = runtime
            .lock()
            .map_err(|_| "runtime lock poisoned while setting config path".to_string())?;
        locked.settings_store.runtime_metadata.config_path =
            Some(config_path.to_string_lossy().to_string());
        settings = locked.settings_store.settings.clone();
    }

    let runtime_paths = host_platform_adapter.runtime_paths().runtime_paths(
        &settings.core.instance_name,
        Path::new(&settings.core.data_dir),
    );
    let transport = host_platform_adapter.control_plane_transport();
    let transport_lifecycle = transport.lifecycle_semantics();
    host_platform_adapter.runtime_logger().log(
        RuntimeLogLevel::Info,
        RuntimeLogCategory::Transport,
        &format!(
            "control-plane transport selected: kind={}, attach='{}', request_response='{}', lifecycle='{}'",
            transport.transport_kind(),
            transport_lifecycle.attach,
            transport_lifecycle.request_response,
            transport_lifecycle.lifecycle
        ),
    );
    for diagnostic in transport.diagnostics(&settings.core.instance_name, &runtime_paths) {
        let level = if diagnostic.status == bridgingio_platform::CapabilityStatus::Ready {
            RuntimeLogLevel::Info
        } else {
            RuntimeLogLevel::Warn
        };
        host_platform_adapter.runtime_logger().log(
            level,
            RuntimeLogCategory::Transport,
            &format!(
                "[{}] {} (hint: {})",
                diagnostic.code, diagnostic.message, diagnostic.recovery_hint
            ),
        );
    }

    let mut handles = Vec::new();
    if mcp_trace_enabled() {
        println!("mcp trace enabled via BRIDGINGIO_MCP_TRACE");
        host_platform_adapter.runtime_logger().log(
            RuntimeLogLevel::Info,
            RuntimeLogCategory::Mcp,
            "mcp trace enabled via BRIDGINGIO_MCP_TRACE",
        );
    }

    if settings.model_plane.http.enabled {
        let server = ModelPlaneHttpServer::bind(runtime.clone(), &settings)
            .map_err(|err| format!("bind model-plane http failed: {err:?}"))?;
        let http_addr = server
            .local_addr()
            .map_err(|err| format!("read model-plane address failed: {err:?}"))?;
        println!(
            "model-plane http listening on {}:{}",
            http_addr.ip(),
            http_addr.port()
        );
        handles.push(thread::spawn(move || loop {
            if let Err(err) = server.serve_once() {
                detect_host_platform_adapter("info").runtime_logger().log(
                    RuntimeLogLevel::Warn,
                    RuntimeLogCategory::Transport,
                    &format!("model-plane http stopped: {err:?}"),
                );
                break;
            }
        }));
    } else {
        println!("model-plane http disabled by config");
        host_platform_adapter.runtime_logger().log(
            RuntimeLogLevel::Info,
            RuntimeLogCategory::Transport,
            "model-plane http disabled by config",
        );
    }

    #[cfg(unix)]
    if settings.control_plane.enabled {
        let socket = control_plane_socket_path(&settings);
        let server = ControlPlaneIpcServer::bind(runtime.clone(), &socket)
            .map_err(|err| map_control_plane_bind_error(err, &socket))?;
        println!(
            "control-plane ipc listening on {}",
            socket.to_string_lossy()
        );
        handles.push(thread::spawn(move || loop {
            if let Err(err) = server.serve_once() {
                detect_host_platform_adapter("info").runtime_logger().log(
                    RuntimeLogLevel::Warn,
                    RuntimeLogCategory::Transport,
                    &format!("control-plane ipc stopped: {err:?}"),
                );
                break;
            }
        }));
    } else {
        println!("control-plane ipc disabled by config");
        host_platform_adapter.runtime_logger().log(
            RuntimeLogLevel::Info,
            RuntimeLogCategory::Transport,
            "control-plane ipc disabled by config",
        );
    }

    #[cfg(not(unix))]
    if settings.control_plane.enabled {
        let endpoint_semantics =
            transport.endpoint_semantics(&settings.core.instance_name, &runtime_paths);
        let message = format!(
            "control-plane transport is not wired on this build (kind={}, endpoint={}, naming_rule={})",
            transport.transport_kind(),
            endpoint_semantics.endpoint,
            endpoint_semantics.naming_rule
        );
        println!("{message}");
        host_platform_adapter.runtime_logger().log(
            RuntimeLogLevel::Warn,
            RuntimeLogCategory::Transport,
            &message,
        );
        if host_mode == CoreHostMode::UiManagedEphemeral {
            return Err(
                "ui-managed-ephemeral requires local control-plane attach, but the current host transport is deferred. Recovery: run on unix host for now or switch to standalone run mode.".to_string(),
            );
        }
    }

    if handles.is_empty() {
        return Err("no endpoint enabled (both control-plane and model-plane are disabled)".into());
    }

    let startup_state = {
        let runtime = runtime
            .lock()
            .map_err(|_| "runtime lock poisoned at startup".to_string())?;
        runtime.readiness_state_label().to_string()
    };
    println!(
        "bridgingio-core started in mode={:?} (host_mode={:?}, readiness={})",
        mode, host_mode, startup_state
    );
    host_platform_adapter.runtime_logger().log(
        RuntimeLogLevel::Info,
        RuntimeLogCategory::Startup,
        &format!(
            "bridgingio-core started (mode={mode:?}, host_mode={host_mode:?}, readiness={startup_state})"
        ),
    );
    if host_mode == CoreHostMode::UiManagedEphemeral {
        println!("waiting for ui attach before model-plane becomes ready");
    } else {
        println!("model-plane is ready immediately in standalone mode");
    }

    loop {
        thread::sleep(Duration::from_millis(300));
        let shutdown_requested = {
            let runtime = runtime
                .lock()
                .map_err(|_| "runtime lock poisoned while polling shutdown".to_string())?;
            runtime.shutdown_requested()
        };
        if shutdown_requested {
            println!("shutdown requested from control-plane, exiting core process");
            host_platform_adapter.runtime_logger().log(
                RuntimeLogLevel::Info,
                RuntimeLogCategory::Startup,
                "shutdown requested from control-plane; exiting core process",
            );
            break;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn map_control_plane_bind_error(err: CoreRuntimeError, endpoint: &Path) -> String {
    let detail = format!("{err:?}");
    if detail.contains("Permission denied") || detail.contains("Operation not permitted") {
        return format!(
            "bind control-plane ipc failed at {}: {detail}. Recovery: choose a writable runtime root or use --control-plane-socket-override to point to a writable location.",
            endpoint.display()
        );
    }
    if detail.contains("Address already in use") {
        return format!(
            "bind control-plane ipc failed at {}: {detail}. Recovery: stop the existing core process or use --control-plane-socket-override with a different endpoint.",
            endpoint.display()
        );
    }
    format!(
        "bind control-plane ipc failed at {}: {detail}",
        endpoint.display()
    )
}

fn spawn_detached_child(
    config_path: &Path,
    control_plane_socket_override: Option<&Path>,
) -> Result<(), String> {
    let exe = env::current_exe().map_err(|err| format!("resolve current_exe failed: {err}"))?;
    let mut command = Command::new(exe);
    command
        .arg("run")
        .arg("--config")
        .arg(config_path)
        .arg("--detached-child");
    if let Some(path) = control_plane_socket_override {
        command.arg("--control-plane-socket-override").arg(path);
    }
    let child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| format!("spawn detached child failed: {err}"))?;
    println!(
        "bridgingio-core detached in background (pid={})",
        child.id()
    );
    Ok(())
}

fn mcp_trace_enabled() -> bool {
    match env::var("BRIDGINGIO_MCP_TRACE") {
        Ok(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        }
        Err(_) => false,
    }
}

fn apply_control_plane_socket_override(
    settings: &mut CoreSettings,
    control_plane_socket_override: Option<&Path>,
) -> Result<(), String> {
    let Some(override_path) = control_plane_socket_override else {
        return Ok(());
    };
    if settings.control_plane.transport != "platform-ipc" {
        return Ok(());
    }
    let normalized = normalize_control_plane_socket_override(override_path)?;
    if let Some(parent) = normalized.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "create control-plane socket override dir failed: {} ({err})",
                parent.display()
            )
        })?;
    }
    settings.control_plane.endpoint = normalized.to_string_lossy().to_string();
    Ok(())
}

fn normalize_control_plane_socket_override(path: &Path) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() {
        return Err("--control-plane-socket-override requires a non-empty path".to_string());
    }
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        let cwd = env::current_dir().map_err(|err| format!("resolve current dir failed: {err}"))?;
        Ok(cwd.join(path))
    }
}

fn default_toolchain_resolver(settings: &CoreSettings, config_path: &Path) -> ToolchainResolver {
    let config_dir = config_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let bundled_root = config_dir.join("bin");
    let specs = vec![
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
    ];
    let _ = settings;
    ToolchainResolver::new(ExecutableResolver::from_system_path(), bundled_root, specs)
}

fn parse_args() -> Result<CliArgs, String> {
    parse_args_from(env::args().skip(1))
}

fn parse_args_from<I>(args: I) -> Result<CliArgs, String>
where
    I: IntoIterator<Item = String>,
{
    let mut mode = LaunchMode::StandaloneRun;
    let mut config_path = None::<PathBuf>;
    let mut runtime_root = None::<PathBuf>;
    let mut control_plane_socket_override = None::<PathBuf>;
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--self-test" => mode = LaunchMode::SelfTest,
            "run" => mode = LaunchMode::StandaloneRun,
            "ui-managed-ephemeral" => mode = LaunchMode::UiManagedEphemeral,
            "-d" => mode = LaunchMode::StandaloneDetachedLauncher,
            "--detached-child" => mode = LaunchMode::StandaloneDetachedChild,
            "--config" => {
                let path = iter
                    .next()
                    .ok_or_else(|| "--config requires a path".to_string())?;
                config_path = Some(PathBuf::from(path));
            }
            "--runtime-root" => {
                let path = iter
                    .next()
                    .ok_or_else(|| "--runtime-root requires a path".to_string())?;
                runtime_root = Some(PathBuf::from(path));
            }
            "--control-plane-socket-override" => {
                let path = iter
                    .next()
                    .ok_or_else(|| "--control-plane-socket-override requires a path".to_string())?;
                control_plane_socket_override = Some(PathBuf::from(path));
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            _ => {
                return Err(format!("unknown argument: {arg}"));
            }
        }
    }

    match mode {
        LaunchMode::SelfTest => {
            if config_path.is_some()
                || runtime_root.is_some()
                || control_plane_socket_override.is_some()
            {
                return Err(
                    "--self-test does not accept --config/--runtime-root/--control-plane-socket-override"
                        .to_string(),
                );
            }
        }
        LaunchMode::UiManagedEphemeral => {
            if runtime_root.is_none() && config_path.is_none() {
                return Err(
                    "ui-managed-ephemeral requires --runtime-root <dir> or --config <path>"
                        .to_string(),
                );
            }
            if runtime_root.is_some() && config_path.is_some() {
                return Err(
                    "ui-managed-ephemeral accepts either --runtime-root or --config, not both"
                        .to_string(),
                );
            }
        }
        _ => {
            if config_path.is_none() {
                return Err("missing required argument: --config <path>".to_string());
            }
            if runtime_root.is_some() {
                return Err("--runtime-root is only supported in ui-managed-ephemeral mode".into());
            }
        }
    }
    Ok(CliArgs {
        config_path,
        runtime_root,
        control_plane_socket_override,
        mode,
    })
}

fn print_usage() {
    eprintln!("usage:");
    eprintln!("  bridgingio-core --self-test");
    eprintln!("  bridgingio-core run --config <path-to-standalone.toml>");
    eprintln!("  bridgingio-core -d --config <path-to-standalone.toml>");
    eprintln!("  bridgingio-core ui-managed-ephemeral --runtime-root <runtime-root-dir>");
    eprintln!("  bridgingio-core ui-managed-ephemeral --config <path-to-managed-core.toml>");
    eprintln!("  optional for all modes: --control-plane-socket-override <path-or-endpoint>");
}

fn resolve_config_path(args: &CliArgs) -> Result<PathBuf, String> {
    if let Some(path) = args.config_path.clone() {
        return Ok(path);
    }
    if args.mode == LaunchMode::UiManagedEphemeral {
        if let Some(runtime_root) = args.runtime_root.as_ref() {
            return ensure_runtime_root_layout(runtime_root);
        }
    }
    Err("missing config path".to_string())
}

fn ensure_runtime_root_layout(runtime_root: &Path) -> Result<PathBuf, String> {
    ensure_runtime_dir(runtime_root, "runtime root")?;
    let host_platform_adapter = detect_host_platform_adapter("info");
    let runtime_paths = host_platform_adapter
        .runtime_paths()
        .runtime_paths("bridgingio-ui-managed", runtime_root);
    let config_dir = runtime_root.join("config");
    let state_dir = runtime_paths.state_dir.clone();
    let artifacts_dir = runtime_paths.artifact_root.clone();
    let logs_dir = runtime_paths.logs_dir.clone();
    ensure_runtime_dir(&config_dir, "config dir")?;
    ensure_runtime_dir(&state_dir, "state dir")?;
    ensure_runtime_dir(&artifacts_dir, "artifacts dir")?;
    ensure_runtime_dir(&logs_dir, "logs dir")?;

    if let Some(parent) = runtime_paths.metadata_path.parent() {
        ensure_runtime_dir(parent, "metadata parent dir")?;
        ensure_writable_probe(parent, "metadata parent dir")?;
    } else {
        return Err(format!(
            "metadata path has no parent: {}. Recovery: choose a different runtime root.",
            runtime_paths.metadata_path.display()
        ));
    }
    ensure_writable_probe(runtime_root, "runtime root")?;
    ensure_writable_probe(&state_dir, "state dir")?;
    ensure_writable_probe(&artifacts_dir, "artifacts dir")?;
    ensure_writable_probe(&logs_dir, "logs dir")?;

    let config_path = config_dir.join("managed-core.toml");
    if !config_path.exists() {
        fs::write(
            &config_path,
            default_managed_core_config(runtime_root, &runtime_paths),
        )
        .map_err(|err| {
            format!(
                "create managed config failed: {} ({err})",
                config_path.display()
            )
        })?;
    }
    Ok(config_path)
}

fn ensure_runtime_dir(path: &Path, label: &str) -> Result<(), String> {
    if path.exists() && !path.is_dir() {
        return Err(format!(
            "{label} is not a directory: {}. Recovery: remove/rename this path or choose another runtime root.",
            path.display()
        ));
    }
    fs::create_dir_all(path).map_err(|err| {
        format!(
            "create {label} failed: {} ({err}). Recovery: choose a writable runtime root and retry.",
            path.display()
        )
    })
}

fn ensure_writable_probe(path: &Path, label: &str) -> Result<(), String> {
    let probe = path.join(".bridgingio-write-probe");
    fs::write(&probe, b"ok").map_err(|err| {
        format!(
            "{label} is not writable: {} ({err}). Recovery: grant write permission or choose another runtime root.",
            path.display()
        )
    })?;
    let _ = fs::remove_file(probe);
    Ok(())
}

fn default_managed_core_config(
    runtime_root: &Path,
    runtime_paths: &bridgingio_platform::RuntimePaths,
) -> String {
    let data_dir = toml_escape_path(runtime_root);
    let metadata_path = toml_escape_path(&runtime_paths.metadata_path);
    let artifacts_path = toml_escape_path(&runtime_paths.artifact_root);
    let endpoint = toml_escape_string(&runtime_paths.control_plane_endpoint);
    format!(
        r#"schema_version = 1

[core]
instance_name = "bridgingio-ui-managed"
data_dir = "{data_dir}"
log_level = "info"

[storage]
metadata_backend = "sqlite"
metadata_path = "{metadata_path}"

[storage.artifacts]
backend = "filesystem"
root = "{artifacts_path}"
max_bytes = 268435456
eviction_policy = "lru"

[vault]
backend = "os-native"
namespace = "io.bridgingio"

[control_plane]
enabled = true
transport = "platform-ipc"
endpoint = "{endpoint}"

[model_plane.http]
enabled = true
host = "127.0.0.1"
port = 19718
allow_non_loopback = false

[model_plane.http.auth]
mode = "none"
required_when_non_loopback = true

[policies.defaults]
reuse_policy = "resume_or_create"
approval_mode = "on-risk"
capture_env_fingerprint = true
"#
    )
}

fn toml_escape_path(path: &Path) -> String {
    toml_escape_string(&path.to_string_lossy())
}

fn toml_escape_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use bridgingio_engine::CoreSettings;

    use super::{parse_args_from, CliArgs, LaunchMode};

    fn parse(items: &[&str]) -> LaunchMode {
        let args = items
            .iter()
            .map(|item| item.to_string())
            .collect::<Vec<_>>();
        parse_args_from(args).expect("parse").mode
    }

    fn parse_cli(items: &[&str]) -> CliArgs {
        let args = items
            .iter()
            .map(|item| item.to_string())
            .collect::<Vec<_>>();
        parse_args_from(args).expect("parse")
    }

    #[test]
    fn parses_default_as_standalone_run() {
        assert_eq!(
            parse(&["--config", "/tmp/standalone.toml"]),
            LaunchMode::StandaloneRun
        );
    }

    #[test]
    fn parses_detached_mode() {
        assert_eq!(
            parse(&["-d", "--config", "/tmp/standalone.toml"]),
            LaunchMode::StandaloneDetachedLauncher
        );
    }

    #[test]
    fn parses_ui_managed_mode() {
        assert_eq!(
            parse(&["ui-managed-ephemeral", "--runtime-root", "/tmp/runtime"]),
            LaunchMode::UiManagedEphemeral
        );
    }

    #[test]
    fn parses_self_test_mode() {
        assert_eq!(parse(&["--self-test"]), LaunchMode::SelfTest);
    }

    #[test]
    fn rejects_self_test_with_config() {
        let args = ["--self-test", "--config", "/tmp/standalone.toml"]
            .iter()
            .map(|item| item.to_string())
            .collect::<Vec<_>>();
        let err = parse_args_from(args).expect_err("must reject mixed self-test arguments");
        assert!(err.contains("--self-test does not accept"));
    }

    #[test]
    fn parses_control_plane_socket_override_for_ui_mode() {
        let parsed = parse_cli(&[
            "ui-managed-ephemeral",
            "--runtime-root",
            "/tmp/runtime",
            "--control-plane-socket-override",
            "/tmp/bridgingio-ui.sock",
        ]);
        assert_eq!(
            parsed.control_plane_socket_override,
            Some(PathBuf::from("/tmp/bridgingio-ui.sock"))
        );
    }

    #[test]
    fn parses_control_plane_socket_override_for_standalone_mode() {
        let parsed = parse_cli(&[
            "run",
            "--config",
            "/tmp/standalone.toml",
            "--control-plane-socket-override",
            "relative.sock",
        ]);
        assert_eq!(
            parsed.control_plane_socket_override,
            Some(PathBuf::from("relative.sock"))
        );
    }

    #[test]
    fn creates_runtime_root_layout_and_default_config() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = PathBuf::from("/tmp").join(format!("bridgingio-runtime-{stamp}"));
        fs::create_dir_all(&root).expect("create runtime root");
        let config_path = super::ensure_runtime_root_layout(&root).expect("layout");
        assert!(root.join("config").is_dir());
        assert!(root.join("state").is_dir());
        assert!(root.join("artifacts").is_dir());
        assert!(root.join("logs").is_dir());
        assert!(config_path.exists());

        let settings = CoreSettings::load_from_file(&config_path).expect("parse config");
        assert_eq!(settings.model_plane.http.host, "127.0.0.1");
        assert_eq!(settings.model_plane.http.port, 19718);
        assert_eq!(settings.core.data_dir, root.to_string_lossy().to_string());
    }
}
