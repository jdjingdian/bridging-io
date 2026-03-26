use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use bridgingio_connectors::{
    BuiltInBinarySpec, BuiltInDistributionKind, ExecutableResolver, ToolchainResolver,
};
use bridgingio_engine::CoreSettings;
use bridgingio_mcp::{
    control_plane_socket_path, CoreHostMode, ModelPlaneHttpServer, StandaloneCoreRuntime,
};

#[cfg(unix)]
use bridgingio_mcp::ControlPlaneIpcServer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LaunchMode {
    UiManagedEphemeral,
    StandaloneRun,
    StandaloneDetachedLauncher,
    StandaloneDetachedChild,
}

struct CliArgs {
    config_path: PathBuf,
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
    match args.mode {
        LaunchMode::StandaloneDetachedLauncher => spawn_detached_child(&args.config_path),
        LaunchMode::UiManagedEphemeral => run_core(args, CoreHostMode::UiManagedEphemeral),
        LaunchMode::StandaloneRun => run_core(args, CoreHostMode::StandaloneRun),
        LaunchMode::StandaloneDetachedChild => run_core(args, CoreHostMode::StandaloneDetached),
    }
}

fn run_core(args: CliArgs, host_mode: CoreHostMode) -> Result<(), String> {
    let settings = CoreSettings::load_from_file(&args.config_path)
        .map_err(|err| format!("invalid config: {err:?}"))?;
    let toolchain_resolver = default_toolchain_resolver(&settings, &args.config_path);
    let runtime =
        StandaloneCoreRuntime::from_settings_with_mode(settings.clone(), toolchain_resolver, host_mode)
            .map_err(|err| format!("{err:?}"))?
            .shared();

    let mut handles = Vec::new();
    if mcp_trace_enabled() {
        println!("mcp trace enabled via BRIDGINGIO_MCP_TRACE");
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
                eprintln!("model-plane http stopped: {err:?}");
                break;
            }
        }));
    } else {
        println!("model-plane http disabled by config");
    }

    #[cfg(unix)]
    if settings.control_plane.enabled {
        let socket = control_plane_socket_path(&settings);
        let server = ControlPlaneIpcServer::bind(runtime.clone(), &socket)
            .map_err(|err| format!("bind control-plane ipc failed: {err:?}"))?;
        println!(
            "control-plane ipc listening on {}",
            socket.to_string_lossy()
        );
        handles.push(thread::spawn(move || loop {
            if let Err(err) = server.serve_once() {
                eprintln!("control-plane ipc stopped: {err:?}");
                break;
            }
        }));
    } else {
        println!("control-plane ipc disabled by config");
    }

    #[cfg(not(unix))]
    if settings.control_plane.enabled {
        println!(
            "control-plane ipc requested, but current platform build has no unix socket support"
        );
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
        args.mode, host_mode, startup_state
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
            break;
        }
    }
    Ok(())
}

fn spawn_detached_child(config_path: &Path) -> Result<(), String> {
    let exe = env::current_exe().map_err(|err| format!("resolve current_exe failed: {err}"))?;
    let child = Command::new(exe)
        .arg("run")
        .arg("--config")
        .arg(config_path)
        .arg("--detached-child")
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
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
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
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            _ => {
                return Err(format!("unknown argument: {arg}"));
            }
        }
    }

    let config_path =
        config_path.ok_or_else(|| "missing required argument: --config <path>".to_string())?;
    Ok(CliArgs { config_path, mode })
}

fn print_usage() {
    eprintln!("usage:");
    eprintln!("  bridgingio-core run --config <path-to-standalone.toml>");
    eprintln!("  bridgingio-core -d --config <path-to-standalone.toml>");
    eprintln!("  bridgingio-core ui-managed-ephemeral --config <path-to-standalone.toml>");
}

#[cfg(test)]
mod tests {
    use super::{parse_args_from, LaunchMode};

    fn parse(items: &[&str]) -> LaunchMode {
        let args = items.iter().map(|item| item.to_string()).collect::<Vec<_>>();
        parse_args_from(args).expect("parse").mode
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
            parse(&["ui-managed-ephemeral", "--config", "/tmp/standalone.toml"]),
            LaunchMode::UiManagedEphemeral
        );
    }
}
