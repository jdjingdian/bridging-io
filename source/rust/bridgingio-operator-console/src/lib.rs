use std::fs;
use std::io::{self, Stdout};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, TryRecvError};
use std::thread;
use std::time::{Duration, SystemTime};

use bridgingio_domain::TargetKind;
use bridgingio_engine::{
    i18n::Catalog, CoreSettings, StandaloneConnectionSection, StandaloneTargetProfile,
    StandaloneTerminalSection, TerminalProviderSection,
};
use bridgingio_platform::{
    next_local_authorization_flow_id, LocalAuthorizationEvent, LocalAuthorizationLogStream,
    LocalAuthorizationRecorder, LocalOperatorSurface, RuntimeLogLevel,
};
#[cfg(test)]
use bridgingio_secrets::VaultUnlockTriggerPolicy;
use bridgingio_secrets::{
    canonical_ssh_private_key_ref_from_key_name, local_admin_create_token_target,
    local_admin_delete_vault_target, local_admin_payload_digest_for_create_agent_token,
    local_admin_payload_digest_for_delete_agent_token, local_admin_payload_digest_for_delete_vault,
    read_passive_vault_projection, take_last_verified_os_native_event, CreateAgentTokenRequest,
    DeleteAgentTokenRequest, DeleteVaultRequest, LocalAdminActionKind, SecretVaultRouter,
    TokenScopeInput, TrustedLocalSshKeyImportRequest, TrustedLocalSshKeyImportResult,
    UpdateAgentTokenAccessRequest, UpdateAgentTokenLabelRequest, VaultError, VaultPassiveProjection,
    VerifiedOsNativeEvent,
};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, size as terminal_size, EnterAlternateScreen,
    LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Terminal;

