use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bridgingio_domain::{
    ChannelKind, ChannelRecord, ChannelStatus, ConnectionConfig, SessionRecord, SessionState,
    TargetKind, TargetProfile,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecutableSource {
    UserOverride,
    SystemPath,
    BuiltInFallback,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutableSelection {
    pub source: ExecutableSource,
    pub path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolveError {
    NotFound { command: String },
    InvalidTargetKind { expected: String, actual: String },
}

#[derive(Clone, Debug, Default)]
pub struct ExecutableResolver {
    search_paths: Vec<PathBuf>,
}

impl ExecutableResolver {
    pub fn from_system_path() -> Self {
        let search_paths = std::env::var_os("PATH")
            .map(|raw| std::env::split_paths(&raw).collect())
            .unwrap_or_default();
        Self { search_paths }
    }

    pub fn with_search_paths(search_paths: Vec<PathBuf>) -> Self {
        Self { search_paths }
    }

    pub fn resolve(
        &self,
        command: &str,
        user_override: Option<&Path>,
        builtin_fallback: Option<&Path>,
    ) -> Result<ExecutableSelection, ResolveError> {
        if let Some(path) = user_override {
            if path.is_file() {
                return Ok(ExecutableSelection {
                    source: ExecutableSource::UserOverride,
                    path: path.to_path_buf(),
                });
            }
        }

        for dir in &self.search_paths {
            for candidate in command_variants(command) {
                let full = dir.join(candidate);
                if full.is_file() {
                    return Ok(ExecutableSelection {
                        source: ExecutableSource::SystemPath,
                        path: full,
                    });
                }
            }
        }

        if let Some(path) = builtin_fallback {
            if path.is_file() {
                return Ok(ExecutableSelection {
                    source: ExecutableSource::BuiltInFallback,
                    path: path.to_path_buf(),
                });
            }
        }

        Err(ResolveError::NotFound {
            command: command.to_string(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuiltInDistributionKind {
    AppBundleResource,
    StandalonePackage,
    PlatformAsset,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuiltInBinarySpec {
    pub command: String,
    pub relative_path: PathBuf,
    pub distribution: BuiltInDistributionKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolchainResolutionDiagnostics {
    pub command: String,
    pub selected_source: Option<String>,
    pub selected_path: Option<PathBuf>,
    pub user_override: Option<PathBuf>,
    pub searched_system_paths: Vec<PathBuf>,
    pub builtin_candidates: Vec<PathBuf>,
    pub warnings: Vec<String>,
}

impl ToolchainResolutionDiagnostics {
    pub fn selected_label(&self) -> Option<&str> {
        self.selected_source.as_deref()
    }
}

#[derive(Clone, Debug)]
pub struct ToolchainResolver {
    executable: ExecutableResolver,
    builtin_root: PathBuf,
    builtin_specs: HashMap<String, BuiltInBinarySpec>,
}

impl ToolchainResolver {
    pub fn new(
        executable: ExecutableResolver,
        builtin_root: impl Into<PathBuf>,
        builtin_specs: Vec<BuiltInBinarySpec>,
    ) -> Self {
        let mut specs = HashMap::new();
        for spec in builtin_specs {
            specs.insert(spec.command.clone(), spec);
        }
        Self {
            executable,
            builtin_root: builtin_root.into(),
            builtin_specs: specs,
        }
    }

    pub fn resolve_with_diagnostics(
        &self,
        command: &str,
        user_override: Option<&Path>,
    ) -> (
        Result<ExecutableSelection, ResolveError>,
        ToolchainResolutionDiagnostics,
    ) {
        let builtin_candidates = self.builtin_candidate_paths(command);
        let mut result = self.executable.resolve(command, user_override, None);
        if matches!(result, Err(ResolveError::NotFound { .. })) {
            if let Some(path) = builtin_candidates.iter().find(|candidate| candidate.is_file()) {
                result = Ok(ExecutableSelection {
                    source: ExecutableSource::BuiltInFallback,
                    path: path.clone(),
                });
            }
        }
        let searched_system_paths = self
            .executable
            .search_paths
            .iter()
            .flat_map(|path| command_variants(command).into_iter().map(|candidate| path.join(candidate)))
            .collect::<Vec<_>>();

        let mut diagnostics = ToolchainResolutionDiagnostics {
            command: command.to_string(),
            selected_source: None,
            selected_path: None,
            user_override: user_override.map(Path::to_path_buf),
            searched_system_paths,
            builtin_candidates,
            warnings: Vec::new(),
        };

        match &result {
            Ok(selected) => {
                diagnostics.selected_source = Some(
                    match selected.source {
                        ExecutableSource::UserOverride => "user_override",
                        ExecutableSource::SystemPath => "system_path",
                        ExecutableSource::BuiltInFallback => "builtin_fallback",
                    }
                    .to_string(),
                );
                diagnostics.selected_path = Some(selected.path.clone());
            }
            Err(ResolveError::NotFound { .. }) => {
                diagnostics.warnings.push(
                    "no executable found in override, system path, or built-in fallback".into(),
                );
            }
            Err(_) => {}
        }

        (result, diagnostics)
    }

    fn builtin_candidate_paths(&self, command: &str) -> Vec<PathBuf> {
        let Some(spec) = self.builtin_specs.get(command) else {
            return Vec::new();
        };
        let mut candidates = Vec::new();
        for prefix in distribution_prefixes(&spec.distribution) {
            let relative = if prefix.as_os_str().is_empty() {
                spec.relative_path.clone()
            } else {
                prefix.join(&spec.relative_path)
            };
            for variant in path_with_command_variants(&relative) {
                let full = self.builtin_root.join(variant);
                if !candidates.contains(&full) {
                    candidates.push(full);
                }
            }
        }
        candidates
    }
}

fn command_variants(command: &str) -> Vec<String> {
    if cfg!(windows) {
        vec![
            command.to_string(),
            format!("{command}.exe"),
            format!("{command}.bat"),
        ]
    } else {
        vec![command.to_string()]
    }
}

fn path_with_command_variants(path: &Path) -> Vec<PathBuf> {
    if cfg!(windows) && path.extension().is_none() {
        let text = path.to_string_lossy().to_string();
        vec![
            PathBuf::from(&text),
            PathBuf::from(format!("{text}.exe")),
            PathBuf::from(format!("{text}.bat")),
        ]
    } else {
        vec![path.to_path_buf()]
    }
}

fn distribution_prefixes(kind: &BuiltInDistributionKind) -> Vec<PathBuf> {
    match kind {
        BuiltInDistributionKind::StandalonePackage => vec![PathBuf::new(), PathBuf::from("bin")],
        BuiltInDistributionKind::AppBundleResource => vec![
            PathBuf::new(),
            PathBuf::from("Resources"),
            PathBuf::from("Resources/bin"),
            PathBuf::from("resources"),
            PathBuf::from("resources/bin"),
        ],
        BuiltInDistributionKind::PlatformAsset => {
            let platform = if cfg!(windows) { "windows" } else { "unix" };
            vec![
                PathBuf::new(),
                PathBuf::from(platform),
                PathBuf::from(format!("{platform}/bin")),
            ]
        }
    }
}

pub const TARGET_TERMINAL_SHELL_METADATA_KEY: &str = "terminal.shell";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvocationKind {
    OneShot,
    Interactive,
}

impl InvocationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OneShot => "one_shot",
            Self::Interactive => "interactive",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetShellDialect {
    SshPosix,
    AdbAndroidShell,
    SshWindowsCmd,
    SshPowerShell,
    Other(String),
}

impl TargetShellDialect {
    pub fn as_str(&self) -> &str {
        match self {
            Self::SshPosix => "ssh-posix",
            Self::AdbAndroidShell => "adb-android-shell",
            Self::SshWindowsCmd => "ssh-windows-cmd",
            Self::SshPowerShell => "ssh-powershell",
            Self::Other(value) => value.as_str(),
        }
    }

    pub fn support_level(&self) -> &'static str {
        match self {
            Self::SshPosix | Self::AdbAndroidShell => "supported",
            Self::SshWindowsCmd | Self::SshPowerShell | Self::Other(_) => "deferred",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InvocationResolution {
    pub target_override_path: Option<String>,
    pub global_override_path: Option<String>,
    pub effective_scope: Option<String>,
    pub effective_source: Option<String>,
    pub effective_path: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvocationQuotingBoundary {
    pub host_shell_runtime: String,
    pub target_shell_dialect: String,
}

impl Default for InvocationQuotingBoundary {
    fn default() -> Self {
        Self {
            host_shell_runtime: "quotes local program/args for host shell tokenization".to_string(),
            target_shell_dialect:
                "defines remote shell semantics and payload argument shape".to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvocationDiagnosticsView {
    pub mode: String,
    pub program: String,
    pub args: Vec<String>,
    pub target_shell_dialect: String,
    pub dialect_support: String,
    pub target_override_path: Option<String>,
    pub global_override_path: Option<String>,
    pub effective_scope: Option<String>,
    pub effective_source: Option<String>,
    pub effective_path: Option<String>,
    pub warnings: Vec<String>,
    pub quoting_host_shell_runtime: String,
    pub quoting_target_shell_dialect: String,
}

pub fn target_shell_dialect_for(target: &TargetProfile) -> TargetShellDialect {
    let shell_override = target
        .metadata
        .get(TARGET_TERMINAL_SHELL_METADATA_KEY)
        .map(String::as_str)
        .unwrap_or_default()
        .trim();
    let normalized_shell = shell_override
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(shell_override)
        .trim()
        .to_ascii_lowercase();

    match &target.kind {
        TargetKind::Adb => match normalized_shell.as_str() {
            "" | "adb-android-shell" | "android-shell" | "sh" | "ash" => {
                TargetShellDialect::AdbAndroidShell
            }
            other => TargetShellDialect::Other(other.to_string()),
        },
        TargetKind::Ssh => match normalized_shell.as_str() {
            "" | "ssh-posix" | "posix" | "sh" | "bash" | "zsh" => TargetShellDialect::SshPosix,
            "ssh-windows-cmd" | "windows-cmd" | "cmd" | "cmd.exe" => {
                TargetShellDialect::SshWindowsCmd
            }
            "ssh-powershell" | "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe" => {
                TargetShellDialect::SshPowerShell
            }
            other => TargetShellDialect::Other(other.to_string()),
        },
        _ => {
            if normalized_shell.is_empty() {
                TargetShellDialect::Other("unknown".to_string())
            } else {
                TargetShellDialect::Other(normalized_shell)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandInvocation {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub invocation_kind: InvocationKind,
    pub target_shell_dialect: TargetShellDialect,
    pub resolution: InvocationResolution,
    pub quoting_boundary: InvocationQuotingBoundary,
}

impl CommandInvocation {
    pub fn with_resolution(mut self, resolution: InvocationResolution) -> Self {
        self.resolution = resolution;
        self
    }

    pub fn to_host_shell_command(&self) -> String {
        let mut tokens = Vec::with_capacity(self.args.len() + 1);
        tokens.push(host_shell_quote(self.program.to_string_lossy().as_ref()));
        tokens.extend(self.args.iter().map(|arg| host_shell_quote(arg)));
        tokens.join(" ")
    }

    pub fn diagnostics_view(&self) -> InvocationDiagnosticsView {
        InvocationDiagnosticsView {
            mode: self.invocation_kind.as_str().to_string(),
            program: self.program.to_string_lossy().to_string(),
            args: self.args.clone(),
            target_shell_dialect: self.target_shell_dialect.as_str().to_string(),
            dialect_support: self.target_shell_dialect.support_level().to_string(),
            target_override_path: self.resolution.target_override_path.clone(),
            global_override_path: self.resolution.global_override_path.clone(),
            effective_scope: self.resolution.effective_scope.clone(),
            effective_source: self.resolution.effective_source.clone(),
            effective_path: self.resolution.effective_path.clone(),
            warnings: self.resolution.warnings.clone(),
            quoting_host_shell_runtime: self.quoting_boundary.host_shell_runtime.clone(),
            quoting_target_shell_dialect: self.quoting_boundary.target_shell_dialect.clone(),
        }
    }
}

fn host_shell_quote(value: &str) -> String {
    #[cfg(windows)]
    {
        value.to_string()
    }
    #[cfg(not(windows))]
    {
        format!("'{}'", value.replace('\'', "'\"'\"'"))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnvironmentProbePlan {
    pub commands: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ConnectorChannelTracker {
    pub channels: HashMap<String, ChannelRecord>,
    next_channel_seq: u64,
}

impl ConnectorChannelTracker {
    pub fn open_channel(
        &mut self,
        prefix: &str,
        logical_session_id: &str,
        transport_session_id: &str,
        target_id: &str,
        channel_kind: ChannelKind,
        display_name: Option<String>,
        now: SystemTime,
    ) -> ChannelRecord {
        self.next_channel_seq += 1;
        let channel_id = format!("{prefix}-ch-{:06}", self.next_channel_seq);
        let record = ChannelRecord {
            channel_id: channel_id.clone(),
            logical_session_id: logical_session_id.to_string(),
            transport_session_id: transport_session_id.to_string(),
            target_id: target_id.to_string(),
            channel_kind,
            status: ChannelStatus::Active,
            display_name,
            working_directory: None,
            foreground_command: None,
            created_at: now,
            last_activity_at: now,
            close_reason: None,
        };
        self.channels.insert(channel_id, record.clone());
        record
    }

    pub fn update_channel_status(
        &mut self,
        channel_id: &str,
        status: ChannelStatus,
        close_reason: Option<String>,
        now: SystemTime,
    ) -> Option<ChannelRecord> {
        let channel = self.channels.get_mut(channel_id)?;
        channel.status = status;
        channel.last_activity_at = now;
        channel.close_reason = close_reason;
        Some(channel.clone())
    }

    pub fn list_channels(&self, logical_session_id: &str) -> Vec<ChannelRecord> {
        let mut channels: Vec<_> = self
            .channels
            .values()
            .filter(|c| c.logical_session_id == logical_session_id)
            .cloned()
            .collect();
        channels.sort_by(|a, b| a.channel_id.cmp(&b.channel_id));
        channels
    }
}

pub struct SshConnector {
    pub executable: PathBuf,
    pub channel_tracker: ConnectorChannelTracker,
}

impl SshConnector {
    pub fn new(executable: PathBuf) -> Self {
        Self {
            executable,
            channel_tracker: ConnectorChannelTracker::default(),
        }
    }

    pub fn connect(
        &self,
        target: &TargetProfile,
        now: SystemTime,
    ) -> Result<SessionRecord, ResolveError> {
        if !matches!(target.kind, TargetKind::Ssh) {
            return Err(ResolveError::InvalidTargetKind {
                expected: "ssh".into(),
                actual: format!("{:?}", target.kind),
            });
        }
        let mut session = SessionRecord::new(format!("ssh:{}", target.id), target.id.clone(), now);
        session.transition(SessionState::Connected, now, None);
        Ok(session)
    }

    pub fn build_exec_invocation(
        &self,
        target: &TargetProfile,
        command: &str,
    ) -> Result<CommandInvocation, ResolveError> {
        if !matches!(target.kind, TargetKind::Ssh) {
            return Err(ResolveError::InvalidTargetKind {
                expected: "ssh".into(),
                actual: format!("{:?}", target.kind),
            });
        }
        let (host, port, username) = match &target.connection {
            ConnectionConfig::Ssh {
                host,
                port,
                username,
            } => (host.clone(), *port, username.clone()),
            _ => {
                return Err(ResolveError::InvalidTargetKind {
                    expected: "ConnectionConfig::Ssh".into(),
                    actual: "other config".into(),
                })
            }
        };

        Ok(CommandInvocation {
            program: self.executable.clone(),
            args: vec![
                "-p".into(),
                port.to_string(),
                format!("{username}@{host}"),
                command.to_string(),
            ],
            invocation_kind: InvocationKind::OneShot,
            target_shell_dialect: target_shell_dialect_for(target),
            resolution: InvocationResolution::default(),
            quoting_boundary: InvocationQuotingBoundary::default(),
        })
    }

    pub fn build_interactive_invocation(
        &self,
        target: &TargetProfile,
    ) -> Result<CommandInvocation, ResolveError> {
        if !matches!(target.kind, TargetKind::Ssh) {
            return Err(ResolveError::InvalidTargetKind {
                expected: "ssh".into(),
                actual: format!("{:?}", target.kind),
            });
        }
        let (host, port, username) = match &target.connection {
            ConnectionConfig::Ssh {
                host,
                port,
                username,
            } => (host.clone(), *port, username.clone()),
            _ => {
                return Err(ResolveError::InvalidTargetKind {
                    expected: "ConnectionConfig::Ssh".into(),
                    actual: "other config".into(),
                })
            }
        };

        Ok(CommandInvocation {
            program: self.executable.clone(),
            args: vec!["-p".into(), port.to_string(), format!("{username}@{host}")],
            invocation_kind: InvocationKind::Interactive,
            target_shell_dialect: target_shell_dialect_for(target),
            resolution: InvocationResolution::default(),
            quoting_boundary: InvocationQuotingBoundary::default(),
        })
    }

    pub fn environment_probe_plan(&self) -> EnvironmentProbePlan {
        EnvironmentProbePlan {
            commands: vec![
                "uname -s".into(),
                "uname -r".into(),
                "uname -m".into(),
                "echo $SHELL".into(),
                "command -v git adb ssh".into(),
            ],
        }
    }

    pub fn open_channel(
        &mut self,
        logical_session_id: &str,
        transport_session_id: &str,
        target_id: &str,
        channel_kind: ChannelKind,
        display_name: Option<String>,
        now: SystemTime,
    ) -> ChannelRecord {
        self.channel_tracker.open_channel(
            "ssh",
            logical_session_id,
            transport_session_id,
            target_id,
            channel_kind,
            display_name,
            now,
        )
    }

    pub fn update_channel_status(
        &mut self,
        channel_id: &str,
        status: ChannelStatus,
        close_reason: Option<String>,
        now: SystemTime,
    ) -> Option<ChannelRecord> {
        self.channel_tracker
            .update_channel_status(channel_id, status, close_reason, now)
    }

    pub fn list_channels(&self, logical_session_id: &str) -> Vec<ChannelRecord> {
        self.channel_tracker.list_channels(logical_session_id)
    }
}

pub struct AdbConnector {
    pub executable: PathBuf,
    pub channel_tracker: ConnectorChannelTracker,
}

impl AdbConnector {
    pub fn new(executable: PathBuf) -> Self {
        Self {
            executable,
            channel_tracker: ConnectorChannelTracker::default(),
        }
    }

    pub fn connect(
        &self,
        target: &TargetProfile,
        now: SystemTime,
    ) -> Result<SessionRecord, ResolveError> {
        if !matches!(target.kind, TargetKind::Adb) {
            return Err(ResolveError::InvalidTargetKind {
                expected: "adb".into(),
                actual: format!("{:?}", target.kind),
            });
        }
        let mut session = SessionRecord::new(format!("adb:{}", target.id), target.id.clone(), now);
        session.transition(SessionState::Connected, now, None);
        Ok(session)
    }

    pub fn build_exec_invocation(
        &self,
        target: &TargetProfile,
        command: &str,
    ) -> Result<CommandInvocation, ResolveError> {
        if !matches!(target.kind, TargetKind::Adb) {
            return Err(ResolveError::InvalidTargetKind {
                expected: "adb".into(),
                actual: format!("{:?}", target.kind),
            });
        }

        let mut args = Vec::<String>::new();
        if let ConnectionConfig::Adb { serial, .. } = &target.connection {
            if let Some(s) = serial {
                args.push("-s".into());
                args.push(s.clone());
            }
        }
        args.push("shell".into());
        args.push(command.to_string());

        Ok(CommandInvocation {
            program: self.executable.clone(),
            args,
            invocation_kind: InvocationKind::OneShot,
            target_shell_dialect: target_shell_dialect_for(target),
            resolution: InvocationResolution::default(),
            quoting_boundary: InvocationQuotingBoundary::default(),
        })
    }

    pub fn build_interactive_invocation(
        &self,
        target: &TargetProfile,
    ) -> Result<CommandInvocation, ResolveError> {
        if !matches!(target.kind, TargetKind::Adb) {
            return Err(ResolveError::InvalidTargetKind {
                expected: "adb".into(),
                actual: format!("{:?}", target.kind),
            });
        }

        let mut args = Vec::<String>::new();
        if let ConnectionConfig::Adb { serial, .. } = &target.connection {
            if let Some(s) = serial {
                args.push("-s".into());
                args.push(s.clone());
            }
        }
        args.push("shell".into());

        Ok(CommandInvocation {
            program: self.executable.clone(),
            args,
            invocation_kind: InvocationKind::Interactive,
            target_shell_dialect: target_shell_dialect_for(target),
            resolution: InvocationResolution::default(),
            quoting_boundary: InvocationQuotingBoundary::default(),
        })
    }

    pub fn environment_probe_plan(&self) -> EnvironmentProbePlan {
        EnvironmentProbePlan {
            commands: vec![
                "getprop ro.build.version.release".into(),
                "getprop ro.product.cpu.abi".into(),
                "uname -r".into(),
                "echo $SHELL".into(),
                "which sh toybox toolbox".into(),
            ],
        }
    }

    pub fn open_channel(
        &mut self,
        logical_session_id: &str,
        transport_session_id: &str,
        target_id: &str,
        channel_kind: ChannelKind,
        display_name: Option<String>,
        now: SystemTime,
    ) -> ChannelRecord {
        self.channel_tracker.open_channel(
            "adb",
            logical_session_id,
            transport_session_id,
            target_id,
            channel_kind,
            display_name,
            now,
        )
    }

    pub fn update_channel_status(
        &mut self,
        channel_id: &str,
        status: ChannelStatus,
        close_reason: Option<String>,
        now: SystemTime,
    ) -> Option<ChannelRecord> {
        self.channel_tracker
            .update_channel_status(channel_id, status, close_reason, now)
    }

    pub fn list_channels(&self, logical_session_id: &str) -> Vec<ChannelRecord> {
        self.channel_tracker.list_channels(logical_session_id)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use bridgingio_domain::{
        ChannelKind, ChannelStatus, ConnectionConfig, PolicyProfile, TargetKind, TargetProfile,
    };

    use super::{
        target_shell_dialect_for, AdbConnector, BuiltInBinarySpec, BuiltInDistributionKind,
        ExecutableResolver, ExecutableSource, InvocationKind, InvocationResolution,
        TargetShellDialect, SshConnector, ToolchainResolver, TARGET_TERMINAL_SHELL_METADATA_KEY,
    };

    fn temp_dir(prefix: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("bridgingio-{prefix}-{stamp}"));
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    #[test]
    fn prefers_user_override_before_system_path() {
        let root = temp_dir("override");
        let user = root.join("custom-adb");
        let system = root.join("adb");
        fs::write(&user, "binary").expect("write user");
        fs::write(&system, "binary").expect("write system");

        let resolver = ExecutableResolver::with_search_paths(vec![root]);
        let selected = resolver
            .resolve("adb", Some(&user), None)
            .expect("resolve executable");

        assert_eq!(selected.source, ExecutableSource::UserOverride);
        assert_eq!(selected.path, user);
    }

    #[test]
    fn falls_back_to_builtin_when_system_missing() {
        let root = temp_dir("fallback");
        let fallback = root.join("embedded-adb");
        fs::write(&fallback, "binary").expect("write fallback");

        let resolver = ExecutableResolver::with_search_paths(vec![root.join("empty")]);
        let selected = resolver
            .resolve("adb", None, Some(&fallback))
            .expect("resolve executable");

        assert_eq!(selected.source, ExecutableSource::BuiltInFallback);
        assert_eq!(selected.path, fallback);
    }

    #[test]
    fn resolver_reports_builtin_distribution_diagnostics() {
        let root = temp_dir("toolchain-diag");
        let bundled = root.join("bin").join("adb");
        fs::create_dir_all(bundled.parent().expect("parent")).expect("create dir");
        fs::write(&bundled, "binary").expect("write bundled");

        let toolchain = ToolchainResolver::new(
            ExecutableResolver::with_search_paths(vec![root.join("not-found")]),
            &root,
            vec![BuiltInBinarySpec {
                command: "adb".into(),
                relative_path: PathBuf::from("bin/adb"),
                distribution: BuiltInDistributionKind::StandalonePackage,
            }],
        );

        let (selected, diagnostics) = toolchain.resolve_with_diagnostics("adb", None);
        let selected = selected.expect("must resolve");
        assert_eq!(selected.source, ExecutableSource::BuiltInFallback);
        assert_eq!(diagnostics.selected_label(), Some("builtin_fallback"));
        assert!(!diagnostics.builtin_candidates.is_empty());
    }

    #[test]
    fn resolver_supports_app_bundle_distribution_layout() {
        let root = temp_dir("toolchain-app-bundle");
        let bundled = root.join("Resources").join("bin").join("adb");
        fs::create_dir_all(bundled.parent().expect("parent")).expect("create dir");
        fs::write(&bundled, "binary").expect("write bundled");

        let toolchain = ToolchainResolver::new(
            ExecutableResolver::with_search_paths(vec![root.join("not-found")]),
            &root,
            vec![BuiltInBinarySpec {
                command: "adb".into(),
                relative_path: PathBuf::from("adb"),
                distribution: BuiltInDistributionKind::AppBundleResource,
            }],
        );

        let (selected, diagnostics) = toolchain.resolve_with_diagnostics("adb", None);
        let selected = selected.expect("must resolve");
        assert_eq!(selected.source, ExecutableSource::BuiltInFallback);
        assert_eq!(
            selected.path.to_string_lossy(),
            bundled.to_string_lossy(),
        );
        assert!(
            diagnostics
                .builtin_candidates
                .iter()
                .any(|candidate| candidate == &bundled)
        );
    }

    #[test]
    fn builds_ssh_invocation_with_user_host_and_port() {
        let connector = SshConnector::new(PathBuf::from("/usr/bin/ssh"));
        let target = TargetProfile {
            id: "t-ssh".into(),
            name: "ssh-host".into(),
            kind: TargetKind::Ssh,
            connection: ConnectionConfig::Ssh {
                host: "10.1.1.8".into(),
                port: 2222,
                username: "root".into(),
            },
            credential_ref: None,
            default_policy: PolicyProfile::default(),
            notes: None,
            metadata: Default::default(),
            toolchains: Default::default(),
        };

        let invocation = connector
            .build_exec_invocation(&target, "uname -a")
            .expect("ssh invocation");
        assert_eq!(invocation.args[0], "-p");
        assert_eq!(invocation.args[1], "2222");
        assert_eq!(invocation.args[2], "root@10.1.1.8");
    }

    #[test]
    fn builds_ssh_interactive_invocation() {
        let connector = SshConnector::new(PathBuf::from("/usr/bin/ssh"));
        let target = TargetProfile {
            id: "t-ssh".into(),
            name: "ssh-host".into(),
            kind: TargetKind::Ssh,
            connection: ConnectionConfig::Ssh {
                host: "10.1.1.8".into(),
                port: 22,
                username: "root".into(),
            },
            credential_ref: None,
            default_policy: PolicyProfile::default(),
            notes: None,
            metadata: Default::default(),
            toolchains: Default::default(),
        };

        let invocation = connector
            .build_interactive_invocation(&target)
            .expect("ssh interactive invocation");
        assert_eq!(invocation.args[0], "-p");
        assert_eq!(invocation.args[1], "22");
        assert_eq!(invocation.args[2], "root@10.1.1.8");
        assert_eq!(invocation.invocation_kind, InvocationKind::Interactive);
        assert_eq!(invocation.target_shell_dialect.as_str(), "ssh-posix");
    }

    #[test]
    fn invocation_diagnostics_exposes_resolution_hierarchy() {
        let connector = SshConnector::new(PathBuf::from("/usr/bin/ssh"));
        let mut target = TargetProfile {
            id: "t-ssh".into(),
            name: "ssh-host".into(),
            kind: TargetKind::Ssh,
            connection: ConnectionConfig::Ssh {
                host: "10.1.1.8".into(),
                port: 22,
                username: "root".into(),
            },
            credential_ref: None,
            default_policy: PolicyProfile::default(),
            notes: None,
            metadata: Default::default(),
            toolchains: Default::default(),
        };
        target.metadata.insert(
            TARGET_TERMINAL_SHELL_METADATA_KEY.to_string(),
            "ssh-windows-cmd".to_string(),
        );
        let invocation = connector
            .build_exec_invocation(&target, "whoami")
            .expect("ssh invocation")
            .with_resolution(InvocationResolution {
                target_override_path: Some("/tmp/target-ssh".into()),
                global_override_path: Some("/tmp/global-ssh".into()),
                effective_scope: Some("target_override".into()),
                effective_source: Some("user_override".into()),
                effective_path: Some("/tmp/target-ssh".into()),
                warnings: vec!["future dialect semantics are deferred".into()],
            });

        let view = invocation.diagnostics_view();
        assert_eq!(view.target_shell_dialect, "ssh-windows-cmd");
        assert_eq!(view.dialect_support, "deferred");
        assert_eq!(view.effective_scope.as_deref(), Some("target_override"));
        assert_eq!(view.effective_source.as_deref(), Some("user_override"));
        assert_eq!(view.effective_path.as_deref(), Some("/tmp/target-ssh"));
    }

    #[test]
    fn target_shell_override_selects_dialect() {
        let mut target = TargetProfile {
            id: "t-ssh".into(),
            name: "ssh-host".into(),
            kind: TargetKind::Ssh,
            connection: ConnectionConfig::Ssh {
                host: "10.1.1.8".into(),
                port: 22,
                username: "root".into(),
            },
            credential_ref: None,
            default_policy: PolicyProfile::default(),
            notes: None,
            metadata: Default::default(),
            toolchains: Default::default(),
        };
        assert_eq!(target_shell_dialect_for(&target).as_str(), "ssh-posix");
        target.metadata.insert(
            TARGET_TERMINAL_SHELL_METADATA_KEY.to_string(),
            "powershell".to_string(),
        );
        assert_eq!(target_shell_dialect_for(&target).as_str(), "ssh-powershell");
    }

    #[test]
    fn adb_targets_default_to_android_shell_dialect() {
        let target = TargetProfile {
            id: "t-adb".into(),
            name: "pixel".into(),
            kind: TargetKind::Adb,
            connection: ConnectionConfig::Adb {
                serial: Some("device-01".into()),
                transport: None,
            },
            credential_ref: None,
            default_policy: PolicyProfile::default(),
            notes: None,
            metadata: Default::default(),
            toolchains: Default::default(),
        };
        let dialect = target_shell_dialect_for(&target);
        assert_eq!(dialect, TargetShellDialect::AdbAndroidShell);
        assert_eq!(dialect.support_level(), "supported");
    }

    #[test]
    fn future_shell_dialect_is_marked_deferred() {
        let mut target = TargetProfile {
            id: "t-ssh".into(),
            name: "ssh-future".into(),
            kind: TargetKind::Ssh,
            connection: ConnectionConfig::Ssh {
                host: "10.1.1.9".into(),
                port: 22,
                username: "root".into(),
            },
            credential_ref: None,
            default_policy: PolicyProfile::default(),
            notes: None,
            metadata: Default::default(),
            toolchains: Default::default(),
        };
        target.metadata.insert(
            TARGET_TERMINAL_SHELL_METADATA_KEY.to_string(),
            "ssh-windows-cmd".to_string(),
        );
        let dialect = target_shell_dialect_for(&target);
        assert_eq!(dialect, TargetShellDialect::SshWindowsCmd);
        assert_eq!(dialect.support_level(), "deferred");
    }

    #[test]
    fn builds_adb_shell_invocation_with_serial() {
        let connector = AdbConnector::new(PathBuf::from("/usr/bin/adb"));
        let target = TargetProfile {
            id: "t-adb".into(),
            name: "pixel".into(),
            kind: TargetKind::Adb,
            connection: ConnectionConfig::Adb {
                serial: Some("device-01".into()),
                transport: None,
            },
            credential_ref: None,
            default_policy: PolicyProfile::default(),
            notes: None,
            metadata: Default::default(),
            toolchains: Default::default(),
        };

        let invocation = connector
            .build_exec_invocation(&target, "getprop ro.build.version.release")
            .expect("adb invocation");
        assert_eq!(invocation.args[0], "-s");
        assert_eq!(invocation.args[1], "device-01");
        assert_eq!(invocation.args[2], "shell");
    }

    #[test]
    fn builds_adb_interactive_invocation_with_serial() {
        let connector = AdbConnector::new(PathBuf::from("/usr/bin/adb"));
        let target = TargetProfile {
            id: "t-adb".into(),
            name: "pixel".into(),
            kind: TargetKind::Adb,
            connection: ConnectionConfig::Adb {
                serial: Some("device-01".into()),
                transport: None,
            },
            credential_ref: None,
            default_policy: PolicyProfile::default(),
            notes: None,
            metadata: Default::default(),
            toolchains: Default::default(),
        };

        let invocation = connector
            .build_interactive_invocation(&target)
            .expect("adb interactive invocation");
        assert_eq!(invocation.args[0], "-s");
        assert_eq!(invocation.args[1], "device-01");
        assert_eq!(invocation.args[2], "shell");
    }

    #[test]
    fn structured_invocation_serializes_to_host_shell_command() {
        let connector = SshConnector::new(PathBuf::from("/opt/tools/ssh"));
        let target = TargetProfile {
            id: "t-ssh".into(),
            name: "ssh-host".into(),
            kind: TargetKind::Ssh,
            connection: ConnectionConfig::Ssh {
                host: "10.1.1.8".into(),
                port: 22,
                username: "root".into(),
            },
            credential_ref: None,
            default_policy: PolicyProfile::default(),
            notes: None,
            metadata: Default::default(),
            toolchains: Default::default(),
        };
        let invocation = connector
            .build_exec_invocation(&target, "uname -r")
            .expect("ssh invocation");
        let command = invocation.to_host_shell_command();
        #[cfg(windows)]
        assert_eq!(command, "/opt/tools/ssh -p 22 root@10.1.1.8 uname -r");
        #[cfg(not(windows))]
        assert_eq!(command, "'/opt/tools/ssh' '-p' '22' 'root@10.1.1.8' 'uname -r'");
    }

    #[test]
    fn ssh_supports_multi_channel_in_one_logical_session() {
        let mut connector = SshConnector::new(PathBuf::from("/usr/bin/ssh"));
        let now = SystemTime::now();
        let one = connector.open_channel(
            "ls-1",
            "ts-1",
            "target-ssh",
            ChannelKind::OneShotExec,
            Some("command".into()),
            now,
        );
        let two = connector.open_channel(
            "ls-1",
            "ts-1",
            "target-ssh",
            ChannelKind::LogStream,
            Some("logs".into()),
            now,
        );

        assert_ne!(one.channel_id, two.channel_id);
        assert_eq!(connector.list_channels("ls-1").len(), 2);
    }

    #[test]
    fn adb_channel_state_can_be_tracked_independently() {
        let mut connector = AdbConnector::new(PathBuf::from("/usr/bin/adb"));
        let now = SystemTime::now();
        let top_channel = connector.open_channel(
            "ls-adb",
            "ts-adb",
            "target-adb",
            ChannelKind::InteractiveShell,
            Some("top".into()),
            now,
        );
        let whoami_channel = connector.open_channel(
            "ls-adb",
            "ts-adb",
            "target-adb",
            ChannelKind::InteractiveShell,
            Some("whoami".into()),
            now,
        );

        let updated = connector
            .update_channel_status(
                &top_channel.channel_id,
                ChannelStatus::Draining,
                Some("streaming".into()),
                now,
            )
            .expect("update channel status");
        assert_eq!(updated.status, ChannelStatus::Draining);
        assert_eq!(connector.list_channels("ls-adb").len(), 2);
        assert_ne!(top_channel.channel_id, whoami_channel.channel_id);
    }
}
