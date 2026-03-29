use std::io;
use std::path::PathBuf;

use crate::storage::{
    validate_runtime_root, RuntimeRootPreferenceStore, RuntimeRootValidationError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartupRoute {
    Onboarding,
    Recovery {
        saved_runtime_root: PathBuf,
        reason: RuntimeRootValidationError,
    },
    Workspace {
        runtime_root: PathBuf,
    },
}

pub fn resolve_startup_route(store: &RuntimeRootPreferenceStore) -> io::Result<StartupRoute> {
    let Some(saved_runtime_root) = store.load()? else {
        return Ok(StartupRoute::Onboarding);
    };

    match validate_runtime_root(&saved_runtime_root) {
        Ok(()) => Ok(StartupRoute::Workspace {
            runtime_root: saved_runtime_root,
        }),
        Err(reason) => Ok(StartupRoute::Recovery {
            saved_runtime_root,
            reason,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_startup_route, StartupRoute};
    use crate::storage::{RuntimeRootPreferenceStore, RuntimeRootValidationError};
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
    fn startup_without_saved_root_enters_onboarding() -> io::Result<()> {
        let root = temp_dir("desktop-host-startup");
        fs::create_dir_all(&root)?;
        let store = RuntimeRootPreferenceStore::new(root.join("prefs/runtime-root.cfg"));

        let route = resolve_startup_route(&store)?;
        assert_eq!(route, StartupRoute::Onboarding);

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn startup_with_invalid_saved_root_enters_recovery() -> io::Result<()> {
        let root = temp_dir("desktop-host-startup-recovery");
        fs::create_dir_all(&root)?;
        let store = RuntimeRootPreferenceStore::new(root.join("prefs/runtime-root.cfg"));
        let missing = root.join("missing-runtime");
        store.save(&missing)?;

        let route = resolve_startup_route(&store)?;
        assert_eq!(
            route,
            StartupRoute::Recovery {
                saved_runtime_root: missing,
                reason: RuntimeRootValidationError::MissingPath,
            }
        );

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn startup_with_valid_saved_root_enters_workspace() -> io::Result<()> {
        let root = temp_dir("desktop-host-startup-workspace");
        fs::create_dir_all(&root)?;
        let store = RuntimeRootPreferenceStore::new(root.join("prefs/runtime-root.cfg"));
        let runtime = root.join("runtime");
        fs::create_dir_all(&runtime)?;
        store.save(&runtime)?;

        let route = resolve_startup_route(&store)?;
        assert_eq!(
            route,
            StartupRoute::Workspace {
                runtime_root: runtime,
            }
        );

        fs::remove_dir_all(&root)?;
        Ok(())
    }
}