pub struct MenuConfigOutcome {
    pub saved: bool,
    pub apply_strategy: Option<String>,
    pub config_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecuritySummary {
    pub backend: String,
    pub lock_state: String,
    pub trigger_policy: String,
    pub preferred_method: String,
    pub secret_count: usize,
    pub ssh_key_count: usize,
    pub token_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Screen {
    Root,
    Core,
    Storage,
    ModelPlane,
    Vault,
    Targets,
    Security,
    SshKeyImport,
    SshKeyManagement,
    SshKeyDetail(String),
    TokenManagement,
    TokenDetail(String),
    TargetAddMode,
    TargetAddTypePlain,
    TargetAddTypeSensitive,
    TargetEditor(usize),
    TargetPublicDescriptor(usize),
    TargetConnectionProfile(usize),
    TargetSensitiveOverlay(usize),
    TargetCredentialSource(usize),
    TargetCredentialPicker(usize),
    TargetPolicy(usize),
    SearchResults,
}

impl Screen {
    fn title(&self, catalog: &Catalog) -> String {
        match self {
            Screen::Root => catalog.t("menu.root.title"),
            Screen::Core => catalog.t("menu.core.title"),
            Screen::Storage => catalog.t("menu.storage.title"),
            Screen::ModelPlane => catalog.t("menu.model_plane.title"),
            Screen::Vault => catalog.t("menu.vault.title"),
            Screen::Targets => catalog.t("menu.targets.title"),
            Screen::Security => catalog.t("menu.security.title"),
            Screen::SshKeyImport => catalog.t("menu.ssh_key.import.title"),
            Screen::SshKeyManagement => catalog.t("menu.ssh_key.management.title"),
            Screen::SshKeyDetail(_) => catalog.t("menu.ssh_key.detail.title"),
            Screen::TokenManagement => catalog.t("menu.token_management.title"),
            Screen::TokenDetail(_) => catalog.t("menu.token_detail.title"),
            Screen::TargetAddMode => catalog.t("menu.targets.add_target"),
            Screen::TargetAddTypePlain => catalog.t("menu.targets.choose_type_plain"),
            Screen::TargetAddTypeSensitive => catalog.t("menu.targets.choose_type_sensitive"),
            Screen::TargetEditor(_) => catalog.t("menu.target.title"),
            Screen::TargetPublicDescriptor(_) => catalog.t("menu.target.public_descriptor"),
            Screen::TargetConnectionProfile(_) => catalog.t("menu.target.connection_profile"),
            Screen::TargetSensitiveOverlay(_) => catalog.t("menu.target.sensitive_overlay"),
            Screen::TargetCredentialSource(_) => catalog.t("menu.target.credential_source"),
            Screen::TargetCredentialPicker(_) => catalog.t("menu.target.credential_picker"),
            Screen::TargetPolicy(_) => catalog.t("menu.target.policy"),
            Screen::SearchResults => catalog.t("menu.search_results.title"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MenuEntry {
    label: String,
    value: Option<String>,
    description: String,
    dirty_key: Option<String>,
    kind: MenuEntryKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum MenuEntryKind {
    Navigate(Screen),
    EditField(String),
    FocusField { screen: Screen, field: String },
    Action(ActionKind),
    Info,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ActionKind {
    OpenAddTarget,
    ChoosePlainTargetMode,
    ChooseSensitiveTargetMode,
    AddSensitiveSshTarget,
    AddSensitiveAdbTarget,
    AddSshTarget,
    AddAdbTarget,
    OpenSshKeyImport,
    ExecuteSshKeyImport,
    OpenSshKeyManagement,
    OpenSshKeyDetail(String),
    DeleteSshKey(String),
    OpenTargetCredentialSource(usize),
    OpenTargetCredentialPicker(usize),
    ApplyTarget(usize),
    BindTargetCredentialRef {
        target_index: usize,
        credential_ref: String,
    },
    ClearTargetCredentialRef(usize),
    EditTargetCredentialRef(usize),
    ImportLocalSshKeyIntoVault(usize),
    UnlockVault,
    InitVault,
    DeleteVault,
    CreateToken,
    OpenTokenManagement,
    OpenTokenDetail(String),
    EditTokenLabel(String),
    ToggleTokenAccess(String),
    OpenTokenPermissions(String),
    RevokeToken(String),
    DeleteToken(String),
    DeleteTarget(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditModeKind {
    Text,
    Choice,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FooterButton {
    Select,
    Exit,
    Help,
}

impl FooterButton {
    fn from_index(index: usize) -> Self {
        match index % 3 {
            0 => FooterButton::Select,
            1 => FooterButton::Exit,
            _ => FooterButton::Help,
        }
    }

    fn label(self, catalog: &Catalog) -> String {
        match self {
            FooterButton::Select => catalog.t("menu.footer.select"),
            FooterButton::Exit => catalog.t("menu.footer.exit"),
            FooterButton::Help => catalog.t("menu.footer.help"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelectionHighlightMode {
    Reverse,
    Fallback,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum UnlockFlowState {
    Idle,
    Waiting,
    Success,
    Failed { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TokenManagementRow {
    token_id: String,
    label: String,
    token_fingerprint: String,
    status: String,
    expires_at: Option<SystemTime>,
    revoked_at: Option<SystemTime>,
    revoke_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SshKeyManagementRow {
    credential_ref: String,
    label: String,
    status: String,
    active_version: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
struct SshKeyImportDraft {
    key_name: String,
    label: String,
    source_path: String,
    passphrase: Option<String>,
    bind_target_index: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ConfirmAction {
    DeleteVault,
    ConfirmPlainSshRisk,
    DeleteSshKey(String),
    RevokeToken(String),
    DeleteToken(String),
    DeleteTarget(usize),
    DiscardNewTarget(usize),
    DiscardTargetChanges(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TargetEditSession {
    index: usize,
    is_new: bool,
    baseline: Option<StandaloneTargetProfile>,
}

const MENUCONFIG_FORCE_FALLBACK_HIGHLIGHT_ENV: &str =
    "BRIDGINGIO_MENUCONFIG_FORCE_FALLBACK_HIGHLIGHT";
const MENU_UNLOCK_REASON_REQUIRES_OS_NATIVE: &str = "__menu_unlock_requires_os_native__";
const FOOTER_LOCK_STATE_TOKEN: &str = "__bridgingio_footer_lock_state_token__";
const TOKEN_CREATE_LABEL_FIELD: &str = "__token_create_label__";
const TOKEN_CREATE_EXPIRY_MODE_FIELD: &str = "__token_create_expiry_mode__";
const TOKEN_CREATE_EXPIRY_AT_FIELD: &str = "__token_create_expiry_at_unix_sec__";
const SSH_IMPORT_KEY_NAME_FIELD: &str = "__ssh_import_key_name__";
const SSH_IMPORT_LABEL_FIELD: &str = "__ssh_import_label__";
const SSH_IMPORT_SOURCE_PATH_FIELD: &str = "__ssh_import_source_path__";
const SSH_IMPORT_PASSPHRASE_FIELD: &str = "__ssh_import_passphrase__";
const OP_VAULT_UNLOCK: &str = "vault.unlock";
const OP_VAULT_DELETE: &str = "vault.delete";
const OP_SSH_KEY_IMPORT: &str = "ssh_key.import";
const OP_SSH_KEY_DELETE: &str = "ssh_key.delete";
const OP_AUTH_TOKEN_CREATE: &str = "auth.token.create";
const OP_AUTH_TOKEN_DELETE: &str = "auth.token.delete";
const MENUCONFIG_MIN_VIEWPORT_WIDTH: u16 = 80;
const MENUCONFIG_MIN_VIEWPORT_HEIGHT: u16 = 24;

struct UnlockWorkerOutcome {
    router: SecretVaultRouter,
    result: Result<(), String>,
    verified_event: Option<VerifiedOsNativeEvent>,
}

struct UnlockWorkerHandle {
    receiver: mpsc::Receiver<UnlockWorkerOutcome>,
    cancel_requested: bool,
}

pub struct MenuConfigApp {
    config_path: PathBuf,
    settings: CoreSettings,
    catalog: Catalog,
    authorization_recorder: LocalAuthorizationRecorder,
    session_flow_id: String,
    vault_router: Option<SecretVaultRouter>,
    security_summary: SecuritySummary,
    screen: Screen,
    selected: usize,
    navigation_stack: Vec<(Screen, usize)>,
    footer_selected: usize,
    dirty_paths: Vec<String>,
    search_mode: bool,
    search_input: String,
    edit_mode: bool,
    edit_mode_kind: EditModeKind,
    edit_field: Option<String>,
    edit_input: String,
    edit_cursor: usize,
    edit_options: Vec<String>,
    edit_option_selected: usize,
    show_help: bool,
    exit_confirm_mode: bool,
    exit_confirm_selected: usize,
    unlock_flow_state: UnlockFlowState,
    unlock_flow_id: Option<String>,
    unlock_flow_pending: bool,
    unlock_worker: Option<UnlockWorkerHandle>,
    token_rows: Vec<TokenManagementRow>,
    ssh_key_rows: Vec<SshKeyManagementRow>,
    ssh_import_draft: SshKeyImportDraft,
    ssh_import_last_result: Option<TrustedLocalSshKeyImportResult>,
    target_edit_session: Option<TargetEditSession>,
    confirm_action: Option<ConfirmAction>,
    confirm_flow_id: Option<String>,
    confirm_selected: usize,
    token_reveal: Option<String>,
    pending_token_label: Option<String>,
    selection_highlight_mode: SelectionHighlightMode,
    last_status: String,
    last_apply_strategy: Option<String>,
}

impl MenuConfigApp {
    pub fn load(config_path: impl Into<PathBuf>) -> Result<Self, String> {
        let config_path = config_path.into();
        let settings = CoreSettings::load_from_file(&config_path)
            .map_err(|err| format!("load menuconfig settings failed: {err:?}"))?;
        let catalog = Catalog::load(&settings.core.operator_locale).map_err(|err| {
            format!(
                "load menuconfig catalog failed (locale={}): {err}",
                settings.core.operator_locale
            )
        })?;
        let initial_status = catalog.t("menu.status.initial");
        let security_summary = build_security_summary_passive(&settings);
        let authorization_recorder = LocalAuthorizationRecorder::for_runtime_root(
            Path::new(&settings.core.data_dir),
            RuntimeLogLevel::parse(&settings.core.log_level),
        );
        Ok(Self {
            config_path,
            settings,
            catalog,
            authorization_recorder,
            session_flow_id: next_local_authorization_flow_id("menuconfig.session"),
            vault_router: None,
            security_summary,
            screen: Screen::Root,
            selected: 0,
            navigation_stack: Vec::new(),
            footer_selected: 0,
            dirty_paths: Vec::new(),
            search_mode: false,
            search_input: String::new(),
            edit_mode: false,
            edit_mode_kind: EditModeKind::Text,
            edit_field: None,
            edit_input: String::new(),
            edit_cursor: 0,
            edit_options: Vec::new(),
            edit_option_selected: 0,
            show_help: false,
            exit_confirm_mode: false,
            exit_confirm_selected: 0,
            unlock_flow_state: UnlockFlowState::Idle,
            unlock_flow_id: None,
            unlock_flow_pending: false,
            unlock_worker: None,
            token_rows: Vec::new(),
            ssh_key_rows: Vec::new(),
            ssh_import_draft: SshKeyImportDraft::default(),
            ssh_import_last_result: None,
            target_edit_session: None,
            confirm_action: None,
            confirm_flow_id: None,
            confirm_selected: 0,
            token_reveal: None,
            pending_token_label: None,
            selection_highlight_mode: detect_selection_highlight_mode(),
            last_status: initial_status,
            last_apply_strategy: None,
        })
    }

    fn t(&self, key: &str) -> String {
        self.catalog.t(key)
    }

    fn tf(&self, key: &str, vars: &[(&str, &str)]) -> String {
        self.catalog.tf(key, vars)
    }

    fn current_screen_id(&self) -> String {
        match &self.screen {
            Screen::Root => "Root".to_string(),
            Screen::Core => "Core".to_string(),
            Screen::Storage => "Storage".to_string(),
            Screen::ModelPlane => "ModelPlane".to_string(),
            Screen::Vault => "Vault".to_string(),
            Screen::Targets => "Targets".to_string(),
            Screen::Security => "Security".to_string(),
            Screen::SshKeyImport => "SshKeyImport".to_string(),
            Screen::SshKeyManagement => "SshKeyManagement".to_string(),
            Screen::SshKeyDetail(token) => format!("SshKeyDetail:{token}"),
            Screen::TokenManagement => "TokenManagement".to_string(),
            Screen::TokenDetail(token) => format!("TokenDetail:{token}"),
            Screen::TargetAddMode => "TargetAddMode".to_string(),
            Screen::TargetAddTypePlain => "TargetAddTypePlain".to_string(),
            Screen::TargetAddTypeSensitive => "TargetAddTypeSensitive".to_string(),
            Screen::TargetEditor(index) => format!("TargetEditor:{index}"),
            Screen::TargetPublicDescriptor(index) => format!("TargetPublicDescriptor:{index}"),
            Screen::TargetConnectionProfile(index) => format!("TargetConnectionProfile:{index}"),
            Screen::TargetSensitiveOverlay(index) => format!("TargetSensitiveOverlay:{index}"),
            Screen::TargetCredentialSource(index) => format!("TargetCredentialSource:{index}"),
            Screen::TargetCredentialPicker(index) => format!("TargetCredentialPicker:{index}"),
            Screen::TargetPolicy(index) => format!("TargetPolicy:{index}"),
            Screen::SearchResults => "SearchResults".to_string(),
        }
    }

    fn record_authorization_event(
        &self,
        flow_id: &str,
        action: &str,
        operation: &str,
        phase: &str,
        result: &str,
        dedupe_state: &str,
        error_code: Option<&str>,
        token_id: Option<&str>,
        credential_ref: Option<&str>,
    ) {
        let mut event = LocalAuthorizationEvent::new(
            flow_id,
            LocalOperatorSurface::Menuconfig,
            self.current_screen_id(),
            action,
            operation,
            phase,
            result,
            dedupe_state,
        );
        event.error_code = error_code.map(ToString::to_string);
        event.token_id = token_id.map(ToString::to_string);
        event.credential_ref = credential_ref.map(ToString::to_string);
        self.authorization_recorder.append_event_with_fallback(
            LocalAuthorizationLogStream::Authorization,
            RuntimeLogLevel::Info,
            &event,
        );
    }

    fn record_session_event(
        &self,
        action: &str,
        phase: &str,
        result: &str,
        level: RuntimeLogLevel,
        error_code: Option<&str>,
    ) {
        self.record_session_event_with_flow(
            &self.session_flow_id,
            action,
            phase,
            result,
            level,
            error_code,
        );
    }

    fn record_session_event_with_flow(
        &self,
        flow_id: &str,
        action: &str,
        phase: &str,
        result: &str,
        level: RuntimeLogLevel,
        error_code: Option<&str>,
    ) {
        let mut event = LocalAuthorizationEvent::new(
            flow_id,
            LocalOperatorSurface::Menuconfig,
            self.current_screen_id(),
            action,
            "menuconfig.session",
            phase,
            result,
            "not-applicable",
        );
        event.error_code = error_code.map(ToString::to_string);
        self.authorization_recorder.append_event_with_fallback(
            LocalAuthorizationLogStream::MenuconfigSession,
            level,
            &event,
        );
    }

    fn finish_session(&self, outcome: MenuConfigOutcome) -> MenuConfigOutcome {
        self.record_session_event("menuconfig.exit", "completed", "ok", RuntimeLogLevel::Info, None);
        outcome
    }

    pub fn run(&mut self) -> Result<MenuConfigOutcome, String> {
        self.record_session_event(
            "menuconfig.start",
            "started",
            "ok",
            RuntimeLogLevel::Info,
            None,
        );
        let mut ui =
            TerminalUi::enter().map_err(|err| format!("enter menuconfig ui failed: {err}"))?;
        loop {
            self.poll_unlock_worker();
            self.sync_selection_to_focusable();
            ui.terminal
                .draw(|frame| render(frame, self))
                .map_err(|err| format!("draw menuconfig failed: {err}"))?;

            if self.unlock_flow_pending {
                self.unlock_flow_pending = false;
                self.start_unlock_worker();
                continue;
            }

            if !event::poll(Duration::from_millis(100))
                .map_err(|err| format!("poll menuconfig event failed: {err}"))?
            {
                continue;
            }
            let Event::Key(key) =
                event::read().map_err(|err| format!("read menuconfig event failed: {err}"))?
            else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }

            if runtime_viewport_too_small() {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        if let Some(outcome) = self.request_exit_or_back()? {
                            return Ok(self.finish_session(outcome));
                        }
                    }
                    _ => {}
                }
                continue;
            }

            if !matches!(self.unlock_flow_state, UnlockFlowState::Idle) {
                self.handle_unlock_flow_key(key.code);
                continue;
            }
            if self.token_reveal.is_some() {
                self.handle_token_reveal_key(key.code);
                continue;
            }
            if self.confirm_action.is_some() {
                self.handle_confirm_key(key.code)?;
                continue;
            }
            if self.exit_confirm_mode {
                if let Some(outcome) = self.handle_exit_confirm_key(key.code)? {
                    return Ok(self.finish_session(outcome));
                }
                continue;
            }
            if self.edit_mode {
                self.handle_edit_key(key.code)?;
                continue;
            }
            if self.search_mode {
                self.handle_search_key(key.code);
                continue;
            }

            match key.code {
                KeyCode::Char('q') => {
                    if let Some(outcome) = self.request_exit_or_back()? {
                        return Ok(self.finish_session(outcome));
                    }
                }
                KeyCode::Esc => {
                    if self.show_help {
                        self.show_help = false;
                        self.last_status = self.t("menu.status.help_closed");
                    } else if let Some(outcome) = self.request_exit_or_back()? {
                        return Ok(self.finish_session(outcome));
                    }
                }
                KeyCode::Left => self.move_footer_selection(-1),
                KeyCode::Right => self.move_footer_selection(1),
                KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
                KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
                KeyCode::Char('/') => {
                    self.search_mode = true;
                    self.search_input.clear();
                    self.last_status = self.t("menu.status.search_prompt");
                }
                KeyCode::Char('?') | KeyCode::F(1) => self.show_help = !self.show_help,
                KeyCode::Char('u') => self.begin_unlock_flow(),
                KeyCode::Char('s') => self.save()?,
                KeyCode::Char(' ') => self.handle_space_on_selected()?,
                KeyCode::Enter => {
                    if let Some(outcome) = self.activate_footer_button()? {
                        return Ok(self.finish_session(outcome));
                    }
                }
                _ => {}
            }
        }
    }

    pub fn is_dirty(&self) -> bool {
        !self.dirty_paths.is_empty()
    }

    fn handle_edit_key(&mut self, code: KeyCode) -> Result<(), String> {
        match self.edit_mode_kind {
            EditModeKind::Text => match code {
                KeyCode::Esc => self.cancel_edit("menu.status.edit_cancelled"),
                KeyCode::Enter => self.commit_edit()?,
                KeyCode::Left => self.move_text_cursor(-1),
                KeyCode::Right => self.move_text_cursor(1),
                KeyCode::Backspace => self.backspace_text_char(),
                KeyCode::Char(ch) => self.insert_text_char(ch),
                _ => {}
            },
            EditModeKind::Choice => match code {
                KeyCode::Esc => self.cancel_edit("menu.status.select_cancelled"),
                KeyCode::Down | KeyCode::Char('j') => self.move_edit_option(1),
                KeyCode::Up | KeyCode::Char('k') => self.move_edit_option(-1),
                KeyCode::Enter | KeyCode::Char(' ') => self.commit_edit()?,
                _ => {}
            },
        }
        Ok(())
    }

    fn handle_search_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => {
                self.search_mode = false;
                self.search_input.clear();
                self.last_status = self.t("menu.status.search_cancelled");
            }
            KeyCode::Enter => {
                self.search_mode = false;
                self.push_navigation_state();
                self.screen = Screen::SearchResults;
                self.selected = 0;
                self.sync_selection_to_focusable();
                self.last_status = if self.search_input.trim().is_empty() {
                    self.t("menu.status.search_empty")
                } else {
                    self.tf(
                        "menu.status.search_results",
                        &[("query", self.search_input.as_str())],
                    )
                };
            }
            KeyCode::Backspace => {
                self.search_input.pop();
            }
            KeyCode::Char(ch) => self.search_input.push(ch),
            _ => {}
        }
    }

    fn activate_selected(&mut self) -> Result<(), String> {
        let Some(entry) = self.entries().into_iter().nth(self.selected) else {
            return Ok(());
        };
        match entry.kind {
            MenuEntryKind::Navigate(screen) => {
                self.push_navigation_state();
                let screen_title = screen.title(&self.catalog);
                if let Some(index) = target_screen_index(&screen) {
                    self.ensure_target_edit_session(index);
                }
                self.screen = screen;
                self.selected = 0;
                self.sync_selection_to_focusable();
                self.last_status =
                    self.tf("menu.status.opened_screen", &[("screen", &screen_title)]);
            }
            MenuEntryKind::EditField(field) => {
                if is_boolean_toggle_field(&field) {
                    self.last_status = self.t("menu.status.bool_toggle_space_only");
                    return Ok(());
                }
                self.begin_edit(field)?;
            }
            MenuEntryKind::FocusField { screen, field } => {
                self.push_navigation_state();
                if let Some(index) = target_screen_index(&screen) {
                    self.ensure_target_edit_session(index);
                }
                self.screen = screen;
                self.select_field(&field);
                self.last_status = self.tf("menu.status.focused_field", &[("field", &field)]);
            }
            MenuEntryKind::Action(ActionKind::ToggleTokenAccess(_)) => {
                self.last_status = self.t("menu.status.token_access_space_only");
            }
            MenuEntryKind::Action(ActionKind::BindTargetCredentialRef { .. }) => {
                self.last_status = self.t("menu.status.target_credential_picker_space_only");
            }
            MenuEntryKind::Action(action) => self.run_action(action)?,
            MenuEntryKind::Info => {
                self.last_status = entry.description;
            }
        }
        Ok(())
    }

    fn begin_edit(&mut self, field: String) -> Result<(), String> {
        if field == SSH_IMPORT_KEY_NAME_FIELD {
            self.edit_mode = true;
            self.edit_mode_kind = EditModeKind::Text;
            self.edit_field = Some(field);
            self.edit_input = self.ssh_import_draft.key_name.clone();
            self.edit_cursor = self.edit_input.chars().count();
            self.edit_options.clear();
            self.edit_option_selected = 0;
            self.last_status = self.t("menu.status.ssh_import_key_name_prompt");
            return Ok(());
        }
        if field == SSH_IMPORT_LABEL_FIELD {
            self.edit_mode = true;
            self.edit_mode_kind = EditModeKind::Text;
            self.edit_field = Some(field);
            self.edit_input = self.ssh_import_draft.label.clone();
            self.edit_cursor = self.edit_input.chars().count();
            self.edit_options.clear();
            self.edit_option_selected = 0;
            self.last_status = self.t("menu.status.ssh_import_label_prompt");
            return Ok(());
        }
        if field == SSH_IMPORT_SOURCE_PATH_FIELD {
            self.edit_mode = true;
            self.edit_mode_kind = EditModeKind::Text;
            self.edit_field = Some(field);
            self.edit_input = self.ssh_import_draft.source_path.clone();
            self.edit_cursor = self.edit_input.chars().count();
            self.edit_options.clear();
            self.edit_option_selected = 0;
            self.last_status = self.t("menu.status.ssh_import_source_path_prompt");
            return Ok(());
        }
        if field == SSH_IMPORT_PASSPHRASE_FIELD {
            self.edit_mode = true;
            self.edit_mode_kind = EditModeKind::Text;
            self.edit_field = Some(field);
            self.edit_input.clear();
            self.edit_cursor = 0;
            self.edit_options.clear();
            self.edit_option_selected = 0;
            self.last_status = self.t("menu.status.ssh_import_passphrase_prompt");
            return Ok(());
        }
        if field == TOKEN_CREATE_LABEL_FIELD {
            self.edit_mode = true;
            self.edit_mode_kind = EditModeKind::Text;
            self.edit_field = Some(field);
            self.edit_input = self.pending_token_label.clone().unwrap_or_default();
            self.edit_cursor = self.edit_input.chars().count();
            self.edit_options.clear();
            self.edit_option_selected = 0;
            self.last_status = self.t("menu.status.token_create_label_prompt");
            return Ok(());
        }
        if field == TOKEN_CREATE_EXPIRY_MODE_FIELD {
            self.edit_mode = true;
            self.edit_mode_kind = EditModeKind::Choice;
            self.edit_field = Some(field);
            self.edit_options = vec!["long-lived".to_string(), "expires-at-time".to_string()];
            self.edit_option_selected = 0;
            self.edit_input = self
                .edit_options
                .get(self.edit_option_selected)
                .cloned()
                .unwrap_or_default();
            self.edit_cursor = 0;
            self.last_status = self.t("menu.status.token_create_expiry_mode_prompt");
            return Ok(());
        }
        if field == TOKEN_CREATE_EXPIRY_AT_FIELD {
            self.edit_mode = true;
            self.edit_mode_kind = EditModeKind::Text;
            self.edit_field = Some(field);
            let default_expiry = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|duration| duration.as_secs().saturating_add(3600))
                .unwrap_or(3600);
            self.edit_input = default_expiry.to_string();
            self.edit_cursor = self.edit_input.chars().count();
            self.edit_options.clear();
            self.edit_option_selected = 0;
            self.last_status = self.t("menu.status.token_create_expiry_at_prompt");
            return Ok(());
        }
        if let Some(token_id) = field.strip_prefix("__token_label_edit__:") {
            let current_label = self
                .token_rows
                .iter()
                .find(|item| item.token_id == token_id)
                .map(|item| item.label.clone())
                .unwrap_or_default();
            self.edit_mode = true;
            self.edit_mode_kind = EditModeKind::Text;
            self.edit_field = Some(field);
            self.edit_input = current_label;
            self.edit_cursor = self.edit_input.chars().count();
            self.edit_options.clear();
            self.edit_option_selected = 0;
            self.last_status = self.t("menu.status.token_label_edit_prompt");
            return Ok(());
        }
        let Some(value) = field_value(&self.settings, &field) else {
            self.last_status = self.tf("menu.status.read_only", &[("field", &field)]);
            return Ok(());
        };
        self.edit_mode = true;
        self.edit_field = Some(field.clone());
        if let Some(mut options) = field_options(&field) {
            if !options
                .iter()
                .any(|option| option.eq_ignore_ascii_case(&value))
            {
                options.push(value.clone());
            }
            let selected = options
                .iter()
                .position(|option| option.eq_ignore_ascii_case(&value))
                .unwrap_or(0);
            self.edit_mode_kind = EditModeKind::Choice;
            self.edit_options = options;
            self.edit_option_selected = selected;
            self.edit_input = self
                .edit_options
                .get(selected)
                .cloned()
                .unwrap_or_else(|| value.clone());
            self.edit_cursor = 0;
            self.last_status = self.tf("menu.status.selecting", &[("field", &field)]);
        } else {
            self.edit_mode_kind = EditModeKind::Text;
            self.edit_input = value;
            self.edit_cursor = self.edit_input.chars().count();
            self.edit_options.clear();
            self.edit_option_selected = 0;
            self.last_status = self.tf("menu.status.editing", &[("field", &field)]);
        }
        Ok(())
    }

    fn commit_edit(&mut self) -> Result<(), String> {
        let Some(field) = self.edit_field.clone() else {
            return Ok(());
        };
        let value = match self.edit_mode_kind {
            EditModeKind::Text => self.edit_input.clone(),
            EditModeKind::Choice => self
                .edit_options
                .get(self.edit_option_selected)
                .cloned()
                .ok_or_else(|| self.t("menu.error.no_available_option"))?,
        };
        if field == SSH_IMPORT_KEY_NAME_FIELD {
            self.ssh_import_draft.key_name = value.trim().to_string();
            self.reset_edit_state();
            self.last_status = self.t("menu.status.ssh_import_key_name_saved");
            return Ok(());
        }
        if field == SSH_IMPORT_LABEL_FIELD {
            self.ssh_import_draft.label = value.trim().to_string();
            self.reset_edit_state();
            self.last_status = self.t("menu.status.ssh_import_label_saved");
            return Ok(());
        }
        if field == SSH_IMPORT_SOURCE_PATH_FIELD {
            self.ssh_import_draft.source_path = value.trim().to_string();
            self.reset_edit_state();
            self.last_status = self.t("menu.status.ssh_import_source_path_saved");
            return Ok(());
        }
        if field == SSH_IMPORT_PASSPHRASE_FIELD {
            self.ssh_import_draft.passphrase = if value.trim().is_empty() {
                None
            } else {
                Some(value)
            };
            self.reset_edit_state();
            self.last_status = self.t("menu.status.ssh_import_passphrase_captured");
            return Ok(());
        }
        if field == TOKEN_CREATE_LABEL_FIELD {
            let label = self.validate_token_label_input(&value)?;
            self.pending_token_label = Some(label);
            self.reset_edit_state();
            self.begin_edit(TOKEN_CREATE_EXPIRY_MODE_FIELD.to_string())?;
            return Ok(());
        }
        if field == TOKEN_CREATE_EXPIRY_MODE_FIELD {
            let label = self
                .pending_token_label
                .clone()
                .ok_or_else(|| self.t("menu.error.token_label_required"))?;
            self.reset_edit_state();
            if value == "long-lived" {
                self.create_token_with_flow(label, None)?;
            } else {
                self.begin_edit(TOKEN_CREATE_EXPIRY_AT_FIELD.to_string())?;
            }
            return Ok(());
        }
        if field == TOKEN_CREATE_EXPIRY_AT_FIELD {
            let label = self
                .pending_token_label
                .clone()
                .ok_or_else(|| self.t("menu.error.token_label_required"))?;
            let expires_at_unix_sec = value
                .trim()
                .parse::<u64>()
                .map_err(|_| self.t("menu.error.token_expiry_at_invalid"))?;
            let now_unix_sec = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|duration| duration.as_secs())
                .unwrap_or(0);
            if expires_at_unix_sec <= now_unix_sec {
                return Err(self.t("menu.error.token_expiry_at_invalid"));
            }
            self.reset_edit_state();
            self.create_token_with_flow(label, Some(expires_at_unix_sec - now_unix_sec))?;
            return Ok(());
        }
        if let Some(token_id) = field.strip_prefix("__token_label_edit__:") {
            let label = self.validate_token_label_input(&value)?;
            self.reset_edit_state();
            self.update_token_label(token_id, &label)?;
            return Ok(());
        }
        self.apply_edit_value(&field, &value)?;
        self.reset_edit_state();
        self.last_status = self.tf("menu.status.updated", &[("field", &field)]);
        Ok(())
    }

    fn validate_token_label_input(&self, raw: &str) -> Result<String, String> {
        let value = raw.trim();
        if value.is_empty() {
            return Err(self.t("menu.error.token_label_required"));
        }
        if value.len() > 64 {
            return Err(self.t("menu.error.token_label_format_invalid"));
        }
        let mut seen_any = false;
        let mut prev_separator = false;
        for ch in value.chars() {
            if ch.is_ascii_alphanumeric() {
                seen_any = true;
                prev_separator = false;
                continue;
            }
            if (ch == '-' || ch == '_') && seen_any && !prev_separator {
                prev_separator = true;
                continue;
            }
            return Err(self.t("menu.error.token_label_format_invalid"));
        }
        if !seen_any || prev_separator {
            return Err(self.t("menu.error.token_label_format_invalid"));
        }
        Ok(value.to_string())
    }

    fn apply_edit_value(&mut self, field: &str, value: &str) -> Result<(), String> {
        let mut next = self.settings.clone();
        apply_field_edit(&mut next, field, value)?;
        next.validate().map_err(|err| {
            self.tf(
                "menu.error.validate_edited",
                &[("error", &format!("{err:?}"))],
            )
        })?;
        self.settings = next;
        self.catalog = Catalog::load(&self.settings.core.operator_locale).map_err(|err| {
            format!(
                "reload menuconfig catalog failed (locale={}): {err}",
                self.settings.core.operator_locale
            )
        })?;
        if !self.dirty_paths.contains(&field.to_string()) {
            self.dirty_paths.push(field.to_string());
        }
        self.refresh_security_summary();
        Ok(())
    }

    fn reset_edit_state(&mut self) {
        self.edit_mode = false;
        self.edit_mode_kind = EditModeKind::Text;
        self.edit_field = None;
        self.edit_input.clear();
        self.edit_cursor = 0;
        self.edit_options.clear();
        self.edit_option_selected = 0;
    }

    fn cancel_edit(&mut self, status_key: &str) {
        self.reset_edit_state();
        self.last_status = self.t(status_key);
    }

    fn move_edit_option(&mut self, delta: isize) {
        if self.edit_options.is_empty() {
            self.edit_option_selected = 0;
            return;
        }
        let next = self.edit_option_selected as isize + delta;
        self.edit_option_selected =
            next.clamp(0, self.edit_options.len().saturating_sub(1) as isize) as usize;
    }

    fn move_text_cursor(&mut self, delta: isize) {
        let len = self.edit_input.chars().count();
        let next = self.edit_cursor as isize + delta;
        self.edit_cursor = next.clamp(0, len as isize) as usize;
    }

    fn insert_text_char(&mut self, ch: char) {
        let byte_index = char_to_byte_index(&self.edit_input, self.edit_cursor);
        self.edit_input.insert(byte_index, ch);
        self.edit_cursor += 1;
    }

    fn backspace_text_char(&mut self) {
        if self.edit_cursor == 0 {
            return;
        }
        let remove_char_index = self.edit_cursor - 1;
        let start = char_to_byte_index(&self.edit_input, remove_char_index);
        let end = char_to_byte_index(&self.edit_input, self.edit_cursor);
        self.edit_input.drain(start..end);
        self.edit_cursor = remove_char_index;
    }

    fn handle_space_on_selected(&mut self) -> Result<(), String> {
        let Some(entry) = self.entries().into_iter().nth(self.selected) else {
            return Ok(());
        };
        match entry.kind {
            MenuEntryKind::EditField(field) => {
                if is_boolean_toggle_field(&field) {
                    let current =
                        field_value(&self.settings, &field).unwrap_or_else(|| "false".into());
                    let parsed = current
                        .trim()
                        .parse::<bool>()
                        .map_err(|_| self.tf("menu.error.bool_toggle", &[("field", &field)]))?;
                    let toggled = (!parsed).to_string();
                    self.apply_edit_value(&field, &toggled)?;
                    self.last_status = self.tf(
                        "menu.status.toggle",
                        &[("field", &field), ("value", &toggled)],
                    );
                } else if field_options(&field).is_some() {
                    self.begin_edit(field)?;
                }
            }
            MenuEntryKind::Action(ActionKind::ToggleTokenAccess(token_id)) => {
                self.run_action(ActionKind::ToggleTokenAccess(token_id))?;
            }
            MenuEntryKind::Action(ActionKind::BindTargetCredentialRef {
                target_index,
                credential_ref,
            }) => {
                self.run_action(ActionKind::BindTargetCredentialRef {
                    target_index,
                    credential_ref,
                })?;
            }
            _ => {}
        }
        Ok(())
    }

    fn save(&mut self) -> Result<(), String> {
        self.settings.validate().map_err(|err| {
            self.tf(
                "menu.error.validate_settings",
                &[("error", &format!("{err:?}"))],
            )
        })?;
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                self.tf(
                    "menu.error.create_config_parent",
                    &[
                        ("path", &parent.display().to_string()),
                        ("error", &err.to_string()),
                    ],
                )
            })?;
        }
        fs::write(&self.config_path, self.settings.to_toml_string()).map_err(|err| {
            self.tf(
                "menu.error.persist_menuconfig",
                &[
                    ("path", &self.config_path.display().to_string()),
                    ("error", &err.to_string()),
                ],
            )
        })?;
        self.last_apply_strategy = Some("restart_required".into());
        self.dirty_paths.clear();
        self.last_status = self.tf(
            "menu.status.saved",
            &[("path", &self.config_path.display().to_string())],
        );
        self.record_session_event("menuconfig.save", "succeeded", "ok", RuntimeLogLevel::Info, None);
        Ok(())
    }

    fn begin_unlock_flow(&mut self) {
        if self.security_summary.lock_state == "unlocked" {
            self.last_status = self.t("menu.status.vault_already_unlocked");
            self.record_session_event(
                "vault.unlock",
                "ignored",
                "already-unlocked",
                RuntimeLogLevel::Debug,
                None,
            );
            return;
        }
        if self.unlock_worker.is_some() {
            self.last_status = self.t("menu.status.vault_unlock_inflight");
            self.record_authorization_event(
                self.unlock_flow_id.as_deref().unwrap_or("flow-inflight"),
                "Unlock Vault",
                OP_VAULT_UNLOCK,
                "joined",
                "inflight",
                "joined",
                None,
                None,
                None,
            );
            return;
        }
        let flow_id = next_local_authorization_flow_id(OP_VAULT_UNLOCK);
        self.unlock_flow_id = Some(flow_id.clone());
        self.unlock_flow_state = UnlockFlowState::Waiting;
        self.unlock_flow_pending = true;
        self.last_status = self.t("menu.status.vault_unlock_waiting");
        self.record_authorization_event(
            &flow_id,
            "Unlock Vault",
            OP_VAULT_UNLOCK,
            "requested",
            "pending",
            "leader",
            None,
            None,
            None,
        );
        self.record_session_event_with_flow(
            &flow_id,
            "unlock.worker",
            "requested",
            "pending",
            RuntimeLogLevel::Debug,
            None,
        );
    }

    fn start_unlock_worker(&mut self) {
        let flow_id = self
            .unlock_flow_id
            .clone()
            .unwrap_or_else(|| next_local_authorization_flow_id(OP_VAULT_UNLOCK));
        self.unlock_flow_id = Some(flow_id.clone());
        let mut router = match self.ensure_vault_router_loaded() {
            Ok(_) => self
                .vault_router
                .take()
                .expect("vault router should exist after ensure_vault_router_loaded"),
            Err(err) => {
                self.unlock_flow_state = UnlockFlowState::Failed {
                    reason: err.clone(),
                };
                self.last_status = self.tf(
                    "menu.status.vault_unlock_failed",
                    &[
                        ("reason", &err),
                        ("methods", &ordered_unlock_methods(&self.settings).join(",")),
                    ],
                );
                self.record_authorization_event(
                    &flow_id,
                    "Unlock Vault",
                    OP_VAULT_UNLOCK,
                    "failed",
                    "error",
                    "leader",
                    Some("router-load-failed"),
                    None,
                    None,
                );
                return;
            }
        };
        self.record_authorization_event(
            &flow_id,
            "Unlock Vault",
            OP_VAULT_UNLOCK,
            "started",
            "pending",
            "leader",
            None,
            None,
            None,
        );
        self.record_session_event_with_flow(
            &flow_id,
            "unlock.worker",
            "started",
            "pending",
            RuntimeLogLevel::Debug,
            None,
        );
        let settings = self.settings.clone();
        let (tx, rx) = mpsc::channel::<UnlockWorkerOutcome>();
        thread::Builder::new()
            .name("menuconfig-unlock-worker".into())
            .spawn(move || {
                let methods = ordered_unlock_methods(&settings);
                let mut last_error = None::<String>;
                let mut last_verified_event = None::<VerifiedOsNativeEvent>;
                let mut attempted_os_native = false;
                for method in methods {
                    if method.as_str() != "os-native" {
                        continue;
                    }
                    attempted_os_native = true;
                    match router.unlock_with_os_native_verified() {
                        Ok(()) => {
                            let verified_event = take_last_verified_os_native_event();
                            let _ = tx.send(UnlockWorkerOutcome {
                                router,
                                result: Ok(()),
                                verified_event,
                            });
                            return;
                        }
                        Err(err) => {
                            last_verified_event = take_last_verified_os_native_event();
                            last_error = Some(format!("{err:?}"));
                        }
                    }
                }
                if !attempted_os_native {
                    last_error = Some(MENU_UNLOCK_REASON_REQUIRES_OS_NATIVE.to_string());
                }
                let _ = tx.send(UnlockWorkerOutcome {
                    router,
                    result: Err(
                        last_error.unwrap_or_else(|| "No allowed unlock method succeeded.".into())
                    ),
                    verified_event: last_verified_event,
                });
            })
            .expect("spawn menuconfig unlock worker");
        self.unlock_worker = Some(UnlockWorkerHandle {
            receiver: rx,
            cancel_requested: false,
        });
    }

    fn poll_unlock_worker(&mut self) {
        let mut completed = None::<(UnlockWorkerOutcome, bool)>;
        let mut disconnected = false;
        if let Some(worker) = self.unlock_worker.as_mut() {
            match worker.receiver.try_recv() {
                Ok(outcome) => {
                    completed = Some((outcome, worker.cancel_requested));
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    disconnected = true;
                }
            }
        }

        if let Some((mut outcome, cancel_requested)) = completed {
            self.unlock_worker = None;
            if cancel_requested {
                let _ = outcome.router.lock_vault("menuconfig unlock cancelled");
                self.vault_router = Some(outcome.router);
                self.refresh_security_summary();
                self.last_status = self.t("menu.status.vault_unlock_cancelled");
                let flow_id = self
                    .unlock_flow_id
                    .as_deref()
                    .unwrap_or("flow-unlock-cancelled");
                self.record_authorization_event(
                    flow_id,
                    "Unlock Vault",
                    OP_VAULT_UNLOCK,
                    "cancelled",
                    "cancelled",
                    "leader",
                    None,
                    None,
                    None,
                );
                self.record_session_event_with_flow(
                    flow_id,
                    "unlock.worker",
                    "cancelled",
                    "cancelled",
                    RuntimeLogLevel::Debug,
                    None,
                );
                self.unlock_flow_id = None;
                return;
            }
            self.vault_router = Some(outcome.router);
            match outcome.result {
                Ok(()) => {
                    self.refresh_security_summary();
                    self.unlock_flow_state = UnlockFlowState::Success;
                    self.last_status = self.t("menu.status.vault_unlocked_os_native");
                    let flow_id = self
                        .unlock_flow_id
                        .as_deref()
                        .unwrap_or("flow-unlock-succeeded");
                    let verified = outcome.verified_event.as_ref();
                    let dedupe_state = verified
                        .as_ref()
                        .map(|event| event.dedupe_state.as_str())
                        .unwrap_or("leader");
                    self.record_authorization_event(
                        flow_id,
                        "Unlock Vault",
                        OP_VAULT_UNLOCK,
                        "succeeded",
                        "ok",
                        dedupe_state,
                        None,
                        None,
                        None,
                    );
                    self.record_session_event_with_flow(
                        flow_id,
                        "unlock.worker",
                        "completed",
                        "ok",
                        RuntimeLogLevel::Debug,
                        None,
                    );
                }
                Err(reason) => {
                    let reason = if reason == MENU_UNLOCK_REASON_REQUIRES_OS_NATIVE {
                        self.t("menu.error.menu_unlock_requires_os_native")
                    } else {
                        reason
                    };
                    self.unlock_flow_state = UnlockFlowState::Failed {
                        reason: reason.clone(),
                    };
                    let methods = ordered_unlock_methods(&self.settings);
                    self.last_status = self.tf(
                        "menu.status.vault_unlock_failed",
                        &[("reason", &reason), ("methods", &methods.join(","))],
                    );
                    let flow_id = self
                        .unlock_flow_id
                        .as_deref()
                        .unwrap_or("flow-unlock-failed");
                    let verified = outcome.verified_event.as_ref();
                    let dedupe_state = verified
                        .as_ref()
                        .map(|event| event.dedupe_state.as_str())
                        .unwrap_or("leader");
                    self.record_authorization_event(
                        flow_id,
                        "Unlock Vault",
                        OP_VAULT_UNLOCK,
                        "failed",
                        "error",
                        dedupe_state,
                        Some("unlock-failed"),
                        None,
                        None,
                    );
                    self.record_session_event_with_flow(
                        flow_id,
                        "unlock.worker",
                        "failed",
                        "error",
                        RuntimeLogLevel::Debug,
                        Some("unlock-failed"),
                    );
                }
            }
            self.unlock_flow_id = None;
            return;
        }

        if disconnected {
            self.unlock_worker = None;
            self.unlock_flow_state = UnlockFlowState::Failed {
                reason: self.t("menu.error.unlock_worker_disconnected"),
            };
            let methods = ordered_unlock_methods(&self.settings);
            self.last_status = self.tf(
                "menu.status.vault_unlock_failed",
                &[
                    ("reason", &self.t("menu.error.unlock_worker_disconnected")),
                    ("methods", &methods.join(",")),
                ],
            );
            let flow_id = self
                .unlock_flow_id
                .as_deref()
                .unwrap_or("flow-unlock-disconnected");
            self.record_authorization_event(
                flow_id,
                "Unlock Vault",
                OP_VAULT_UNLOCK,
                "failed",
                "error",
                "leader",
                Some("unlock-worker-disconnected"),
                None,
                None,
            );
            self.record_session_event_with_flow(
                flow_id,
                "unlock.worker",
                "failed",
                "error",
                RuntimeLogLevel::Debug,
                Some("unlock-worker-disconnected"),
            );
            self.unlock_flow_id = None;
        }
    }

    #[cfg(test)]
    fn execute_pending_unlock_flow(&mut self) {
        let methods = ordered_unlock_methods(&self.settings);
        let mut last_error = None::<String>;
        let mut attempted_os_native = false;
        for method in methods {
            match method.as_str() {
                "os-native" => {
                    attempted_os_native = true;
                    let unlock_result = self.ensure_vault_router_loaded().and_then(|router| {
                        router
                            .unlock_with_os_native_verified()
                            .map_err(|err| format!("{err:?}"))
                    });
                    match unlock_result {
                        Ok(()) => {
                            self.refresh_security_summary();
                            self.unlock_flow_state = UnlockFlowState::Success;
                            self.last_status = self.t("menu.status.vault_unlocked_os_native");
                            return;
                        }
                        Err(err) => last_error = Some(err),
                    }
                }
                _ => continue,
            }
        }
        if !attempted_os_native {
            last_error = Some(self.t("menu.error.menu_unlock_requires_os_native"));
        }
        let reason = last_error.unwrap_or_else(|| self.t("menu.error.no_unlock_method_succeeded"));
        self.unlock_flow_state = UnlockFlowState::Failed {
            reason: reason.clone(),
        };
        let methods = ordered_unlock_methods(&self.settings);
        self.last_status = self.tf(
            "menu.status.vault_unlock_failed",
            &[("reason", &reason), ("methods", &methods.join(","))],
        );
    }

    fn handle_unlock_flow_key(&mut self, code: KeyCode) {
        match (&self.unlock_flow_state, code) {
            (UnlockFlowState::Waiting, KeyCode::Esc) => {
                self.unlock_flow_state = UnlockFlowState::Idle;
                if let Some(worker) = self.unlock_worker.as_mut() {
                    worker.cancel_requested = true;
                }
                self.last_status = self.t("menu.status.vault_unlock_cancel_requested");
            }
            (UnlockFlowState::Waiting, _) => {}
            (UnlockFlowState::Success, KeyCode::Enter)
            | (UnlockFlowState::Success, KeyCode::Esc)
            | (UnlockFlowState::Success, KeyCode::Char(' ')) => {
                self.unlock_flow_state = UnlockFlowState::Idle;
                self.last_status = self.t("menu.status.vault_unlocked_notice");
            }
            (UnlockFlowState::Failed { .. }, KeyCode::Enter)
            | (UnlockFlowState::Failed { .. }, KeyCode::Esc)
            | (UnlockFlowState::Failed { .. }, KeyCode::Char(' ')) => {
                self.unlock_flow_state = UnlockFlowState::Idle;
            }
            _ => {}
        }
    }

    fn ensure_vault_router_loaded(&mut self) -> Result<&mut SecretVaultRouter, String> {
        if self.vault_router.is_none() {
            self.vault_router = Some(load_vault_router(&self.settings)?);
        }
        self.vault_router
            .as_mut()
            .ok_or_else(|| "vault router is unavailable".to_string())
    }

    fn refresh_security_summary(&mut self) {
        if let Some(router) = self.vault_router.as_mut() {
            self.security_summary = build_security_summary_from_router(&self.settings, router);
            self.ssh_key_rows = router
                .list_secret_summaries()
                .into_iter()
                .filter(|item| item.kind.eq_ignore_ascii_case("ssh-private-key"))
                .map(|item| SshKeyManagementRow {
                    credential_ref: item.reference,
                    label: item.label,
                    status: item.status.as_str().to_string(),
                    active_version: item
                        .active_version_id
                        .unwrap_or_else(|| "unknown".to_string()),
                })
                .collect();
            if self.security_summary.lock_state == "unlocked" {
                self.token_rows = router
                    .list_agent_tokens()
                    .into_iter()
                    .map(|item| TokenManagementRow {
                        token_id: item.token_id,
                        label: item.label,
                        token_fingerprint: item.token_fingerprint,
                        status: item.status.as_str().to_string(),
                        expires_at: item.expires_at,
                        revoked_at: item.revoked_at,
                        revoke_reason: item.revoke_reason,
                    })
                    .collect();
            } else {
                self.token_rows.clear();
            }
        } else {
            self.security_summary = build_security_summary_passive(&self.settings);
            self.token_rows.clear();
            self.ssh_key_rows.clear();
        }
    }

    fn execute_ssh_key_import(&mut self) -> Result<(), String> {
        let flow_id = next_local_authorization_flow_id(OP_SSH_KEY_IMPORT);
        if self.vault_router.is_none() {
            let _ = self.ensure_vault_router_loaded()?;
        }
        self.refresh_security_summary();
        if self.security_summary.lock_state != "unlocked" {
            self.last_status = self.t("menu.status.vault_locked_for_ssh_key_action");
            self.record_authorization_event(
                &flow_id,
                "Import SSH Key",
                OP_SSH_KEY_IMPORT,
                "failed",
                "locked",
                "not-applicable",
                Some("vault-locked"),
                None,
                None,
            );
            return Ok(());
        }
        self.record_authorization_event(
            &flow_id,
            "Import SSH Key",
            OP_SSH_KEY_IMPORT,
            "started",
            "pending",
            "leader",
            None,
            None,
            None,
        );

        let key_name = self.ssh_import_draft.key_name.trim().to_string();
        if key_name.is_empty() {
            self.last_status = self.t("menu.error.ssh_import_key_name_required");
            self.record_authorization_event(
                &flow_id,
                "Import SSH Key",
                OP_SSH_KEY_IMPORT,
                "failed",
                "invalid-input",
                "leader",
                Some("missing-key-name"),
                None,
                None,
            );
            return Ok(());
        }
        let canonical_ref = canonical_ssh_private_key_ref_from_key_name(&key_name)
            .map_err(|_| self.t("menu.error.ssh_import_key_name_invalid"));
        let canonical_ref = match canonical_ref {
            Ok(value) => value,
            Err(message) => {
                self.last_status = message;
                self.record_authorization_event(
                    &flow_id,
                    "Import SSH Key",
                    OP_SSH_KEY_IMPORT,
                    "failed",
                    "invalid-input",
                    "leader",
                    Some("invalid-key-name"),
                    None,
                    None,
                );
                return Ok(());
            }
        };
        let source_path = self.ssh_import_draft.source_path.trim().to_string();
        if source_path.is_empty() {
            self.last_status = self.t("menu.error.ssh_import_source_path_required");
            self.record_authorization_event(
                &flow_id,
                "Import SSH Key",
                OP_SSH_KEY_IMPORT,
                "failed",
                "invalid-input",
                "leader",
                Some("missing-source-path"),
                None,
                None,
            );
            return Ok(());
        }
        let key_material = fs::read_to_string(&source_path).map_err(|err| {
            self.tf(
                "menu.error.ssh_import_read_source_failed",
                &[("path", source_path.as_str()), ("error", &err.to_string())],
            )
        });
        let key_material = match key_material {
            Ok(value) => value,
            Err(message) => {
                self.last_status = message;
                self.record_authorization_event(
                    &flow_id,
                    "Import SSH Key",
                    OP_SSH_KEY_IMPORT,
                    "failed",
                    "error",
                    "leader",
                    Some("source-read-failed"),
                    None,
                    None,
                );
                return Ok(());
            }
        };
        if self
            .ssh_key_rows
            .iter()
            .any(|item| item.credential_ref == canonical_ref)
        {
            self.last_status = self.tf(
                "menu.error.ssh_import_key_already_exists",
                &[("ref", canonical_ref.as_str())],
            );
            self.record_authorization_event(
                &flow_id,
                "Import SSH Key",
                OP_SSH_KEY_IMPORT,
                "failed",
                "duplicate",
                "leader",
                Some("duplicate-credential-ref"),
                None,
                Some(canonical_ref.as_str()),
            );
            return Ok(());
        }
        let label = self.ssh_import_draft.label.trim();
        let request = TrustedLocalSshKeyImportRequest {
            key_name: key_name.clone(),
            label: if label.is_empty() {
                None
            } else {
                Some(label.to_string())
            },
            private_key: bridgingio_secrets::SecretBytes::from_utf8(key_material),
            passphrase: self.ssh_import_draft.passphrase.clone(),
            imported_by: "menuconfig:local-operator".to_string(),
            rotation_reason: None,
        };
        let result = self
            .ensure_vault_router_loaded()?
            .import_ssh_private_key_trusted_local(request);
        match result {
            Ok(imported) => {
                self.ssh_import_last_result = Some(imported.clone());
                self.ssh_import_draft.passphrase = None;
                self.ssh_import_draft.source_path.clear();
                let bound_target = self.ssh_import_draft.bind_target_index;
                if let Some(index) = bound_target {
                    apply_target_field_edit(
                        &mut self.settings,
                        &format!("targets[{index}].credential_ref"),
                        &imported.credential_ref,
                    )?;
                    if !self
                        .dirty_paths
                        .contains(&format!("targets[{index}].credential_ref"))
                    {
                        self.dirty_paths
                            .push(format!("targets[{index}].credential_ref"));
                    }
                    self.ssh_import_draft.bind_target_index = None;
                    self.screen = Screen::TargetCredentialSource(index);
                    self.selected = 0;
                    self.sync_selection_to_focusable();
                    self.last_status = self.tf(
                        "menu.status.inline_ssh_import_bound_target",
                        &[("ref", imported.credential_ref.as_str())],
                    );
                } else {
                    self.screen = Screen::SshKeyDetail(imported.credential_ref.clone());
                    self.selected = 0;
                    self.sync_selection_to_focusable();
                    self.last_status = self.tf(
                        "menu.status.ssh_import_completed",
                        &[("ref", imported.credential_ref.as_str())],
                    );
                }
                self.refresh_security_summary();
                self.record_authorization_event(
                    &flow_id,
                    "Import SSH Key",
                    OP_SSH_KEY_IMPORT,
                    "succeeded",
                    "ok",
                    "leader",
                    None,
                    None,
                    Some(imported.credential_ref.as_str()),
                );
                Ok(())
            }
            Err(VaultError::SshKeyPassphraseRequired(_)) => {
                self.last_status = self.t("menu.status.ssh_import_passphrase_required");
                self.begin_edit(SSH_IMPORT_PASSPHRASE_FIELD.to_string())?;
                self.record_authorization_event(
                    &flow_id,
                    "Import SSH Key",
                    OP_SSH_KEY_IMPORT,
                    "failed",
                    "passphrase-required",
                    "leader",
                    Some("passphrase-required"),
                    None,
                    Some(canonical_ref.as_str()),
                );
                Ok(())
            }
            Err(err) => {
                self.last_status = format!("import ssh key failed: {err:?}");
                self.record_authorization_event(
                    &flow_id,
                    "Import SSH Key",
                    OP_SSH_KEY_IMPORT,
                    "failed",
                    "error",
                    "leader",
                    Some("import-failed"),
                    None,
                    Some(canonical_ref.as_str()),
                );
                Ok(())
            }
        }
    }

    fn run_action(&mut self, action: ActionKind) -> Result<(), String> {
        match action {
            ActionKind::OpenAddTarget => {
                self.push_navigation_state();
                self.screen = Screen::TargetAddMode;
                self.selected = 0;
                self.sync_selection_to_focusable();
                self.last_status = self.t("menu.status.opened_add_target");
            }
            ActionKind::ChoosePlainTargetMode => {
                self.push_navigation_state();
                self.screen = Screen::TargetAddTypePlain;
                self.selected = 0;
                self.sync_selection_to_focusable();
                self.last_status = self.t("menu.status.opened_add_target_plain");
            }
            ActionKind::ChooseSensitiveTargetMode => {
                if self.security_summary.lock_state != "unlocked" {
                    self.last_status = self.t("menu.status.vault_locked_for_sensitive_target");
                    return Ok(());
                }
                self.push_navigation_state();
                self.screen = Screen::TargetAddTypeSensitive;
                self.selected = 0;
                self.sync_selection_to_focusable();
                self.last_status = self.t("menu.status.opened_add_target_sensitive");
            }
            ActionKind::AddSshTarget => {
                self.confirm_action = Some(ConfirmAction::ConfirmPlainSshRisk);
                self.confirm_selected = 0;
                self.last_status = self.t("menu.status.plain_ssh_risk_confirm_pending");
            }
            ActionKind::AddAdbTarget => {
                let index = self.settings.targets.len();
                self.settings.targets.push(default_adb_target(index));
                self.push_navigation_state();
                self.start_target_edit_session(index, true);
                self.screen = Screen::TargetEditor(index);
                self.selected = 0;
                self.sync_selection_to_focusable();
                self.last_status = self.t("menu.status.added_adb_target");
            }
            ActionKind::OpenSshKeyImport => {
                if self.vault_router.is_none() {
                    let _ = self.ensure_vault_router_loaded()?;
                }
                self.refresh_security_summary();
                if self.security_summary.lock_state != "unlocked" {
                    self.last_status = self.t("menu.status.vault_locked_for_ssh_key_action");
                    return Ok(());
                }
                self.ssh_import_draft = SshKeyImportDraft::default();
                self.ssh_import_last_result = None;
                self.push_navigation_state();
                self.screen = Screen::SshKeyImport;
                self.selected = 0;
                self.sync_selection_to_focusable();
                self.last_status = self.t("menu.status.opened_ssh_key_import");
            }
            ActionKind::ExecuteSshKeyImport => {
                self.execute_ssh_key_import()?;
            }
            ActionKind::OpenSshKeyManagement => {
                if self.vault_router.is_none() {
                    let _ = self.ensure_vault_router_loaded()?;
                }
                self.refresh_security_summary();
                if self.security_summary.lock_state != "unlocked" {
                    self.last_status = self.t("menu.status.vault_locked_for_ssh_key_action");
                    return Ok(());
                }
                self.push_navigation_state();
                self.screen = Screen::SshKeyManagement;
                self.selected = 0;
                self.sync_selection_to_focusable();
                self.last_status = self.t("menu.status.opened_ssh_key_management");
            }
            ActionKind::OpenSshKeyDetail(reference) => {
                self.push_navigation_state();
                self.screen = Screen::SshKeyDetail(reference);
                self.selected = 0;
                self.sync_selection_to_focusable();
                self.last_status = self.t("menu.status.opened_ssh_key_detail");
            }
            ActionKind::DeleteSshKey(reference) => {
                if self.vault_router.is_none() {
                    let _ = self.ensure_vault_router_loaded()?;
                }
                self.refresh_security_summary();
                if self.security_summary.lock_state != "unlocked" {
                    self.last_status = self.t("menu.status.vault_locked_for_ssh_key_action");
                    return Ok(());
                }
                let flow_id = next_local_authorization_flow_id(OP_SSH_KEY_DELETE);
                self.confirm_action = Some(ConfirmAction::DeleteSshKey(reference));
                self.confirm_flow_id = Some(flow_id.clone());
                self.confirm_selected = 0;
                self.last_status = self.t("menu.status.ssh_key_delete_confirm_pending");
                self.record_authorization_event(
                    &flow_id,
                    "Delete SSH Key",
                    OP_SSH_KEY_DELETE,
                    "requested",
                    "pending",
                    "not-applicable",
                    None,
                    None,
                    None,
                );
            }
            ActionKind::OpenTargetCredentialSource(index) => {
                let Some(target) = self.settings.targets.get(index) else {
                    self.last_status = self.t("menu.error.target_not_found");
                    return Ok(());
                };
                if !matches!(target.kind, TargetKind::Ssh) {
                    self.last_status = self.t("menu.status.target_credential_source_ssh_only");
                    return Ok(());
                }
                if is_sensitive_target(target) && self.security_summary.lock_state != "unlocked" {
                    self.last_status = self.t("menu.status.vault_locked_for_sensitive_target");
                    return Ok(());
                }
                self.ensure_target_edit_session(index);
                self.push_navigation_state();
                self.screen = Screen::TargetCredentialSource(index);
                self.selected = 0;
                self.sync_selection_to_focusable();
                self.last_status = self.t("menu.status.opened_target_credential_source");
            }
            ActionKind::OpenTargetCredentialPicker(index) => {
                if self.vault_router.is_none() {
                    let _ = self.ensure_vault_router_loaded()?;
                }
                self.refresh_security_summary();
                if self.security_summary.lock_state != "unlocked" {
                    self.last_status = self.t("menu.status.vault_locked_for_ssh_key_action");
                    return Ok(());
                }
                self.ensure_target_edit_session(index);
                self.push_navigation_state();
                self.screen = Screen::TargetCredentialPicker(index);
                self.selected = 0;
                self.sync_selection_to_focusable();
                self.last_status = self.t("menu.status.opened_target_credential_picker");
            }
            ActionKind::ApplyTarget(index) => {
                let Some(current) = self.settings.targets.get(index).cloned() else {
                    self.last_status = self.t("menu.error.target_not_found");
                    return Ok(());
                };
                let mut created = false;
                if let Some(session) = self.target_edit_session.as_mut() {
                    if session.index == index {
                        created = session.is_new;
                        session.is_new = false;
                        session.baseline = Some(current);
                    }
                } else {
                    self.target_edit_session = Some(TargetEditSession {
                        index,
                        is_new: false,
                        baseline: Some(current),
                    });
                }
                self.last_status = if created {
                    self.t("menu.status.target_created_applied")
                } else {
                    self.t("menu.status.target_changes_applied")
                };
            }
            ActionKind::BindTargetCredentialRef {
                target_index,
                credential_ref,
            } => {
                apply_target_field_edit(
                    &mut self.settings,
                    &format!("targets[{target_index}].credential_ref"),
                    &credential_ref,
                )?;
                if !self
                    .dirty_paths
                    .contains(&format!("targets[{target_index}].credential_ref"))
                {
                    self.dirty_paths
                        .push(format!("targets[{target_index}].credential_ref"));
                }
                self.last_status = self.tf(
                    "menu.status.bound_target_credential_ref",
                    &[("ref", credential_ref.as_str())],
                );
            }
            ActionKind::ClearTargetCredentialRef(index) => {
                apply_target_field_edit(
                    &mut self.settings,
                    &format!("targets[{index}].credential_ref"),
                    "",
                )?;
                if !self
                    .dirty_paths
                    .contains(&format!("targets[{index}].credential_ref"))
                {
                    self.dirty_paths
                        .push(format!("targets[{index}].credential_ref"));
                }
                self.last_status = self.t("menu.status.cleared_target_credential_ref");
            }
            ActionKind::EditTargetCredentialRef(index) => {
                self.begin_edit(format!("targets[{index}].credential_ref"))?;
            }
            ActionKind::ImportLocalSshKeyIntoVault(index) => {
                if self.vault_router.is_none() {
                    let _ = self.ensure_vault_router_loaded()?;
                }
                self.refresh_security_summary();
                if self.security_summary.lock_state != "unlocked" {
                    self.last_status = self.t("menu.status.vault_locked_for_ssh_key_action");
                    return Ok(());
                }
                self.ssh_import_draft = SshKeyImportDraft {
                    bind_target_index: Some(index),
                    ..SshKeyImportDraft::default()
                };
                self.ssh_import_last_result = None;
                self.push_navigation_state();
                self.screen = Screen::SshKeyImport;
                self.selected = 0;
                self.sync_selection_to_focusable();
                self.last_status = self.t("menu.status.opened_inline_ssh_import_for_target");
            }
            ActionKind::AddSensitiveSshTarget => {
                if self.security_summary.lock_state != "unlocked" {
                    self.last_status = self.t("menu.status.vault_locked_for_sensitive_target");
                    return Ok(());
                }
                let index = self.settings.targets.len();
                self.settings
                    .targets
                    .push(default_sensitive_ssh_target(index));
                self.push_navigation_state();
                self.start_target_edit_session(index, true);
                self.screen = Screen::TargetEditor(index);
                self.selected = 0;
                self.sync_selection_to_focusable();
                self.last_status = self.t("menu.status.added_sensitive_ssh_target");
            }
            ActionKind::AddSensitiveAdbTarget => {
                if self.security_summary.lock_state != "unlocked" {
                    self.last_status = self.t("menu.status.vault_locked_for_sensitive_target");
                    return Ok(());
                }
                let index = self.settings.targets.len();
                self.settings
                    .targets
                    .push(default_sensitive_adb_target(index));
                self.push_navigation_state();
                self.start_target_edit_session(index, true);
                self.screen = Screen::TargetEditor(index);
                self.selected = 0;
                self.sync_selection_to_focusable();
                self.last_status = self.t("menu.status.added_sensitive_adb_target");
            }
            ActionKind::UnlockVault => {
                self.begin_unlock_flow();
            }
            ActionKind::InitVault => {
                let router = self.ensure_vault_router_loaded()?;
                router
                    .init_vault_store()
                    .map_err(|err| format!("init vault failed: {err:?}"))?;
                self.refresh_security_summary();
                self.last_status = self.t("menu.status.vault_initialized");
            }
            ActionKind::DeleteVault => {
                let flow_id = next_local_authorization_flow_id(OP_VAULT_DELETE);
                self.confirm_action = Some(ConfirmAction::DeleteVault);
                self.confirm_flow_id = Some(flow_id.clone());
                self.confirm_selected = 0;
                self.last_status = self.t("menu.status.vault_delete_confirm_pending");
                self.record_authorization_event(
                    &flow_id,
                    "Delete Vault",
                    OP_VAULT_DELETE,
                    "requested",
                    "pending",
                    "not-applicable",
                    None,
                    None,
                    None,
                );
            }
            ActionKind::CreateToken => {
                if self.vault_router.is_none() {
                    let _ = self.ensure_vault_router_loaded()?;
                }
                self.refresh_security_summary();
                if self.security_summary.lock_state != "unlocked" {
                    self.last_status = self.t("menu.status.vault_locked_for_token_action");
                    let flow_id = next_local_authorization_flow_id(OP_AUTH_TOKEN_CREATE);
                    self.record_authorization_event(
                        &flow_id,
                        "Create Token",
                        OP_AUTH_TOKEN_CREATE,
                        "failed",
                        "locked",
                        "not-applicable",
                        Some("vault-locked"),
                        None,
                        None,
                    );
                    return Ok(());
                }
                let flow_id = next_local_authorization_flow_id(OP_AUTH_TOKEN_CREATE);
                self.record_authorization_event(
                    &flow_id,
                    "Create Token",
                    OP_AUTH_TOKEN_CREATE,
                    "requested",
                    "pending",
                    "leader",
                    None,
                    None,
                    None,
                );
                self.pending_token_label = None;
                self.begin_edit(TOKEN_CREATE_LABEL_FIELD.to_string())?;
            }
            ActionKind::OpenTokenManagement => {
                if self.vault_router.is_none() {
                    let _ = self.ensure_vault_router_loaded()?;
                }
                self.refresh_security_summary();
                if self.security_summary.lock_state != "unlocked" {
                    self.last_status = self.t("menu.status.vault_locked_for_token_action");
                    return Ok(());
                }
                self.push_navigation_state();
                self.screen = Screen::TokenManagement;
                self.selected = 0;
                self.sync_selection_to_focusable();
                self.last_status = self.t("menu.status.opened_token_management");
            }
            ActionKind::OpenTokenDetail(token_id) => {
                self.push_navigation_state();
                self.screen = Screen::TokenDetail(token_id);
                self.selected = 0;
                self.sync_selection_to_focusable();
                self.last_status = self.t("menu.status.opened_token_detail");
            }
            ActionKind::EditTokenLabel(token_id) => {
                self.begin_edit(format!("__token_label_edit__:{token_id}"))?;
            }
            ActionKind::ToggleTokenAccess(token_id) => {
                let token = self
                    .token_rows
                    .iter()
                    .find(|item| item.token_id == token_id)
                    .cloned()
                    .ok_or_else(|| self.t("menu.error.token_not_found"))?;
                let status = token.status.to_ascii_lowercase();
                if status == "active" {
                    self.update_token_access(&token_id, false)?;
                } else if status == "disabled" {
                    self.update_token_access(&token_id, true)?;
                } else {
                    self.last_status = self.t("menu.status.token_access_toggle_unavailable");
                }
            }
            ActionKind::OpenTokenPermissions(_token_id) => {
                self.last_status = self.t("menu.status.token_permissions_placeholder");
            }
            ActionKind::RevokeToken(token_id) => {
                self.confirm_action = Some(ConfirmAction::RevokeToken(token_id));
                self.confirm_selected = 0;
                self.last_status = self.t("menu.status.token_revoke_confirm_pending");
            }
            ActionKind::DeleteToken(token_id) => {
                let flow_id = next_local_authorization_flow_id(OP_AUTH_TOKEN_DELETE);
                self.confirm_action = Some(ConfirmAction::DeleteToken(token_id));
                self.confirm_flow_id = Some(flow_id.clone());
                self.confirm_selected = 0;
                self.last_status = self.t("menu.status.token_delete_confirm_pending");
                self.record_authorization_event(
                    &flow_id,
                    "Delete Token",
                    OP_AUTH_TOKEN_DELETE,
                    "requested",
                    "pending",
                    "not-applicable",
                    None,
                    None,
                    None,
                );
            }
            ActionKind::DeleteTarget(index) => {
                let sensitive = self
                    .settings
                    .targets
                    .get(index)
                    .map(is_sensitive_target)
                    .unwrap_or(false);
                if sensitive && self.security_summary.lock_state != "unlocked" {
                    self.last_status = self.t("menu.status.vault_locked_for_sensitive_target");
                    return Ok(());
                }
                self.confirm_action = Some(ConfirmAction::DeleteTarget(index));
                self.confirm_selected = 0;
                self.last_status = self.t("menu.status.target_delete_confirm_pending");
            }
        }
        Ok(())
    }

    fn handle_token_reveal_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Enter | KeyCode::Esc | KeyCode::Char(' ') => {
                self.token_reveal = None;
                self.last_status = self.t("menu.status.token_reveal_closed");
            }
            _ => {}
        }
    }

    fn handle_confirm_key(&mut self, code: KeyCode) -> Result<(), String> {
        match code {
            KeyCode::Left => {
                let next = self.confirm_selected as isize - 1;
                self.confirm_selected = next.clamp(0, 1) as usize;
            }
            KeyCode::Right => {
                let next = self.confirm_selected as isize + 1;
                self.confirm_selected = next.clamp(0, 1) as usize;
            }
            KeyCode::Esc => {
                if let (Some(flow_id), Some(action)) =
                    (self.confirm_flow_id.as_deref(), self.confirm_action.as_ref())
                {
                    match action {
                        ConfirmAction::DeleteVault => self.record_authorization_event(
                            flow_id,
                            "Delete Vault",
                            OP_VAULT_DELETE,
                            "cancelled",
                            "cancelled",
                            "not-applicable",
                            None,
                            None,
                            None,
                        ),
                        ConfirmAction::DeleteToken(token_id) => self.record_authorization_event(
                            flow_id,
                            "Delete Token",
                            OP_AUTH_TOKEN_DELETE,
                            "cancelled",
                            "cancelled",
                            "not-applicable",
                            None,
                            Some(token_id.as_str()),
                            None,
                        ),
                        ConfirmAction::DeleteSshKey(reference) => self.record_authorization_event(
                            flow_id,
                            "Delete SSH Key",
                            OP_SSH_KEY_DELETE,
                            "cancelled",
                            "cancelled",
                            "not-applicable",
                            None,
                            None,
                            Some(reference.as_str()),
                        ),
                        _ => {}
                    }
                }
                self.confirm_action = None;
                self.confirm_flow_id = None;
                self.confirm_selected = 0;
                self.last_status = self.t("menu.status.confirm_cancelled");
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if self.confirm_selected == 1 {
                    if let (Some(flow_id), Some(action)) =
                        (self.confirm_flow_id.as_deref(), self.confirm_action.as_ref())
                    {
                        match action {
                            ConfirmAction::DeleteVault => self.record_authorization_event(
                                flow_id,
                                "Delete Vault",
                                OP_VAULT_DELETE,
                                "cancelled",
                                "cancelled",
                                "not-applicable",
                                None,
                                None,
                                None,
                            ),
                            ConfirmAction::DeleteToken(token_id) => self.record_authorization_event(
                                flow_id,
                                "Delete Token",
                                OP_AUTH_TOKEN_DELETE,
                                "cancelled",
                                "cancelled",
                                "not-applicable",
                                None,
                                Some(token_id.as_str()),
                                None,
                            ),
                            ConfirmAction::DeleteSshKey(reference) => self.record_authorization_event(
                                flow_id,
                                "Delete SSH Key",
                                OP_SSH_KEY_DELETE,
                                "cancelled",
                                "cancelled",
                                "not-applicable",
                                None,
                                None,
                                Some(reference.as_str()),
                            ),
                            _ => {}
                        }
                    }
                    self.confirm_action = None;
                    self.confirm_flow_id = None;
                    self.confirm_selected = 0;
                    self.last_status = self.t("menu.status.confirm_cancelled");
                    return Ok(());
                }
                let action = self.confirm_action.clone();
                let managed_auth_flow = matches!(
                    action.as_ref(),
                    Some(ConfirmAction::DeleteVault)
                        | Some(ConfirmAction::DeleteToken(_))
                        | Some(ConfirmAction::DeleteSshKey(_))
                );
                self.confirm_action = None;
                self.confirm_selected = 0;
                match action {
                    Some(ConfirmAction::DeleteVault) => self.delete_vault_confirmed()?,
                    Some(ConfirmAction::ConfirmPlainSshRisk) => {
                        let index = self.settings.targets.len();
                        self.settings.targets.push(default_ssh_target(index));
                        self.push_navigation_state();
                        self.start_target_edit_session(index, true);
                        self.screen = Screen::TargetEditor(index);
                        self.selected = 0;
                        self.sync_selection_to_focusable();
                        self.last_status = self.t("menu.status.added_ssh_target");
                    }
                    Some(ConfirmAction::RevokeToken(token_id)) => {
                        self.revoke_token_confirmed(&token_id)?
                    }
                    Some(ConfirmAction::DeleteToken(token_id)) => {
                        self.delete_token_confirmed(&token_id)?
                    }
                    Some(ConfirmAction::DeleteSshKey(reference)) => {
                        self.delete_ssh_key_confirmed(&reference)?
                    }
                    Some(ConfirmAction::DeleteTarget(index)) => {
                        if index < self.settings.targets.len() {
                            self.settings.targets.remove(index);
                            self.strip_dirty_paths_for_target(index);
                            self.clear_target_edit_session_if_matches(index);
                            self.screen = Screen::Targets;
                            self.selected = index.saturating_sub(1);
                            self.sync_selection_to_focusable();
                            self.last_status = self.t("menu.status.deleted_target");
                        }
                    }
                    Some(ConfirmAction::DiscardNewTarget(index)) => {
                        if index < self.settings.targets.len() {
                            self.settings.targets.remove(index);
                        }
                        self.strip_dirty_paths_for_target(index);
                        self.clear_target_edit_session_if_matches(index);
                        self.go_back();
                        self.last_status = self.t("menu.status.target_draft_discarded");
                    }
                    Some(ConfirmAction::DiscardTargetChanges(index)) => {
                        if let Some(session) = self.target_edit_session.as_ref() {
                            if session.index == index {
                                if let Some(original) = session.baseline.clone() {
                                    if let Some(target) = self.settings.targets.get_mut(index) {
                                        *target = original;
                                    }
                                }
                            }
                        }
                        self.strip_dirty_paths_for_target(index);
                        self.clear_target_edit_session_if_matches(index);
                        self.go_back();
                        self.last_status = self.t("menu.status.target_changes_discarded");
                    }
                    None => {}
                }
                if !managed_auth_flow {
                    self.confirm_flow_id = None;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn create_token_with_flow(
        &mut self,
        label: String,
        expires_in_seconds: Option<u64>,
    ) -> Result<(), String> {
        let flow_id = next_local_authorization_flow_id(OP_AUTH_TOKEN_CREATE);
        self.record_authorization_event(
            &flow_id,
            "Create Token",
            OP_AUTH_TOKEN_CREATE,
            "started",
            "pending",
            "leader",
            None,
            None,
            None,
        );
        let operator_principal = "menuconfig:local-operator".to_string();
        let mut request = CreateAgentTokenRequest {
            label,
            created_by: operator_principal.clone(),
            expires_in: expires_in_seconds.map(Duration::from_secs),
            idle_timeout_sec: None,
            scope: TokenScopeInput {
                scope_profile: Some("strict-default".to_string()),
                target_ids: Vec::new(),
                tool_ids: Vec::new(),
                max_risk_envelope: Some("deny-all".to_string()),
                allow_open_shell: Some(false),
                allow_write_shell_input: Some(false),
                allow_artifact_cross_principal: Some(false),
                allow_delegation: Some(false),
                allow_admin_actions: Some(false),
            },
            attestation_id: None,
        };
        let digest = local_admin_payload_digest_for_create_agent_token(&request);
        let router = match self.ensure_vault_router_loaded() {
            Ok(router) => router,
            Err(err) => {
                self.record_authorization_event(
                    &flow_id,
                    "Create Token",
                    OP_AUTH_TOKEN_CREATE,
                    "failed",
                    "error",
                    "leader",
                    Some("router-load-failed"),
                    None,
                    None,
                );
                return Err(err);
            }
        };
        let attestation_id = mint_local_admin_attestation(
            router,
            LocalAdminActionKind::CreateAgentToken,
            local_admin_create_token_target(),
            &digest,
            &operator_principal,
        )?;
        request.attestation_id = Some(attestation_id);
        let created = router
            .create_agent_token(request)
            .map_err(|err| format!("create token failed: {err:?}"))?;
        self.pending_token_label = None;
        self.token_reveal = Some(created.plaintext_token);
        self.refresh_security_summary();
        self.last_status = self.t("menu.status.token_created");
        self.record_authorization_event(
            &flow_id,
            "Create Token",
            OP_AUTH_TOKEN_CREATE,
            "succeeded",
            "ok",
            "leader",
            None,
            Some(created.summary.token_id.as_str()),
            None,
        );
        Ok(())
    }

    fn update_token_label(&mut self, token_id: &str, label: &str) -> Result<(), String> {
        let operator_principal = "menuconfig:local-operator".to_string();
        let router = self.ensure_vault_router_loaded()?;
        router
            .update_agent_token_label(UpdateAgentTokenLabelRequest {
                token_id: token_id.to_string(),
                label: label.to_string(),
                changed_by: operator_principal,
            })
            .map_err(|err| format!("update token label failed: {err:?}"))?;
        self.refresh_security_summary();
        self.last_status = self.t("menu.status.token_label_updated");
        Ok(())
    }

    fn update_token_access(&mut self, token_id: &str, enabled: bool) -> Result<(), String> {
        let operator_principal = "menuconfig:local-operator".to_string();
        let router = self.ensure_vault_router_loaded()?;
        router
            .update_agent_token_access(UpdateAgentTokenAccessRequest {
                token_id: token_id.to_string(),
                enabled,
                changed_by: operator_principal,
            })
            .map_err(|err| format!("update token access failed: {err:?}"))?;
        self.refresh_security_summary();
        self.last_status = if enabled {
            self.t("menu.status.token_access_enabled")
        } else {
            self.t("menu.status.token_access_disabled")
        };
        Ok(())
    }

    fn revoke_token_confirmed(&mut self, token_id: &str) -> Result<(), String> {
        let router = self.ensure_vault_router_loaded()?;
        router
            .revoke_agent_token(token_id, Some("menuconfig revoke".to_string()))
            .map_err(|err| format!("revoke token failed: {err:?}"))?;
        self.refresh_security_summary();
        self.last_status = self.t("menu.status.token_revoked");
        Ok(())
    }

    fn delete_token_confirmed(&mut self, token_id: &str) -> Result<(), String> {
        let flow_id = self
            .confirm_flow_id
            .take()
            .unwrap_or_else(|| next_local_authorization_flow_id(OP_AUTH_TOKEN_DELETE));
        let operator_principal = "menuconfig:local-operator".to_string();
        let digest = local_admin_payload_digest_for_delete_agent_token(token_id);
        self.record_authorization_event(
            &flow_id,
            "Delete Token",
            OP_AUTH_TOKEN_DELETE,
            "started",
            "pending",
            "leader",
            None,
            Some(token_id),
            None,
        );
        let router = match self.ensure_vault_router_loaded() {
            Ok(router) => router,
            Err(err) => {
                self.record_authorization_event(
                    &flow_id,
                    "Delete Token",
                    OP_AUTH_TOKEN_DELETE,
                    "failed",
                    "error",
                    "leader",
                    Some("router-load-failed"),
                    Some(token_id),
                    None,
                );
                return Err(err);
            }
        };
        let attestation_id = mint_local_admin_attestation(
            router,
            LocalAdminActionKind::DeleteAgentToken,
            token_id,
            &digest,
            &operator_principal,
        )?;
        router
            .delete_agent_token_with_attestation(DeleteAgentTokenRequest {
                token_id: token_id.to_string(),
                requested_by: operator_principal,
                attestation_id,
            })
            .map_err(|err| format!("delete token failed: {err:?}"))?;
        self.refresh_security_summary();
        self.last_status = self.t("menu.status.token_deleted");
        self.record_authorization_event(
            &flow_id,
            "Delete Token",
            OP_AUTH_TOKEN_DELETE,
            "succeeded",
            "ok",
            "leader",
            None,
            Some(token_id),
            None,
        );
        Ok(())
    }

    fn delete_ssh_key_confirmed(&mut self, credential_ref: &str) -> Result<(), String> {
        let flow_id = self
            .confirm_flow_id
            .take()
            .unwrap_or_else(|| next_local_authorization_flow_id(OP_SSH_KEY_DELETE));
        self.record_authorization_event(
            &flow_id,
            "Delete SSH Key",
            OP_SSH_KEY_DELETE,
            "started",
            "pending",
            "leader",
            None,
            None,
            Some(credential_ref),
        );
        let router = match self.ensure_vault_router_loaded() {
            Ok(router) => router,
            Err(err) => {
                self.record_authorization_event(
                    &flow_id,
                    "Delete SSH Key",
                    OP_SSH_KEY_DELETE,
                    "failed",
                    "error",
                    "leader",
                    Some("router-load-failed"),
                    None,
                    Some(credential_ref),
                );
                return Err(err);
            }
        };
        router
            .delete_secret_trusted_local(credential_ref)
            .map_err(|err| format!("delete ssh key failed: {err:?}"))?;

        for target in &mut self.settings.targets {
            if target
                .credential_ref
                .as_deref()
                .map(|value| value.eq_ignore_ascii_case(credential_ref))
                .unwrap_or(false)
            {
                target.credential_ref = None;
            }
        }
        self.screen = Screen::SshKeyManagement;
        self.selected = 0;
        self.sync_selection_to_focusable();
        self.refresh_security_summary();
        self.last_status = self.tf("menu.status.ssh_key_deleted", &[("ref", credential_ref)]);
        self.record_authorization_event(
            &flow_id,
            "Delete SSH Key",
            OP_SSH_KEY_DELETE,
            "succeeded",
            "ok",
            "leader",
            None,
            None,
            Some(credential_ref),
        );
        Ok(())
    }

    fn delete_vault_confirmed(&mut self) -> Result<(), String> {
        let flow_id = self
            .confirm_flow_id
            .take()
            .unwrap_or_else(|| next_local_authorization_flow_id(OP_VAULT_DELETE));
        let operator_principal = "menuconfig:local-operator".to_string();
        let digest = local_admin_payload_digest_for_delete_vault();
        self.record_authorization_event(
            &flow_id,
            "Delete Vault",
            OP_VAULT_DELETE,
            "started",
            "pending",
            "leader",
            None,
            None,
            None,
        );
        let router = match self.ensure_vault_router_loaded() {
            Ok(router) => router,
            Err(err) => {
                self.record_authorization_event(
                    &flow_id,
                    "Delete Vault",
                    OP_VAULT_DELETE,
                    "failed",
                    "error",
                    "leader",
                    Some("router-load-failed"),
                    None,
                    None,
                );
                return Err(err);
            }
        };
        let attestation_id = mint_local_admin_attestation(
            router,
            LocalAdminActionKind::DeleteVault,
            local_admin_delete_vault_target(),
            &digest,
            &operator_principal,
        )?;
        router
            .delete_vault_with_attestation(DeleteVaultRequest {
                requested_by: operator_principal,
                attestation_id,
            })
            .map_err(|err| format!("delete vault failed: {err:?}"))?;
        self.screen = Screen::Security;
        self.selected = 0;
        self.refresh_security_summary();
        self.sync_selection_to_focusable();
        self.last_status = self.t("menu.status.vault_deleted");
        self.record_authorization_event(
            &flow_id,
            "Delete Vault",
            OP_VAULT_DELETE,
            "succeeded",
            "ok",
            "leader",
            None,
            None,
            None,
        );
        Ok(())
    }

    fn go_back(&mut self) {
        if let Some((screen, selected)) = self.navigation_stack.pop() {
            let previous_screen = self.current_screen_id();
            self.screen = screen;
            self.selected = selected;
            self.sync_selection_to_focusable();
            let screen_title = self.screen.title(&self.catalog);
            self.last_status = self.tf("menu.status.returned", &[("screen", &screen_title)]);
            self.record_session_event(
                &format!("screen.back:{previous_screen}"),
                "navigated",
                "ok",
                RuntimeLogLevel::Debug,
                None,
            );
        } else {
            self.last_status = self.t("menu.status.already_top");
        }
    }

    fn push_navigation_state(&mut self) {
        self.navigation_stack
            .push((self.screen.clone(), self.selected));
        self.record_session_event("screen.push", "navigated", "ok", RuntimeLogLevel::Debug, None);
    }

    fn start_target_edit_session(&mut self, index: usize, is_new: bool) {
        let baseline = if is_new {
            None
        } else {
            self.settings.targets.get(index).cloned()
        };
        self.target_edit_session = Some(TargetEditSession {
            index,
            is_new,
            baseline,
        });
    }

    fn ensure_target_edit_session(&mut self, index: usize) {
        if self
            .target_edit_session
            .as_ref()
            .map(|session| session.index == index)
            .unwrap_or(false)
        {
            return;
        }
        self.start_target_edit_session(index, false);
    }

    fn clear_target_edit_session_if_matches(&mut self, index: usize) {
        if self
            .target_edit_session
            .as_ref()
            .map(|session| session.index == index)
            .unwrap_or(false)
        {
            self.target_edit_session = None;
        }
    }

    fn strip_dirty_paths_for_target(&mut self, index: usize) {
        let prefix = format!("targets[{index}].");
        self.dirty_paths.retain(|path| !path.starts_with(&prefix));
    }

    fn leaving_target_flow_on_back(&self) -> Option<usize> {
        let index = target_screen_index(&self.screen)?;
        let (previous, _) = self.navigation_stack.last()?;
        if target_screen_index(previous) == Some(index) {
            return None;
        }
        Some(index)
    }

    fn target_session_has_pending_changes(&self, index: usize) -> bool {
        let Some(session) = self.target_edit_session.as_ref() else {
            return false;
        };
        if session.index != index {
            return false;
        }
        if session.is_new {
            return true;
        }
        let Some(current) = self.settings.targets.get(index) else {
            return false;
        };
        let Some(baseline) = session.baseline.as_ref() else {
            return false;
        };
        current != baseline
    }

    fn move_footer_selection(&mut self, delta: isize) {
        let next = self.footer_selected as isize + delta;
        self.footer_selected = next.clamp(0, 2) as usize;
        let button_label = FooterButton::from_index(self.footer_selected).label(&self.catalog);
        self.last_status = self.tf("menu.status.button_selected", &[("button", &button_label)]);
    }

    fn activate_footer_button(&mut self) -> Result<Option<MenuConfigOutcome>, String> {
        match FooterButton::from_index(self.footer_selected) {
            FooterButton::Select => {
                self.activate_selected()?;
                Ok(None)
            }
            FooterButton::Exit => self.request_exit_or_back(),
            FooterButton::Help => {
                self.show_help = !self.show_help;
                self.last_status = if self.show_help {
                    self.t("menu.status.help_opened")
                } else {
                    self.t("menu.status.help_closed")
                };
                Ok(None)
            }
        }
    }

    fn request_exit_or_back(&mut self) -> Result<Option<MenuConfigOutcome>, String> {
        if self.screen == Screen::Root {
            if self.is_dirty() {
                self.begin_exit_confirm();
                return Ok(None);
            }
            return Ok(Some(MenuConfigOutcome {
                saved: true,
                apply_strategy: self.last_apply_strategy.clone(),
                config_path: self.config_path.clone(),
            }));
        }
        if let Some(index) = self.leaving_target_flow_on_back() {
            if self.target_session_has_pending_changes(index) {
                let discard = self
                    .target_edit_session
                    .as_ref()
                    .map(|session| session.is_new)
                    .unwrap_or(false);
                self.confirm_action = Some(if discard {
                    ConfirmAction::DiscardNewTarget(index)
                } else {
                    ConfirmAction::DiscardTargetChanges(index)
                });
                self.confirm_selected = 0;
                self.last_status = if discard {
                    self.t("menu.status.target_discard_new_confirm_pending")
                } else {
                    self.t("menu.status.target_discard_changes_confirm_pending")
                };
                return Ok(None);
            }
            self.clear_target_edit_session_if_matches(index);
        }
        self.go_back();
        Ok(None)
    }

    fn begin_exit_confirm(&mut self) {
        self.exit_confirm_mode = true;
        self.exit_confirm_selected = 0;
        self.last_status = self.t("menu.status.unsaved_confirm");
    }

    fn handle_exit_confirm_key(
        &mut self,
        code: KeyCode,
    ) -> Result<Option<MenuConfigOutcome>, String> {
        match code {
            KeyCode::Left => {
                let next = self.exit_confirm_selected as isize - 1;
                self.exit_confirm_selected = next.clamp(0, 2) as usize;
            }
            KeyCode::Right => {
                let next = self.exit_confirm_selected as isize + 1;
                self.exit_confirm_selected = next.clamp(0, 2) as usize;
            }
            KeyCode::Esc => {
                self.exit_confirm_mode = false;
                self.last_status = self.t("menu.status.exit_cancelled");
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.exit_confirm_mode = false;
                match self.exit_confirm_selected {
                    0 => {
                        self.save()?;
                        return Ok(Some(MenuConfigOutcome {
                            saved: true,
                            apply_strategy: self.last_apply_strategy.clone(),
                            config_path: self.config_path.clone(),
                        }));
                    }
                    1 => {
                        return Ok(Some(MenuConfigOutcome {
                            saved: false,
                            apply_strategy: self.last_apply_strategy.clone(),
                            config_path: self.config_path.clone(),
                        }));
                    }
                    _ => {
                        self.last_status = self.t("menu.status.exit_cancelled");
                    }
                }
            }
            _ => {}
        }
        Ok(None)
    }

    fn move_selection(&mut self, delta: isize) {
        let entries = self.entries();
        self.selected = wrap_focusable_selection_index(self.selected, delta, &entries);
    }

    fn select_field(&mut self, field: &str) {
        if let Some(index) = self.entries().iter().position(
            |entry| matches!(&entry.kind, MenuEntryKind::EditField(value) if value == field),
        ) {
            self.selected = index;
        } else {
            self.selected = 0;
        }
        self.sync_selection_to_focusable();
    }

    fn sync_selection_to_focusable(&mut self) {
        let entries = self.entries();
        self.selected = normalize_selected_index(self.selected, &entries);
    }

    fn entries(&self) -> Vec<MenuEntry> {
        match self.screen {
            Screen::Root => root_entries(&self.catalog),
            Screen::Core => core_entries(&self.settings, &self.catalog),
            Screen::Storage => storage_entries(&self.settings, &self.catalog),
            Screen::ModelPlane => model_plane_entries(&self.settings, &self.catalog),
            Screen::Vault => vault_entries(&self.settings, &self.catalog),
            Screen::Targets => targets_entries(self, &self.catalog),
            Screen::Security => security_entries(&self.security_summary, &self.catalog),
            Screen::SshKeyImport => ssh_key_import_entries(self, &self.catalog),
            Screen::SshKeyManagement => ssh_key_management_entries(self, &self.catalog),
            Screen::SshKeyDetail(ref credential_ref) => {
                ssh_key_detail_entries(self, &self.catalog, credential_ref)
            }
            Screen::TokenManagement => token_management_entries(self, &self.catalog),
            Screen::TokenDetail(ref token_id) => {
                token_detail_entries(self, &self.catalog, token_id)
            }
            Screen::TargetAddMode => target_add_mode_entries(self, &self.catalog),
            Screen::TargetAddTypePlain => target_add_type_entries(false, &self.catalog),
            Screen::TargetAddTypeSensitive => target_add_type_entries(true, &self.catalog),
            Screen::TargetEditor(index) => target_editor_entries(self, index, &self.catalog),
            Screen::TargetPublicDescriptor(index) => {
                target_public_descriptor_entries(&self.settings, index, &self.catalog)
            }
            Screen::TargetConnectionProfile(index) => {
                target_connection_profile_entries(&self.settings, index, &self.catalog)
            }
            Screen::TargetSensitiveOverlay(index) => {
                target_sensitive_overlay_entries(self, index, &self.catalog)
            }
            Screen::TargetCredentialSource(index) => {
                target_credential_source_entries(self, index, &self.catalog)
            }
            Screen::TargetCredentialPicker(index) => {
                target_credential_picker_entries(self, index, &self.catalog)
            }
            Screen::TargetPolicy(index) => target_policy_entries(self, index, &self.catalog),
            Screen::SearchResults => {
                search_entries(&self.settings, &self.search_input, &self.catalog)
            }
        }
    }
}

struct TerminalUi {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalUi {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }
}

impl Drop for TerminalUi {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

pub fn run_menuconfig(config_path: impl Into<PathBuf>) -> Result<MenuConfigOutcome, String> {
    let mut app = MenuConfigApp::load(config_path)?;
    app.run()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuEntryVisualState {
    Normal,
    Selected,
    Disabled,
}

fn detect_selection_highlight_mode() -> SelectionHighlightMode {
    let term = std::env::var("TERM").ok();
    detect_selection_highlight_mode_from_values(
        env_var_truthy(MENUCONFIG_FORCE_FALLBACK_HIGHLIGHT_ENV),
        std::env::var_os("NO_COLOR").is_some(),
        term.as_deref(),
    )
}

fn detect_selection_highlight_mode_from_values(
    force_fallback: bool,
    no_color: bool,
    term: Option<&str>,
) -> SelectionHighlightMode {
    if force_fallback
        || no_color
        || term
            .map(|value| value.trim().eq_ignore_ascii_case("dumb"))
            .unwrap_or(false)
    {
        SelectionHighlightMode::Fallback
    } else {
        SelectionHighlightMode::Reverse
    }
}

fn env_var_truthy(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn selected_highlight_style(mode: SelectionHighlightMode) -> Style {
    match mode {
        SelectionHighlightMode::Reverse => Style::default()
            .add_modifier(Modifier::REVERSED)
            .add_modifier(Modifier::BOLD),
        SelectionHighlightMode::Fallback => Style::default()
            .add_modifier(Modifier::BOLD)
            .add_modifier(Modifier::UNDERLINED),
    }
}

fn wrap_selection_index(current: usize, delta: isize, total: usize) -> usize {
    if total == 0 {
        return 0;
    }
    let total_i128 = total as i128;
    let current_i128 = current.min(total.saturating_sub(1)) as i128;
    let next = (current_i128 + delta as i128).rem_euclid(total_i128);
    next as usize
}

fn first_focusable_index(entries: &[MenuEntry]) -> Option<usize> {
    entries
        .iter()
        .position(|entry| !menu_entry_is_disabled(entry))
}

fn normalize_selected_index(selected: usize, entries: &[MenuEntry]) -> usize {
    if entries.is_empty() {
        return 0;
    }
    let bounded = selected.min(entries.len().saturating_sub(1));
    if !menu_entry_is_disabled(&entries[bounded]) {
        bounded
    } else {
        first_focusable_index(entries).unwrap_or(0)
    }
}

fn wrap_focusable_selection_index(current: usize, delta: isize, entries: &[MenuEntry]) -> usize {
    if entries.is_empty() {
        return 0;
    }
    if first_focusable_index(entries).is_none() {
        return 0;
    }
    let total = entries.len();
    let mut index = current.min(total.saturating_sub(1));
    for _ in 0..total {
        index = wrap_selection_index(index, delta, total);
        if !menu_entry_is_disabled(&entries[index]) {
            return index;
        }
    }
    normalize_selected_index(current, entries)
}

fn menu_entry_style(state: MenuEntryVisualState, mode: SelectionHighlightMode) -> Style {
    match state {
        MenuEntryVisualState::Normal => Style::default(),
        MenuEntryVisualState::Selected => selected_highlight_style(mode),
        MenuEntryVisualState::Disabled => Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::DIM),
    }
}

fn menu_entry_is_disabled(entry: &MenuEntry) -> bool {
    matches!(entry.kind, MenuEntryKind::Info)
}

fn menu_entry_visual_state(
    selected: usize,
    index: usize,
    entry: &MenuEntry,
) -> MenuEntryVisualState {
    if menu_entry_is_disabled(entry) {
        MenuEntryVisualState::Disabled
    } else if index == selected {
        MenuEntryVisualState::Selected
    } else {
        MenuEntryVisualState::Normal
    }
}

fn render(frame: &mut ratatui::Frame, app: &MenuConfigApp) {
    if menuconfig_viewport_too_small(frame.area()) {
        render_resize_popup(frame, app);
        return;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(5)])
        .split(frame.area());

    render_main_menu(frame, layout[0], app);
    render_footer(frame, layout[1], app);

    if app.show_help {
        render_help(frame, app);
    }
    if app.edit_mode {
        render_edit_popup(frame, app);
    }
    if app.exit_confirm_mode {
        render_exit_confirm(frame, app);
    }
    if !matches!(app.unlock_flow_state, UnlockFlowState::Idle) {
        render_unlock_flow_popup(frame, app);
    }
    if app.confirm_action.is_some() {
        render_confirm_popup(frame, app);
    }
    if app.token_reveal.is_some() {
        render_token_reveal_popup(frame, app);
    }
}

fn render_main_menu(frame: &mut ratatui::Frame, area: Rect, app: &MenuConfigApp) {
    let rendered_rows = app
        .entries()
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let visual_state = menu_entry_visual_state(app.selected, index, entry);
            let line = format_menu_entry_styled_line(app, index, entry);
            let style = menu_entry_style(visual_state, app.selection_highlight_mode);
            (line, style)
        })
        .collect::<Vec<_>>();

    let dirty_suffix = if app.is_dirty() {
        app.t("menu.render.dirty_suffix")
    } else {
        String::new()
    };
    let screen_title = app.screen.title(&app.catalog);
    let title = app.tf(
        "menu.render.title",
        &[("screen", &screen_title), ("dirty", &dirty_suffix)],
    );
    let block = Block::default().title(title).borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let available_row_width = chunks[0].width.saturating_sub(1) as usize;
    let rendered_rows = rendered_rows
        .into_iter()
        .map(|(line, style)| {
            (
                normalize_menu_line_for_width(line, available_row_width),
                style,
            )
        })
        .collect::<Vec<_>>();
    let content_width = rendered_rows
        .iter()
        .map(|(line, _)| line_char_width(line))
        .max()
        .unwrap_or(0);
    let list_width = chunks[0].width as usize;
    let left_padding = if list_width > content_width {
        (list_width - content_width) / 2
    } else {
        0
    };
    let left_pad = " ".repeat(left_padding);
    let items = rendered_rows
        .into_iter()
        .map(|(line, style)| {
            let mut spans = Vec::with_capacity(line.spans.len() + 1);
            spans.push(Span::raw(left_pad.clone()));
            spans.extend(line.spans.clone());
            ListItem::new(Line::from(spans)).style(style)
        })
        .collect::<Vec<_>>();
    let list = List::new(items);
    frame.render_widget(list, chunks[0]);
    if chunks.len() > 1 {
        let buttons = Paragraph::new(render_footer_buttons_line(
            app.footer_selected,
            &app.catalog,
            app.selection_highlight_mode,
        ))
        .alignment(Alignment::Center);
        frame.render_widget(buttons, chunks[1]);
    }
}

fn render_footer(frame: &mut ratatui::Frame, area: Rect, app: &MenuConfigApp) {
    let selected_description = app
        .entries()
        .get(app.selected)
        .map(|entry| entry.description.clone())
        .unwrap_or_else(|| app.last_status.clone());
    let screen_title = app.screen.title(&app.catalog);
    let search_value = if app.search_input.is_empty() {
        app.t("menu.render.no_search")
    } else {
        app.search_input.clone()
    };
    let footer = Paragraph::new(vec![
        render_footer_path_line(app, &screen_title, &search_value),
        Line::from(app.tf(
            "menu.render.description",
            &[("description", &selected_description)],
        )),
        Line::from(if app.edit_mode {
            match app.edit_mode_kind {
                EditModeKind::Text => app.tf("menu.render.edit", &[("value", &app.edit_input)]),
                EditModeKind::Choice => app.tf(
                    "menu.render.select",
                    &[(
                        "value",
                        &app.edit_options
                            .get(app.edit_option_selected)
                            .cloned()
                            .unwrap_or_default(),
                    )],
                ),
            }
        } else if app.exit_confirm_mode {
            app.t("menu.render.exit_hint")
        } else if app.confirm_action.is_some() {
            app.t("menu.render.confirm_hint")
        } else if app.token_reveal.is_some() {
            app.t("menu.render.token_reveal_hint")
        } else if matches!(app.unlock_flow_state, UnlockFlowState::Waiting) {
            app.t("menu.render.unlock_waiting")
        } else if matches!(app.unlock_flow_state, UnlockFlowState::Success) {
            app.t("menu.render.unlock_success_hint")
        } else if let UnlockFlowState::Failed { reason } = &app.unlock_flow_state {
            app.tf("menu.render.unlock_failed_hint", &[("reason", reason)])
        } else if app.search_mode {
            app.tf("menu.render.search", &[("query", &app.search_input)])
        } else {
            app.tf(
                "menu.render.keys",
                &[(
                    "apply",
                    &app.last_apply_strategy
                        .as_deref()
                        .map(|value| format!(", apply_strategy={value}"))
                        .unwrap_or_default(),
                )],
            )
        }),
    ])
    .block(
        Block::default()
            .title(app.t("menu.render.status_block"))
            .borders(Borders::ALL),
    )
    .wrap(Wrap { trim: false });
    frame.render_widget(footer, area);
}

fn line_char_width(line: &Line<'_>) -> usize {
    line.spans
        .iter()
        .map(|span| span.content.chars().count())
        .sum()
}

fn line_to_plain_text(line: &Line<'_>) -> String {
    let mut combined = String::new();
    for span in &line.spans {
        combined.push_str(span.content.as_ref());
    }
    combined
}

fn normalize_menu_line_for_width(line: Line<'static>, max_width: usize) -> Line<'static> {
    if max_width == 0 {
        return Line::from(String::new());
    }
    let text = line_to_plain_text(&line);
    if text.chars().count() <= max_width {
        line
    } else {
        Line::from(truncate_menu_row_text(&text, max_width))
    }
}

fn detect_menu_prefix_width(text: &str) -> usize {
    if text.starts_with(">> ") {
        7
    } else {
        6
    }
}

fn truncate_menu_row_text(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let chars = text.chars().collect::<Vec<_>>();
    if chars.len() <= max_width {
        return text.to_string();
    }
    if max_width <= 3 {
        return chars.into_iter().take(max_width).collect();
    }

    let has_arrow_suffix = text.ends_with("--->");
    let suffix = if has_arrow_suffix { " --->" } else { "" };
    let suffix_len = suffix.chars().count().min(max_width);
    let prefix_len = detect_menu_prefix_width(text).min(max_width.saturating_sub(suffix_len));
    let body_start = prefix_len.min(chars.len());
    let body_end = chars.len().saturating_sub(suffix_len);
    if body_end <= body_start {
        return chars.into_iter().take(max_width).collect();
    }

    let body_budget = max_width.saturating_sub(prefix_len + suffix_len);
    if body_budget <= 3 {
        return chars.into_iter().take(max_width).collect();
    }
    let body_keep = body_budget.saturating_sub(3);

    let mut out = String::new();
    out.extend(chars[..prefix_len].iter());
    let body_len = body_end.saturating_sub(body_start);
    if body_len > body_budget {
        out.extend(chars[body_start..body_start + body_keep].iter());
        out.push_str("...");
    } else {
        out.extend(chars[body_start..body_end].iter());
    }
    if suffix_len > 0 {
        out.extend(chars[chars.len() - suffix_len..].iter());
    }
    out.chars().take(max_width).collect()
}

fn menuconfig_viewport_too_small(area: Rect) -> bool {
    area.width < MENUCONFIG_MIN_VIEWPORT_WIDTH || area.height < MENUCONFIG_MIN_VIEWPORT_HEIGHT
}

fn runtime_viewport_too_small() -> bool {
    terminal_size()
        .map(|(width, height)| {
            width < MENUCONFIG_MIN_VIEWPORT_WIDTH || height < MENUCONFIG_MIN_VIEWPORT_HEIGHT
        })
        .unwrap_or(false)
}

fn render_resize_popup(frame: &mut ratatui::Frame, app: &MenuConfigApp) {
    let area = centered_rect(72, 34, frame.area());
    frame.render_widget(Clear, area);
    let popup = Paragraph::new(vec![
        Line::from(app.tf(
            "menu.resize.message",
            &[
                ("width", &frame.area().width.to_string()),
                ("height", &frame.area().height.to_string()),
                ("min_width", &MENUCONFIG_MIN_VIEWPORT_WIDTH.to_string()),
                ("min_height", &MENUCONFIG_MIN_VIEWPORT_HEIGHT.to_string()),
            ],
        )),
        Line::from(""),
        Line::from(app.t("menu.resize.hint")),
    ])
    .block(
        Block::default()
            .title(app.t("menu.resize.title"))
            .borders(Borders::ALL),
    )
    .wrap(Wrap { trim: false });
    frame.render_widget(popup, area);
}

fn render_footer_path_line(
    app: &MenuConfigApp,
    screen_title: &str,
    search_value: &str,
) -> Line<'static> {
    let formatted = app.tf(
        "menu.render.path",
        &[
            ("path", screen_title),
            ("dirty", &app.is_dirty().to_string()),
            ("lock_state", FOOTER_LOCK_STATE_TOKEN),
            ("search", search_value),
        ],
    );
    if let Some((prefix, suffix)) = formatted.split_once(FOOTER_LOCK_STATE_TOKEN) {
        return Line::from(vec![
            Span::raw(prefix.to_string()),
            Span::styled(
                app.security_summary.lock_state.clone(),
                lock_state_highlight_style(&app.security_summary.lock_state),
            ),
            Span::raw(suffix.to_string()),
        ]);
    }
    Line::from(formatted)
}

fn lock_state_highlight_style(lock_state: &str) -> Style {
    let color = match lock_state.trim().to_ascii_lowercase().as_str() {
        "unlocked" => Color::Green,
        "locked" => Color::Red,
        _ => Color::Yellow,
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

fn render_help(frame: &mut ratatui::Frame, app: &MenuConfigApp) {
    let area = centered_rect(70, 45, frame.area());
    frame.render_widget(Clear, area);
    let catalog = &app.catalog;
    let help = Paragraph::new(vec![
        Line::from(catalog.t("menu.help.h1")),
        Line::from(""),
        Line::from(catalog.t("menu.help.l1")),
        Line::from(catalog.t("menu.help.l2")),
        Line::from(catalog.t("menu.help.l3")),
        Line::from(catalog.t("menu.help.l4")),
        Line::from(catalog.t("menu.help.l5")),
        Line::from(catalog.t("menu.help.l6")),
        Line::from(catalog.t("menu.help.l7")),
        Line::from(catalog.t("menu.help.l8")),
        Line::from(catalog.t("menu.help.l9")),
        Line::from(""),
        Line::from(catalog.t("menu.help.l10")),
    ])
    .block(
        Block::default()
            .title(catalog.t("menu.help.title"))
            .borders(Borders::ALL),
    )
    .wrap(Wrap { trim: false });
    frame.render_widget(help, area);
}

fn render_exit_confirm(frame: &mut ratatui::Frame, app: &MenuConfigApp) {
    let area = centered_rect(56, 30, frame.area());
    frame.render_widget(Clear, area);
    let options = vec![
        app.t("menu.exit.yes"),
        app.t("menu.exit.no"),
        app.t("menu.exit.cancel"),
    ];
    let popup = Paragraph::new(vec![
        Line::from(app.t("menu.exit.q1")),
        Line::from(app.t("menu.exit.q2")),
        Line::from(""),
        render_popup_button_line(
            &options,
            app.exit_confirm_selected,
            app.selection_highlight_mode,
        ),
    ])
    .block(
        Block::default()
            .title(app.t("menu.exit.title"))
            .borders(Borders::ALL),
    )
    .wrap(Wrap { trim: false });
    frame.render_widget(popup, area);
}

fn render_unlock_flow_popup(frame: &mut ratatui::Frame, app: &MenuConfigApp) {
    let area = centered_rect(62, 34, frame.area());
    frame.render_widget(Clear, area);
    let popup = match &app.unlock_flow_state {
        UnlockFlowState::Waiting => Paragraph::new(vec![
            Line::from(app.t("menu.unlock.waiting.message")),
            Line::from(""),
            Line::from(app.t("menu.unlock.waiting.hint")),
        ])
        .block(
            Block::default()
                .title(app.t("menu.unlock.waiting.title"))
                .borders(Borders::ALL),
        )
        .wrap(Wrap { trim: false }),
        UnlockFlowState::Success => Paragraph::new(vec![
            Line::from(app.t("menu.unlock.success.message")),
            Line::from(""),
            Line::from(app.t("menu.unlock.success.hint")),
        ])
        .block(
            Block::default()
                .title(app.t("menu.unlock.success.title"))
                .borders(Borders::ALL),
        )
        .wrap(Wrap { trim: false }),
        UnlockFlowState::Failed { reason } => Paragraph::new(vec![
            Line::from(app.tf("menu.unlock.failed.message", &[("reason", reason)])),
            Line::from(""),
            Line::from(app.t("menu.unlock.failed.hint")),
        ])
        .block(
            Block::default()
                .title(app.t("menu.unlock.failed.title"))
                .borders(Borders::ALL),
        )
        .wrap(Wrap { trim: false }),
        UnlockFlowState::Idle => return,
    };
    frame.render_widget(popup, area);
}

fn render_confirm_popup(frame: &mut ratatui::Frame, app: &MenuConfigApp) {
    let Some(action) = app.confirm_action.clone() else {
        return;
    };
    let action_text = match action {
        ConfirmAction::DeleteVault => app.t("menu.confirm.delete_vault"),
        ConfirmAction::ConfirmPlainSshRisk => app.t("menu.confirm.plain_ssh_risk"),
        ConfirmAction::DeleteSshKey(reference) => app.tf(
            "menu.confirm.delete_ssh_key",
            &[("ref", reference.as_str())],
        ),
        ConfirmAction::RevokeToken(token_id) => {
            app.tf("menu.confirm.revoke_token", &[("id", token_id.as_str())])
        }
        ConfirmAction::DeleteToken(token_id) => {
            app.tf("menu.confirm.delete_token", &[("id", token_id.as_str())])
        }
        ConfirmAction::DeleteTarget(index) => {
            app.tf("menu.confirm.delete_target", &[("id", &index.to_string())])
        }
        ConfirmAction::DiscardNewTarget(_) => app.t("menu.confirm.discard_new_target"),
        ConfirmAction::DiscardTargetChanges(_) => app.t("menu.confirm.discard_target_changes"),
    };
    let area = centered_rect(62, 32, frame.area());
    frame.render_widget(Clear, area);
    let options = vec![app.t("menu.confirm.confirm"), app.t("menu.confirm.cancel")];
    let popup = Paragraph::new(vec![
        Line::from(action_text),
        Line::from(""),
        Line::from(app.t("menu.confirm.message")),
        Line::from(""),
        render_popup_button_line(&options, app.confirm_selected, app.selection_highlight_mode),
    ])
    .block(
        Block::default()
            .title(app.t("menu.confirm.title"))
            .borders(Borders::ALL),
    )
    .wrap(Wrap { trim: false });
    frame.render_widget(popup, area);
}

fn render_token_reveal_popup(frame: &mut ratatui::Frame, app: &MenuConfigApp) {
    let Some(token) = app.token_reveal.clone() else {
        return;
    };
    let area = centered_rect(76, 38, frame.area());
    frame.render_widget(Clear, area);
    let popup = Paragraph::new(vec![
        Line::from(app.t("menu.token_reveal.message")),
        Line::from(""),
        Line::from(token),
        Line::from(""),
        Line::from(app.t("menu.token_reveal.hint")),
    ])
    .block(
        Block::default()
            .title(app.t("menu.token_reveal.title"))
            .borders(Borders::ALL),
    )
    .wrap(Wrap { trim: false });
    frame.render_widget(popup, area);
}

fn render_footer_buttons_line(
    selected: usize,
    catalog: &Catalog,
    mode: SelectionHighlightMode,
) -> Line<'static> {
    let mut spans = Vec::new();
    for index in 0..3 {
        if index > 0 {
            spans.push(Span::raw("    "));
        }
        let base = FooterButton::from_index(index).label(&catalog);
        if selected == index {
            let selected_label = if mode == SelectionHighlightMode::Fallback {
                format!(">>{base}<<")
            } else {
                base
            };
            spans.push(Span::styled(selected_label, selected_highlight_style(mode)));
        } else {
            spans.push(Span::raw(base));
        }
    }
    Line::from(spans)
}

fn render_popup_button_line(
    labels: &[String],
    selected: usize,
    mode: SelectionHighlightMode,
) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, label) in labels.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("   "));
        }
        spans.push(render_popup_button_span(label, index == selected, mode));
    }
    Line::from(spans)
}

fn render_popup_button_span(
    label: &str,
    selected: bool,
    mode: SelectionHighlightMode,
) -> Span<'static> {
    if !selected {
        return Span::raw(format!("  {label}  "));
    }
    let selected_label = if mode == SelectionHighlightMode::Fallback {
        format!(">>{label}<<")
    } else {
        format!(" {label} ")
    };
    Span::styled(selected_label, selected_highlight_style(mode))
}

fn format_menu_entry_line(app: &MenuConfigApp, index: usize, entry: &MenuEntry) -> String {
    let selector = menu_entry_selector(app, index, entry);
    if is_security_unlocked_notice_entry(app, entry) {
        return format!("{selector} -*- {}", entry.label);
    }
    match &entry.kind {
        MenuEntryKind::Navigate(screen) => {
            if let Screen::TargetEditor(target_index) = screen {
                let enabled = app
                    .settings
                    .targets
                    .get(*target_index)
                    .map(|target| target.enabled)
                    .unwrap_or(false);
                let marker = if enabled { "<*>" } else { "< >" };
                let label = if let Some(value) = entry.value.as_deref() {
                    format!("{} ({value})", entry.label)
                } else {
                    entry.label.clone()
                };
                format!("{selector} {marker} {label} --->")
            } else {
                format!("{selector}     {} --->", entry.label)
            }
        }
        MenuEntryKind::EditField(field) => {
            if is_boolean_toggle_field(field) {
                let enabled = field_value(&app.settings, field)
                    .and_then(|value| value.parse::<bool>().ok())
                    .unwrap_or(false);
                let marker = if is_single_choice_toggle_field(field) {
                    if enabled {
                        "<*>"
                    } else {
                        "< >"
                    }
                } else if enabled {
                    "[*]"
                } else {
                    "[ ]"
                };
                format!("{selector} {marker} {}", entry.label)
            } else if let Some(value) = entry.value.as_deref() {
                format!("{selector}     {} ({value}) --->", entry.label)
            } else {
                format!("{selector}     {} --->", entry.label)
            }
        }
        MenuEntryKind::FocusField { field, .. } => {
            if is_boolean_toggle_field(field) {
                let enabled = field_value(&app.settings, field)
                    .and_then(|value| value.parse::<bool>().ok())
                    .unwrap_or(false);
                let marker = if is_single_choice_toggle_field(field) {
                    if enabled {
                        "<*>"
                    } else {
                        "< >"
                    }
                } else if enabled {
                    "[*]"
                } else {
                    "[ ]"
                };
                format!("{selector} {marker} {} --->", entry.label)
            } else if let Some(value) = entry.value.as_deref() {
                format!("{selector}     {} ({value}) --->", entry.label)
            } else {
                format!("{selector}     {} --->", entry.label)
            }
        }
        MenuEntryKind::Info => match entry.value.as_deref() {
            Some(value) => format!("{selector} --- {} = {}", entry.label, value),
            None => format!("{selector} --- {}", entry.label),
        },
        MenuEntryKind::Action(action) => {
            if matches!(action, ActionKind::ToggleTokenAccess(_)) {
                let marker = match entry.value.as_deref() {
                    Some("enabled") => "<*>",
                    _ => "< >",
                };
                return format!("{selector} {marker} {}", entry.label);
            }
            if matches!(action, ActionKind::BindTargetCredentialRef { .. }) {
                let marker = match entry.value.as_deref() {
                    Some("selected") => "<*>",
                    _ => "< >",
                };
                return format!("{selector} {marker} {}", entry.label);
            }
            format!("{selector}     {} --->", entry.label)
        }
    }
}

fn format_menu_entry_styled_line(
    app: &MenuConfigApp,
    index: usize,
    entry: &MenuEntry,
) -> Line<'static> {
    if is_security_lock_state_entry(app, entry) {
        let selector = menu_entry_selector(app, index, entry);
        let lock_state = entry.value.clone().unwrap_or_default();
        return Line::from(vec![
            Span::raw(format!("{selector} --- {} = ", entry.label)),
            Span::styled(lock_state.clone(), lock_state_highlight_style(&lock_state)),
        ]);
    }
    if is_security_unlocked_notice_entry(app, entry) {
        let selector = menu_entry_selector(app, index, entry);
        return Line::from(vec![
            Span::raw(format!("{selector} -*- ")),
            Span::styled(entry.label.clone(), lock_state_highlight_style("unlocked")),
        ]);
    }
    Line::from(format_menu_entry_line(app, index, entry))
}

fn menu_entry_selector(app: &MenuConfigApp, index: usize, entry: &MenuEntry) -> &'static str {
    let visual_state = menu_entry_visual_state(app.selected, index, entry);
    if index != app.selected || menu_entry_is_disabled(entry) {
        " "
    } else if app.selection_highlight_mode == SelectionHighlightMode::Fallback
        && visual_state == MenuEntryVisualState::Selected
    {
        ">>"
    } else {
        ">"
    }
}

fn is_security_lock_state_entry(app: &MenuConfigApp, entry: &MenuEntry) -> bool {
    matches!(app.screen, Screen::Security)
        && matches!(entry.kind, MenuEntryKind::Info)
        && entry.label == app.t("menu.security.lock_state")
}

fn is_security_unlocked_notice_entry(app: &MenuConfigApp, entry: &MenuEntry) -> bool {
    matches!(app.screen, Screen::Security)
        && matches!(entry.kind, MenuEntryKind::Info)
        && entry.label == app.t("menu.security.unlocked_notice")
        && entry.value.is_none()
}

fn target_screen_index(screen: &Screen) -> Option<usize> {
    match screen {
        Screen::TargetEditor(index)
        | Screen::TargetPublicDescriptor(index)
        | Screen::TargetConnectionProfile(index)
        | Screen::TargetSensitiveOverlay(index)
        | Screen::TargetCredentialSource(index)
        | Screen::TargetCredentialPicker(index)
        | Screen::TargetPolicy(index) => Some(*index),
        _ => None,
    }
}

fn display_edit_field_label(app: &MenuConfigApp, field: &str) -> String {
    if field == SSH_IMPORT_KEY_NAME_FIELD {
        return app.t("menu.edit.field.ssh_import_key_name");
    }
    if field == SSH_IMPORT_LABEL_FIELD {
        return app.t("menu.edit.field.ssh_import_label");
    }
    if field == SSH_IMPORT_SOURCE_PATH_FIELD {
        return app.t("menu.edit.field.ssh_import_source_path");
    }
    if field == SSH_IMPORT_PASSPHRASE_FIELD {
        return app.t("menu.edit.field.ssh_import_passphrase");
    }
    if field == TOKEN_CREATE_LABEL_FIELD {
        return app.t("menu.edit.field.token_create_label");
    }
    if field == TOKEN_CREATE_EXPIRY_MODE_FIELD {
        return app.t("menu.edit.field.token_create_expiry_mode");
    }
    if field == TOKEN_CREATE_EXPIRY_AT_FIELD {
        return app.t("menu.edit.field.token_create_expiry_at");
    }
    if let Some(token_id) = field.strip_prefix("__token_label_edit__:") {
        return app.tf("menu.edit.field.token_label_edit", &[("id", token_id)]);
    }
    field.to_string()
}

fn render_edit_popup(frame: &mut ratatui::Frame, app: &MenuConfigApp) {
    let Some(field) = app.edit_field.as_deref() else {
        return;
    };
    let field_label = display_edit_field_label(app, field);
    match app.edit_mode_kind {
        EditModeKind::Text => {
            let area = centered_rect(70, 28, frame.area());
            frame.render_widget(Clear, area);
            let input_prefix = app.t("menu.edit.input_prefix");
            let display_value = if field == SSH_IMPORT_PASSPHRASE_FIELD {
                "•".repeat(app.edit_input.chars().count())
            } else {
                app.edit_input.clone()
            };
            let popup = Paragraph::new(vec![
                Line::from(app.tf("menu.edit.field", &[("field", &field_label)])),
                Line::from(app.t("menu.edit.hint")),
                Line::from(""),
                Line::from(format!("{input_prefix}{display_value}")),
            ])
            .block(
                Block::default()
                    .title(app.t("menu.edit.title"))
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false });
            frame.render_widget(popup, area);

            // Show the terminal caret at the end of input so operators can see the edit position.
            let content_x = area.x.saturating_add(1);
            let content_y = area.y.saturating_add(4);
            let desired_offset = input_prefix.chars().count().saturating_add(app.edit_cursor);
            let max_offset = area.width.saturating_sub(3) as usize;
            let cursor_x = content_x.saturating_add(desired_offset.min(max_offset) as u16);
            frame.set_cursor_position((cursor_x, content_y));
        }
        EditModeKind::Choice => {
            let area = centered_rect(70, 55, frame.area());
            frame.render_widget(Clear, area);
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(4)])
                .split(area);
            let header = Paragraph::new(vec![
                Line::from(app.tf("menu.choice.field", &[("field", &field_label)])),
                Line::from(app.t("menu.choice.hint")),
            ])
            .block(
                Block::default()
                    .title(app.t("menu.choice.title"))
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false });
            frame.render_widget(header, chunks[0]);

            let items = app
                .edit_options
                .iter()
                .enumerate()
                .map(|(index, option)| {
                    let marker = if index == app.edit_option_selected {
                        "*"
                    } else {
                        " "
                    };
                    ListItem::new(render_choice_option_line(
                        option,
                        marker,
                        index == app.edit_option_selected,
                        app.selection_highlight_mode,
                    ))
                })
                .collect::<Vec<_>>();
            let list = List::new(items).block(Block::default().borders(Borders::ALL));
            frame.render_widget(list, chunks[1]);
        }
    }
}

fn render_choice_option_line(
    option: &str,
    marker: &str,
    selected: bool,
    mode: SelectionHighlightMode,
) -> Line<'static> {
    let base = format!("  ({marker}) {option}");
    if !selected {
        return Line::from(base);
    }
    let selected_label = if mode == SelectionHighlightMode::Fallback {
        format!(">>({marker}) {option}<<")
    } else {
        format!("  ({marker}) {option}")
    };
    Line::from(Span::styled(selected_label, selected_highlight_style(mode)))
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn root_entries(catalog: &Catalog) -> Vec<MenuEntry> {
    vec![
        nav_entry(
            &catalog.t("menu.core.title"),
            &catalog.t("menu.nav.core.desc"),
            Screen::Core,
        ),
        nav_entry(
            &catalog.t("menu.storage.title"),
            &catalog.t("menu.nav.storage.desc"),
            Screen::Storage,
        ),
        nav_entry(
            &catalog.t("menu.model_plane.title"),
            &catalog.t("menu.nav.model_plane.desc"),
            Screen::ModelPlane,
        ),
        nav_entry(
            &catalog.t("menu.vault.title"),
            &catalog.t("menu.nav.vault.desc"),
            Screen::Vault,
        ),
        nav_entry(
            &catalog.t("menu.targets.title"),
            &catalog.t("menu.nav.targets.desc"),
            Screen::Targets,
        ),
        nav_entry(
            &catalog.t("menu.security.title"),
            &catalog.t("menu.nav.security.desc"),
            Screen::Security,
        ),
    ]
}

fn core_entries(settings: &CoreSettings, catalog: &Catalog) -> Vec<MenuEntry> {
    vec![
        edit_entry(
            &catalog.t("menu.core.instance_name"),
            "core.instance_name",
            &settings.core.instance_name,
            &catalog.t("menu.core.instance_name.desc"),
        ),
        info_entry(
            &catalog.t("menu.core.data_dir"),
            Some(settings.core.data_dir.clone()),
            &catalog.t("menu.core.data_dir.desc"),
        ),
        edit_entry(
            &catalog.t("menu.core.log_level"),
            "core.log_level",
            &settings.core.log_level,
            &catalog.t("menu.core.log_level.desc"),
        ),
        edit_entry(
            &catalog.t("menu.core.operator_locale"),
            "core.operator_locale",
            &settings.core.operator_locale,
            &catalog.t("menu.core.operator_locale.desc"),
        ),
    ]
}

fn storage_entries(settings: &CoreSettings, catalog: &Catalog) -> Vec<MenuEntry> {
    vec![
        edit_entry(
            &catalog.t("menu.storage.backend"),
            "storage.artifacts.backend",
            &settings.storage.artifacts.backend,
            &catalog.t("menu.storage.backend.desc"),
        ),
        edit_entry(
            &catalog.t("menu.storage.max_bytes"),
            "storage.artifacts.max_bytes",
            &settings.storage.artifacts.max_bytes.to_string(),
            &catalog.t("menu.storage.max_bytes.desc"),
        ),
    ]
}

fn model_plane_entries(settings: &CoreSettings, catalog: &Catalog) -> Vec<MenuEntry> {
    vec![
        edit_entry(
            &catalog.t("menu.model.http_host"),
            "model_plane.http.host",
            &settings.model_plane.http.host,
            &catalog.t("menu.model.http_host.desc"),
        ),
        edit_entry(
            &catalog.t("menu.model.http_port"),
            "model_plane.http.port",
            &settings.model_plane.http.port.to_string(),
            &catalog.t("menu.model.http_port.desc"),
        ),
        edit_entry(
            &catalog.t("menu.model.allow_non_loopback"),
            "model_plane.http.allow_non_loopback",
            &settings.model_plane.http.allow_non_loopback.to_string(),
            &catalog.t("menu.model.allow_non_loopback.desc"),
        ),
    ]
}

fn vault_entries(settings: &CoreSettings, catalog: &Catalog) -> Vec<MenuEntry> {
    vec![
        info_entry(
            &catalog.t("menu.vault.backend"),
            Some(settings.vault.backend.clone()),
            &catalog.t("menu.vault.backend.desc"),
        ),
        edit_entry(
            &catalog.t("menu.vault.trigger"),
            "vault.unlock.trigger_policy",
            &settings.vault.unlock.trigger_policy,
            &catalog.t("menu.vault.trigger.desc"),
        ),
        info_entry(
            &catalog.t("menu.vault.preferred"),
            Some(settings.vault.unlock.preferred_method.clone()),
            &catalog.t("menu.vault.preferred.desc"),
        ),
        info_entry(
            &catalog.t("menu.vault.allowed"),
            Some(settings.vault.unlock.allowed_methods.join(",")),
            &catalog.t("menu.vault.allowed.desc"),
        ),
    ]
}

fn security_entries(summary: &SecuritySummary, catalog: &Catalog) -> Vec<MenuEntry> {
    let mut entries = vec![
        info_entry(
            &catalog.t("menu.security.lock_state"),
            Some(summary.lock_state.clone()),
            &catalog.t("menu.security.lock_state.desc"),
        ),
        info_entry(
            &catalog.t("menu.security.backend"),
            Some(summary.backend.clone()),
            &catalog.t("menu.security.backend.desc"),
        ),
        info_entry(
            &catalog.t("menu.security.preferred"),
            Some(summary.preferred_method.clone()),
            &catalog.t("menu.security.preferred.desc"),
        ),
        info_entry(
            &catalog.t("menu.security.secret_count"),
            Some(summary.secret_count.to_string()),
            &catalog.t("menu.security.secret_count.desc"),
        ),
        info_entry(
            &catalog.t("menu.security.ssh_key_count"),
            Some(summary.ssh_key_count.to_string()),
            &catalog.t("menu.security.ssh_key_count.desc"),
        ),
        info_entry(
            &catalog.t("menu.security.token_count"),
            Some(summary.token_count.to_string()),
            &catalog.t("menu.security.token_count.desc"),
        ),
    ];
    match summary.lock_state.as_str() {
        "uninitialized" => {
            entries.push(action_entry(
                &catalog.t("menu.security.init_action"),
                &catalog.t("menu.security.init_action.desc"),
                ActionKind::InitVault,
            ));
        }
        "locked" => {
            entries.push(action_entry(
                &catalog.t("menu.security.unlock_action"),
                &catalog.t("menu.security.unlock_action.desc"),
                ActionKind::UnlockVault,
            ));
            entries.push(action_entry(
                &catalog.t("menu.security.delete_vault_action"),
                &catalog.t("menu.security.delete_vault_action.desc"),
                ActionKind::DeleteVault,
            ));
        }
        "unlocked" => {
            entries.push(info_entry(
                &catalog.t("menu.security.unlocked_notice"),
                None,
                &catalog.t("menu.security.unlocked_notice.desc"),
            ));
            entries.push(action_entry(
                &catalog.t("menu.security.import_ssh_key_action"),
                &catalog.t("menu.security.import_ssh_key_action.desc"),
                ActionKind::OpenSshKeyImport,
            ));
            entries.push(action_entry(
                &catalog.t("menu.security.ssh_key_management_action"),
                &catalog.t("menu.security.ssh_key_management_action.desc"),
                ActionKind::OpenSshKeyManagement,
            ));
            entries.push(action_entry(
                &catalog.t("menu.security.create_token_action"),
                &catalog.t("menu.security.create_token_action.desc"),
                ActionKind::CreateToken,
            ));
            entries.push(action_entry(
                &catalog.t("menu.security.token_management_action"),
                &catalog.t("menu.security.token_management_action.desc"),
                ActionKind::OpenTokenManagement,
            ));
            entries.push(action_entry(
                &catalog.t("menu.security.delete_vault_action"),
                &catalog.t("menu.security.delete_vault_action.desc"),
                ActionKind::DeleteVault,
            ));
        }
        _ => {}
    }
    entries
}

fn ssh_key_import_entries(app: &MenuConfigApp, catalog: &Catalog) -> Vec<MenuEntry> {
    if app.security_summary.lock_state != "unlocked" {
        return vec![info_entry(
            &catalog.t("menu.ssh_key.management.locked_hint"),
            None,
            &catalog.t("menu.ssh_key.management.locked_hint.desc"),
        )];
    }
    let mut entries = vec![
        MenuEntry {
            label: catalog.t("menu.ssh_key.import.key_name"),
            value: Some(app.ssh_import_draft.key_name.clone()),
            description: catalog.t("menu.ssh_key.import.key_name.desc"),
            dirty_key: None,
            kind: MenuEntryKind::EditField(SSH_IMPORT_KEY_NAME_FIELD.to_string()),
        },
        MenuEntry {
            label: catalog.t("menu.ssh_key.import.label"),
            value: Some(app.ssh_import_draft.label.clone()),
            description: catalog.t("menu.ssh_key.import.label.desc"),
            dirty_key: None,
            kind: MenuEntryKind::EditField(SSH_IMPORT_LABEL_FIELD.to_string()),
        },
        MenuEntry {
            label: catalog.t("menu.ssh_key.import.source_path"),
            value: Some(app.ssh_import_draft.source_path.clone()),
            description: catalog.t("menu.ssh_key.import.source_path.desc"),
            dirty_key: None,
            kind: MenuEntryKind::EditField(SSH_IMPORT_SOURCE_PATH_FIELD.to_string()),
        },
        MenuEntry {
            label: catalog.t("menu.ssh_key.import.passphrase"),
            value: Some(if app.ssh_import_draft.passphrase.is_some() {
                catalog.t("menu.ssh_key.import.passphrase_set")
            } else {
                catalog.t("menu.ssh_key.import.passphrase_unset")
            }),
            description: catalog.t("menu.ssh_key.import.passphrase.desc"),
            dirty_key: None,
            kind: MenuEntryKind::EditField(SSH_IMPORT_PASSPHRASE_FIELD.to_string()),
        },
    ];
    if let Ok(preview) = canonical_ssh_private_key_ref_from_key_name(&app.ssh_import_draft.key_name)
    {
        entries.push(info_entry(
            &catalog.t("menu.ssh_key.import.preview_ref"),
            Some(preview),
            &catalog.t("menu.ssh_key.import.preview_ref.desc"),
        ));
    }
    if let Some(result) = app.ssh_import_last_result.as_ref() {
        entries.push(info_entry(
            &catalog.t("menu.ssh_key.import.result_ref"),
            Some(result.credential_ref.clone()),
            &catalog.t("menu.ssh_key.import.result_ref.desc"),
        ));
        entries.push(info_entry(
            &catalog.t("menu.ssh_key.import.result_version"),
            Some(result.active_version.clone()),
            &catalog.t("menu.ssh_key.import.result_version.desc"),
        ));
        entries.push(info_entry(
            &catalog.t("menu.ssh_key.import.result_status"),
            Some(result.status.clone()),
            &catalog.t("menu.ssh_key.import.result_status.desc"),
        ));
    }
    entries.push(action_entry(
        &catalog.t("menu.ssh_key.import.execute"),
        &catalog.t("menu.ssh_key.import.execute.desc"),
        ActionKind::ExecuteSshKeyImport,
    ));
    entries
}

fn ssh_key_management_entries(app: &MenuConfigApp, catalog: &Catalog) -> Vec<MenuEntry> {
    if app.security_summary.lock_state != "unlocked" {
        return vec![info_entry(
            &catalog.t("menu.ssh_key.management.locked_hint"),
            None,
            &catalog.t("menu.ssh_key.management.locked_hint.desc"),
        )];
    }
    if app.ssh_key_rows.is_empty() {
        return vec![info_entry(
            &catalog.t("menu.ssh_key.management.empty"),
            None,
            &catalog.t("menu.ssh_key.management.empty.desc"),
        )];
    }
    app.ssh_key_rows
        .iter()
        .map(|item| {
            let label = if item.label.trim().is_empty() {
                item.credential_ref.clone()
            } else {
                item.label.clone()
            };
            action_entry(
                &format!("{} [{}] {}", label, item.status, item.active_version),
                &catalog.t("menu.ssh_key.management.row.desc"),
                ActionKind::OpenSshKeyDetail(item.credential_ref.clone()),
            )
        })
        .collect()
}

fn ssh_key_detail_entries(
    app: &MenuConfigApp,
    catalog: &Catalog,
    credential_ref: &str,
) -> Vec<MenuEntry> {
    if app.security_summary.lock_state != "unlocked" {
        return vec![info_entry(
            &catalog.t("menu.ssh_key.management.locked_hint"),
            None,
            &catalog.t("menu.ssh_key.management.locked_hint.desc"),
        )];
    }
    let mut entries = Vec::new();
    if let Some(row) = app
        .ssh_key_rows
        .iter()
        .find(|item| item.credential_ref == credential_ref)
    {
        entries.push(info_entry(
            &catalog.t("menu.ssh_key.detail.credential_ref"),
            Some(row.credential_ref.clone()),
            &catalog.t("menu.ssh_key.detail.credential_ref.desc"),
        ));
        entries.push(info_entry(
            &catalog.t("menu.ssh_key.detail.label"),
            Some(row.label.clone()),
            &catalog.t("menu.ssh_key.detail.label.desc"),
        ));
        entries.push(info_entry(
            &catalog.t("menu.ssh_key.detail.kind"),
            Some("ssh-private-key".to_string()),
            &catalog.t("menu.ssh_key.detail.kind.desc"),
        ));
        entries.push(info_entry(
            &catalog.t("menu.ssh_key.detail.status"),
            Some(row.status.clone()),
            &catalog.t("menu.ssh_key.detail.status.desc"),
        ));
        entries.push(info_entry(
            &catalog.t("menu.ssh_key.detail.active_version"),
            Some(row.active_version.clone()),
            &catalog.t("menu.ssh_key.detail.active_version.desc"),
        ));
    }
    if let Some(router) = app.vault_router.as_ref() {
        if let Ok(Some(metadata)) = router.secret_metadata(credential_ref) {
            entries.push(info_entry(
                &catalog.t("menu.ssh_key.detail.last_rotated"),
                Some(
                    metadata
                        .record
                        .last_rotated_at
                        .and_then(format_short_utc_date)
                        .unwrap_or_else(|| "-".to_string()),
                ),
                &catalog.t("menu.ssh_key.detail.last_rotated.desc"),
            ));
            entries.push(info_entry(
                &catalog.t("menu.ssh_key.detail.last_used"),
                Some(
                    metadata
                        .record
                        .last_used_at
                        .and_then(format_short_utc_date)
                        .unwrap_or_else(|| "-".to_string()),
                ),
                &catalog.t("menu.ssh_key.detail.last_used.desc"),
            ));
        }
    }
    entries.push(action_entry(
        &catalog.t("menu.ssh_key.detail.delete"),
        &catalog.t("menu.ssh_key.detail.delete.desc"),
        ActionKind::DeleteSshKey(credential_ref.to_string()),
    ));
    entries
}

fn target_credential_source_entries(
    app: &MenuConfigApp,
    index: usize,
    catalog: &Catalog,
) -> Vec<MenuEntry> {
    let Some(target) = app.settings.targets.get(index) else {
        return target_missing_entries(catalog);
    };
    if !matches!(target.kind, TargetKind::Ssh) {
        return vec![info_entry(
            &catalog.t("menu.target.credential_source_ssh_only"),
            None,
            &catalog.t("menu.target.credential_source_ssh_only.desc"),
        )];
    }
    if is_sensitive_target(target) && app.security_summary.lock_state != "unlocked" {
        return vec![
            info_entry(
                &catalog.t("menu.target.sensitive_locked_hint"),
                None,
                &catalog.t("menu.target.sensitive_locked_hint.desc"),
            ),
            action_entry(
                &catalog.t("menu.security.unlock_action"),
                &catalog.t("menu.security.unlock_action.desc"),
                ActionKind::UnlockVault,
            ),
        ];
    }
    vec![
        info_entry(
            &catalog.t("menu.target.credential_ref"),
            Some(target.credential_ref.as_deref().unwrap_or("").to_string()),
            &catalog.t("menu.target.credential_ref.desc"),
        ),
        action_entry(
            &catalog.t("menu.target.credential_picker"),
            &catalog.t("menu.target.credential_picker.desc"),
            ActionKind::OpenTargetCredentialPicker(index),
        ),
        action_entry(
            &catalog.t("menu.target.import_local_ssh_key"),
            &catalog.t("menu.target.import_local_ssh_key.desc"),
            ActionKind::ImportLocalSshKeyIntoVault(index),
        ),
        action_entry(
            &catalog.t("menu.target.manual_credential_ref"),
            &catalog.t("menu.target.manual_credential_ref.desc"),
            ActionKind::EditTargetCredentialRef(index),
        ),
        action_entry(
            &catalog.t("menu.target.clear_credential_ref"),
            &catalog.t("menu.target.clear_credential_ref.desc"),
            ActionKind::ClearTargetCredentialRef(index),
        ),
    ]
}

fn target_credential_picker_entries(
    app: &MenuConfigApp,
    index: usize,
    catalog: &Catalog,
) -> Vec<MenuEntry> {
    if app.security_summary.lock_state != "unlocked" {
        return vec![info_entry(
            &catalog.t("menu.ssh_key.management.locked_hint"),
            None,
            &catalog.t("menu.ssh_key.management.locked_hint.desc"),
        )];
    }
    if app.ssh_key_rows.is_empty() {
        return vec![
            info_entry(
                &catalog.t("menu.target.credential_picker_empty"),
                None,
                &catalog.t("menu.target.credential_picker_empty.desc"),
            ),
            action_entry(
                &catalog.t("menu.target.import_local_ssh_key"),
                &catalog.t("menu.target.import_local_ssh_key.desc"),
                ActionKind::ImportLocalSshKeyIntoVault(index),
            ),
        ];
    }
    let selected_ref = app
        .settings
        .targets
        .get(index)
        .and_then(|target| target.credential_ref.as_deref())
        .unwrap_or("");
    let mut entries = app
        .ssh_key_rows
        .iter()
        .map(|item| MenuEntry {
            label: format!("{} [{}] {}", item.label, item.status, item.active_version),
            value: Some(
                if selected_ref == item.credential_ref {
                    "selected"
                } else {
                    "unselected"
                }
                .to_string(),
            ),
            description: catalog.t("menu.target.credential_picker_row.desc"),
            dirty_key: None,
            kind: MenuEntryKind::Action(ActionKind::BindTargetCredentialRef {
                target_index: index,
                credential_ref: item.credential_ref.clone(),
            }),
        })
        .collect::<Vec<_>>();
    entries.push(action_entry(
        &catalog.t("menu.target.import_local_ssh_key"),
        &catalog.t("menu.target.import_local_ssh_key.desc"),
        ActionKind::ImportLocalSshKeyIntoVault(index),
    ));
    entries
}

fn token_management_entries(app: &MenuConfigApp, catalog: &Catalog) -> Vec<MenuEntry> {
    if app.security_summary.lock_state != "unlocked" {
        return vec![info_entry(
            &catalog.t("menu.token_management.locked_hint"),
            None,
            &catalog.t("menu.token_management.locked_hint.desc"),
        )];
    }
    if app.token_rows.is_empty() {
        return vec![info_entry(
            &catalog.t("menu.token_management.empty"),
            None,
            &catalog.t("menu.token_management.empty.desc"),
        )];
    }
    app.token_rows
        .iter()
        .map(|item| {
            let serial = token_serial_from_id(&item.token_id).unwrap_or_else(|| "------".into());
            let label = if item.label.trim().is_empty() {
                catalog.t("menu.token_management.unlabeled")
            } else {
                item.label.clone()
            };
            let expiry = token_expiry_badge(item.expires_at, catalog);
            let status = token_status_badge(&item.status, catalog);
            action_entry(
                &format!("({serial}) {label} {expiry} {status}"),
                &catalog.t("menu.token_management.token_row.desc"),
                ActionKind::OpenTokenDetail(item.token_id.clone()),
            )
        })
        .collect()
}

fn token_detail_entries(app: &MenuConfigApp, catalog: &Catalog, token_id: &str) -> Vec<MenuEntry> {
    if app.security_summary.lock_state != "unlocked" {
        return vec![info_entry(
            &catalog.t("menu.token_management.locked_hint"),
            None,
            &catalog.t("menu.token_management.locked_hint.desc"),
        )];
    }
    let Some(item) = app.token_rows.iter().find(|row| row.token_id == token_id) else {
        return vec![info_entry(
            &catalog.t("menu.token_detail.missing"),
            Some(token_id.to_string()),
            &catalog.t("menu.token_detail.missing.desc"),
        )];
    };

    let mut entries = vec![
        info_entry(
            &catalog.t("menu.token_detail.token_id"),
            Some(item.token_id.clone()),
            &catalog.t("menu.token_detail.token_id.desc"),
        ),
        action_entry(
            &format!("{} = {}", catalog.t("menu.token_detail.label"), item.label),
            &catalog.t("menu.token_detail.label.desc"),
            ActionKind::EditTokenLabel(item.token_id.clone()),
        ),
        info_entry(
            &catalog.t("menu.token_detail.fingerprint"),
            Some(item.token_fingerprint.clone()),
            &catalog.t("menu.token_detail.fingerprint.desc"),
        ),
        info_entry(
            &catalog.t("menu.token_detail.expiry"),
            Some(token_expiry_badge(item.expires_at, catalog)),
            &catalog.t("menu.token_detail.expiry.desc"),
        ),
        info_entry(
            &catalog.t("menu.token_detail.status"),
            Some(token_status_badge(&item.status, catalog)),
            &catalog.t("menu.token_detail.status.desc"),
        ),
    ];

    match item.status.to_ascii_lowercase().as_str() {
        "active" => entries.push(MenuEntry {
            label: format!(
                "{} = {}",
                catalog.t("menu.token_detail.access_switch"),
                catalog.t("menu.token_detail.access_state_enabled")
            ),
            value: Some("enabled".into()),
            description: catalog.t("menu.token_detail.access_switch.desc"),
            dirty_key: None,
            kind: MenuEntryKind::Action(ActionKind::ToggleTokenAccess(item.token_id.clone())),
        }),
        "disabled" => entries.push(MenuEntry {
            label: format!(
                "{} = {}",
                catalog.t("menu.token_detail.access_switch"),
                catalog.t("menu.token_detail.access_state_disabled")
            ),
            value: Some("disabled".into()),
            description: catalog.t("menu.token_detail.access_switch.desc"),
            dirty_key: None,
            kind: MenuEntryKind::Action(ActionKind::ToggleTokenAccess(item.token_id.clone())),
        }),
        _ => entries.push(info_entry(
            &catalog.t("menu.token_detail.access_readonly"),
            None,
            &catalog.t("menu.token_detail.access_readonly.desc"),
        )),
    }

    entries.push(action_entry(
        &catalog.t("menu.token_detail.permissions_action"),
        &catalog.t("menu.token_detail.permissions_action.desc"),
        ActionKind::OpenTokenPermissions(item.token_id.clone()),
    ));

    if item.status.to_ascii_lowercase() != "revoked" {
        entries.push(action_entry(
            &catalog.t("menu.token_detail.revoke_action"),
            &catalog.t("menu.token_detail.revoke_action.desc"),
            ActionKind::RevokeToken(item.token_id.clone()),
        ));
    }

    if item.status.to_ascii_lowercase() == "revoked" {
        entries.push(action_entry(
            &catalog.tf(
                "menu.token_management.delete_action",
                &[("id", &item.token_id)],
            ),
            &catalog.t("menu.token_management.delete_action.desc"),
            ActionKind::DeleteToken(item.token_id.clone()),
        ));
    } else {
        entries.push(info_entry(
            &catalog.t("menu.token_detail.delete_guard"),
            None,
            &catalog.t("menu.token_detail.delete_guard.desc"),
        ));
    }

    if let Some(reason) = item.revoke_reason.as_ref() {
        entries.push(info_entry(
            &catalog.t("menu.token_detail.revoke_reason"),
            Some(reason.clone()),
            &catalog.t("menu.token_detail.revoke_reason.desc"),
        ));
    }
    entries
}

fn token_serial_from_id(token_id: &str) -> Option<String> {
    let suffix = token_id.rsplit_once('-')?.1;
    if suffix.len() == 6 && suffix.chars().all(|ch| ch.is_ascii_digit()) {
        Some(suffix.to_string())
    } else {
        None
    }
}

fn token_expiry_badge(expires_at: Option<SystemTime>, catalog: &Catalog) -> String {
    match expires_at.and_then(format_short_utc_date) {
        Some(value) => format!("[{value}]"),
        None => catalog.t("menu.token_management.expiry.long_lived"),
    }
}

fn token_status_badge(status: &str, catalog: &Catalog) -> String {
    let key = match status.trim().to_ascii_lowercase().as_str() {
        "active" => "menu.token_management.status.active",
        "disabled" => "menu.token_management.status.disabled",
        "revoked" => "menu.token_management.status.revoked",
        "expired" => "menu.token_management.status.expired",
        _ => "menu.token_management.status.unknown",
    };
    format!("[{}]", catalog.t(key))
}

fn format_short_utc_date(value: SystemTime) -> Option<String> {
    let secs = value.duration_since(SystemTime::UNIX_EPOCH).ok()?.as_secs();
    let days = i64::try_from(secs / 86_400).ok()?;
    let (year, month, day) = civil_from_days(days);
    Some(format!(
        "{:02}-{:02}-{:02}",
        year.rem_euclid(100),
        month,
        day
    ))
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += if month <= 2 { 1 } else { 0 };
    (year, month as u32, day as u32)
}

fn targets_entries(app: &MenuConfigApp, catalog: &Catalog) -> Vec<MenuEntry> {
    let mut entries = app
        .settings
        .targets
        .iter()
        .enumerate()
        .map(|(index, target)| MenuEntry {
            label: format!(
                "{} ({})",
                target.display_name,
                target_kind_label(&target.kind, catalog)
            ),
            value: Some(target.id.clone()),
            description: catalog.t("menu.targets.editor_open"),
            dirty_key: None,
            kind: MenuEntryKind::Navigate(Screen::TargetEditor(index)),
        })
        .collect::<Vec<_>>();
    entries.push(action_entry(
        &catalog.t("menu.targets.add_target"),
        &catalog.t("menu.targets.add_target.desc"),
        ActionKind::OpenAddTarget,
    ));
    entries
}

fn targets_entries_from_settings(settings: &CoreSettings, catalog: &Catalog) -> Vec<MenuEntry> {
    let mut entries = settings
        .targets
        .iter()
        .enumerate()
        .map(|(index, target)| MenuEntry {
            label: format!(
                "{} ({})",
                target.display_name,
                target_kind_label(&target.kind, catalog)
            ),
            value: Some(target.id.clone()),
            description: catalog.t("menu.targets.editor_open"),
            dirty_key: None,
            kind: MenuEntryKind::Navigate(Screen::TargetEditor(index)),
        })
        .collect::<Vec<_>>();
    entries.push(action_entry(
        &catalog.t("menu.targets.add_target"),
        &catalog.t("menu.targets.add_target.desc"),
        ActionKind::OpenAddTarget,
    ));
    entries
}

fn target_add_mode_entries(app: &MenuConfigApp, catalog: &Catalog) -> Vec<MenuEntry> {
    let mut entries = vec![action_entry(
        &catalog.t("menu.targets.mode_plain"),
        &catalog.t("menu.targets.mode_plain.desc"),
        ActionKind::ChoosePlainTargetMode,
    )];
    if app.security_summary.lock_state == "unlocked" {
        entries.push(action_entry(
            &catalog.t("menu.targets.mode_sensitive"),
            &catalog.t("menu.targets.mode_sensitive.desc"),
            ActionKind::ChooseSensitiveTargetMode,
        ));
    } else {
        entries.push(action_entry(
            &catalog.t("menu.targets.mode_sensitive_locked"),
            &catalog.t("menu.targets.mode_sensitive_locked.desc"),
            ActionKind::UnlockVault,
        ));
    }
    entries
}

fn target_add_type_entries(sensitive: bool, catalog: &Catalog) -> Vec<MenuEntry> {
    if sensitive {
        return vec![
            action_entry(
                &catalog.t("menu.targets.add_ssh"),
                &catalog.t("menu.targets.add_sensitive_ssh.desc"),
                ActionKind::AddSensitiveSshTarget,
            ),
            action_entry(
                &catalog.t("menu.targets.add_adb"),
                &catalog.t("menu.targets.add_sensitive_adb.desc"),
                ActionKind::AddSensitiveAdbTarget,
            ),
        ];
    }
    vec![
        action_entry(
            &catalog.t("menu.targets.add_ssh"),
            &catalog.t("menu.targets.add_plain_ssh.desc"),
            ActionKind::AddSshTarget,
        ),
        action_entry(
            &catalog.t("menu.targets.add_adb"),
            &catalog.t("menu.targets.add_plain_adb.desc"),
            ActionKind::AddAdbTarget,
        ),
    ]
}

fn target_editor_entries(app: &MenuConfigApp, index: usize, catalog: &Catalog) -> Vec<MenuEntry> {
    let Some(target) = app.settings.targets.get(index) else {
        return target_missing_entries(catalog);
    };
    let creating = app
        .target_edit_session
        .as_ref()
        .map(|session| session.index == index && session.is_new)
        .unwrap_or(false);
    let sensitive = is_sensitive_target(target);
    let locked_sensitive = sensitive && app.security_summary.lock_state != "unlocked";
    let apply_label = if creating {
        catalog.t("menu.target.create_target")
    } else {
        catalog.t("menu.target.apply_target")
    };
    let apply_desc = if creating {
        catalog.t("menu.target.create_target.desc")
    } else {
        catalog.t("menu.target.apply_target.desc")
    };
    let apply_entry = action_entry(
        &apply_label,
        &apply_desc,
        ActionKind::ApplyTarget(index),
    );
    let mut entries = vec![nav_entry(
        &catalog.t("menu.target.public_descriptor"),
        &catalog.t("menu.target.public_descriptor.desc"),
        Screen::TargetPublicDescriptor(index),
    )];
    if !sensitive {
        entries.push(nav_entry(
            &catalog.t("menu.target.connection_profile"),
            &catalog.t("menu.target.connection_profile.desc"),
            Screen::TargetConnectionProfile(index),
        ));
        entries.push(nav_entry(
            &catalog.t("menu.target.policy"),
            &catalog.t("menu.target.policy.desc"),
            Screen::TargetPolicy(index),
        ));
        entries.push(apply_entry);
        if !creating {
            entries.push(action_entry(
                &catalog.t("menu.target.delete"),
                &catalog.t("menu.target.delete.desc"),
                ActionKind::DeleteTarget(index),
            ));
        }
        return entries;
    }
    if locked_sensitive {
        entries.push(info_entry(
            &catalog.t("menu.target.sensitive_locked_hint"),
            None,
            &catalog.t("menu.target.sensitive_locked_hint.desc"),
        ));
        entries.push(action_entry(
            &catalog.t("menu.security.unlock_action"),
            &catalog.t("menu.security.unlock_action.desc"),
            ActionKind::UnlockVault,
        ));
        return entries;
    }
    entries.push(nav_entry(
        &catalog.t("menu.target.sensitive_overlay"),
        &catalog.t("menu.target.sensitive_overlay.desc"),
        Screen::TargetSensitiveOverlay(index),
    ));
    entries.push(nav_entry(
        &catalog.t("menu.target.policy"),
        &catalog.t("menu.target.policy.desc"),
        Screen::TargetPolicy(index),
    ));
    entries.push(apply_entry);
    if !creating {
        entries.push(action_entry(
            &catalog.t("menu.target.delete"),
            &catalog.t("menu.target.delete.desc"),
            ActionKind::DeleteTarget(index),
        ));
    }
    entries
}

fn target_public_descriptor_entries(
    settings: &CoreSettings,
    index: usize,
    catalog: &Catalog,
) -> Vec<MenuEntry> {
    let Some(target) = settings.targets.get(index) else {
        return target_missing_entries(catalog);
    };
    vec![
        info_entry(
            &catalog.t("menu.target.kind"),
            Some(target_kind_label(&target.kind, catalog)),
            &catalog.t("menu.target.kind.desc"),
        ),
        edit_entry(
            &catalog.t("menu.target.id"),
            &format!("targets[{index}].id"),
            &target.id,
            &catalog.t("menu.target.id.desc"),
        ),
        edit_entry(
            &catalog.t("menu.target.display_name"),
            &format!("targets[{index}].display_name"),
            &target.display_name,
            &catalog.t("menu.target.display_name.desc"),
        ),
        edit_entry(
            &catalog.t("menu.target.enabled"),
            &format!("targets[{index}].enabled"),
            &target.enabled.to_string(),
            &catalog.t("menu.target.enabled.desc"),
        ),
        edit_entry(
            &catalog.t("menu.target.aliases"),
            &format!("targets[{index}].aliases"),
            &target.aliases.join(","),
            &catalog.t("menu.target.aliases.desc"),
        ),
    ]
}

fn target_connection_profile_entries(
    settings: &CoreSettings,
    index: usize,
    catalog: &Catalog,
) -> Vec<MenuEntry> {
    let Some(target) = settings.targets.get(index) else {
        return target_missing_entries(catalog);
    };
    if is_sensitive_target(target) {
        return vec![info_entry(
            &catalog.t("menu.target.sensitive_only"),
            None,
            &catalog.t("menu.target.sensitive_only.desc"),
        )];
    }
    let mut entries = vec![edit_entry(
        &catalog.t("menu.target.notes"),
        &format!("targets[{index}].notes"),
        target.notes.as_deref().unwrap_or(""),
        &catalog.t("menu.target.notes.desc"),
    )];
    match &target.kind {
        TargetKind::Ssh => {
            entries.insert(
                0,
                action_entry(
                    &catalog.t("menu.target.credential_source"),
                    &catalog.t("menu.target.credential_source.desc"),
                    ActionKind::OpenTargetCredentialSource(index),
                ),
            );
            entries.insert(
                1,
                info_entry(
                    &catalog.t("menu.target.credential_ref"),
                    Some(target.credential_ref.as_deref().unwrap_or("").to_string()),
                    &catalog.t("menu.target.credential_ref.desc"),
                ),
            );
            entries.push(edit_entry(
                &catalog.t("menu.target.ssh_host"),
                &format!("targets[{index}].connection.host"),
                target.connection.host.as_deref().unwrap_or(""),
                &catalog.t("menu.target.ssh_host.desc"),
            ));
            entries.push(edit_entry(
                &catalog.t("menu.target.ssh_port"),
                &format!("targets[{index}].connection.port"),
                &target
                    .connection
                    .port
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
                &catalog.t("menu.target.ssh_port.desc"),
            ));
            entries.push(edit_entry(
                &catalog.t("menu.target.ssh_username"),
                &format!("targets[{index}].connection.username"),
                target.connection.username.as_deref().unwrap_or(""),
                &catalog.t("menu.target.ssh_username.desc"),
            ));
        }
        TargetKind::Adb => {
            entries.insert(
                0,
                edit_entry(
                    &catalog.t("menu.target.credential_ref"),
                    &format!("targets[{index}].credential_ref"),
                    target.credential_ref.as_deref().unwrap_or(""),
                    &catalog.t("menu.target.credential_ref.desc"),
                ),
            );
            entries.push(edit_entry(
                &catalog.t("menu.target.selector_kind"),
                &format!("targets[{index}].connection.selector_kind"),
                target.connection.selector_kind.as_deref().unwrap_or(""),
                &catalog.t("menu.target.selector_kind.desc"),
            ));
            entries.push(edit_entry(
                &catalog.t("menu.target.selector_value"),
                &format!("targets[{index}].connection.selector_value"),
                target.connection.selector_value.as_deref().unwrap_or(""),
                &catalog.t("menu.target.selector_value.desc"),
            ));
        }
        _ => {
            entries.insert(
                0,
                edit_entry(
                    &catalog.t("menu.target.credential_ref"),
                    &format!("targets[{index}].credential_ref"),
                    target.credential_ref.as_deref().unwrap_or(""),
                    &catalog.t("menu.target.credential_ref.desc"),
                ),
            );
        }
    }
    entries
}

fn target_sensitive_overlay_entries(
    app: &MenuConfigApp,
    index: usize,
    catalog: &Catalog,
) -> Vec<MenuEntry> {
    let Some(target) = app.settings.targets.get(index) else {
        return target_missing_entries(catalog);
    };
    if !is_sensitive_target(target) {
        return vec![info_entry(
            &catalog.t("menu.target.plain_only"),
            None,
            &catalog.t("menu.target.plain_only.desc"),
        )];
    }
    if app.security_summary.lock_state != "unlocked" {
        return vec![
            info_entry(
                &catalog.t("menu.target.sensitive_locked_hint"),
                None,
                &catalog.t("menu.target.sensitive_locked_hint.desc"),
            ),
            action_entry(
                &catalog.t("menu.security.unlock_action"),
                &catalog.t("menu.security.unlock_action.desc"),
                ActionKind::UnlockVault,
            ),
        ];
    }
    target_sensitive_overlay_entries_from_settings(&app.settings, index, catalog)
}

fn target_sensitive_overlay_entries_from_settings(
    settings: &CoreSettings,
    index: usize,
    catalog: &Catalog,
) -> Vec<MenuEntry> {
    let Some(target) = settings.targets.get(index) else {
        return target_missing_entries(catalog);
    };
    let mut entries = Vec::new();
    if target.kind == TargetKind::Ssh {
        entries.push(action_entry(
            &catalog.t("menu.target.credential_source"),
            &catalog.t("menu.target.credential_source.desc"),
            ActionKind::OpenTargetCredentialSource(index),
        ));
        entries.push(info_entry(
            &catalog.t("menu.target.credential_ref"),
            Some(target.credential_ref.as_deref().unwrap_or("").to_string()),
            &catalog.t("menu.target.credential_ref.desc"),
        ));
    } else {
        entries.push(edit_entry(
            &catalog.t("menu.target.credential_ref"),
            &format!("targets[{index}].credential_ref"),
            target.credential_ref.as_deref().unwrap_or(""),
            &catalog.t("menu.target.credential_ref.desc"),
        ));
    }
    entries.push(edit_entry(
        &catalog.t("menu.target.notes"),
        &format!("targets[{index}].notes"),
        target.notes.as_deref().unwrap_or(""),
        &catalog.t("menu.target.notes.desc"),
    ));
    match &target.kind {
        TargetKind::Ssh => {
            entries.push(edit_entry(
                &catalog.t("menu.target.ssh_host"),
                &format!("targets[{index}].connection.host"),
                target.connection.host.as_deref().unwrap_or(""),
                &catalog.t("menu.target.ssh_host.desc"),
            ));
            entries.push(edit_entry(
                &catalog.t("menu.target.ssh_port"),
                &format!("targets[{index}].connection.port"),
                &target
                    .connection
                    .port
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
                &catalog.t("menu.target.ssh_port.desc"),
            ));
            entries.push(edit_entry(
                &catalog.t("menu.target.ssh_username"),
                &format!("targets[{index}].connection.username"),
                target.connection.username.as_deref().unwrap_or(""),
                &catalog.t("menu.target.ssh_username.desc"),
            ));
        }
        TargetKind::Adb => {
            entries.push(edit_entry(
                &catalog.t("menu.target.selector_kind"),
                &format!("targets[{index}].connection.selector_kind"),
                target.connection.selector_kind.as_deref().unwrap_or(""),
                &catalog.t("menu.target.selector_kind.desc"),
            ));
            entries.push(edit_entry(
                &catalog.t("menu.target.selector_value"),
                &format!("targets[{index}].connection.selector_value"),
                target.connection.selector_value.as_deref().unwrap_or(""),
                &catalog.t("menu.target.selector_value.desc"),
            ));
        }
        _ => {}
    }
    entries
}

fn target_policy_entries(app: &MenuConfigApp, index: usize, catalog: &Catalog) -> Vec<MenuEntry> {
    let Some(target) = app.settings.targets.get(index) else {
        return target_missing_entries(catalog);
    };
    if is_sensitive_target(target) && app.security_summary.lock_state != "unlocked" {
        return vec![
            info_entry(
                &catalog.t("menu.target.sensitive_locked_hint"),
                None,
                &catalog.t("menu.target.sensitive_locked_hint.desc"),
            ),
            action_entry(
                &catalog.t("menu.security.unlock_action"),
                &catalog.t("menu.security.unlock_action.desc"),
                ActionKind::UnlockVault,
            ),
        ];
    }
    vec![
        edit_entry(
            &catalog.t("menu.target.storage_class"),
            &format!("targets[{index}].storage_class"),
            &target.storage_class,
            &catalog.t("menu.target.storage_class.desc"),
        ),
        edit_entry(
            &catalog.t("menu.target.access_class"),
            &format!("targets[{index}].access_class"),
            &target.access_class,
            &catalog.t("menu.target.access_class.desc"),
        ),
        edit_entry(
            &catalog.t("menu.target.sealed_profile_ref"),
            &format!("targets[{index}].sealed_profile_ref"),
            target.sealed_profile_ref.as_deref().unwrap_or(""),
            &catalog.t("menu.target.sealed_profile_ref.desc"),
        ),
    ]
}

fn target_editable_entries_from_settings(
    settings: &CoreSettings,
    index: usize,
    catalog: &Catalog,
) -> Vec<MenuEntry> {
    let Some(target) = settings.targets.get(index) else {
        return Vec::new();
    };
    let mut entries = target_public_descriptor_entries(settings, index, catalog)
        .into_iter()
        .filter(|entry| matches!(entry.kind, MenuEntryKind::EditField(_)))
        .collect::<Vec<_>>();
    if is_sensitive_target(target) {
        entries.extend(
            target_sensitive_overlay_entries_from_settings(settings, index, catalog)
                .into_iter()
                .filter(|entry| matches!(entry.kind, MenuEntryKind::EditField(_))),
        );
    } else {
        entries.extend(
            target_connection_profile_entries(settings, index, catalog)
                .into_iter()
                .filter(|entry| matches!(entry.kind, MenuEntryKind::EditField(_))),
        );
    }
    entries.extend(
        target_policy_entries_from_settings(settings, index, catalog)
            .into_iter()
            .filter(|entry| matches!(entry.kind, MenuEntryKind::EditField(_))),
    );
    entries
}

fn target_policy_entries_from_settings(
    settings: &CoreSettings,
    index: usize,
    catalog: &Catalog,
) -> Vec<MenuEntry> {
    let Some(target) = settings.targets.get(index) else {
        return target_missing_entries(catalog);
    };
    vec![
        edit_entry(
            &catalog.t("menu.target.storage_class"),
            &format!("targets[{index}].storage_class"),
            &target.storage_class,
            &catalog.t("menu.target.storage_class.desc"),
        ),
        edit_entry(
            &catalog.t("menu.target.access_class"),
            &format!("targets[{index}].access_class"),
            &target.access_class,
            &catalog.t("menu.target.access_class.desc"),
        ),
        edit_entry(
            &catalog.t("menu.target.sealed_profile_ref"),
            &format!("targets[{index}].sealed_profile_ref"),
            target.sealed_profile_ref.as_deref().unwrap_or(""),
            &catalog.t("menu.target.sealed_profile_ref.desc"),
        ),
    ]
}

fn target_missing_entries(catalog: &Catalog) -> Vec<MenuEntry> {
    vec![info_entry(
        &catalog.t("menu.target.missing"),
        None,
        &catalog.t("menu.target.missing.desc"),
    )]
}

fn search_entries(settings: &CoreSettings, query: &str, catalog: &Catalog) -> Vec<MenuEntry> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return vec![info_entry(
            &catalog.t("menu.search.no_query"),
            None,
            &catalog.t("menu.search.no_query.desc"),
        )];
    }

    let mut entries = Vec::new();
    for entry in core_entries(settings, catalog)
        .into_iter()
        .chain(storage_entries(settings, catalog))
        .chain(model_plane_entries(settings, catalog))
        .chain(vault_entries(settings, catalog))
        .chain(targets_entries_from_settings(settings, catalog))
        .chain(flatten_target_fields(settings, catalog))
    {
        let text = format!(
            "{} {} {} {}",
            entry.label,
            entry.value.clone().unwrap_or_default(),
            entry.description,
            match &entry.kind {
                MenuEntryKind::EditField(field) => field.clone(),
                _ => String::new(),
            }
        )
        .to_ascii_lowercase();
        if text.contains(&query) {
            entries.push(match entry.kind {
                MenuEntryKind::EditField(field) => MenuEntry {
                    label: entry.label,
                    value: entry.value,
                    description: entry.description,
                    dirty_key: entry.dirty_key.clone(),
                    kind: MenuEntryKind::FocusField {
                        screen: screen_for_field(settings, &field),
                        field,
                    },
                },
                MenuEntryKind::Navigate(Screen::TargetEditor(index)) => MenuEntry {
                    label: entry.label,
                    value: entry.value,
                    description: entry.description,
                    dirty_key: None,
                    kind: MenuEntryKind::Navigate(Screen::TargetEditor(index)),
                },
                _ => entry,
            });
        }
    }

    if entries.is_empty() {
        entries.push(info_entry(
            &catalog.t("menu.search.no_matches"),
            None,
            &catalog.t("menu.search.no_matches.desc"),
        ));
    }
    entries
}

fn flatten_target_fields(settings: &CoreSettings, catalog: &Catalog) -> Vec<MenuEntry> {
    let mut entries = Vec::new();
    for index in 0..settings.targets.len() {
        entries.extend(target_editable_entries_from_settings(
            settings, index, catalog,
        ));
    }
    entries
}

fn nav_entry(label: &str, description: &str, screen: Screen) -> MenuEntry {
    MenuEntry {
        label: label.to_string(),
        value: None,
        description: description.to_string(),
        dirty_key: None,
        kind: MenuEntryKind::Navigate(screen),
    }
}

fn edit_entry(label: &str, field: &str, value: &str, description: &str) -> MenuEntry {
    MenuEntry {
        label: label.to_string(),
        value: Some(value.to_string()),
        description: description.to_string(),
        dirty_key: Some(field.to_string()),
        kind: MenuEntryKind::EditField(field.to_string()),
    }
}

fn action_entry(label: &str, description: &str, action: ActionKind) -> MenuEntry {
    MenuEntry {
        label: label.to_string(),
        value: None,
        description: description.to_string(),
        dirty_key: None,
        kind: MenuEntryKind::Action(action),
    }
}

fn info_entry(label: &str, value: Option<String>, description: &str) -> MenuEntry {
    MenuEntry {
        label: label.to_string(),
        value,
        description: description.to_string(),
        dirty_key: None,
        kind: MenuEntryKind::Info,
    }
}

fn field_value(settings: &CoreSettings, field: &str) -> Option<String> {
    match field {
        "core.instance_name" => Some(settings.core.instance_name.clone()),
        "core.log_level" => Some(settings.core.log_level.clone()),
        "core.operator_locale" => Some(settings.core.operator_locale.clone()),
        "storage.artifacts.backend" => Some(settings.storage.artifacts.backend.clone()),
        "storage.artifacts.max_bytes" => Some(settings.storage.artifacts.max_bytes.to_string()),
        "model_plane.http.host" => Some(settings.model_plane.http.host.clone()),
        "model_plane.http.port" => Some(settings.model_plane.http.port.to_string()),
        "model_plane.http.allow_non_loopback" => {
            Some(settings.model_plane.http.allow_non_loopback.to_string())
        }
        "vault.unlock.trigger_policy" => Some(settings.vault.unlock.trigger_policy.clone()),
        _ => target_field_value(settings, field),
    }
}

fn field_options(field: &str) -> Option<Vec<String>> {
    let options = match field {
        "core.log_level" => vec!["trace", "debug", "info", "warn", "error"],
        "core.operator_locale" => vec!["en-US", "zh-CN"],
        "storage.artifacts.backend" => vec!["memory", "filesystem"],
        "vault.unlock.trigger_policy" => vec![
            "on-first-secret-access",
            "on-core-start",
            "on-every-secret-access",
            "manual-only",
        ],
        "model_plane.http.allow_non_loopback" => vec!["false", "true"],
        _ => {
            if let Some((_, suffix)) = parse_target_field(field) {
                if suffix == "enabled" {
                    vec!["false", "true"]
                } else {
                    return None;
                }
            } else {
                return None;
            }
        }
    };
    Some(options.into_iter().map(|value| value.to_string()).collect())
}

fn is_boolean_toggle_field(field: &str) -> bool {
    matches!(field, "model_plane.http.allow_non_loopback")
        || parse_target_field(field)
            .map(|(_, suffix)| suffix == "enabled")
            .unwrap_or(false)
}

fn is_single_choice_toggle_field(field: &str) -> bool {
    matches!(field, "model_plane.http.allow_non_loopback")
}

fn char_to_byte_index(input: &str, char_index: usize) -> usize {
    if char_index == 0 {
        return 0;
    }
    input
        .char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(input.len())
}

fn target_field_value(settings: &CoreSettings, field: &str) -> Option<String> {
    let (index, suffix) = parse_target_field(field)?;
    let target = settings.targets.get(index)?;
    match suffix {
        "id" => Some(target.id.clone()),
        "display_name" => Some(target.display_name.clone()),
        "enabled" => Some(target.enabled.to_string()),
        "aliases" => Some(target.aliases.join(",")),
        "credential_ref" => Some(target.credential_ref.clone().unwrap_or_default()),
        "notes" => Some(target.notes.clone().unwrap_or_default()),
        "storage_class" => Some(target.storage_class.clone()),
        "access_class" => Some(target.access_class.clone()),
        "sealed_profile_ref" => Some(target.sealed_profile_ref.clone().unwrap_or_default()),
        "connection.host" => Some(target.connection.host.clone().unwrap_or_default()),
        "connection.port" => Some(
            target
                .connection
                .port
                .map(|v| v.to_string())
                .unwrap_or_default(),
        ),
        "connection.username" => Some(target.connection.username.clone().unwrap_or_default()),
        "connection.selector_kind" => {
            Some(target.connection.selector_kind.clone().unwrap_or_default())
        }
        "connection.selector_value" => {
            Some(target.connection.selector_value.clone().unwrap_or_default())
        }
        _ => None,
    }
}

fn apply_field_edit(settings: &mut CoreSettings, field: &str, value: &str) -> Result<(), String> {
    match field {
        "core.instance_name" => settings.core.instance_name = value.to_string(),
        "core.log_level" => settings.core.log_level = value.to_string(),
        "core.operator_locale" => settings.core.operator_locale = value.to_string(),
        "storage.artifacts.backend" => settings.storage.artifacts.backend = value.to_string(),
        "storage.artifacts.max_bytes" => {
            settings.storage.artifacts.max_bytes = value
                .parse::<u64>()
                .map_err(|_| "artifact max bytes must be u64".to_string())?;
        }
        "model_plane.http.host" => settings.model_plane.http.host = value.to_string(),
        "model_plane.http.port" => {
            settings.model_plane.http.port = value
                .parse::<u16>()
                .map_err(|_| "model-plane port must be u16".to_string())?;
        }
        "model_plane.http.allow_non_loopback" => {
            settings.model_plane.http.allow_non_loopback = value
                .parse::<bool>()
                .map_err(|_| "allow_non_loopback must be bool".to_string())?;
        }
        "vault.unlock.trigger_policy" => settings.vault.unlock.trigger_policy = value.to_string(),
        _ => apply_target_field_edit(settings, field, value)?,
    }
    Ok(())
}

fn apply_target_field_edit(
    settings: &mut CoreSettings,
    field: &str,
    value: &str,
) -> Result<(), String> {
    let (index, suffix) = parse_target_field(field)
        .ok_or_else(|| format!("`{field}` is not editable in menuconfig"))?;
    let target = settings
        .targets
        .get_mut(index)
        .ok_or_else(|| format!("target index out of range for `{field}`"))?;
    match suffix {
        "id" => target.id = value.to_string(),
        "display_name" => target.display_name = value.to_string(),
        "enabled" => {
            target.enabled = value
                .parse::<bool>()
                .map_err(|_| "enabled must be bool".to_string())?;
        }
        "aliases" => {
            target.aliases = value
                .split(',')
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect();
        }
        "credential_ref" => {
            target.credential_ref = if value.trim().is_empty() {
                None
            } else {
                Some(value.trim().to_string())
            };
        }
        "notes" => {
            target.notes = if value.trim().is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        }
        "storage_class" => target.storage_class = value.to_string(),
        "access_class" => target.access_class = value.to_string(),
        "sealed_profile_ref" => {
            target.sealed_profile_ref = if value.trim().is_empty() {
                None
            } else {
                Some(value.trim().to_string())
            };
        }
        "connection.host" => {
            target.connection.host = if value.trim().is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        }
        "connection.port" => {
            target.connection.port = if value.trim().is_empty() {
                None
            } else {
                Some(
                    value
                        .parse::<u16>()
                        .map_err(|_| "target port must be u16".to_string())?,
                )
            };
        }
        "connection.username" => {
            target.connection.username = if value.trim().is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        }
        "connection.selector_kind" => {
            target.connection.selector_kind = if value.trim().is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        }
        "connection.selector_value" => {
            target.connection.selector_value = if value.trim().is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        }
        other => return Err(format!("`{other}` is not editable in menuconfig")),
    }
    Ok(())
}

fn parse_target_field(field: &str) -> Option<(usize, &str)> {
    let suffix = field.strip_prefix("targets[")?;
    let (index, rest) = suffix.split_once(']')?;
    let index = index.parse::<usize>().ok()?;
    let suffix = rest.strip_prefix('.')?;
    Some((index, suffix))
}

fn screen_for_field(settings: &CoreSettings, field: &str) -> Screen {
    match field {
        "core.instance_name" | "core.log_level" => Screen::Core,
        "core.operator_locale" => Screen::Core,
        "storage.artifacts.backend" | "storage.artifacts.max_bytes" => Screen::Storage,
        "model_plane.http.host"
        | "model_plane.http.port"
        | "model_plane.http.allow_non_loopback" => Screen::ModelPlane,
        "vault.unlock.trigger_policy" => Screen::Vault,
        _ => {
            let Some((index, suffix)) = parse_target_field(field) else {
                return Screen::Root;
            };
            let Some(target) = settings.targets.get(index) else {
                return Screen::TargetEditor(index);
            };
            match suffix {
                "id" | "display_name" | "enabled" | "aliases" => {
                    Screen::TargetPublicDescriptor(index)
                }
                "storage_class" | "access_class" | "sealed_profile_ref" => {
                    Screen::TargetPolicy(index)
                }
                "credential_ref"
                | "notes"
                | "connection.host"
                | "connection.port"
                | "connection.username"
                | "connection.selector_kind"
                | "connection.selector_value" => {
                    if suffix == "credential_ref" && matches!(target.kind, TargetKind::Ssh) {
                        return Screen::TargetCredentialSource(index);
                    }
                    if is_sensitive_target(target) {
                        Screen::TargetSensitiveOverlay(index)
                    } else {
                        Screen::TargetConnectionProfile(index)
                    }
                }
                _ => Screen::TargetEditor(index),
            }
        }
    }
}

fn load_vault_router(settings: &CoreSettings) -> Result<SecretVaultRouter, String> {
    let vault_root = vault_root_path(settings);
    let mut router = SecretVaultRouter::with_persistent_store(&vault_root).map_err(|err| {
        format!(
            "load vault router failed (root={}): {err:?}",
            vault_root.display()
        )
    })?;
    router
        .set_active_backend(&settings.vault.backend)
        .map_err(|err| {
            format!(
                "set active vault backend `{}` failed: {err:?}",
                settings.vault.backend
            )
        })?;
    Ok(router)
}

fn vault_root_path(settings: &CoreSettings) -> PathBuf {
    Path::new(&settings.core.data_dir).join("vault")
}

fn load_passive_projection(settings: &CoreSettings) -> VaultPassiveProjection {
    read_passive_vault_projection(vault_root_path(settings)).unwrap_or_default()
}

fn build_security_summary_passive(settings: &CoreSettings) -> SecuritySummary {
    let projection = load_passive_projection(settings);
    build_security_summary_from_projection(settings, &projection)
}

fn build_security_summary_from_projection(
    settings: &CoreSettings,
    projection: &VaultPassiveProjection,
) -> SecuritySummary {
    // Passive projection cannot prove a live in-memory unlock session after process restart.
    let passive_lock_state = if projection.lock_state.as_str() == "unlocked" {
        "locked".to_string()
    } else {
        projection.lock_state.as_str().to_string()
    };
    SecuritySummary {
        backend: settings.vault.backend.clone(),
        lock_state: passive_lock_state,
        trigger_policy: settings.vault.unlock.trigger_policy.clone(),
        preferred_method: settings.vault.unlock.preferred_method.clone(),
        secret_count: projection.secret_count,
        ssh_key_count: projection.secret_count,
        token_count: projection.token_count,
    }
}

fn build_security_summary_from_router(
    settings: &CoreSettings,
    router: &mut SecretVaultRouter,
) -> SecuritySummary {
    let secret_summaries = router.list_secret_summaries();
    SecuritySummary {
        backend: settings.vault.backend.clone(),
        lock_state: router.vault_lock_state().as_str().to_string(),
        trigger_policy: settings.vault.unlock.trigger_policy.clone(),
        preferred_method: settings.vault.unlock.preferred_method.clone(),
        secret_count: secret_summaries.len(),
        ssh_key_count: secret_summaries
            .iter()
            .filter(|item| item.kind.eq_ignore_ascii_case("ssh-private-key"))
            .count(),
        token_count: router.list_agent_tokens().len(),
    }
}

fn ordered_unlock_methods(settings: &CoreSettings) -> Vec<String> {
    let mut methods = Vec::new();
    let preferred = settings.vault.unlock.preferred_method.trim();
    if !preferred.is_empty() {
        methods.push(preferred.to_string());
    }
    for method in &settings.vault.unlock.allowed_methods {
        if !methods
            .iter()
            .any(|entry| entry.eq_ignore_ascii_case(method))
        {
            methods.push(method.clone());
        }
    }
    methods
}

fn mint_local_admin_attestation(
    router: &mut SecretVaultRouter,
    action_kind: LocalAdminActionKind,
    target_object_ref: &str,
    payload_digest: &str,
    operator_principal: &str,
) -> Result<String, String> {
    let intent = router
        .create_local_admin_intent_with_digest(
            action_kind,
            target_object_ref,
            payload_digest,
            operator_principal,
            Duration::from_secs(300),
        )
        .map_err(|err| format!("create local admin intent failed: {err:?}"))?;
    let attestation = router
        .complete_local_admin_attestation(
            &intent.intent_id,
            operator_principal,
            "menuconfig",
            Duration::from_secs(120),
        )
        .map_err(|err| format!("complete local admin attestation failed: {err:?}"))?;
    Ok(attestation.attestation_id)
}

fn is_sensitive_target(target: &StandaloneTargetProfile) -> bool {
    target
        .storage_class
        .trim()
        .eq_ignore_ascii_case("sealed-overlay")
        || target
            .storage_class
            .trim()
            .eq_ignore_ascii_case("sealed-full")
}

fn default_ssh_target(index: usize) -> StandaloneTargetProfile {
    StandaloneTargetProfile {
        id: format!("ssh-target-{}", index + 1),
        display_name: format!("SSH Target {}", index + 1),
        kind: TargetKind::Ssh,
        enabled: true,
        aliases: Vec::new(),
        storage_class: "plain".into(),
        access_class: "anonymous-local".into(),
        sealed_profile_ref: None,
        credential_ref: None,
        notes: None,
        connection: StandaloneConnectionSection {
            host: Some("127.0.0.1".into()),
            port: Some(22),
            username: Some("user".into()),
            known_hosts_policy: None,
            selector_kind: None,
            selector_value: None,
        },
        terminal: StandaloneTerminalSection::default(),
        toolchains: Default::default(),
        terminal_provider: TerminalProviderSection {
            enabled: true,
            shell: None,
        },
        git_repositories: Vec::new(),
    }
}

fn default_adb_target(index: usize) -> StandaloneTargetProfile {
    StandaloneTargetProfile {
        id: format!("adb-target-{}", index + 1),
        display_name: format!("ADB Target {}", index + 1),
        kind: TargetKind::Adb,
        enabled: true,
        aliases: Vec::new(),
        storage_class: "plain".into(),
        access_class: "anonymous-local".into(),
        sealed_profile_ref: None,
        credential_ref: None,
        notes: None,
        connection: StandaloneConnectionSection {
            host: None,
            port: None,
            username: None,
            known_hosts_policy: None,
            selector_kind: Some("serial".into()),
            selector_value: Some("emulator-5554".into()),
        },
        terminal: StandaloneTerminalSection::default(),
        toolchains: Default::default(),
        terminal_provider: TerminalProviderSection {
            enabled: true,
            shell: Some("/system/bin/sh".into()),
        },
        git_repositories: Vec::new(),
    }
}

fn default_sensitive_ssh_target(index: usize) -> StandaloneTargetProfile {
    let mut target = default_ssh_target(index);
    target.id = format!("sensitive-ssh-{}", index + 1);
    target.display_name = format!("Sensitive SSH {}", index + 1);
    target.storage_class = "sealed-overlay".into();
    target.access_class = "token-scoped".into();
    target.sealed_profile_ref = Some(format!("vault://bridgingio/target-profile/{}", target.id));
    target
}

fn default_sensitive_adb_target(index: usize) -> StandaloneTargetProfile {
    let mut target = default_adb_target(index);
    target.id = format!("sensitive-adb-{}", index + 1);
    target.display_name = format!("Sensitive ADB {}", index + 1);
    target.storage_class = "sealed-overlay".into();
    target.access_class = "token-scoped".into();
    target.sealed_profile_ref = Some(format!("vault://bridgingio/target-profile/{}", target.id));
    target
}

fn target_kind_label(kind: &TargetKind, catalog: &Catalog) -> String {
    match kind {
        TargetKind::Ssh => catalog.t("menu.value.target_kind.ssh"),
        TargetKind::Adb => catalog.t("menu.value.target_kind.adb"),
        TargetKind::Serial => catalog.t("menu.value.target_kind.serial"),
        TargetKind::Docker => catalog.t("menu.value.target_kind.docker"),
        TargetKind::Other(_) => catalog.t("menu.value.target_kind.custom"),
    }
}

#[cfg(test)]
fn parse_trigger_policy(raw: &str) -> VaultUnlockTriggerPolicy {
    match raw.trim().to_ascii_lowercase().as_str() {
        "on-core-start" => VaultUnlockTriggerPolicy::OnCoreStart,
        "on-every-secret-access" => VaultUnlockTriggerPolicy::OnEverySecretAccess,
        "manual-only" => VaultUnlockTriggerPolicy::ManualOnly,
        _ => VaultUnlockTriggerPolicy::OnFirstSecretAccess,
    }
}

#[cfg(test)]
mod tests {
    use super::{ActionKind, EditModeKind, MenuConfigApp, MenuEntryKind, Screen};
    use bridgingio_domain::TargetKind;
    use bridgingio_engine::CoreSettings;
    use bridgingio_platform::RuntimeLogLevel;
    use bridgingio_secrets::{
        SecretBytes, SecretVaultRouter, TrustedLocalSshKeyImportRequest, VaultUnlockTriggerPolicy,
        VerifiedOsNativeEvent,
    };
    use crossterm::event::KeyCode;
    use ratatui::layout::Rect;
    use ratatui::style::{Color, Modifier};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_config_path(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("bridgingio-operator-console-{label}-{stamp}"));
        fs::create_dir_all(&root).expect("create root");
        let config_path = root.join("standalone.toml");
        fs::write(&config_path, CoreSettings::minimal_example()).expect("write config");
        config_path
    }

