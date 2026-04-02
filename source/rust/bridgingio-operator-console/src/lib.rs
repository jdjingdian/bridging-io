use std::fs;
use std::io::{self, Stdout};
use std::path::{Path, PathBuf};
use std::time::Duration;

use bridgingio_domain::TargetKind;
use bridgingio_engine::{
    CoreSettings, StandaloneConnectionSection, StandaloneTargetProfile, StandaloneTerminalSection,
    TerminalProviderSection,
};
use bridgingio_secrets::SecretVaultRouter;
#[cfg(test)]
use bridgingio_secrets::VaultUnlockTriggerPolicy;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
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
    pub token_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    Root,
    Core,
    Storage,
    ModelPlane,
    Vault,
    Targets,
    Security,
    TargetEditor(usize),
    SearchResults,
}

impl Screen {
    fn title(self) -> &'static str {
        match self {
            Screen::Root => "Main Menu",
            Screen::Core => "Core",
            Screen::Storage => "Storage",
            Screen::ModelPlane => "Model Plane",
            Screen::Vault => "Vault",
            Screen::Targets => "Targets",
            Screen::Security => "Security",
            Screen::TargetEditor(_) => "Target",
            Screen::SearchResults => "Search Results",
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActionKind {
    AddSshTarget,
    AddAdbTarget,
    UnlockVault,
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

    fn label(self) -> &'static str {
        match self {
            FooterButton::Select => "<Select>",
            FooterButton::Exit => "< Exit >",
            FooterButton::Help => "< Help >",
        }
    }
}

pub struct MenuConfigApp {
    config_path: PathBuf,
    settings: CoreSettings,
    vault_router: SecretVaultRouter,
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
    last_status: String,
    last_apply_strategy: Option<String>,
}

impl MenuConfigApp {
    pub fn load(config_path: impl Into<PathBuf>) -> Result<Self, String> {
        let config_path = config_path.into();
        let settings = CoreSettings::load_from_file(&config_path)
            .map_err(|err| format!("load menuconfig settings failed: {err:?}"))?;
        let mut vault_router = load_vault_router(&settings);
        let security_summary = build_security_summary(&settings, &mut vault_router);
        Ok(Self {
            config_path,
            settings,
            vault_router,
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
            last_status: "Use Up/Down to pick item, Left/Right to pick <Select>/<Exit>/<Help>, Enter confirms."
                .into(),
            last_apply_strategy: None,
        })
    }

