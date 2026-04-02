use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use bridgingio_domain::{RuntimeBootstrapStatus, RuntimeRecoveryAction};

const RUNTIME_ROOT_KEY: &str = "runtime_root=";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeRootValidationError {
    MissingPath,
    NotDirectory,
    NotWritable,
}

impl RuntimeRootValidationError {
    pub fn bootstrap_status(&self) -> RuntimeBootstrapStatus {
        match self {
            RuntimeRootValidationError::MissingPath | RuntimeRootValidationError::NotDirectory => {
                RuntimeBootstrapStatus::NeedsRelocate
            }
            RuntimeRootValidationError::NotWritable => RuntimeBootstrapStatus::NeedsPermissionFix,
        }
    }

    pub fn recovery_actions(&self) -> Vec<RuntimeRecoveryAction> {
        match self {
            RuntimeRootValidationError::MissingPath | RuntimeRootValidationError::NotDirectory => {
                vec![RuntimeRecoveryAction::ChooseRuntimeRoot, RuntimeRecoveryAction::Retry]
            }
            RuntimeRootValidationError::NotWritable => vec![
                RuntimeRecoveryAction::FixPermissions,
                RuntimeRecoveryAction::ChooseRuntimeRoot,
                RuntimeRecoveryAction::Retry,
            ],
        }
    }

    pub fn recovery_hint(&self) -> &'static str {
        match self {
            RuntimeRootValidationError::MissingPath => {
                "choose a runtime root directory before entering the workspace"
            }
            RuntimeRootValidationError::NotDirectory => {
                "selected path is not a directory; choose a valid runtime root"
            }
            RuntimeRootValidationError::NotWritable => {
                "selected runtime root is not writable; grant permission or choose another path"
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct RuntimeRootPreferenceStore {
    file_path: PathBuf,
}

impl RuntimeRootPreferenceStore {
    pub fn new(file_path: impl Into<PathBuf>) -> Self {
        Self {
            file_path: file_path.into(),
        }
    }

    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    pub fn load(&self) -> io::Result<Option<PathBuf>> {
        if !self.file_path.exists() {
            return Ok(None);
        }
        let text = fs::read_to_string(&self.file_path)?;
        for line in text.lines() {
            if let Some(raw) = line.strip_prefix(RUNTIME_ROOT_KEY) {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(PathBuf::from(trimmed)));
            }
        }
        Ok(None)
    }

    pub fn save(&self, runtime_root: &Path) -> io::Result<()> {
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(
            &self.file_path,
            format!("{RUNTIME_ROOT_KEY}{}\n", runtime_root.to_string_lossy()),
        )
    }

    pub fn clear(&self) -> io::Result<()> {
        if self.file_path.exists() {
            fs::remove_file(&self.file_path)?;
        }
        Ok(())
    }
}

pub fn validate_runtime_root(runtime_root: &Path) -> Result<(), RuntimeRootValidationError> {
    if runtime_root.as_os_str().is_empty() {
        return Err(RuntimeRootValidationError::MissingPath);
    }
    let metadata = runtime_root
        .metadata()
        .map_err(|_| RuntimeRootValidationError::MissingPath)?;
    if !metadata.is_dir() {
        return Err(RuntimeRootValidationError::NotDirectory);
    }

    let probe_file = runtime_root.join(".bridgingio-write-check");
    let write_result = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&probe_file)
        .and_then(|_| fs::remove_file(&probe_file));

    if write_result.is_err() {
        return Err(RuntimeRootValidationError::NotWritable);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate_runtime_root, RuntimeRootPreferenceStore, RuntimeRootValidationError};
    use std::fs;
    use std::io;
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
    fn store_persists_runtime_root_selection() -> io::Result<()> {
        let root = temp_dir("desktop-host-store");
        fs::create_dir_all(&root)?;
        let prefs_path = root.join("prefs/runtime-root.cfg");
        let store = RuntimeRootPreferenceStore::new(&prefs_path);

        let selected = root.join("runtime");
        fs::create_dir_all(&selected)?;
        store.save(&selected)?;

        let loaded = store.load()?;
        assert_eq!(loaded, Some(selected));
        store.clear()?;
        assert_eq!(store.load()?, None);
        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn validator_rejects_missing_or_non_directory() -> io::Result<()> {
        let root = temp_dir("desktop-host-validate");
        fs::create_dir_all(&root)?;
        let missing = root.join("missing");
        assert_eq!(
            validate_runtime_root(&missing),
            Err(RuntimeRootValidationError::MissingPath)
        );

        let file_path = root.join("file.txt");
        fs::write(&file_path, "x")?;
        assert_eq!(
            validate_runtime_root(&file_path),
            Err(RuntimeRootValidationError::NotDirectory)
        );

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn validator_accepts_writable_directory() -> io::Result<()> {
        let root = temp_dir("desktop-host-writable");
        fs::create_dir_all(&root)?;
        assert_eq!(validate_runtime_root(&root), Ok(()));
        fs::remove_dir_all(&root)?;
        Ok(())
    }
}