    fn set_config_data_dir(config_path: &Path, data_dir: &Path) {
        let config = fs::read_to_string(config_path).expect("read config");
        let toml_data_dir = data_dir.to_string_lossy().replace('\\', "\\\\");
        let updated = config.replace(
            "data_dir = \"auto\"",
            &format!("data_dir = \"{toml_data_dir}\""),
        );
        fs::write(config_path, updated).expect("write config with custom data dir");
    }

    fn set_config_log_level(config_path: &Path, level: &str) {
        let config = fs::read_to_string(config_path).expect("read config");
        let updated = config.replace("log_level = \"info\"", &format!("log_level = \"{level}\""));
        fs::write(config_path, updated).expect("write config with custom log level");
    }

    const TEST_PRIVATE_KEY_PEM: &str =
        "-----BEGIN PRIVATE KEY-----\nZm9v\n-----END PRIVATE KEY-----\n";

    #[test]
    fn search_results_find_trigger_policy_field() {
        let config_path = temp_config_path("search");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.search_input = "trigger".into();
        app.screen = Screen::SearchResults;
        let entries = app.entries();
        assert!(entries.iter().any(|entry| matches!(
            &entry.kind,
            MenuEntryKind::FocusField { field, .. } if field == "vault.unlock.trigger_policy"
        )));
    }

