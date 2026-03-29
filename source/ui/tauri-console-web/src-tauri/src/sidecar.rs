use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct SidecarResolution {
    pub executable: PathBuf,
    pub strategy: String,
    pub candidates: Vec<PathBuf>,
}

fn sidecar_file_name() -> &'static str {
    #[cfg(windows)]
    {
        "bridgingio-core.exe"
    }
    #[cfg(not(windows))]
    {
        "bridgingio-core"
    }
}

fn candidate_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(path) = std::env::var("BRIDGINGIO_CORE_SIDECAR") {
        candidates.push(PathBuf::from(path));
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source_root = manifest_dir
        .join("..")
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap_or_else(|_| manifest_dir.join("..").join("..").join(".."));
    let name = sidecar_file_name();
    let target_suffix = option_env!("TAURI_ENV_TARGET_TRIPLE")
        .map(|triple| format!("bridgingio-core-{triple}"))
        .unwrap_or_else(|| "bridgingio-core".to_string());

    candidates.push(manifest_dir.join("bin").join(&target_suffix));
    candidates.push(manifest_dir.join("bin").join(name));
    candidates.push(source_root.join("rust/target/debug").join(name));
    candidates.push(source_root.join("rust/target/release").join(name));

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("sidecars").join(name));
            candidates.push(parent.join("../Resources/sidecars").join(name));
        }
    }

    candidates
}

pub fn resolve_core_sidecar() -> SidecarResolution {
    let candidates = candidate_paths();
    if let Some(path) = candidates.iter().find(|path| path.exists()) {
        return SidecarResolution {
            executable: path.clone(),
            strategy: "resolved_existing_candidate".to_string(),
            candidates,
        };
    }
    SidecarResolution {
        executable: candidates
            .first()
            .cloned()
            .unwrap_or_else(|| PathBuf::from(sidecar_file_name())),
        strategy: "unresolved_use_first_candidate".to_string(),
        candidates,
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_core_sidecar;

    #[test]
    fn sidecar_resolution_tracks_candidates() {
        let resolved = resolve_core_sidecar();
        assert!(!resolved.candidates.is_empty());
        assert!(!resolved.strategy.is_empty());
    }
}
