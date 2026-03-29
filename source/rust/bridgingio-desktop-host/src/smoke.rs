use std::path::Path;

use bridgingio_platform::{detect_host_platform_adapter, CapabilityStatus};

use crate::bundle::TauriShellHostSpec;
use crate::startup::{resolve_startup_route, StartupRoute};
use crate::storage::RuntimeRootPreferenceStore;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SmokeStatus {
    Ready,
    Deferred,
}

impl SmokeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SmokeStatus::Ready => "ready",
            SmokeStatus::Deferred => "deferred",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmokeReport {
    pub status: SmokeStatus,
    pub transport_kind: String,
    pub startup_route: String,
    pub sidecar_commandline: Vec<String>,
    pub diagnostics: Vec<String>,
}

pub fn run_desktop_host_smoke(
    runtime_root: &Path,
    core_sidecar: &Path,
) -> Result<SmokeReport, String> {
    if !runtime_root.exists() {
        return Err(format!(
            "runtime root does not exist: {}",
            runtime_root.display()
        ));
    }

    let store = RuntimeRootPreferenceStore::new(runtime_root.join("prefs/runtime-root.cfg"));
    store
        .save(runtime_root)
        .map_err(|err| format!("persist runtime root failed: {err}"))?;

    let startup_route = resolve_startup_route(&store)
        .map_err(|err| format!("resolve startup route failed: {err}"))?;
    let startup_route_label = match startup_route {
        StartupRoute::Onboarding => "onboarding".to_string(),
        StartupRoute::Recovery { .. } => "recovery".to_string(),
        StartupRoute::Workspace { .. } => "workspace".to_string(),
    };
    if startup_route_label != "workspace" {
        return Err(format!(
            "smoke expected workspace route after persisting runtime root, got {startup_route_label}"
        ));
    }

    let spec = TauriShellHostSpec::new(core_sidecar).with_runtime_root(runtime_root);
    if !spec.bundled_index_html().contains("id=\"workspace-shell\"") {
        return Err("bundled index page contract missing workspace shell".to_string());
    }
    if !spec
        .bundled_onboarding_html()
        .contains("id=\"pick-runtime-root\"")
    {
        return Err("bundled onboarding page contract missing runtime-root picker".to_string());
    }

    let adapter = detect_host_platform_adapter("info");
    let transport = adapter.control_plane_transport();
    let runtime_paths = adapter
        .runtime_paths()
        .runtime_paths("bridgingio-ui-managed", runtime_root);
    let diagnostics = transport
        .diagnostics("bridgingio-ui-managed", &runtime_paths)
        .into_iter()
        .map(|diag| {
            format!(
                "{}:{}:{} | recovery={}",
                diag.code,
                diag.status.as_str(),
                diag.message,
                diag.recovery_hint
            )
        })
        .collect::<Vec<_>>();

    let status = if transport.status() == CapabilityStatus::Ready {
        SmokeStatus::Ready
    } else {
        SmokeStatus::Deferred
    };

    Ok(SmokeReport {
        status,
        transport_kind: transport.transport_kind().to_string(),
        startup_route: startup_route_label,
        sidecar_commandline: spec.sidecar_commandline(),
        diagnostics,
    })
}

#[cfg(test)]
mod tests {
    use super::{run_desktop_host_smoke, SmokeStatus};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("{label}-{stamp}"))
    }

    #[test]
    fn smoke_report_captures_startup_route_and_sidecar_contract() {
        let runtime_root = temp_dir("desktop-host-smoke");
        fs::create_dir_all(&runtime_root).expect("create runtime root");

        let report = run_desktop_host_smoke(
            &runtime_root,
            PathBuf::from("/tmp/bridgingio-core").as_path(),
        )
        .expect("smoke report");
        assert_eq!(report.startup_route, "workspace");
        assert!(report
            .sidecar_commandline
            .iter()
            .any(|arg| arg == "ui-managed-ephemeral"));
        assert!(report
            .sidecar_commandline
            .iter()
            .any(|arg| arg == "--runtime-root"));
        assert!(report
            .sidecar_commandline
            .iter()
            .any(|arg| arg == &runtime_root.to_string_lossy()));
        assert!(!report.diagnostics.is_empty());

        #[cfg(unix)]
        assert_eq!(report.status, SmokeStatus::Ready);
        #[cfg(not(unix))]
        assert_eq!(report.status, SmokeStatus::Deferred);

        fs::remove_dir_all(&runtime_root).expect("cleanup");
    }
}