    #[test]
    fn add_ssh_target_action_creates_target() {
        let config_path = temp_config_path("add-target");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.run_action(ActionKind::AddSshTarget)
            .expect("add ssh target");
        assert!(matches!(
            app.confirm_action,
            Some(super::ConfirmAction::ConfirmPlainSshRisk)
        ));
        app.handle_confirm_key(KeyCode::Enter)
            .expect("confirm plain ssh risk");
        assert_eq!(app.screen, Screen::TargetEditor(1));
        assert_eq!(app.settings.targets.len(), 2);
        assert_eq!(app.settings.targets[1].kind, TargetKind::Ssh);
    }

    #[test]
    fn add_sensitive_target_action_requires_unlock_and_then_creates_target() {
        let config_path = temp_config_path("add-sensitive-target");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");

        app.security_summary.lock_state = "locked".into();
        app.run_action(ActionKind::AddSensitiveSshTarget)
            .expect("attempt add sensitive target while locked");
        assert_eq!(app.settings.targets.len(), 1);
        assert_eq!(
            app.last_status,
            app.t("menu.status.vault_locked_for_sensitive_target")
        );

        app.security_summary.lock_state = "unlocked".into();
        app.run_action(ActionKind::AddSensitiveSshTarget)
            .expect("add sensitive target while unlocked");
        assert_eq!(app.settings.targets.len(), 2);
        assert!(super::is_sensitive_target(&app.settings.targets[1]));
        assert_eq!(app.screen, Screen::TargetEditor(1));
    }