    pub fn run(&mut self) -> Result<MenuConfigOutcome, String> {
        let mut ui =
            TerminalUi::enter().map_err(|err| format!("enter menuconfig ui failed: {err}"))?;
        loop {
            ui.terminal
                .draw(|frame| render(frame, self))
                .map_err(|err| format!("draw menuconfig failed: {err}"))?;

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

            if self.exit_confirm_mode {
                if let Some(outcome) = self.handle_exit_confirm_key(key.code)? {
                    return Ok(outcome);
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
                        return Ok(outcome);
                    }
                }
                KeyCode::Esc => {
                    if self.show_help {
                        self.show_help = false;
                        self.last_status = "Help closed.".into();
                    } else if let Some(outcome) = self.request_exit_or_back()? {
                        return Ok(outcome);
                    }
                }
                KeyCode::Left => self.move_footer_selection(-1),
                KeyCode::Right => self.move_footer_selection(1),
                KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
                KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
                KeyCode::Char('/') => {
                    self.search_mode = true;
                    self.search_input.clear();
                    self.last_status =
                        "Search: type a field, target, or menu label and press Enter.".into();
                }
                KeyCode::Char('?') | KeyCode::F(1) => self.show_help = !self.show_help,
                KeyCode::Char('u') => self.unlock_vault_shortcut()?,
                KeyCode::Char('s') => self.save()?,
                KeyCode::Char(' ') => self.handle_space_on_selected()?,
                KeyCode::Enter => {
                    if let Some(outcome) = self.activate_footer_button()? {
                        return Ok(outcome);
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
                KeyCode::Esc => self.cancel_edit("Edit cancelled."),
                KeyCode::Enter => self.commit_edit()?,
                KeyCode::Left => self.move_text_cursor(-1),
                KeyCode::Right => self.move_text_cursor(1),
                KeyCode::Backspace => self.backspace_text_char(),
                KeyCode::Char(ch) => self.insert_text_char(ch),
                _ => {}
            },
            EditModeKind::Choice => match code {
                KeyCode::Esc => self.cancel_edit("Selection cancelled."),
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
                self.last_status = "Search cancelled.".into();
            }
            KeyCode::Enter => {
                self.search_mode = false;
                self.push_navigation_state();
                self.screen = Screen::SearchResults;
                self.selected = 0;
                self.last_status = if self.search_input.trim().is_empty() {
                    "Search query is empty.".into()
                } else {
                    format!("Search results for `{}`", self.search_input)
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
                self.screen = screen;
                self.selected = 0;
                self.last_status = format!("Opened {}.", screen.title());
            }
            MenuEntryKind::EditField(field) => {
                self.begin_edit(field)?;
            }
            MenuEntryKind::FocusField { screen, field } => {
                self.push_navigation_state();
                self.screen = screen;
                self.select_field(&field);
                self.last_status = format!("Focused `{field}`.");
            }
            MenuEntryKind::Action(action) => self.run_action(action)?,
            MenuEntryKind::Info => {
                self.last_status = entry.description;
            }
        }
        Ok(())
    }

    fn begin_edit(&mut self, field: String) -> Result<(), String> {
        let Some(value) = field_value(&self.settings, &field) else {
            self.last_status = format!("`{field}` is read-only in menuconfig.");
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
            self.last_status =
                format!("Selecting `{field}`. Up/Down move, Space or Enter selects, Esc cancels.");
        } else {
            self.edit_mode_kind = EditModeKind::Text;
            self.edit_input = value;
            self.edit_cursor = self.edit_input.chars().count();
            self.edit_options.clear();
            self.edit_option_selected = 0;
            self.last_status = format!("Editing `{field}`. Enter saves, Esc cancels.");
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
                .ok_or_else(|| "no available option for selected field".to_string())?,
        };
        self.apply_edit_value(&field, &value)?;
        self.reset_edit_state();
        self.last_status = format!("Updated `{field}`.");
        Ok(())
    }

    fn apply_edit_value(&mut self, field: &str, value: &str) -> Result<(), String> {
        let mut next = self.settings.clone();
        apply_field_edit(&mut next, field, value)?;
        next.validate()
            .map_err(|err| format!("validate edited settings failed: {err:?}"))?;
        self.settings = next;
        if !self.dirty_paths.contains(&field.to_string()) {
            self.dirty_paths.push(field.to_string());
        }
        self.security_summary = build_security_summary(&self.settings, &mut self.vault_router);
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

    fn cancel_edit(&mut self, status: &str) {
        self.reset_edit_state();
        self.last_status = status.into();
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
        if let MenuEntryKind::EditField(field) = entry.kind {
            if is_boolean_toggle_field(&field) {
                let current = field_value(&self.settings, &field).unwrap_or_else(|| "false".into());
                let parsed = current
                    .trim()
                    .parse::<bool>()
                    .map_err(|_| format!("`{field}` cannot be toggled as bool"))?;
                let toggled = (!parsed).to_string();
                self.apply_edit_value(&field, &toggled)?;
                self.last_status = format!("Toggled `{field}` to {toggled}.");
            } else if field_options(&field).is_some() {
                self.begin_edit(field)?;
            }
        }
        Ok(())
    }

    fn save(&mut self) -> Result<(), String> {
        self.settings
            .validate()
            .map_err(|err| format!("validate settings failed: {err:?}"))?;
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                format!(
                    "create config parent dir failed ({}): {err}",
                    parent.display()
                )
            })?;
        }
        fs::write(&self.config_path, self.settings.to_toml_string()).map_err(|err| {
            format!(
                "persist menuconfig failed ({}): {err}",
                self.config_path.display()
            )
        })?;
        self.last_apply_strategy = Some("restart_required".into());
        self.dirty_paths.clear();
        self.last_status = format!(
            "Saved {}. Apply strategy: restart_required.",
            self.config_path.display()
        );
        Ok(())
    }

