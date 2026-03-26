use std::env;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use bridgingio_connectors::{
    BuiltInBinarySpec, BuiltInDistributionKind, ExecutableResolver, ToolchainResolver,
};
use bridgingio_engine::CoreSettings;
use bridgingio_mcp::{control_plane_socket_path, ModelPlaneHttpServer, StandaloneCoreRuntime};

#[cfg(unix)]
use bridgingio_mcp::ControlPlaneIpcServer;

struct CliArgs {
    config_path: PathBuf,
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
    let settings = CoreSettings::load_from_file(&args.config_path)
        .map_err(|err| format!("invalid config: {err:?}"))?;
    let toolchain_resolver = default_toolchain_resolver(&settings, &args.config_path);
    let runtime = StandaloneCoreRuntime::from_settings(settings.clone(), toolchain_resolver)
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

    println!("bridgingio-core started. press Ctrl+C to stop.");
    loop {
        thread::sleep(Duration::from_secs(60));
    }
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
    let mut args = env::args().skip(1);
    let mut config_path = None::<PathBuf>;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => {
                let path = args
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
    Ok(CliArgs { config_path })
}

fn print_usage() {
    eprintln!("usage: bridgingio-core --config <path-to-standalone.toml>");
}