    #[test]
    fn target_editor_plain_target_uses_segmented_entries() {
        let config_path = temp_config_path("plain-segmented-target-editor");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.screen = Screen::TargetEditor(0);
        let entries = app.entries();
        assert!(entries.iter().any(|entry| matches!(
            entry.kind,
            MenuEntryKind::Navigate(Screen::TargetPublicDescriptor(0))
        )));
        assert!(entries.iter().any(|entry| matches!(
            entry.kind,
            MenuEntryKind::Navigate(Screen::TargetConnectionProfile(0))
        )));
        assert!(entries
            .iter()
            .any(|entry| matches!(entry.kind, MenuEntryKind::Navigate(Screen::TargetPolicy(0)))));
        assert!(entries.iter().any(|entry| matches!(
            entry.kind,
            MenuEntryKind::Action(ActionKind::ApplyTarget(0))
        )));
        assert!(entries.iter().any(|entry| matches!(
            entry.kind,
            MenuEntryKind::Action(ActionKind::DeleteTarget(0))
        )));
    }

    #[test]
    fn target_editor_create_mode_shows_create_without_delete() {
        let config_path = temp_config_path("target-create-mode-actions");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.run_action(ActionKind::AddAdbTarget)
            .expect("open create target editor");
        let entries = app.entries();
        assert!(entries.iter().any(|entry| matches!(
            entry.kind,
            MenuEntryKind::Action(ActionKind::ApplyTarget(1))
        )));
        let create_entry = entries
            .iter()
            .find(|entry| {
                matches!(
                    entry.kind,
                    MenuEntryKind::Action(ActionKind::ApplyTarget(1))
                )
            })
            .expect("create action entry");
        assert_eq!(create_entry.label, app.t("menu.target.create_target"));
        assert!(!entries.iter().any(|entry| matches!(
            entry.kind,
            MenuEntryKind::Action(ActionKind::DeleteTarget(1))
        )));
    }