    fn unlock_vault_shortcut(&mut self) -> Result<(), String> {
        if self.security_summary.lock_state == "unlocked" {
            self.last_status = "Vault is already unlocked.".into();
            return Ok(());
        }

        let methods = ordered_unlock_methods(&self.settings);
        let mut last_error = None::<String>;
        for method in methods {
            match method.as_str() {
                "os-native" => match self.vault_router.unlock_with_os_native() {
                    Ok(()) => {
                        self.security_summary =
                            build_security_summary(&self.settings, &mut self.vault_router);
                        self.last_status = "Vault unlocked with os-native.".into();
                        return Ok(());
                    }
                    Err(err) => last_error = Some(format!("{err:?}")),
                },
                "passphrase" => {
                    let passphrase = rpassword::prompt_password("Enter vault passphrase: ")
                        .map_err(|err| format!("read vault passphrase failed: {err}"))?;
                    match self.vault_router.unlock_with_passphrase(&passphrase) {
                        Ok(()) => {
                            self.security_summary =
                                build_security_summary(&self.settings, &mut self.vault_router);
                            self.last_status = "Vault unlocked with passphrase.".into();
                            return Ok(());
                        }
                        Err(err) => last_error = Some(format!("{err:?}")),
                    }
                }
                other => {
                    last_error = Some(format!("unsupported unlock method `{other}`"));
                }
            }
        }
        self.last_status = format!(
            "Vault unlock failed. {}",
            last_error.unwrap_or_else(|| "No allowed unlock method succeeded.".into())
        );
        Ok(())
    }

    fn run_action(&mut self, action: ActionKind) -> Result<(), String> {
        match action {
            ActionKind::AddSshTarget => {
                let index = self.settings.targets.len();
                self.settings.targets.push(default_ssh_target(index));
                self.push_navigation_state();
                self.screen = Screen::TargetEditor(index);
                self.selected = 0;
                self.last_status = "Added SSH target. Edit fields before saving.".into();
            }
            ActionKind::AddAdbTarget => {
                let index = self.settings.targets.len();
                self.settings.targets.push(default_adb_target(index));
                self.push_navigation_state();
                self.screen = Screen::TargetEditor(index);
                self.selected = 0;
                self.last_status = "Added ADB target. Edit fields before saving.".into();
            }
            ActionKind::UnlockVault => {
                self.unlock_vault_shortcut()?;
            }
        }
        Ok(())
    }

    fn go_back(&mut self) {
        if let Some((screen, selected)) = self.navigation_stack.pop() {
            self.screen = screen;
            self.selected = selected;
            self.last_status = format!("Returned to {}.", self.screen.title());
        } else {
            self.last_status = "Already at top menu.".into();
        }
    }

    fn push_navigation_state(&mut self) {
        self.navigation_stack.push((self.screen, self.selected));
    }

    fn move_footer_selection(&mut self, delta: isize) {
        let next = self.footer_selected as isize + delta;
        self.footer_selected = next.clamp(0, 2) as usize;
        self.last_status = format!(
            "Button selected: {}",
            FooterButton::from_index(self.footer_selected).label()
        );
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
                    "Help opened.".into()
                } else {
                    "Help closed.".into()
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
        self.go_back();
        Ok(None)
    }

