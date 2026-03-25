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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandInvocation {
    pub program: PathBuf,
    pub args: Vec<String>,
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

    pub fn connect(&self, target: &TargetProfile, now: SystemTime) -> Result<SessionRecord, ResolveError> {
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

    pub fn connect(&self, target: &TargetProfile, now: SystemTime) -> Result<SessionRecord, ResolveError> {
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

    use bridgingio_domain::{ChannelKind, ChannelStatus, ConnectionConfig, PolicyProfile, TargetKind, TargetProfile};

    use super::{AdbConnector, ExecutableResolver, ExecutableSource, SshConnector};

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
        };

        let invocation = connector
            .build_exec_invocation(&target, "uname -a")
            .expect("ssh invocation");
        assert_eq!(invocation.args[0], "-p");
        assert_eq!(invocation.args[1], "2222");
        assert_eq!(invocation.args[2], "root@10.1.1.8");
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
        };

        let invocation = connector
            .build_exec_invocation(&target, "getprop ro.build.version.release")
            .expect("adb invocation");
        assert_eq!(invocation.args[0], "-s");
        assert_eq!(invocation.args[1], "device-01");
        assert_eq!(invocation.args[2], "shell");
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