    #[test]
    fn leaving_create_target_flow_prompts_and_discards_draft() {
        let config_path = temp_config_path("target-create-discard-confirm");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.run_action(ActionKind::AddAdbTarget)
            .expect("open create target editor");
        app.apply_edit_value("targets[1].display_name", "Draft Target")
            .expect("edit draft target");

        let outcome = app.request_exit_or_back().expect("request back");
        assert!(outcome.is_none());
        assert!(matches!(
            app.confirm_action,
            Some(super::ConfirmAction::DiscardNewTarget(1))
        ));
        app.handle_confirm_key(KeyCode::Enter)
            .expect("confirm discard draft");
        assert_eq!(app.settings.targets.len(), 1);
        assert_eq!(app.screen, Screen::Root);
    }

    #[test]
    fn leaving_existing_target_without_apply_prompts_and_reverts_changes() {
        let config_path = temp_config_path("target-existing-discard-confirm");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.screen = Screen::Targets;
        app.selected = 0;
        app.activate_selected()
            .expect("open existing target editor");
        assert_eq!(app.screen, Screen::TargetEditor(0));
        let original = app.settings.targets[0].display_name.clone();
        app.apply_edit_value("targets[0].display_name", "Changed Name")
            .expect("edit existing target");

        let outcome = app.request_exit_or_back().expect("request back");
        assert!(outcome.is_none());
        assert!(matches!(
            app.confirm_action,
            Some(super::ConfirmAction::DiscardTargetChanges(0))
        ));
        app.handle_confirm_key(KeyCode::Enter)
            .expect("confirm discard existing target changes");
        assert_eq!(app.screen, Screen::Targets);
        assert_eq!(app.settings.targets[0].display_name, original);
    }