    fn begin_exit_confirm(&mut self) {
        self.exit_confirm_mode = true;
        self.exit_confirm_selected = 0;
        self.last_status = "Unsaved changes detected. Choose whether to save before exit.".into();
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
                self.last_status = "Exit cancelled.".into();
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
                        self.last_status = "Exit cancelled.".into();
                    }
                }
            }
            _ => {}
        }
        Ok(None)
    }

    fn move_selection(&mut self, delta: isize) {
        let total = self.entries().len();
        if total == 0 {
            self.selected = 0;
            return;
        }
        let next = self.selected as isize + delta;
        self.selected = next.clamp(0, total.saturating_sub(1) as isize) as usize;
    }

    fn select_field(&mut self, field: &str) {
        if let Some(index) = self.entries().iter().position(
            |entry| matches!(&entry.kind, MenuEntryKind::EditField(value) if value == field),
        ) {
            self.selected = index;
        } else {
            self.selected = 0;
        }
    }

    fn entries(&self) -> Vec<MenuEntry> {
        match self.screen {
            Screen::Root => root_entries(),
            Screen::Core => core_entries(&self.settings),
            Screen::Storage => storage_entries(&self.settings),
            Screen::ModelPlane => model_plane_entries(&self.settings),
            Screen::Vault => vault_entries(&self.settings),
            Screen::Targets => targets_entries(&self.settings),
            Screen::Security => security_entries(&self.security_summary),
            Screen::TargetEditor(index) => target_editor_entries(&self.settings, index),
            Screen::SearchResults => search_entries(&self.settings, &self.search_input),
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

fn render(frame: &mut ratatui::Frame, app: &MenuConfigApp) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(5)])
        .split(frame.area());

    render_main_menu(frame, layout[0], app);
    render_footer(frame, layout[1], app);

    if app.show_help {
        render_help(frame);
    }
    if app.edit_mode {
        render_edit_popup(frame, app);
    }
    if app.exit_confirm_mode {
        render_exit_confirm(frame, app);
    }
}

