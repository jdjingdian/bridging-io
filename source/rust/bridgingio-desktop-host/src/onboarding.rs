use std::path::{Path, PathBuf};

use crate::storage::{
    validate_runtime_root, RuntimeRootPreferenceStore, RuntimeRootValidationError,
};

const ONBOARDING_HTML: &str = include_str!("../assets/onboarding.html");

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OnboardingAction {
    ChooseRuntimeRoot,
    ValidateRuntimeRoot,
    ContinueToWorkspace,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OnboardingError {
    RuntimeRootNotSelected,
    RuntimeRootInvalid(RuntimeRootValidationError),
    PersistFailed(String),
}

pub struct OnboardingFlow {
    selected_runtime_root: Option<PathBuf>,
    validated: bool,
    completed: bool,
}

impl OnboardingFlow {
    pub fn new() -> Self {
        Self {
            selected_runtime_root: None,
            validated: false,
            completed: false,
        }
    }

    pub fn onboarding_html(&self) -> &'static str {
        ONBOARDING_HTML
    }

    pub fn available_actions(&self) -> Vec<OnboardingAction> {
        vec![
            OnboardingAction::ChooseRuntimeRoot,
            OnboardingAction::ValidateRuntimeRoot,
            OnboardingAction::ContinueToWorkspace,
        ]
    }

    pub fn selected_runtime_root(&self) -> Option<&Path> {
        self.selected_runtime_root.as_deref()
    }

    pub fn can_enter_workspace(&self) -> bool {
        self.completed && self.validated
    }

    pub fn choose_runtime_root(&mut self, runtime_root: impl Into<PathBuf>) {
        self.selected_runtime_root = Some(runtime_root.into());
        self.validated = false;
        self.completed = false;
    }

    pub fn validate_selected_runtime_root(&mut self) -> Result<(), OnboardingError> {
        let Some(path) = self.selected_runtime_root.as_deref() else {
            return Err(OnboardingError::RuntimeRootNotSelected);
        };
        validate_runtime_root(path).map_err(OnboardingError::RuntimeRootInvalid)?;
        self.validated = true;
        Ok(())
    }

    pub fn complete(
        &mut self,
        store: &RuntimeRootPreferenceStore,
    ) -> Result<PathBuf, OnboardingError> {
        if !self.validated {
            self.validate_selected_runtime_root()?;
        }
        let path = self
            .selected_runtime_root
            .as_ref()
            .ok_or(OnboardingError::RuntimeRootNotSelected)?
            .clone();
        store
            .save(&path)
            .map_err(|err| OnboardingError::PersistFailed(err.to_string()))?;
        self.completed = true;
        Ok(path)
    }
}

pub fn validate_onboarding_markup(html: &str) -> Result<(), String> {
    let required_snippets = [
        "id=\"pick-runtime-root\"",
        "id=\"validate-runtime-root\"",
        "id=\"continue-workspace\"",
        "runtime root",
        "writable",
        "recovery",
    ];

    for snippet in required_snippets {
        if !html
            .to_ascii_lowercase()
            .contains(&snippet.to_ascii_lowercase())
        {
            return Err(format!(
                "onboarding markup missing required snippet: {snippet}"
            ));
        }
    }

    let forbidden_snippets = ["cancel", "skip setup", "enter workspace anyway"];
    let normalized = html.to_ascii_lowercase();
    for forbidden in forbidden_snippets {
        if normalized.contains(forbidden) {
            return Err(format!(
                "onboarding markup contains forbidden bypass snippet: {forbidden}"
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate_onboarding_markup, OnboardingAction, OnboardingError, OnboardingFlow};
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
    fn onboarding_actions_have_no_cancel_path() {
        let flow = OnboardingFlow::new();
        let actions = flow.available_actions();
        assert!(actions.contains(&OnboardingAction::ChooseRuntimeRoot));
        assert!(actions.contains(&OnboardingAction::ValidateRuntimeRoot));
        assert!(actions.contains(&OnboardingAction::ContinueToWorkspace));
    }

    #[test]
    fn onboarding_requires_valid_runtime_root_before_workspace() -> io::Result<()> {
        let root = temp_dir("desktop-host-onboarding");
        fs::create_dir_all(&root)?;
        let prefs = RuntimeRootPreferenceStore::new(root.join("prefs/runtime-root.cfg"));
        let runtime = root.join("runtime");
        fs::create_dir_all(&runtime)?;

        let mut flow = OnboardingFlow::new();
        flow.choose_runtime_root(&runtime);
        flow.validate_selected_runtime_root().expect("validate");
        let persisted = flow.complete(&prefs).expect("complete");

        assert_eq!(persisted, runtime);
        assert!(flow.can_enter_workspace());

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn onboarding_rejects_invalid_runtime_root() -> io::Result<()> {
        let root = temp_dir("desktop-host-onboarding-invalid");
        fs::create_dir_all(&root)?;

        let mut flow = OnboardingFlow::new();
        let missing = root.join("missing");
        flow.choose_runtime_root(&missing);
        let error = flow
            .validate_selected_runtime_root()
            .expect_err("must fail");
        assert_eq!(
            error,
            OnboardingError::RuntimeRootInvalid(RuntimeRootValidationError::MissingPath)
        );
        assert!(!flow.can_enter_workspace());

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn onboarding_html_enforces_no_bypass_controls() {
        let flow = OnboardingFlow::new();
        validate_onboarding_markup(flow.onboarding_html()).expect("markup contract");
    }
}