    #[test]
    fn locked_sensitive_target_editor_hides_overlay_and_delete_entries() {
        let config_path = temp_config_path("locked-sensitive-target-editor");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        let index = app.settings.targets.len();
        app.settings
            .targets
            .push(super::default_sensitive_ssh_target(index));
        app.security_summary.lock_state = "locked".into();
        app.screen = Screen::TargetEditor(index);
        let entries = app.entries();
        assert!(entries.iter().any(|entry| matches!(
            entry.kind,
            MenuEntryKind::Navigate(Screen::TargetPublicDescriptor(i)) if i == index
        )));
        assert!(!entries.iter().any(|entry| matches!(
            entry.kind,
            MenuEntryKind::Navigate(Screen::TargetSensitiveOverlay(i)) if i == index
        )));
        assert!(!entries.iter().any(|entry| matches!(
            entry.kind,
            MenuEntryKind::Action(ActionKind::DeleteTarget(i)) if i == index
        )));
        assert!(entries
            .iter()
            .any(|entry| matches!(entry.kind, MenuEntryKind::Action(ActionKind::UnlockVault))));
    }

    #[test]
    fn sensitive_ssh_overlay_shows_credential_source_only_when_unlocked() {
        let config_path = temp_config_path("sensitive-ssh-credential-source");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        let index = app.settings.targets.len();
        app.settings
            .targets
            .push(super::default_sensitive_ssh_target(index));

        app.security_summary.lock_state = "unlocked".into();
        app.screen = Screen::TargetSensitiveOverlay(index);
        let unlocked_entries = app.entries();
        let credential_entry = unlocked_entries.iter().find(|entry| {
            matches!(
                entry.kind,
                MenuEntryKind::Action(ActionKind::OpenTargetCredentialSource(i)) if i == index
            )
        });
        assert!(credential_entry.is_some());
        assert_eq!(
            credential_entry.expect("credential source entry").label,
            app.t("menu.target.credential_source")
        );

        app.security_summary.lock_state = "locked".into();
        let locked_entries = app.entries();
        assert!(!locked_entries.iter().any(|entry| matches!(
            entry.kind,
            MenuEntryKind::Action(ActionKind::OpenTargetCredentialSource(i)) if i == index
        )));
        assert!(locked_entries
            .iter()
            .any(|entry| entry.label == app.t("menu.target.sensitive_locked_hint")));
    }

    #[test]
    fn deleting_sensitive_target_when_unlocked_returns_to_targets_list() {
        let config_path = temp_config_path("delete-sensitive-target");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        let index = app.settings.targets.len();
        app.settings
            .targets
            .push(super::default_sensitive_ssh_target(index));
        app.security_summary.lock_state = "unlocked".into();
        app.screen = Screen::TargetEditor(index);

        app.run_action(ActionKind::DeleteTarget(index))
            .expect("run delete target");
        assert!(matches!(
            app.confirm_action,
            Some(super::ConfirmAction::DeleteTarget(i)) if i == index
        ));
        app.handle_confirm_key(KeyCode::Enter)
            .expect("confirm delete target");

        assert_eq!(app.screen, Screen::Targets);
        assert_eq!(app.settings.targets.len(), index);
    }

    #[test]
    fn save_persists_target_and_apply_strategy() {
        let config_path = temp_config_path("save");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.run_action(ActionKind::AddAdbTarget)
            .expect("add adb target");
        super::apply_field_edit(&mut app.settings, "targets[1].display_name", "My ADB")
            .expect("edit target");
        app.dirty_paths.push("targets[1].display_name".into());
        app.save().expect("save");
        let saved = fs::read_to_string(&config_path).expect("read saved config");
        assert!(saved.contains("display_name = \"My ADB\""));
        assert_eq!(app.last_apply_strategy.as_deref(), Some("restart_required"));
    }

    #[test]
    fn authorization_log_is_persisted_and_stays_display_safe() {
        let config_path = temp_config_path("authorization-log");
        let runtime_root = config_path
            .parent()
            .expect("config parent")
            .join("runtime-log-root");
        set_config_data_dir(&config_path, &runtime_root);
        let app = MenuConfigApp::load(&config_path).expect("load app");

        app.record_authorization_event(
            "flow-menu-test",
            "Create Token",
            super::OP_AUTH_TOKEN_CREATE,
            "succeeded",
            "ok",
            "leader",
            None,
            Some("token-000001"),
            None,
        );

        let auth_log = runtime_root.join("logs/local-authorization.jsonl");
        let text = fs::read_to_string(&auth_log).expect("read auth log");
        assert!(text.contains("\"flow_id\":\"flow-menu-test\""));
        assert!(text.contains("\"operation\":\"auth.token.create\""));
        assert!(!text.contains("SUPER-SECRET"));
    }

    #[test]
    fn debug_session_breadcrumb_is_gated_by_log_level() {
        let config_path = temp_config_path("session-debug-gate");
        let runtime_root = config_path
            .parent()
            .expect("config parent")
            .join("runtime-log-root");
        set_config_data_dir(&config_path, &runtime_root);
        let app = MenuConfigApp::load(&config_path).expect("load app");

        app.record_session_event(
            "screen.push",
            "navigated",
            "ok",
            RuntimeLogLevel::Debug,
            None,
        );

        assert!(!runtime_root.join("logs/menuconfig-session.jsonl").exists());
    }

    #[test]
    fn unlock_worker_uses_authorization_flow_for_session_and_dedupe() {
        let config_path = temp_config_path("unlock-flow-correlation");
        let runtime_root = config_path
            .parent()
            .expect("config parent")
            .join("runtime-flow-root");
        set_config_data_dir(&config_path, &runtime_root);
        set_config_log_level(&config_path, "debug");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");

        let (tx, rx) = mpsc::channel();
        app.unlock_worker = Some(super::UnlockWorkerHandle {
            receiver: rx,
            cancel_requested: false,
        });
        app.unlock_flow_id = Some("flow-vault.unlock-test".to_string());
        tx.send(super::UnlockWorkerOutcome {
            router: SecretVaultRouter::default(),
            result: Ok(()),
            verified_event: Some(VerifiedOsNativeEvent {
                flow_id: "verified-os-native-flow".to_string(),
                dedupe_state: "joined".to_string(),
                phase: "succeeded".to_string(),
                result: "ok".to_string(),
                cache_state: "stored".to_string(),
            }),
        })
        .expect("send worker outcome");

        app.poll_unlock_worker();

        let auth = fs::read_to_string(runtime_root.join("logs/local-authorization.jsonl"))
            .expect("read authorization log");
        assert!(auth.contains("\"flow_id\":\"flow-vault.unlock-test\""));
        assert!(auth.contains("\"dedupe_state\":\"joined\""));

        let session = fs::read_to_string(runtime_root.join("logs/menuconfig-session.jsonl"))
            .expect("read session log");
        assert!(session.contains("\"flow_id\":\"flow-vault.unlock-test\""));
        assert!(session.contains("\"action\":\"unlock.worker\""));
    }

    #[test]
    fn startup_does_not_auto_unlock_vault() {
        let config_path = temp_config_path("locked");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.settings.vault.unlock.trigger_policy = "on-core-start".into();
        app.refresh_security_summary();
        assert!(app.vault_router.is_none());
        assert_ne!(app.security_summary.lock_state, "unlocked");
        assert_eq!(
            super::parse_trigger_policy(&app.settings.vault.unlock.trigger_policy),
            VaultUnlockTriggerPolicy::OnCoreStart
        );
    }

    #[test]
    fn passive_unlocked_projection_is_not_rendered_as_live_unlocked_state() {
        let config_path = temp_config_path("passive-unlocked-not-live");
        let runtime_root = config_path
            .parent()
            .expect("config parent")
            .join("runtime-root");
        set_config_data_dir(&config_path, &runtime_root);
        let vault_root = runtime_root.join("vault");
        fs::create_dir_all(&vault_root).expect("create vault root");
        fs::write(
            vault_root.join("metadata.db"),
            r#"{"schema_version":2,"lock_state":"Unlocked","secrets":[],"agent_tokens":[]}"#,
        )
        .expect("write metadata");

        let app = MenuConfigApp::load(&config_path).expect("load app");
        assert_eq!(app.security_summary.lock_state, "locked");
        assert!(app.vault_router.is_none());
    }

    #[test]
    fn vault_router_load_failure_does_not_fall_back_to_in_memory_router() {
        let config_path = temp_config_path("router-load-failure");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        let invalid_data_dir = config_path
            .parent()
            .expect("config parent")
            .join("invalid-data-dir-file");
        fs::write(&invalid_data_dir, "not-a-directory").expect("write sentinel file");
        app.settings.core.data_dir = invalid_data_dir.to_string_lossy().to_string();

        let err = match app.ensure_vault_router_loaded() {
            Ok(_) => panic!("router load should fail"),
            Err(err) => err,
        };
        assert!(err.contains("load vault router failed"));
        assert!(app.vault_router.is_none());
    }

    #[test]
    fn create_token_action_refreshes_live_state_before_opening_flow() {
        let config_path = temp_config_path("create-token-live-state");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.security_summary.lock_state = "unlocked".into();
        app.vault_router = Some(SecretVaultRouter::default());

        app.run_action(ActionKind::CreateToken)
            .expect("run create token action");

        assert_eq!(app.security_summary.lock_state, "locked");
        assert!(!app.edit_mode);
        assert_eq!(
            app.last_status,
            app.t("menu.status.vault_locked_for_token_action")
        );
    }

    #[test]
    fn log_level_field_opens_choice_selector() {
        let config_path = temp_config_path("log-level-choice");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.begin_edit("core.log_level".into())
            .expect("begin log-level choice edit");
        assert!(app.edit_mode);
        assert_eq!(app.edit_mode_kind, EditModeKind::Choice);
        assert!(app.edit_options.iter().any(|value| value == "info"));
    }

    #[test]
    fn space_toggles_target_enabled_field() {
        let config_path = temp_config_path("space-toggle");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.screen = Screen::TargetPublicDescriptor(0);
        app.select_field("targets[0].enabled");
        let before = app.settings.targets[0].enabled;
        app.handle_space_on_selected().expect("toggle enabled");
        assert_eq!(app.settings.targets[0].enabled, !before);
        assert!(app
            .dirty_paths
            .iter()
            .any(|path| path == "targets[0].enabled"));
    }

    #[test]
    fn choice_selector_commits_with_space() {
        let config_path = temp_config_path("choice-commit-space");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.begin_edit("core.log_level".into())
            .expect("begin choice edit");
        let warn_index = app
            .edit_options
            .iter()
            .position(|value| value == "warn")
            .expect("warn option");
        app.edit_option_selected = warn_index;
        app.handle_edit_key(KeyCode::Char(' '))
            .expect("commit with space");
        assert_eq!(app.settings.core.log_level, "warn");
        assert!(!app.edit_mode);
    }

    #[test]
    fn allow_non_loopback_uses_single_choice_toggle_marker() {
        let config_path = temp_config_path("model-allow-non-loopback-marker");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.screen = Screen::ModelPlane;
        app.settings.model_plane.http.allow_non_loopback = false;
        let entries = app.entries();
        let index = entries
            .iter()
            .position(|entry| {
                matches!(
                    entry.kind,
                    MenuEntryKind::EditField(ref field) if field == "model_plane.http.allow_non_loopback"
                )
            })
            .expect("allow_non_loopback entry");
        let off_line = super::format_menu_entry_line(&app, index, &entries[index]);
        assert!(off_line.contains("< >"));
        assert!(!off_line.contains("[ ]"));

        app.settings.model_plane.http.allow_non_loopback = true;
        let entries = app.entries();
        let on_line = super::format_menu_entry_line(&app, index, &entries[index]);
        assert!(on_line.contains("<*>"));
        assert!(!on_line.contains("[*]"));
    }

    #[test]
    fn enter_on_boolean_toggle_requires_space() {
        let config_path = temp_config_path("enter-boolean-space-only");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.screen = Screen::ModelPlane;
        app.select_field("model_plane.http.allow_non_loopback");
        app.activate_selected().expect("activate selected toggle");
        assert_eq!(app.last_status, app.t("menu.status.bool_toggle_space_only"));
        assert!(!app.edit_mode);
    }

    #[test]
    fn go_back_restores_previous_selection() {
        let config_path = temp_config_path("back-selection-restore");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.screen = Screen::Root;
        app.selected = 4;
        app.activate_selected().expect("open targets");
        assert_eq!(app.screen, Screen::Targets);
        app.go_back();
        assert_eq!(app.screen, Screen::Root);
        assert_eq!(app.selected, 4);
    }

    #[test]
    fn root_exit_with_dirty_changes_opens_confirm_dialog() {
        let config_path = temp_config_path("root-exit-dirty-confirm");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.dirty_paths.push("core.instance_name".into());
        let outcome = app.request_exit_or_back().expect("request exit");
        assert!(outcome.is_none());
        assert!(app.exit_confirm_mode);
    }

    #[test]
    fn exit_confirm_yes_saves_before_exit() {
        let config_path = temp_config_path("exit-confirm-save");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.dirty_paths.push("core.instance_name".into());
        app.begin_exit_confirm();
        app.exit_confirm_selected = 0;
        let outcome = app
            .handle_exit_confirm_key(KeyCode::Enter)
            .expect("handle confirm")
            .expect("must exit");
        assert!(outcome.saved);
        assert!(!app.is_dirty());
    }

    #[test]
    fn footer_exit_button_returns_parent_screen() {
        let config_path = temp_config_path("footer-exit-parent");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.selected = 0;
        app.activate_selected().expect("open core");
        assert_eq!(app.screen, Screen::Core);
        app.footer_selected = 1;
        let outcome = app.activate_footer_button().expect("footer exit");
        assert!(outcome.is_none());
        assert_eq!(app.screen, Screen::Root);
    }

    #[test]
    fn popup_editable_fields_use_parenthesized_value_and_arrow() {
        let config_path = temp_config_path("popup-line-style");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.screen = Screen::Core;
        let entries = app.entries();
        let instance_line = super::format_menu_entry_line(&app, 0, &entries[0]);
        let log_level_line = super::format_menu_entry_line(&app, 2, &entries[2]);
        assert!(instance_line.contains("Instance Name ("));
        assert!(instance_line.ends_with("--->"));
        assert!(log_level_line.contains("Log Level ("));
        assert!(log_level_line.ends_with("--->"));
    }