fn render_main_menu(frame: &mut ratatui::Frame, area: Rect, app: &MenuConfigApp) {
    let raw_lines = app
        .entries()
        .iter()
        .enumerate()
        .map(|(index, entry)| format_menu_entry_line(app, index, entry))
        .collect::<Vec<_>>();

    let title = format!(
        "BridgingIO Menuconfig / {}{}",
        app.screen.title(),
        if app.is_dirty() { " [*]" } else { "" }
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
    let content_width = raw_lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    let list_width = chunks[0].width as usize;
    let left_padding = if list_width > content_width {
        (list_width - content_width) / 2
    } else {
        0
    };
    let left_pad = " ".repeat(left_padding);
    let items = raw_lines
        .into_iter()
        .map(|line| ListItem::new(Line::from(format!("{left_pad}{line}"))))
        .collect::<Vec<_>>();
    let list = List::new(items);
    frame.render_widget(list, chunks[0]);
    if chunks.len() > 1 {
        let buttons = Paragraph::new(render_footer_buttons_line(app.footer_selected))
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
    let footer = Paragraph::new(vec![
        Line::from(format!(
            "path={} dirty={} lock_state={} search={}",
            app.screen.title(),
            app.is_dirty(),
            app.security_summary.lock_state,
            if app.search_input.is_empty() {
                "<none>".to_string()
            } else {
                app.search_input.clone()
            }
        )),
        Line::from(format!("*** {} ****", selected_description)),
        Line::from(if app.edit_mode {
            match app.edit_mode_kind {
                EditModeKind::Text => format!("edit> {}", app.edit_input),
                EditModeKind::Choice => format!(
                    "select> {}",
                    app.edit_options
                        .get(app.edit_option_selected)
                        .cloned()
                        .unwrap_or_default()
                ),
            }
        } else if app.exit_confirm_mode {
            "exit> choose with Left/Right, Enter confirms, Esc cancels".to_string()
        } else if app.search_mode {
            format!("search> {}", app.search_input)
        } else {
            format!(
                "keys: Up/Down item, Left/Right buttons, Enter confirm, Esc back/exit, Space toggle, / search{}",
                app.last_apply_strategy
                    .as_deref()
                    .map(|value| format!(", apply_strategy={value}"))
                    .unwrap_or_default()
            )
        }),
    ])
    .block(Block::default().title("Status").borders(Borders::ALL))
    .wrap(Wrap { trim: false });
    frame.render_widget(footer, area);
}

fn render_help(frame: &mut ratatui::Frame) {
    let area = centered_rect(70, 45, frame.area());
    frame.render_widget(Clear, area);
    let help = Paragraph::new(vec![
        Line::from("menuconfig keys"),
        Line::from(""),
        Line::from("Enter: run highlighted footer button"),
        Line::from("Left/Right: switch <Select> < Exit > < Help >"),
        Line::from("Esc: back one level, or exit from Main Menu"),
        Line::from("Space: toggle boolean field or open chooser"),
        Line::from("Text edit: Left/Right move cursor, Backspace deletes"),
        Line::from("/: global search"),
        Line::from("u: unlock vault on demand"),
        Line::from("s: save config"),
        Line::from("q: quit"),
        Line::from(""),
        Line::from("Targets can be added from the Targets menu."),
    ])
    .block(Block::default().title("Help").borders(Borders::ALL))
    .wrap(Wrap { trim: false });
    frame.render_widget(help, area);
}

fn render_exit_confirm(frame: &mut ratatui::Frame, app: &MenuConfigApp) {
    let area = centered_rect(56, 30, frame.area());
    frame.render_widget(Clear, area);
    let option = |index: usize, label: &str| -> String {
        if app.exit_confirm_selected == index {
            format!("< {label} >")
        } else {
            format!("  {label}  ")
        }
    };
    let popup = Paragraph::new(vec![
        Line::from("Do you wish to save your new configuration?"),
        Line::from("Press Enter to confirm, Esc to continue menuconfig."),
        Line::from(""),
        Line::from(format!(
            "{}   {}   {}",
            option(0, "Yes"),
            option(1, "No"),
            option(2, "Cancel")
        )),
    ])
    .block(
        Block::default()
            .title("Unsaved Changes")
            .borders(Borders::ALL),
    )
    .wrap(Wrap { trim: false });
    frame.render_widget(popup, area);
}

fn render_footer_buttons_line(selected: usize) -> Line<'static> {
    let mut spans = Vec::new();
    for index in 0..3 {
        if index > 0 {
            spans.push(Span::raw("    "));
        }
        let base = FooterButton::from_index(index).label();
        if selected == index {
            spans.push(Span::styled(
                base.to_string(),
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::raw(base.to_string()));
        }
    }
    Line::from(spans)
}

fn format_menu_entry_line(app: &MenuConfigApp, index: usize, entry: &MenuEntry) -> String {
    let selector = if index == app.selected { ">" } else { " " };
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
                let marker = if enabled { "[*]" } else { "[ ]" };
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
                let marker = if enabled { "[*]" } else { "[ ]" };
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
        MenuEntryKind::Action(_) => format!("{selector} *** {} ****", entry.label),
    }
}

fn render_edit_popup(frame: &mut ratatui::Frame, app: &MenuConfigApp) {
    let Some(field) = app.edit_field.as_deref() else {
        return;
    };
    match app.edit_mode_kind {
        EditModeKind::Text => {
            let area = centered_rect(70, 28, frame.area());
            frame.render_widget(Clear, area);
            let input_prefix = "value> ";
            let popup = Paragraph::new(vec![
                Line::from(format!("Field: {field}")),
                Line::from("Enter saves, Esc cancels."),
                Line::from(""),
                Line::from(format!("{input_prefix}{}", app.edit_input)),
            ])
            .block(Block::default().title("Edit Value").borders(Borders::ALL))
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
                Line::from(format!("Select value for: {field}")),
                Line::from("Use Up/Down and press Space or Enter to confirm."),
            ])
            .block(Block::default().title("Select Value").borders(Borders::ALL))
            .wrap(Wrap { trim: false });
            frame.render_widget(header, chunks[0]);

            let items = app
                .edit_options
                .iter()
                .enumerate()
                .map(|(index, option)| {
                    let cursor = if index == app.edit_option_selected {
                        ">"
                    } else {
                        " "
                    };
                    let marker = if index == app.edit_option_selected {
                        "*"
                    } else {
                        " "
                    };
                    ListItem::new(Line::from(format!("{cursor} ({marker}) {option}")))
                })
                .collect::<Vec<_>>();
            let list = List::new(items).block(Block::default().borders(Borders::ALL));
            frame.render_widget(list, chunks[1]);
        }
    }
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

