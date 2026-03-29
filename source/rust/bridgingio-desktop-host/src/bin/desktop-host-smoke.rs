use std::path::PathBuf;

use bridgingio_desktop_host::smoke::{run_desktop_host_smoke, SmokeStatus};

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let runtime_root = if let Some(value) = args.next() {
        PathBuf::from(value)
    } else {
        std::env::temp_dir().join("bridgingio-desktop-smoke-runtime")
    };
    let core_sidecar = if let Some(value) = args.next() {
        PathBuf::from(value)
    } else if let Ok(value) = std::env::var("BRIDGINGIO_CORE_SIDECAR") {
        PathBuf::from(value)
    } else {
        PathBuf::from("bridgingio-core")
    };

    std::fs::create_dir_all(&runtime_root).map_err(|err| {
        format!(
            "create runtime root for smoke failed ({}): {err}",
            runtime_root.display()
        )
    })?;

    let report = run_desktop_host_smoke(&runtime_root, &core_sidecar)?;
    println!("desktop_host_smoke.status={}", report.status.as_str());
    println!(
        "desktop_host_smoke.transport_kind={}",
        report.transport_kind
    );
    println!("desktop_host_smoke.startup_route={}", report.startup_route);
    println!(
        "desktop_host_smoke.sidecar_command={}",
        report.sidecar_commandline.join(" ")
    );
    for diagnostic in report.diagnostics {
        println!("desktop_host_smoke.diagnostic={diagnostic}");
    }

    if report.status == SmokeStatus::Deferred {
        return Err(
            "local control-plane transport is deferred/unsupported on this host; see diagnostics above"
                .to_string(),
        );
    }
    Ok(())
}