    #[test]
    fn token_create_edit_popup_uses_human_friendly_field_label() {
        let config_path = temp_config_path("token-create-popup-label");
        let app = MenuConfigApp::load(&config_path).expect("load app");
        let label = super::display_edit_field_label(&app, super::TOKEN_CREATE_LABEL_FIELD);
        assert_ne!(label, super::TOKEN_CREATE_LABEL_FIELD);
        assert!(label.contains("Token"));
    }

    #[test]
    fn keyboard_navigation_moves_selected_visual_state() {
        let config_path = temp_config_path("selected-visual-navigation");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.screen = Screen::Root;

        let entries = app.entries();
        assert_eq!(
            super::menu_entry_visual_state(app.selected, 0, &entries[0]),
            super::MenuEntryVisualState::Selected
        );
        assert_eq!(
            super::menu_entry_visual_state(app.selected, 1, &entries[1]),
            super::MenuEntryVisualState::Normal
        );

        app.move_selection(1);
        let entries = app.entries();
        assert_eq!(
            super::menu_entry_visual_state(app.selected, 0, &entries[0]),
            super::MenuEntryVisualState::Normal
        );
        assert_eq!(
            super::menu_entry_visual_state(app.selected, 1, &entries[1]),
            super::MenuEntryVisualState::Selected
        );
    }

    #[test]
    fn keyboard_navigation_wraps_from_first_item_to_last() {
        let config_path = temp_config_path("selected-wrap-up");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.screen = Screen::Root;
        app.selected = 0;

        let total = app.entries().len();
        assert!(total > 1);

        app.move_selection(-1);
        assert_eq!(app.selected, total - 1);
    }

    #[test]
    fn keyboard_navigation_wraps_from_last_item_to_first() {
        let config_path = temp_config_path("selected-wrap-down");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.screen = Screen::Root;

        let total = app.entries().len();
        assert!(total > 1);
        app.selected = total - 1;

        app.move_selection(1);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn keyboard_navigation_wraps_inside_submenu() {
        let config_path = temp_config_path("selected-wrap-submenu");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.screen = Screen::Core;
        app.selected = 0;

        let total = app.entries().len();
        assert!(total > 1);

        app.move_selection(-1);
        assert_eq!(app.selected, total - 1);
        app.move_selection(1);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn keyboard_navigation_skips_non_focusable_rows() {
        let config_path = temp_config_path("selected-skip-disabled");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.screen = Screen::Core;
        app.selected = 0;

        // core_entries index 1 is an Info row; selection should jump to index 2.
        app.move_selection(1);
        assert_eq!(app.selected, 2);

        app.move_selection(-1);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn wrap_selection_index_handles_empty_and_single_item_lists() {
        assert_eq!(super::wrap_selection_index(7, 1, 0), 0);
        assert_eq!(super::wrap_selection_index(0, 1, 1), 0);
        assert_eq!(super::wrap_selection_index(0, -1, 1), 0);
    }

    #[test]
    fn selected_style_is_shared_between_menu_and_footer() {
        let config_path = temp_config_path("shared-selected-style");
        let app = MenuConfigApp::load(&config_path).expect("load app");
        let menu_selected = super::menu_entry_style(
            super::MenuEntryVisualState::Selected,
            super::SelectionHighlightMode::Reverse,
        );
        let footer_line = super::render_footer_buttons_line(
            0,
            &app.catalog,
            super::SelectionHighlightMode::Reverse,
        );
        let footer_selected = &footer_line.spans[0];
        assert_eq!(footer_selected.style, menu_selected);
        assert!(footer_selected
            .style
            .add_modifier
            .contains(Modifier::REVERSED));
    }

    #[test]
    fn fallback_detection_uses_force_no_color_and_term_dumb() {
        assert_eq!(
            super::detect_selection_highlight_mode_from_values(true, false, Some("xterm-256color")),
            super::SelectionHighlightMode::Fallback
        );
        assert_eq!(
            super::detect_selection_highlight_mode_from_values(false, true, Some("xterm-256color")),
            super::SelectionHighlightMode::Fallback
        );
        assert_eq!(
            super::detect_selection_highlight_mode_from_values(false, false, Some("dumb")),
            super::SelectionHighlightMode::Fallback
        );
        assert_eq!(
            super::detect_selection_highlight_mode_from_values(
                false,
                false,
                Some("xterm-256color")
            ),
            super::SelectionHighlightMode::Reverse
        );
    }

    #[test]
    fn fallback_mode_uses_visible_markers_for_selected_items() {
        let config_path = temp_config_path("fallback-selected-markers");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.screen = Screen::Root;
        app.selection_highlight_mode = super::SelectionHighlightMode::Fallback;

        let entries = app.entries();
        let selected_line = super::format_menu_entry_line(&app, 0, &entries[0]);
        let not_selected_line = super::format_menu_entry_line(&app, 1, &entries[1]);
        assert!(selected_line.starts_with(">>"));
        assert!(not_selected_line.starts_with(" "));

        let footer_line = super::render_footer_buttons_line(
            0,
            &app.catalog,
            super::SelectionHighlightMode::Fallback,
        );
        assert!(footer_line.spans[0].content.contains(">>"));
    }

    #[test]
    fn lock_state_uses_strong_color_hint() {
        let config_path = temp_config_path("lock-state-color");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");

        app.security_summary.lock_state = "locked".into();
        let locked = super::render_footer_path_line(&app, "Main Menu", "<none>");
        let locked_span = locked
            .spans
            .iter()
            .find(|span| span.content == "locked")
            .expect("locked span");
        assert_eq!(locked_span.style.fg, Some(Color::Red));
        assert!(locked_span.style.add_modifier.contains(Modifier::BOLD));

        app.security_summary.lock_state = "unlocked".into();
        let unlocked = super::render_footer_path_line(&app, "Main Menu", "<none>");
        let unlocked_span = unlocked
            .spans
            .iter()
            .find(|span| span.content == "unlocked")
            .expect("unlocked span");
        assert_eq!(unlocked_span.style.fg, Some(Color::Green));
        assert!(unlocked_span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn disabled_state_takes_priority_over_selected_highlight() {
        let config_path = temp_config_path("disabled-priority");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.screen = Screen::Core;
        app.selected = 1;
        app.selection_highlight_mode = super::SelectionHighlightMode::Fallback;

        let entries = app.entries();
        assert!(matches!(entries[1].kind, MenuEntryKind::Info));
        let state = super::menu_entry_visual_state(app.selected, 1, &entries[1]);
        assert_eq!(state, super::MenuEntryVisualState::Disabled);
        let style = super::menu_entry_style(state, app.selection_highlight_mode);
        assert!(!style.add_modifier.contains(Modifier::REVERSED));

        let line = super::format_menu_entry_line(&app, 1, &entries[1]);
        assert!(line.starts_with(" "));
        assert!(!line.starts_with(">>"));
    }

    #[test]
    fn popup_buttons_share_selected_highlight_style() {
        let labels = vec!["Confirm".to_string(), "Cancel".to_string()];
        let line =
            super::render_popup_button_line(&labels, 0, super::SelectionHighlightMode::Reverse);
        assert_eq!(
            line.spans[0].style,
            super::selected_highlight_style(super::SelectionHighlightMode::Reverse)
        );
        assert!(line.spans[0]
            .style
            .add_modifier
            .contains(Modifier::REVERSED));

        let fallback =
            super::render_popup_button_line(&labels, 1, super::SelectionHighlightMode::Fallback);
        assert!(fallback.spans[2].content.contains(">>"));
    }

    #[test]
    fn choice_popup_line_uses_shared_selected_style() {
        let selected = super::render_choice_option_line(
            "warn",
            "*",
            true,
            super::SelectionHighlightMode::Reverse,
        );
        assert_eq!(
            selected.spans[0].style,
            super::selected_highlight_style(super::SelectionHighlightMode::Reverse)
        );
        assert!(selected.spans[0]
            .style
            .add_modifier
            .contains(Modifier::REVERSED));
    }

    #[test]
    fn long_menu_row_truncation_preserves_prefix_and_arrow() {
        let text = "> [*] this-is-a-very-long-label-for-overflow-check --->";
        let truncated = super::truncate_menu_row_text(text, 24);
        assert!(truncated.starts_with("> [*] "));
        assert!(truncated.ends_with(" --->"));
        assert!(truncated.contains("..."));
    }

    #[test]
    fn viewport_guard_detects_minimum_size() {
        assert!(super::menuconfig_viewport_too_small(Rect::new(
            0, 0, 79, 24
        )));
        assert!(super::menuconfig_viewport_too_small(Rect::new(
            0, 0, 80, 23
        )));
        assert!(!super::menuconfig_viewport_too_small(Rect::new(
            0, 0, 80, 24
        )));
    }

    #[test]
    fn navigating_vault_and_security_screens_keeps_passive_projection() {
        let config_path = temp_config_path("passive-security");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        assert!(app.vault_router.is_none());

        app.screen = Screen::Vault;
        let _ = app.entries();
        assert!(app.vault_router.is_none());

        app.screen = Screen::Security;
        let _ = app.entries();
        assert!(app.vault_router.is_none());
    }

    #[test]
    fn unlock_flow_enters_waiting_and_fails_for_unsupported_method() {
        let config_path = temp_config_path("unlock-failure");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.settings.vault.unlock.preferred_method = "unsupported-local-method".into();
        app.settings.vault.unlock.allowed_methods = vec!["unsupported-local-method".into()];

        app.begin_unlock_flow();
        assert_eq!(app.unlock_flow_state, super::UnlockFlowState::Waiting);
        assert!(app.unlock_flow_pending);

        app.unlock_flow_pending = false;
        app.execute_pending_unlock_flow();
        assert!(matches!(
            app.unlock_flow_state,
            super::UnlockFlowState::Failed { .. }
        ));

        app.handle_unlock_flow_key(KeyCode::Enter);
        assert_eq!(app.unlock_flow_state, super::UnlockFlowState::Idle);
    }

    #[test]
    fn unlock_flow_does_not_block_on_passphrase_fallback() {
        let config_path = temp_config_path("unlock-passphrase-fallback");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.settings.vault.unlock.preferred_method = "passphrase".into();
        app.settings.vault.unlock.allowed_methods = vec!["passphrase".into()];

        app.begin_unlock_flow();
        assert_eq!(app.unlock_flow_state, super::UnlockFlowState::Waiting);
        assert!(app.unlock_flow_pending);

        app.unlock_flow_pending = false;
        app.execute_pending_unlock_flow();
        assert!(matches!(
            app.unlock_flow_state,
            super::UnlockFlowState::Failed { .. }
        ));
        assert!(app
            .last_status
            .contains(&app.t("menu.error.menu_unlock_requires_os_native")));
    }

    #[test]
    fn waiting_unlock_flow_supports_esc_cancel() {
        let config_path = temp_config_path("unlock-cancel");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.unlock_flow_state = super::UnlockFlowState::Waiting;

        app.handle_unlock_flow_key(KeyCode::Esc);
        assert_eq!(app.unlock_flow_state, super::UnlockFlowState::Idle);
        assert_eq!(
            app.last_status,
            app.t("menu.status.vault_unlock_cancel_requested")
        );
    }

    #[test]
    fn unlock_success_popup_dismiss_updates_status_notice() {
        let config_path = temp_config_path("unlock-success-dismiss");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.unlock_flow_state = super::UnlockFlowState::Success;
        app.handle_unlock_flow_key(KeyCode::Enter);
        assert_eq!(app.unlock_flow_state, super::UnlockFlowState::Idle);
        assert_eq!(app.last_status, app.t("menu.status.vault_unlocked_notice"));
    }

    #[test]
    fn security_screen_hides_unlock_action_when_unlocked() {
        let config_path = temp_config_path("security-unlocked");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.screen = Screen::Security;
        app.security_summary.lock_state = "unlocked".into();
        let entries = app.entries();
        assert!(entries
            .iter()
            .all(|entry| !matches!(entry.kind, MenuEntryKind::Action(ActionKind::UnlockVault))));
        assert!(entries
            .iter()
            .any(|entry| entry.label == app.t("menu.security.unlocked_notice")));
    }

    #[test]
    fn security_screen_shows_init_only_when_uninitialized() {
        let config_path = temp_config_path("security-uninitialized");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.screen = Screen::Security;
        app.security_summary.lock_state = "uninitialized".into();
        let entries = app.entries();

        assert!(entries
            .iter()
            .any(|entry| matches!(entry.kind, MenuEntryKind::Action(ActionKind::InitVault))));
        assert!(entries
            .iter()
            .all(|entry| !matches!(entry.kind, MenuEntryKind::Action(ActionKind::UnlockVault))));
        assert!(entries.iter().all(|entry| !matches!(
            entry.kind,
            MenuEntryKind::Action(ActionKind::CreateToken | ActionKind::OpenTokenManagement)
        )));
    }

    #[test]
    fn security_screen_locked_state_hides_token_actions() {
        let config_path = temp_config_path("security-locked-actions");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.screen = Screen::Security;
        app.security_summary.lock_state = "locked".into();
        let entries = app.entries();

        assert!(entries
            .iter()
            .any(|entry| matches!(entry.kind, MenuEntryKind::Action(ActionKind::UnlockVault))));
        assert!(entries
            .iter()
            .any(|entry| matches!(entry.kind, MenuEntryKind::Action(ActionKind::DeleteVault))));
        assert!(entries.iter().all(|entry| !matches!(
            entry.kind,
            MenuEntryKind::Action(ActionKind::CreateToken | ActionKind::OpenTokenManagement)
        )));
    }

    #[test]
    fn create_token_flow_validates_label_and_expiry_deadline() {
        let config_path = temp_config_path("create-token-flow-validation");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.begin_edit(super::TOKEN_CREATE_LABEL_FIELD.to_string())
            .expect("begin label step");
        app.edit_input = "   ".into();
        let empty_label = app.commit_edit();
        assert!(empty_label.is_err());

        app.edit_input = "Codex".into();
        app.commit_edit().expect("commit label");
        assert_eq!(
            app.edit_field.as_deref(),
            Some(super::TOKEN_CREATE_EXPIRY_MODE_FIELD)
        );

        app.edit_option_selected = 1;
        app.commit_edit().expect("commit expiry mode");
        assert_eq!(
            app.edit_field.as_deref(),
            Some(super::TOKEN_CREATE_EXPIRY_AT_FIELD)
        );

        let past = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_secs()
            .saturating_sub(1);
        app.edit_input = past.to_string();
        let past_deadline = app.commit_edit();
        assert!(past_deadline.is_err());
    }

    #[test]
    fn destructive_actions_wait_for_confirmation() {
        let config_path = temp_config_path("destructive-confirm");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.screen = Screen::Security;
        app.security_summary.lock_state = "locked".into();

        app.run_action(ActionKind::DeleteVault)
            .expect("open delete-vault confirm");
        assert!(matches!(
            app.confirm_action,
            Some(super::ConfirmAction::DeleteVault)
        ));
        app.handle_confirm_key(KeyCode::Esc)
            .expect("cancel destructive action");
        assert!(app.confirm_action.is_none());
    }

    #[test]
    fn ssh_key_detail_shows_record_id_and_delete_action() {
        let config_path = temp_config_path("ssh-key-detail-record-id-delete");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.security_summary.lock_state = "unlocked".into();
        app.ssh_key_rows = vec![super::SshKeyManagementRow {
            credential_ref: "vault://bridgingio/ssh-private-key/ops-main".into(),
            label: "ops-main".into(),
            status: "active".into(),
            active_version: "ver-000001".into(),
        }];
        app.screen = Screen::SshKeyDetail("vault://bridgingio/ssh-private-key/ops-main".into());

        let entries = app.entries();
        assert!(entries.iter().any(|entry| {
            entry.label == app.t("menu.ssh_key.detail.active_version")
                && entry.value.as_deref() == Some("ver-000001")
        }));
        assert!(entries.iter().any(|entry| {
            matches!(
                entry.kind,
                MenuEntryKind::Action(ActionKind::DeleteSshKey(ref reference))
                    if reference == "vault://bridgingio/ssh-private-key/ops-main"
            )
        }));
    }

    #[test]
    fn deleting_ssh_key_clears_bound_target_credential_refs() {
        let config_path = temp_config_path("ssh-key-delete-clears-target-bindings");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        let mut router = SecretVaultRouter::default();
        router
            .set_active_backend("builtin-encrypted")
            .expect("switch backend");
        router.unlock_with_os_native().expect("unlock vault");
        let imported = router
            .import_ssh_private_key_trusted_local(TrustedLocalSshKeyImportRequest {
                key_name: "ops-main".into(),
                label: Some("ops-main".into()),
                private_key: SecretBytes::from_utf8(TEST_PRIVATE_KEY_PEM),
                passphrase: None,
                imported_by: "menuconfig:test".into(),
                rotation_reason: None,
            })
            .expect("import key");
        app.vault_router = Some(router);
        app.settings.targets[0].kind = TargetKind::Ssh;
        app.settings.targets[0].credential_ref = Some(imported.credential_ref.clone());
        app.settings.targets.push(super::default_ssh_target(1));
        app.settings.targets[1].credential_ref =
            Some("vault://bridgingio/ssh-private-key/other".into());

        app.delete_ssh_key_confirmed(&imported.credential_ref)
            .expect("delete ssh key");

        assert_eq!(app.settings.targets[0].credential_ref, None);
        assert_eq!(
            app.settings.targets[1].credential_ref.as_deref(),
            Some("vault://bridgingio/ssh-private-key/other")
        );
        assert!(matches!(app.screen, Screen::SshKeyManagement));
        assert!(app
            .last_status
            .contains("vault://bridgingio/ssh-private-key/ops-main"));
    }

    #[test]
    fn successful_ssh_key_import_navigates_to_detail_screen() {
        let config_path = temp_config_path("ssh-key-import-navigates-to-detail");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        let mut router = SecretVaultRouter::default();
        router
            .set_active_backend("builtin-encrypted")
            .expect("switch backend");
        router.unlock_with_os_native().expect("unlock vault");
        app.vault_router = Some(router);

        let key_path = config_path
            .parent()
            .expect("config parent")
            .join("id_test_import");
        fs::write(&key_path, TEST_PRIVATE_KEY_PEM).expect("write key file");
        app.ssh_import_draft.key_name = "ops-main".into();
        app.ssh_import_draft.label = "ops-main".into();
        app.ssh_import_draft.source_path = key_path.to_string_lossy().to_string();
        app.screen = Screen::SshKeyImport;

        app.execute_ssh_key_import().expect("execute ssh import");

        assert!(matches!(
            app.screen,
            Screen::SshKeyDetail(ref reference)
                if reference == "vault://bridgingio/ssh-private-key/ops-main"
        ));
        assert!(app.ssh_import_draft.source_path.is_empty());
    }

    #[test]
    fn security_lock_state_line_uses_colored_status_value() {
        let config_path = temp_config_path("security-lock-state-color");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.screen = Screen::Security;
        app.security_summary.lock_state = "locked".into();
        app.selected = 0;
        let entries = app.entries();
        let lock_line = super::format_menu_entry_styled_line(&app, 0, &entries[0]);
        let lock_state_span = lock_line
            .spans
            .iter()
            .find(|span| span.content == "locked")
            .expect("lock_state span");
        assert_eq!(lock_state_span.style.fg, Some(Color::Red));
        assert!(lock_state_span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn security_unlock_action_uses_arrow_style() {
        let config_path = temp_config_path("security-unlock-arrow");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.screen = Screen::Security;
        app.security_summary.lock_state = "locked".into();
        let entries = app.entries();
        let action_index = entries
            .iter()
            .position(|entry| matches!(entry.kind, MenuEntryKind::Action(ActionKind::UnlockVault)))
            .expect("unlock action entry");
        let line = super::format_menu_entry_line(&app, action_index, &entries[action_index]);
        assert!(line.contains(&format!("{} --->", entries[action_index].label)));
        assert!(!line.contains("***"));
    }

    #[test]
    fn security_action_flows_use_arrow_style() {
        let config_path = temp_config_path("security-action-flows-arrow");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.screen = Screen::Security;
        app.security_summary.lock_state = "unlocked".into();
        let entries = app.entries();
        for action in [
            ActionKind::CreateToken,
            ActionKind::OpenTokenManagement,
            ActionKind::DeleteVault,
        ] {
            let index = entries
                .iter()
                .position(|entry| matches!(entry.kind, MenuEntryKind::Action(ref kind) if *kind == action))
                .expect("security action entry");
            let line = super::format_menu_entry_line(&app, index, &entries[index]);
            assert!(line.contains(&format!("{} --->", entries[index].label)));
            assert!(!line.contains("***"));
        }
    }

    #[test]
    fn token_management_actions_use_arrow_style() {
        let config_path = temp_config_path("token-management-actions-arrow");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.screen = Screen::TokenManagement;
        app.security_summary.lock_state = "unlocked".into();
        app.token_rows = vec![super::TokenManagementRow {
            token_id: "token-000001".into(),
            label: "test_tok".into(),
            token_fingerprint: "fp-deadbeef0001".into(),
            status: "active".into(),
            expires_at: None,
            revoked_at: None,
            revoke_reason: None,
        }];

        let entries = app.entries();
        let index = entries
            .iter()
            .position(|entry| {
                matches!(
                    entry.kind,
                    MenuEntryKind::Action(ActionKind::OpenTokenDetail(ref token_id))
                        if token_id == "token-000001"
                )
            })
            .expect("token management detail entry");
        let line = super::format_menu_entry_line(&app, index, &entries[index]);
        assert!(line.contains("--->"));
        assert!(!line.contains("***"));
    }

    #[test]
    fn token_detail_hides_delete_until_revoked() {
        let config_path = temp_config_path("token-detail-delete-guard");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.security_summary.lock_state = "unlocked".into();
        app.token_rows = vec![super::TokenManagementRow {
            token_id: "token-000001".into(),
            label: "test_tok".into(),
            token_fingerprint: "fp-deadbeef0001".into(),
            status: "active".into(),
            expires_at: None,
            revoked_at: None,
            revoke_reason: None,
        }];
        app.screen = Screen::TokenDetail("token-000001".into());
        let entries = app.entries();
        assert!(!entries.iter().any(|entry| {
            matches!(
                entry.kind,
                MenuEntryKind::Action(ActionKind::DeleteToken(ref token_id))
                    if token_id == "token-000001"
            )
        }));
    }

    #[test]
    fn token_detail_permissions_entry_returns_placeholder_status() {
        let config_path = temp_config_path("token-detail-permissions-placeholder");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.security_summary.lock_state = "unlocked".into();
        app.token_rows = vec![super::TokenManagementRow {
            token_id: "token-000001".into(),
            label: "test_tok".into(),
            token_fingerprint: "fp-deadbeef0001".into(),
            status: "active".into(),
            expires_at: None,
            revoked_at: None,
            revoke_reason: None,
        }];
        app.run_action(ActionKind::OpenTokenDetail("token-000001".into()))
            .expect("open detail");
        app.run_action(ActionKind::OpenTokenPermissions("token-000001".into()))
            .expect("open permissions placeholder");
        assert_eq!(
            app.last_status,
            app.t("menu.status.token_permissions_placeholder")
        );
    }

    #[test]
    fn token_detail_access_switch_requires_space_instead_of_enter() {
        let config_path = temp_config_path("token-detail-access-space-only");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.security_summary.lock_state = "unlocked".into();
        app.token_rows = vec![super::TokenManagementRow {
            token_id: "token-000001".into(),
            label: "test_tok".into(),
            token_fingerprint: "fp-deadbeef0001".into(),
            status: "active".into(),
            expires_at: None,
            revoked_at: None,
            revoke_reason: None,
        }];
        app.screen = Screen::TokenDetail("token-000001".into());
        let entries = app.entries();
        app.selected = entries
            .iter()
            .position(|entry| {
                matches!(
                    entry.kind,
                    MenuEntryKind::Action(ActionKind::ToggleTokenAccess(ref token_id))
                        if token_id == "token-000001"
                )
            })
            .expect("access switch entry");
        app.activate_selected().expect("enter on access switch");
        assert_eq!(
            app.last_status,
            app.t("menu.status.token_access_space_only")
        );
    }

    #[test]
    fn credential_picker_rows_use_single_choice_toggle_markers_without_arrow() {
        let config_path = temp_config_path("credential-picker-toggle-style");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.security_summary.lock_state = "unlocked".into();
        app.settings.targets[0].kind = TargetKind::Ssh;
        app.settings.targets[0].credential_ref =
            Some("vault://bridgingio/ssh-private-key/ops-main".into());
        app.ssh_key_rows = vec![
            super::SshKeyManagementRow {
                credential_ref: "vault://bridgingio/ssh-private-key/ops-main".into(),
                label: "ops-main".into(),
                status: "active".into(),
                active_version: "v1".into(),
            },
            super::SshKeyManagementRow {
                credential_ref: "vault://bridgingio/ssh-private-key/ops-fallback".into(),
                label: "ops-fallback".into(),
                status: "active".into(),
                active_version: "v3".into(),
            },
        ];
        app.screen = Screen::TargetCredentialPicker(0);

        let entries = app.entries();
        let selected_index = entries
            .iter()
            .position(|entry| matches!(
                entry.kind,
                MenuEntryKind::Action(ActionKind::BindTargetCredentialRef { ref credential_ref, .. })
                if credential_ref == "vault://bridgingio/ssh-private-key/ops-main"
            ))
            .expect("selected credential row");
        let unselected_index = entries
            .iter()
            .position(|entry| matches!(
                entry.kind,
                MenuEntryKind::Action(ActionKind::BindTargetCredentialRef { ref credential_ref, .. })
                if credential_ref == "vault://bridgingio/ssh-private-key/ops-fallback"
            ))
            .expect("unselected credential row");

        let selected_line =
            super::format_menu_entry_line(&app, selected_index, &entries[selected_index]);
        let unselected_line =
            super::format_menu_entry_line(&app, unselected_index, &entries[unselected_index]);
        assert!(selected_line.contains("<*>"));
        assert!(unselected_line.contains("< >"));
        assert!(!selected_line.contains("--->"));
        assert!(!unselected_line.contains("--->"));
    }

    #[test]
    fn credential_picker_enter_requires_space_instead_of_binding() {
        let config_path = temp_config_path("credential-picker-enter-space-only");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.security_summary.lock_state = "unlocked".into();
        app.settings.targets[0].kind = TargetKind::Ssh;
        app.settings.targets[0].credential_ref =
            Some("vault://bridgingio/ssh-private-key/ops-main".into());
        app.ssh_key_rows = vec![
            super::SshKeyManagementRow {
                credential_ref: "vault://bridgingio/ssh-private-key/ops-main".into(),
                label: "ops-main".into(),
                status: "active".into(),
                active_version: "v1".into(),
            },
            super::SshKeyManagementRow {
                credential_ref: "vault://bridgingio/ssh-private-key/ops-fallback".into(),
                label: "ops-fallback".into(),
                status: "active".into(),
                active_version: "v3".into(),
            },
        ];
        app.screen = Screen::TargetCredentialPicker(0);
        let entries = app.entries();
        app.selected = entries
            .iter()
            .position(|entry| matches!(
                entry.kind,
                MenuEntryKind::Action(ActionKind::BindTargetCredentialRef { ref credential_ref, .. })
                if credential_ref == "vault://bridgingio/ssh-private-key/ops-fallback"
            ))
            .expect("fallback credential row");

        app.activate_selected()
            .expect("enter should not bind credential row");
        assert_eq!(
            app.last_status,
            app.t("menu.status.target_credential_picker_space_only")
        );
        assert_eq!(
            app.settings.targets[0].credential_ref.as_deref(),
            Some("vault://bridgingio/ssh-private-key/ops-main")
        );
    }

    #[test]
    fn credential_picker_space_binds_selected_reference() {
        let config_path = temp_config_path("credential-picker-space-binds");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.security_summary.lock_state = "unlocked".into();
        app.settings.targets[0].kind = TargetKind::Ssh;
        app.settings.targets[0].credential_ref =
            Some("vault://bridgingio/ssh-private-key/ops-main".into());
        app.ssh_key_rows = vec![
            super::SshKeyManagementRow {
                credential_ref: "vault://bridgingio/ssh-private-key/ops-main".into(),
                label: "ops-main".into(),
                status: "active".into(),
                active_version: "v1".into(),
            },
            super::SshKeyManagementRow {
                credential_ref: "vault://bridgingio/ssh-private-key/ops-fallback".into(),
                label: "ops-fallback".into(),
                status: "active".into(),
                active_version: "v3".into(),
            },
        ];
        app.screen = Screen::TargetCredentialPicker(0);
        let entries = app.entries();
        app.selected = entries
            .iter()
            .position(|entry| matches!(
                entry.kind,
                MenuEntryKind::Action(ActionKind::BindTargetCredentialRef { ref credential_ref, .. })
                if credential_ref == "vault://bridgingio/ssh-private-key/ops-fallback"
            ))
            .expect("fallback credential row");

        app.handle_space_on_selected()
            .expect("space binds selected credential row");
        assert_eq!(
            app.settings.targets[0].credential_ref.as_deref(),
            Some("vault://bridgingio/ssh-private-key/ops-fallback")
        );
        assert!(app
            .dirty_paths
            .iter()
            .any(|path| path == "targets[0].credential_ref"));
    }

    #[test]
    fn security_unlocked_notice_uses_notice_marker() {
        let config_path = temp_config_path("security-unlocked-marker");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.screen = Screen::Security;
        app.security_summary.lock_state = "unlocked".into();
        let entries = app.entries();
        let notice_index = entries
            .iter()
            .position(|entry| entry.label == app.t("menu.security.unlocked_notice"))
            .expect("unlocked notice entry");
        let line = super::format_menu_entry_line(&app, notice_index, &entries[notice_index]);
        assert!(line.contains("-*-"));
        assert!(line.contains(&entries[notice_index].label));
    }
}