fn root_entries() -> Vec<MenuEntry> {
    vec![
        nav_entry(
            "Core",
            "Core runtime identity and logging settings.",
            Screen::Core,
        ),
        nav_entry(
            "Storage",
            "Artifact backend and storage sizing.",
            Screen::Storage,
        ),
        nav_entry(
            "Model Plane",
            "HTTP listener and exposure settings.",
            Screen::ModelPlane,
        ),
        nav_entry("Vault", "Vault policy and unlock behavior.", Screen::Vault),
        nav_entry(
            "Targets",
            "Configure SSH/ADB targets and add new entries.",
            Screen::Targets,
        ),
        nav_entry(
            "Security",
            "Display-safe vault and token summary.",
            Screen::Security,
        ),
    ]
}

fn core_entries(settings: &CoreSettings) -> Vec<MenuEntry> {
    vec![
        edit_entry(
            "Instance Name",
            "core.instance_name",
            &settings.core.instance_name,
            "Core instance identifier used in diagnostics and runtime labels.",
        ),
        info_entry(
            "Data Dir",
            Some(settings.core.data_dir.clone()),
            "Canonical runtime root currently used by the config.",
        ),
        edit_entry(
            "Log Level",
            "core.log_level",
            &settings.core.log_level,
            "Core log level for runtime diagnostics.",
        ),
    ]
}

fn storage_entries(settings: &CoreSettings) -> Vec<MenuEntry> {
    vec![
        edit_entry(
            "Artifact Backend",
            "storage.artifacts.backend",
            &settings.storage.artifacts.backend,
            "Artifact cache backend. Use memory or filesystem.",
        ),
        edit_entry(
            "Artifact Max Bytes",
            "storage.artifacts.max_bytes",
            &settings.storage.artifacts.max_bytes.to_string(),
            "Maximum artifact cache size in bytes.",
        ),
    ]
}

fn model_plane_entries(settings: &CoreSettings) -> Vec<MenuEntry> {
    vec![
        edit_entry(
            "HTTP Host",
            "model_plane.http.host",
            &settings.model_plane.http.host,
            "Model-plane HTTP bind host.",
        ),
        edit_entry(
            "HTTP Port",
            "model_plane.http.port",
            &settings.model_plane.http.port.to_string(),
            "Model-plane HTTP bind port.",
        ),
        edit_entry(
            "Allow Non Loopback",
            "model_plane.http.allow_non_loopback",
            &settings.model_plane.http.allow_non_loopback.to_string(),
            "Whether non-loopback exposure is allowed.",
        ),
    ]
}

fn vault_entries(settings: &CoreSettings) -> Vec<MenuEntry> {
    vec![
        info_entry(
            "Vault Backend",
            Some(settings.vault.backend.clone()),
            "Canonical vault backend in use.",
        ),
        edit_entry(
            "Unlock Trigger",
            "vault.unlock.trigger_policy",
            &settings.vault.unlock.trigger_policy,
            "Vault unlock trigger policy. This changes when unlock is required.",
        ),
        info_entry(
            "Preferred Unlock",
            Some(settings.vault.unlock.preferred_method.clone()),
            "Preferred local unlock method. Press 'u' to unlock on demand.",
        ),
        info_entry(
            "Allowed Methods",
            Some(settings.vault.unlock.allowed_methods.join(",")),
            "Configured local unlock methods.",
        ),
    ]
}

fn security_entries(summary: &SecuritySummary) -> Vec<MenuEntry> {
    vec![
        info_entry(
            "Vault Lock State",
            Some(summary.lock_state.clone()),
            "Current display-safe vault lock state.",
        ),
        info_entry(
            "Vault Backend",
            Some(summary.backend.clone()),
            "Configured vault backend.",
        ),
        info_entry(
            "Preferred Unlock",
            Some(summary.preferred_method.clone()),
            "Preferred unlock method from config.",
        ),
        info_entry(
            "Secret Count",
            Some(summary.secret_count.to_string()),
            "Display-safe number of persisted vault secrets.",
        ),
        info_entry(
            "Token Count",
            Some(summary.token_count.to_string()),
            "Display-safe number of agent tokens.",
        ),
        action_entry(
            "Unlock Vault",
            "Unlock the vault on demand without forcing unlock on menuconfig startup.",
            ActionKind::UnlockVault,
        ),
    ]
}

fn targets_entries(settings: &CoreSettings) -> Vec<MenuEntry> {
    let mut entries = settings
        .targets
        .iter()
        .enumerate()
        .map(|(index, target)| MenuEntry {
            label: format!(
                "{} ({})",
                target.display_name,
                target_kind_label(&target.kind)
            ),
            value: Some(target.id.clone()),
            description: "Open target editor.".into(),
            dirty_key: None,
            kind: MenuEntryKind::Navigate(Screen::TargetEditor(index)),
        })
        .collect::<Vec<_>>();
    entries.push(action_entry(
        "Add SSH Target",
        "Create a new SSH target with editable defaults.",
        ActionKind::AddSshTarget,
    ));
    entries.push(action_entry(
        "Add ADB Target",
        "Create a new ADB target with editable defaults.",
        ActionKind::AddAdbTarget,
    ));
    entries
}

fn target_editor_entries(settings: &CoreSettings, index: usize) -> Vec<MenuEntry> {
    let Some(target) = settings.targets.get(index) else {
        return vec![info_entry(
            "Missing Target",
            None,
            "Target index is out of range.",
        )];
    };
    let mut entries = vec![
        edit_entry(
            "Target Id",
            &format!("targets[{index}].id"),
            &target.id,
            "Stable target identifier used by CLI, MCP, and future UI.",
        ),
        edit_entry(
            "Display Name",
            &format!("targets[{index}].display_name"),
            &target.display_name,
            "Display-safe name shown in menus and diagnostics.",
        ),
        edit_entry(
            "Enabled",
            &format!("targets[{index}].enabled"),
            &target.enabled.to_string(),
            "Whether this target can be resolved and used.",
        ),
        edit_entry(
            "Aliases",
            &format!("targets[{index}].aliases"),
            &target.aliases.join(","),
            "Comma-separated aliases for this target.",
        ),
        edit_entry(
            "Credential Ref",
            &format!("targets[{index}].credential_ref"),
            target.credential_ref.as_deref().unwrap_or(""),
            "Vault credential reference for this target.",
        ),
        edit_entry(
            "Notes",
            &format!("targets[{index}].notes"),
            target.notes.as_deref().unwrap_or(""),
            "Optional display-safe notes for the target.",
        ),
    ];
    match &target.kind {
        TargetKind::Ssh => {
            entries.push(edit_entry(
                "SSH Host",
                &format!("targets[{index}].connection.host"),
                target.connection.host.as_deref().unwrap_or(""),
                "SSH host or IP address.",
            ));
            entries.push(edit_entry(
                "SSH Port",
                &format!("targets[{index}].connection.port"),
                &target
                    .connection
                    .port
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
                "SSH port.",
            ));
            entries.push(edit_entry(
                "SSH Username",
                &format!("targets[{index}].connection.username"),
                target.connection.username.as_deref().unwrap_or(""),
                "SSH username.",
            ));
        }
        TargetKind::Adb => {
            entries.push(edit_entry(
                "Selector Kind",
                &format!("targets[{index}].connection.selector_kind"),
                target.connection.selector_kind.as_deref().unwrap_or(""),
                "ADB selector kind such as serial.",
            ));
            entries.push(edit_entry(
                "Selector Value",
                &format!("targets[{index}].connection.selector_value"),
                target.connection.selector_value.as_deref().unwrap_or(""),
                "ADB selector value such as emulator-5554.",
            ));
        }
        _ => {}
    }
    entries
}

fn search_entries(settings: &CoreSettings, query: &str) -> Vec<MenuEntry> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return vec![info_entry(
            "No Query",
            None,
            "Type a field, menu, or target name and press Enter to search.",
        )];
    }

    let mut entries = Vec::new();
    for entry in core_entries(settings)
        .into_iter()
        .chain(storage_entries(settings))
        .chain(model_plane_entries(settings))
        .chain(vault_entries(settings))
        .chain(targets_entries(settings))
        .chain(flatten_target_fields(settings))
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
                        screen: screen_for_field(&field),
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
            "No Matches",
            None,
            "Search returned no matching menus or fields.",
        ));
    }
    entries
}

fn flatten_target_fields(settings: &CoreSettings) -> Vec<MenuEntry> {
    let mut entries = Vec::new();
    for index in 0..settings.targets.len() {
        entries.extend(target_editor_entries(settings, index));
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

fn screen_for_field(field: &str) -> Screen {
    match field {
        "core.instance_name" | "core.log_level" => Screen::Core,
        "storage.artifacts.backend" | "storage.artifacts.max_bytes" => Screen::Storage,
        "model_plane.http.host"
        | "model_plane.http.port"
        | "model_plane.http.allow_non_loopback" => Screen::ModelPlane,
        "vault.unlock.trigger_policy" => Screen::Vault,
        _ => parse_target_field(field)
            .map(|(index, _)| Screen::TargetEditor(index))
            .unwrap_or(Screen::Root),
    }
}

fn load_vault_router(settings: &CoreSettings) -> SecretVaultRouter {
    let vault_root = Path::new(&settings.core.data_dir).join("vault");
    let mut router = if vault_root.exists() {
        SecretVaultRouter::with_persistent_store(&vault_root).unwrap_or_default()
    } else {
        SecretVaultRouter::default()
    };
    let _ = router.set_active_backend(&settings.vault.backend);
    router
}

fn build_security_summary(
    settings: &CoreSettings,
    router: &mut SecretVaultRouter,
) -> SecuritySummary {
    SecuritySummary {
        backend: settings.vault.backend.clone(),
        lock_state: router.vault_lock_state().as_str().to_string(),
        trigger_policy: settings.vault.unlock.trigger_policy.clone(),
        preferred_method: settings.vault.unlock.preferred_method.clone(),
        secret_count: router.list_secret_summaries().len(),
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

fn target_kind_label(kind: &TargetKind) -> &'static str {
    match kind {
        TargetKind::Ssh => "ssh",
        TargetKind::Adb => "adb",
        TargetKind::Serial => "serial",
        TargetKind::Docker => "docker",
        TargetKind::Other(_) => "custom",
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
    use bridgingio_secrets::VaultUnlockTriggerPolicy;
    use crossterm::event::KeyCode;
    use std::fs;
    use std::path::PathBuf;
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
        assert_eq!(app.screen, Screen::TargetEditor(1));
        assert_eq!(app.settings.targets.len(), 2);
        assert_eq!(app.settings.targets[1].kind, TargetKind::Ssh);
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
    fn startup_does_not_auto_unlock_vault() {
        let config_path = temp_config_path("locked");
        let mut app = MenuConfigApp::load(&config_path).expect("load app");
        app.settings.vault.unlock.trigger_policy = "on-core-start".into();
        app.security_summary = super::build_security_summary(&app.settings, &mut app.vault_router);
        assert_ne!(app.security_summary.lock_state, "unlocked");
        assert_eq!(
            super::parse_trigger_policy(&app.settings.vault.unlock.trigger_policy),
            VaultUnlockTriggerPolicy::OnCoreStart
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
        app.screen = Screen::TargetEditor(0);
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
}
