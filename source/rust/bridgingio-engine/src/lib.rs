use std::collections::HashMap;
use std::fs;
use std::net::IpAddr;
use std::path::Path;
use std::time::SystemTime;

use bridgingio_domain::{
    build_logical_session_key, AccessScope, ApprovalRequestRecord, ArtifactRecord, AuditEvent,
    ChannelKind, ChannelRecord, ChannelStatus, EnvironmentFingerprint, LogicalSessionRecord,
    LogicalSessionStatus, SessionRecord, SessionReusePolicy, TargetKind, TargetProfile,
    TransportSessionRecord, TransportSessionStatus,
};

#[derive(Default)]
pub struct InMemoryMetadataStore {
    pub profiles: HashMap<String, TargetProfile>,
    pub sessions: HashMap<String, SessionRecord>,
    pub logical_sessions: HashMap<String, LogicalSessionRecord>,
    pub logical_session_keys: HashMap<String, String>,
    pub transport_sessions: HashMap<String, TransportSessionRecord>,
    pub channels: HashMap<String, ChannelRecord>,
    pub artifacts: HashMap<String, ArtifactRecord>,
    pub fingerprints: HashMap<String, EnvironmentFingerprint>,
    pub audit_events: Vec<AuditEvent>,
    pub approvals: HashMap<String, ApprovalRequestRecord>,
    next_logical_session_seq: u64,
    next_transport_session_seq: u64,
    next_channel_seq: u64,
}

impl InMemoryMetadataStore {
    pub fn upsert_profile(&mut self, profile: TargetProfile) {
        self.profiles.insert(profile.id.clone(), profile);
    }

    pub fn upsert_session(&mut self, session: SessionRecord) {
        self.sessions.insert(session.id.clone(), session);
    }

    pub fn upsert_logical_session(&mut self, session: LogicalSessionRecord) {
        self.logical_sessions
            .insert(session.logical_session_id.clone(), session);
    }

    pub fn upsert_transport_session(&mut self, session: TransportSessionRecord) {
        self.transport_sessions
            .insert(session.transport_session_id.clone(), session);
    }

    pub fn upsert_channel(&mut self, channel: ChannelRecord) {
        self.channels.insert(channel.channel_id.clone(), channel);
    }

    pub fn upsert_artifact(&mut self, artifact: ArtifactRecord) {
        self.artifacts.insert(artifact.id.clone(), artifact);
    }

    pub fn upsert_fingerprint(
        &mut self,
        session_id: impl Into<String>,
        fingerprint: EnvironmentFingerprint,
    ) {
        self.fingerprints.insert(session_id.into(), fingerprint);
    }

    pub fn append_audit(&mut self, event: AuditEvent) {
        self.audit_events.push(event);
    }

    pub fn upsert_approval(&mut self, request: ApprovalRequestRecord) {
        self.approvals.insert(request.id.clone(), request);
    }

    pub fn resolve_logical_session(
        &mut self,
        scope: &AccessScope,
        target_id: &str,
        reuse_policy: SessionReusePolicy,
        now: SystemTime,
    ) -> LogicalSessionRecord {
        let session_key = build_logical_session_key(scope, target_id, &scope.client_session_id);
        if !matches!(reuse_policy, SessionReusePolicy::AlwaysNew) {
            if let Some(existing_id) = self.logical_session_keys.get(&session_key).cloned() {
                if let Some(existing) = self.logical_sessions.get_mut(&existing_id) {
                    let should_reuse = match reuse_policy {
                        SessionReusePolicy::AlwaysNew => false,
                        SessionReusePolicy::ReuseIfAlive => existing.status.is_alive(),
                        SessionReusePolicy::ResumeOrCreate => true,
                    };

                    if should_reuse {
                        existing.last_activity_at = now;
                        existing.reuse_policy = reuse_policy.clone();
                        if matches!(reuse_policy, SessionReusePolicy::ResumeOrCreate)
                            && !existing.status.is_alive()
                        {
                            existing.status = LogicalSessionStatus::Active;
                            existing.closed_at = None;
                            existing.close_reason = None;
                        }
                        return existing.clone();
                    }
                }
            }
        }

        let logical_session_id = allocate_id(&mut self.next_logical_session_seq, "ls");
        let record = LogicalSessionRecord {
            logical_session_id: logical_session_id.clone(),
            session_key: session_key.clone(),
            scope_id: scope.scope_id.clone(),
            target_id: target_id.to_string(),
            reuse_policy,
            status: LogicalSessionStatus::Active,
            created_at: now,
            last_activity_at: now,
            closed_at: None,
            close_reason: None,
        };

        self.logical_session_keys
            .insert(session_key, logical_session_id);
        self.logical_sessions
            .insert(record.logical_session_id.clone(), record.clone());
        record
    }

    pub fn open_transport_session(
        &mut self,
        logical_session_id: &str,
        target_id: &str,
        connector_kind: TargetKind,
        resolved_executable_path: Option<String>,
        resolved_executable_source: Option<String>,
        now: SystemTime,
    ) -> TransportSessionRecord {
        let transport_session_id = allocate_id(&mut self.next_transport_session_seq, "ts");
        let record = TransportSessionRecord {
            transport_session_id: transport_session_id.clone(),
            logical_session_id: logical_session_id.to_string(),
            target_id: target_id.to_string(),
            connector_kind,
            status: TransportSessionStatus::Connected,
            created_at: now,
            last_activity_at: now,
            close_reason: None,
            resolved_executable_path,
            resolved_executable_source,
        };

        if let Some(logical) = self.logical_sessions.get_mut(logical_session_id) {
            logical.last_activity_at = now;
            if !logical.status.is_alive() {
                logical.status = LogicalSessionStatus::Active;
                logical.closed_at = None;
                logical.close_reason = None;
            }
        }
        self.transport_sessions
            .insert(transport_session_id, record.clone());
        record
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
        let channel_id = allocate_id(&mut self.next_channel_seq, "ch");
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

        if let Some(logical) = self.logical_sessions.get_mut(logical_session_id) {
            logical.last_activity_at = now;
        }
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

    pub fn channels_for_logical_session(&self, logical_session_id: &str) -> Vec<ChannelRecord> {
        let mut channels: Vec<_> = self
            .channels
            .values()
            .filter(|c| c.logical_session_id == logical_session_id)
            .cloned()
            .collect();
        channels.sort_by(|a, b| a.channel_id.cmp(&b.channel_id));
        channels
    }

    pub fn close_logical_session(
        &mut self,
        logical_session_id: &str,
        reason: impl Into<String>,
        now: SystemTime,
    ) -> Option<LogicalSessionRecord> {
        let session = self.logical_sessions.get_mut(logical_session_id)?;
        session.status = LogicalSessionStatus::Closed;
        session.last_activity_at = now;
        session.closed_at = Some(now);
        session.close_reason = Some(reason.into());
        Some(session.clone())
    }
}

fn allocate_id(counter: &mut u64, prefix: &str) -> String {
    *counter += 1;
    format!("{prefix}-{:06}", *counter)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreSettings {
    pub schema_version: u32,
    pub core: CoreSection,
    pub storage: StorageSection,
    pub vault: VaultSection,
    pub control_plane: ControlPlaneSection,
    pub model_plane: ModelPlaneSection,
    pub toolchains: HashMap<String, ToolchainSection>,
    pub policies: PolicyDefaultsSection,
    pub targets: Vec<StandaloneTargetProfile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreSection {
    pub instance_name: String,
    pub data_dir: String,
    pub log_level: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageSection {
    pub metadata_backend: String,
    pub metadata_path: String,
    pub artifacts: ArtifactStorageSection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactStorageSection {
    pub backend: String,
    pub root: String,
    pub max_bytes: u64,
    pub eviction_policy: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultSection {
    pub backend: String,
    pub namespace: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlPlaneSection {
    pub enabled: bool,
    pub transport: String,
    pub endpoint: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelPlaneSection {
    pub http: ModelPlaneHttpSection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelPlaneHttpSection {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub allow_non_loopback: bool,
    pub auth: ModelPlaneHttpAuthSection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelPlaneHttpAuthSection {
    pub mode: String,
    pub required_when_non_loopback: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolchainSection {
    pub path_override: String,
    pub prefer_builtin_fallback: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyDefaultsSection {
    pub reuse_policy: SessionReusePolicy,
    pub approval_mode: String,
    pub capture_env_fingerprint: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StandaloneTargetProfile {
    pub id: String,
    pub display_name: String,
    pub kind: TargetKind,
    pub enabled: bool,
    pub aliases: Vec<String>,
    pub credential_ref: Option<String>,
    pub notes: Option<String>,
    pub connection: StandaloneConnectionSection,
    pub terminal_provider: TerminalProviderSection,
    pub git_repositories: Vec<GitRepositorySection>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct StandaloneConnectionSection {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub known_hosts_policy: Option<String>,
    pub selector_kind: Option<String>,
    pub selector_value: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalProviderSection {
    pub enabled: bool,
    pub shell: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitRepositorySection {
    pub id: String,
    pub path: String,
    pub remote_name: Option<String>,
    pub web_url: Option<String>,
    pub review: Option<GitReviewSection>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitReviewSection {
    pub kind: String,
    pub base_url: String,
    pub project: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeMetadata {
    pub started_at: SystemTime,
    pub config_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreSettingsStore {
    pub settings: CoreSettings,
    pub runtime_metadata: RuntimeMetadata,
}

impl CoreSettingsStore {
    pub fn from_settings(settings: CoreSettings, config_path: Option<String>) -> Self {
        Self {
            settings,
            runtime_metadata: RuntimeMetadata {
                started_at: SystemTime::now(),
                config_path,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigFieldDescription {
    pub path: &'static str,
    pub description: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigError {
    MissingSection(&'static str),
    MissingField(&'static str),
    InvalidValue { field: String, reason: String },
    UnsupportedSchemaVersion(u32),
    SensitiveFieldInConfig(String),
    NonLoopbackExplicitEnableRequired(String),
    NonLoopbackAuthRequired(String),
    Io(String),
}

impl CoreSettings {
    pub const SUPPORTED_SCHEMA_VERSION: u32 = 1;

    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let text =
            fs::read_to_string(path.as_ref()).map_err(|err| ConfigError::Io(err.to_string()))?;
        Self::from_toml_str(&text)
    }

    pub fn from_toml_str(input: &str) -> Result<Self, ConfigError> {
        #[derive(Clone, Debug, PartialEq, Eq)]
        enum Section {
            Root,
            Core,
            Storage,
            StorageArtifacts,
            Vault,
            ControlPlane,
            ModelPlaneHttp,
            ModelPlaneHttpAuth,
            Toolchain(String),
            PoliciesDefaults,
            Target,
            TargetConnection,
            TargetTerminal,
            TargetGitRepo,
            TargetGitReview,
        }

        let mut section = Section::Root;
        let mut schema_version = None::<u32>;
        let mut core = CoreSection {
            instance_name: String::new(),
            data_dir: String::new(),
            log_level: "info".into(),
        };
        let mut storage = StorageSection {
            metadata_backend: "sqlite".into(),
            metadata_path: String::new(),
            artifacts: ArtifactStorageSection {
                backend: "memory".into(),
                root: String::new(),
                max_bytes: 256 * 1024 * 1024,
                eviction_policy: "lru".into(),
            },
        };
        let mut vault = VaultSection {
            backend: String::new(),
            namespace: String::new(),
        };
        let mut control_plane = ControlPlaneSection {
            enabled: true,
            transport: "platform-ipc".into(),
            endpoint: "auto".into(),
        };
        let mut model_plane = ModelPlaneSection {
            http: ModelPlaneHttpSection {
                enabled: true,
                host: "127.0.0.1".into(),
                port: 19718,
                allow_non_loopback: false,
                auth: ModelPlaneHttpAuthSection {
                    mode: "none".into(),
                    required_when_non_loopback: true,
                },
            },
        };
        let mut toolchains = HashMap::<String, ToolchainSection>::new();
        let mut policies = PolicyDefaultsSection {
            reuse_policy: SessionReusePolicy::ResumeOrCreate,
            approval_mode: "on-risk".into(),
            capture_env_fingerprint: true,
        };
        let mut targets = Vec::<StandaloneTargetProfile>::new();

        for raw_line in input.lines() {
            let line = strip_comment(raw_line).trim();
            if line.is_empty() {
                continue;
            }

            if line.starts_with("[[") && line.ends_with("]]") {
                let section_name = &line[2..line.len() - 2];
                match section_name {
                    "targets" => {
                        targets.push(StandaloneTargetProfile {
                            id: String::new(),
                            display_name: String::new(),
                            kind: TargetKind::Other("unknown".into()),
                            enabled: true,
                            aliases: Vec::new(),
                            credential_ref: None,
                            notes: None,
                            connection: StandaloneConnectionSection::default(),
                            terminal_provider: TerminalProviderSection {
                                enabled: false,
                                shell: None,
                            },
                            git_repositories: Vec::new(),
                        });
                        section = Section::Target;
                    }
                    "targets.providers.git.repositories" => {
                        let target = targets
                            .last_mut()
                            .ok_or(ConfigError::MissingSection("targets"))?;
                        target.git_repositories.push(GitRepositorySection {
                            id: String::new(),
                            path: String::new(),
                            remote_name: None,
                            web_url: None,
                            review: None,
                        });
                        section = Section::TargetGitRepo;
                    }
                    _ => {
                        return Err(ConfigError::InvalidValue {
                            field: "section".into(),
                            reason: format!("unsupported array section: {section_name}"),
                        });
                    }
                }
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') {
                let section_name = &line[1..line.len() - 1];
                section = match section_name {
                    "core" => Section::Core,
                    "storage" => Section::Storage,
                    "storage.artifacts" => Section::StorageArtifacts,
                    "vault" => Section::Vault,
                    "control_plane" => Section::ControlPlane,
                    "model_plane.http" => Section::ModelPlaneHttp,
                    "model_plane.http.auth" => Section::ModelPlaneHttpAuth,
                    "policies.defaults" => Section::PoliciesDefaults,
                    "targets.connection" => Section::TargetConnection,
                    "targets.providers.terminal" => Section::TargetTerminal,
                    "targets.providers.git.repositories.review" => Section::TargetGitReview,
                    _ if section_name.starts_with("toolchains.") => {
                        let name = section_name.trim_start_matches("toolchains.").to_string();
                        Section::Toolchain(name)
                    }
                    _ => {
                        return Err(ConfigError::InvalidValue {
                            field: "section".into(),
                            reason: format!("unsupported section: {section_name}"),
                        });
                    }
                };
                continue;
            }

            let (key, value) = split_key_value(line).ok_or_else(|| ConfigError::InvalidValue {
                field: "line".into(),
                reason: format!("expected key=value, got: {line}"),
            })?;
            if is_sensitive_key(key) {
                return Err(ConfigError::SensitiveFieldInConfig(key.to_string()));
            }

            match &section {
                Section::Root => {
                    if key == "schema_version" {
                        schema_version = Some(parse_u32(key, value)?);
                    } else {
                        return Err(ConfigError::InvalidValue {
                            field: key.into(),
                            reason: "unsupported root field".into(),
                        });
                    }
                }
                Section::Core => match key {
                    "instance_name" => core.instance_name = parse_string(key, value)?,
                    "data_dir" => core.data_dir = parse_string(key, value)?,
                    "log_level" => core.log_level = parse_string(key, value)?,
                    _ => return Err(invalid_field(key, "core")),
                },
                Section::Storage => match key {
                    "metadata_backend" => storage.metadata_backend = parse_string(key, value)?,
                    "metadata_path" => storage.metadata_path = parse_string(key, value)?,
                    _ => return Err(invalid_field(key, "storage")),
                },
                Section::StorageArtifacts => match key {
                    "backend" => storage.artifacts.backend = parse_string(key, value)?,
                    "root" => storage.artifacts.root = parse_string(key, value)?,
                    "max_bytes" => storage.artifacts.max_bytes = parse_u64(key, value)?,
                    "eviction_policy" => {
                        storage.artifacts.eviction_policy = parse_string(key, value)?
                    }
                    _ => return Err(invalid_field(key, "storage.artifacts")),
                },
                Section::Vault => match key {
                    "backend" => vault.backend = parse_string(key, value)?,
                    "namespace" => vault.namespace = parse_string(key, value)?,
                    _ => return Err(invalid_field(key, "vault")),
                },
                Section::ControlPlane => match key {
                    "enabled" => control_plane.enabled = parse_bool(key, value)?,
                    "transport" => control_plane.transport = parse_string(key, value)?,
                    "endpoint" => control_plane.endpoint = parse_string(key, value)?,
                    _ => return Err(invalid_field(key, "control_plane")),
                },
                Section::ModelPlaneHttp => match key {
                    "enabled" => model_plane.http.enabled = parse_bool(key, value)?,
                    "host" => model_plane.http.host = parse_string(key, value)?,
                    "port" => model_plane.http.port = parse_u16(key, value)?,
                    "allow_non_loopback" => {
                        model_plane.http.allow_non_loopback = parse_bool(key, value)?
                    }
                    _ => return Err(invalid_field(key, "model_plane.http")),
                },
                Section::ModelPlaneHttpAuth => match key {
                    "mode" => model_plane.http.auth.mode = parse_string(key, value)?,
                    "required_when_non_loopback" => {
                        model_plane.http.auth.required_when_non_loopback = parse_bool(key, value)?
                    }
                    _ => return Err(invalid_field(key, "model_plane.http.auth")),
                },
                Section::Toolchain(name) => {
                    let entry = toolchains.entry(name.clone()).or_insert(ToolchainSection {
                        path_override: String::new(),
                        prefer_builtin_fallback: false,
                    });
                    match key {
                        "path_override" => entry.path_override = parse_string(key, value)?,
                        "prefer_builtin_fallback" => {
                            entry.prefer_builtin_fallback = parse_bool(key, value)?
                        }
                        _ => return Err(invalid_field(key, "toolchains.<name>")),
                    }
                }
                Section::PoliciesDefaults => match key {
                    "reuse_policy" => {
                        policies.reuse_policy = parse_reuse_policy(value)?;
                    }
                    "approval_mode" => policies.approval_mode = parse_string(key, value)?,
                    "capture_env_fingerprint" => {
                        policies.capture_env_fingerprint = parse_bool(key, value)?
                    }
                    _ => return Err(invalid_field(key, "policies.defaults")),
                },
                Section::Target => {
                    let target = targets
                        .last_mut()
                        .ok_or(ConfigError::MissingSection("targets"))?;
                    match key {
                        "id" => target.id = parse_string(key, value)?,
                        "display_name" => target.display_name = parse_string(key, value)?,
                        "kind" => {
                            target.kind = parse_target_kind(value)?;
                        }
                        "enabled" => target.enabled = parse_bool(key, value)?,
                        "aliases" => target.aliases = parse_string_array(key, value)?,
                        "credential_ref" => {
                            target.credential_ref = Some(parse_string(key, value)?);
                        }
                        "notes" => target.notes = Some(parse_string(key, value)?),
                        _ => return Err(invalid_field(key, "targets")),
                    }
                }
                Section::TargetConnection => {
                    let target = targets
                        .last_mut()
                        .ok_or(ConfigError::MissingSection("targets"))?;
                    match key {
                        "host" => target.connection.host = Some(parse_string(key, value)?),
                        "port" => target.connection.port = Some(parse_u16(key, value)?),
                        "username" => target.connection.username = Some(parse_string(key, value)?),
                        "known_hosts_policy" => {
                            target.connection.known_hosts_policy = Some(parse_string(key, value)?)
                        }
                        "selector_kind" => {
                            target.connection.selector_kind = Some(parse_string(key, value)?)
                        }
                        "selector_value" => {
                            target.connection.selector_value = Some(parse_string(key, value)?)
                        }
                        _ => return Err(invalid_field(key, "targets.connection")),
                    }
                }
                Section::TargetTerminal => {
                    let target = targets
                        .last_mut()
                        .ok_or(ConfigError::MissingSection("targets"))?;
                    match key {
                        "enabled" => {
                            target.terminal_provider.enabled = parse_bool(key, value)?;
                        }
                        "shell" => target.terminal_provider.shell = Some(parse_string(key, value)?),
                        _ => return Err(invalid_field(key, "targets.providers.terminal")),
                    }
                }
                Section::TargetGitRepo => {
                    let target = targets
                        .last_mut()
                        .ok_or(ConfigError::MissingSection("targets"))?;
                    let repo =
                        target
                            .git_repositories
                            .last_mut()
                            .ok_or(ConfigError::MissingSection(
                                "targets.providers.git.repositories",
                            ))?;
                    match key {
                        "id" => repo.id = parse_string(key, value)?,
                        "path" => repo.path = parse_string(key, value)?,
                        "remote_name" => repo.remote_name = Some(parse_string(key, value)?),
                        "web_url" => repo.web_url = Some(parse_string(key, value)?),
                        _ => return Err(invalid_field(key, "targets.providers.git.repositories")),
                    }
                }
                Section::TargetGitReview => {
                    let target = targets
                        .last_mut()
                        .ok_or(ConfigError::MissingSection("targets"))?;
                    let repo =
                        target
                            .git_repositories
                            .last_mut()
                            .ok_or(ConfigError::MissingSection(
                                "targets.providers.git.repositories",
                            ))?;
                    let review = repo.review.get_or_insert(GitReviewSection {
                        kind: String::new(),
                        base_url: String::new(),
                        project: String::new(),
                    });
                    match key {
                        "kind" => review.kind = parse_string(key, value)?,
                        "base_url" => review.base_url = parse_string(key, value)?,
                        "project" => review.project = parse_string(key, value)?,
                        _ => {
                            return Err(invalid_field(
                                key,
                                "targets.providers.git.repositories.review",
                            ))
                        }
                    }
                }
            }
        }

        let config = Self {
            schema_version: schema_version.ok_or(ConfigError::MissingField("schema_version"))?,
            core,
            storage,
            vault,
            control_plane,
            model_plane,
            toolchains,
            policies,
            targets,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.schema_version != Self::SUPPORTED_SCHEMA_VERSION {
            return Err(ConfigError::UnsupportedSchemaVersion(self.schema_version));
        }
        if self.core.instance_name.trim().is_empty() {
            return Err(ConfigError::MissingField("core.instance_name"));
        }
        if self.core.data_dir.trim().is_empty() {
            return Err(ConfigError::MissingField("core.data_dir"));
        }
        if self.storage.metadata_path.trim().is_empty() {
            return Err(ConfigError::MissingField("storage.metadata_path"));
        }
        if self.storage.artifacts.root.trim().is_empty() {
            return Err(ConfigError::MissingField("storage.artifacts.root"));
        }
        if self.storage.artifacts.max_bytes == 0 {
            return Err(ConfigError::InvalidValue {
                field: "storage.artifacts.max_bytes".into(),
                reason: "expected positive u64".into(),
            });
        }
        match self.storage.artifacts.backend.as_str() {
            "memory" | "filesystem" => {}
            other => {
                return Err(ConfigError::InvalidValue {
                    field: "storage.artifacts.backend".into(),
                    reason: format!("unsupported artifact backend: {other}"),
                })
            }
        }
        match self.storage.artifacts.eviction_policy.as_str() {
            "lru" => {}
            other => {
                return Err(ConfigError::InvalidValue {
                    field: "storage.artifacts.eviction_policy".into(),
                    reason: format!("unsupported artifact eviction policy: {other}"),
                })
            }
        }
        if self.vault.backend.trim().is_empty() {
            return Err(ConfigError::MissingField("vault.backend"));
        }

        let host = self.model_plane.http.host.trim();
        let parsed_host = host
            .parse::<IpAddr>()
            .map_err(|_| ConfigError::InvalidValue {
                field: "model_plane.http.host".into(),
                reason: format!("invalid IP address: {host}"),
            })?;
        let is_loopback = parsed_host.is_loopback();
        if !is_loopback && !self.model_plane.http.allow_non_loopback {
            return Err(ConfigError::NonLoopbackExplicitEnableRequired(
                host.to_string(),
            ));
        }
        if !is_loopback
            && self.model_plane.http.auth.required_when_non_loopback
            && self.model_plane.http.auth.mode == "none"
        {
            return Err(ConfigError::NonLoopbackAuthRequired(host.to_string()));
        }

        for target in &self.targets {
            if target.id.trim().is_empty() {
                return Err(ConfigError::MissingField("targets[].id"));
            }
            if target.display_name.trim().is_empty() {
                return Err(ConfigError::MissingField("targets[].display_name"));
            }
        }

        Ok(())
    }

    pub fn field_descriptions() -> Vec<ConfigFieldDescription> {
        vec![
            ConfigFieldDescription {
                path: "schema_version",
                description: "配置 schema 版本，当前必须为 1。",
            },
            ConfigFieldDescription {
                path: "core.instance_name",
                description: "core 实例名，用于日志和诊断标识。",
            },
            ConfigFieldDescription {
                path: "core.data_dir",
                description: "core 本地数据目录。",
            },
            ConfigFieldDescription {
                path: "storage.metadata_path",
                description: "metadata 存储文件路径（例如 sqlite 文件）。",
            },
            ConfigFieldDescription {
                path: "storage.artifacts.backend",
                description: "artifact cache backend，MVP 支持 memory 和 filesystem。",
            },
            ConfigFieldDescription {
                path: "storage.artifacts.root",
                description: "artifact 持久化根目录。",
            },
            ConfigFieldDescription {
                path: "storage.artifacts.max_bytes",
                description: "artifact cache 的最大容量限制，单位字节。",
            },
            ConfigFieldDescription {
                path: "storage.artifacts.eviction_policy",
                description: "artifact cache 的淘汰策略，MVP 为 lru。",
            },
            ConfigFieldDescription {
                path: "vault.backend",
                description: "保险库后端类型，使用可替换语义。",
            },
            ConfigFieldDescription {
                path: "control_plane.transport",
                description: "受信任 control plane 传输，MVP 默认为 platform-ipc。",
            },
            ConfigFieldDescription {
                path: "model_plane.http.host",
                description: "MCP HTTP 监听地址，默认 127.0.0.1。",
            },
            ConfigFieldDescription {
                path: "model_plane.http.port",
                description: "MCP HTTP 监听端口，默认 19718。",
            },
            ConfigFieldDescription {
                path: "model_plane.http.allow_non_loopback",
                description: "非 loopback 暴露开关，必须显式启用。",
            },
            ConfigFieldDescription {
                path: "model_plane.http.auth.mode",
                description: "模型平面的认证模式。",
            },
            ConfigFieldDescription {
                path: "toolchains.<name>.path_override",
                description: "工具用户覆盖路径。",
            },
            ConfigFieldDescription {
                path: "policies.defaults.reuse_policy",
                description: "逻辑会话复用策略。",
            },
            ConfigFieldDescription {
                path: "targets[].credential_ref",
                description: "凭据引用，只允许引用，不允许明文敏感字段。",
            },
        ]
    }

    pub fn minimal_example() -> &'static str {
        include_str!("../tests/fixtures/standalone-minimal.toml")
    }

    pub fn complete_example() -> &'static str {
        include_str!("../tests/fixtures/standalone-complete.toml")
    }
}

fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    for (idx, ch) in line.char_indices() {
        if ch == '"' {
            in_string = !in_string;
        } else if ch == '#' && !in_string {
            return &line[..idx];
        }
    }
    line
}

fn split_key_value(line: &str) -> Option<(&str, &str)> {
    let (left, right) = line.split_once('=')?;
    Some((left.trim(), right.trim()))
}

fn parse_string(field: &str, value: &str) -> Result<String, ConfigError> {
    if !(value.starts_with('"') && value.ends_with('"') && value.len() >= 2) {
        return Err(ConfigError::InvalidValue {
            field: field.into(),
            reason: "expected string".into(),
        });
    }
    Ok(value[1..value.len() - 1].to_string())
}

fn parse_bool(field: &str, value: &str) -> Result<bool, ConfigError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(ConfigError::InvalidValue {
            field: field.into(),
            reason: "expected boolean".into(),
        }),
    }
}

fn parse_u16(field: &str, value: &str) -> Result<u16, ConfigError> {
    value.parse::<u16>().map_err(|_| ConfigError::InvalidValue {
        field: field.into(),
        reason: "expected u16".into(),
    })
}

fn parse_u32(field: &str, value: &str) -> Result<u32, ConfigError> {
    value.parse::<u32>().map_err(|_| ConfigError::InvalidValue {
        field: field.into(),
        reason: "expected u32".into(),
    })
}

fn parse_u64(field: &str, value: &str) -> Result<u64, ConfigError> {
    value.parse::<u64>().map_err(|_| ConfigError::InvalidValue {
        field: field.into(),
        reason: "expected u64".into(),
    })
}

fn parse_string_array(field: &str, value: &str) -> Result<Vec<String>, ConfigError> {
    if !(value.starts_with('[') && value.ends_with(']')) {
        return Err(ConfigError::InvalidValue {
            field: field.into(),
            reason: "expected string array".into(),
        });
    }
    let inner = value[1..value.len() - 1].trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for item in inner.split(',') {
        result.push(parse_string(field, item.trim())?);
    }
    Ok(result)
}

fn parse_target_kind(value: &str) -> Result<TargetKind, ConfigError> {
    let raw = parse_string("targets[].kind", value)?;
    Ok(match raw.as_str() {
        "ssh" => TargetKind::Ssh,
        "adb" => TargetKind::Adb,
        "serial" => TargetKind::Serial,
        "docker" => TargetKind::Docker,
        other => TargetKind::Other(other.to_string()),
    })
}

fn parse_reuse_policy(value: &str) -> Result<SessionReusePolicy, ConfigError> {
    let raw = parse_string("policies.defaults.reuse_policy", value)?;
    match raw.as_str() {
        "always_new" => Ok(SessionReusePolicy::AlwaysNew),
        "reuse_if_alive" => Ok(SessionReusePolicy::ReuseIfAlive),
        "resume_or_create" => Ok(SessionReusePolicy::ResumeOrCreate),
        _ => Err(ConfigError::InvalidValue {
            field: "policies.defaults.reuse_policy".into(),
            reason: format!("unknown reuse policy: {raw}"),
        }),
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let lowered = key.to_ascii_lowercase();
    lowered.contains("password")
        || lowered.contains("private_key")
        || lowered.contains("token")
        || lowered.contains("secret")
}

fn invalid_field(field: &str, section: &str) -> ConfigError {
    ConfigError::InvalidValue {
        field: field.into(),
        reason: format!("unsupported field in section {section}"),
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use bridgingio_domain::{
        AccessScope, ArtifactKind, ArtifactRecord, ChannelKind, ConnectionConfig, PolicyProfile,
        SessionRecord, SessionReusePolicy, TargetKind,
    };

    use super::InMemoryMetadataStore;

    #[test]
    fn stores_profile_session_and_artifact() {
        let mut store = InMemoryMetadataStore::default();
        store.upsert_profile(bridgingio_domain::TargetProfile {
            id: "t1".into(),
            name: "local".into(),
            kind: TargetKind::Adb,
            connection: ConnectionConfig::Adb {
                serial: Some("ABC".into()),
                transport: None,
            },
            credential_ref: None,
            default_policy: PolicyProfile::default(),
            notes: None,
            metadata: Default::default(),
        });

        let now = SystemTime::now();
        store.upsert_session(SessionRecord::new("s1", "t1", now));
        store.upsert_artifact(ArtifactRecord {
            id: "a1".into(),
            content_digest: "digest-a1".into(),
            logical_session_id: "ls-1".into(),
            transport_session_id: Some("ts-1".into()),
            channel_id: Some("ch-1".into()),
            session_id: "s1".into(),
            parent_id: None,
            kind: ArtifactKind::RawCommandOutput,
            source_command: Some("dmesg -w".into()),
            filter: None,
            created_at: now,
            last_accessed_at: now,
            byte_count: 128,
            line_count: 2,
            summary: "boot logs".into(),
        });

        assert_eq!(store.profiles.len(), 1);
        assert_eq!(store.sessions.len(), 1);
        assert_eq!(store.artifacts.len(), 1);
    }

    fn scope(agent_id: &str, client_session_id: &str) -> AccessScope {
        AccessScope {
            scope_id: format!("scope-{agent_id}"),
            workspace_id: "ws".into(),
            principal_id: "user".into(),
            agent_id: agent_id.into(),
            run_id: "run-1".into(),
            thread_id: Some("thread-1".into()),
            client_session_id: client_session_id.into(),
            origin: "mcp".into(),
            created_at: SystemTime::now(),
        }
    }

    #[test]
    fn isolates_logical_session_by_agent_scope() {
        let mut store = InMemoryMetadataStore::default();
        let now = SystemTime::now();

        let s1 = store.resolve_logical_session(
            &scope("agent-a", "client-1"),
            "target-1",
            SessionReusePolicy::ReuseIfAlive,
            now,
        );
        let s2 = store.resolve_logical_session(
            &scope("agent-b", "client-1"),
            "target-1",
            SessionReusePolicy::ReuseIfAlive,
            now,
        );

        assert_ne!(s1.logical_session_id, s2.logical_session_id);
    }

    #[test]
    fn resume_or_create_reopens_closed_logical_session() {
        let mut store = InMemoryMetadataStore::default();
        let now = SystemTime::now();
        let scope = scope("agent-a", "client-1");

        let created =
            store.resolve_logical_session(&scope, "target-1", SessionReusePolicy::AlwaysNew, now);
        {
            let record = store
                .logical_sessions
                .get_mut(&created.logical_session_id)
                .expect("logical session");
            record.status = bridgingio_domain::LogicalSessionStatus::Closed;
            record.closed_at = Some(now);
            record.close_reason = Some("network drop".into());
        }

        let resumed = store.resolve_logical_session(
            &scope,
            "target-1",
            SessionReusePolicy::ResumeOrCreate,
            now,
        );
        assert_eq!(created.logical_session_id, resumed.logical_session_id);
        assert!(resumed.status.is_alive());
        assert!(resumed.closed_at.is_none());
    }

    #[test]
    fn supports_multiple_channels_in_same_logical_session() {
        let mut store = InMemoryMetadataStore::default();
        let now = SystemTime::now();
        let logical = store.resolve_logical_session(
            &scope("agent-a", "client-7"),
            "target-1",
            SessionReusePolicy::ReuseIfAlive,
            now,
        );
        let transport = store.open_transport_session(
            &logical.logical_session_id,
            "target-1",
            TargetKind::Ssh,
            Some("/usr/bin/ssh".into()),
            Some("system_path".into()),
            now,
        );

        let command_channel = store.open_channel(
            &logical.logical_session_id,
            &transport.transport_session_id,
            "target-1",
            ChannelKind::OneShotExec,
            Some("command".into()),
            now,
        );
        let log_channel = store.open_channel(
            &logical.logical_session_id,
            &transport.transport_session_id,
            "target-1",
            ChannelKind::LogStream,
            Some("logs".into()),
            now,
        );

        assert_ne!(command_channel.channel_id, log_channel.channel_id);
        let channels = store.channels_for_logical_session(&logical.logical_session_id);
        assert_eq!(channels.len(), 2);
    }
}

#[cfg(test)]
mod config_tests {
    use super::{ConfigError, CoreSettings};

    #[test]
    fn parses_minimal_standalone_example() {
        let config =
            CoreSettings::from_toml_str(CoreSettings::minimal_example()).expect("parse minimal");
        assert_eq!(config.schema_version, 1);
        assert_eq!(config.model_plane.http.host, "127.0.0.1");
        assert_eq!(config.model_plane.http.port, 19718);
        assert_eq!(config.storage.artifacts.backend, "memory");
        assert_eq!(config.targets.len(), 1);
    }

    #[test]
    fn parses_complete_standalone_example() {
        let config =
            CoreSettings::from_toml_str(CoreSettings::complete_example()).expect("parse complete");
        assert_eq!(config.targets.len(), 2);
        assert!(config.toolchains.contains_key("adb"));
        assert_eq!(config.model_plane.http.host, "127.0.0.1");
        assert_eq!(config.storage.artifacts.backend, "filesystem");
    }

    #[test]
    fn rejects_unsupported_schema_version() {
        let invalid =
            CoreSettings::minimal_example().replace("schema_version = 1", "schema_version = 99");
        let err = CoreSettings::from_toml_str(&invalid).expect_err("must fail");
        assert!(matches!(err, ConfigError::UnsupportedSchemaVersion(99)));
    }

    #[test]
    fn rejects_sensitive_plaintext_fields() {
        let invalid = format!(
            "{}\n[core]\ninstance_name = \"x\"\ndata_dir = \"/tmp\"\nlog_level = \"info\"\nprivate_key = \"abc\"",
            "schema_version = 1"
        );
        let err = CoreSettings::from_toml_str(&invalid).expect_err("must reject secret field");
        assert!(matches!(err, ConfigError::SensitiveFieldInConfig(_)));
    }

    #[test]
    fn requires_non_loopback_explicit_enable_and_auth() {
        let mut invalid =
            CoreSettings::minimal_example().replace("host = \"127.0.0.1\"", "host = \"0.0.0.0\"");
        let err = CoreSettings::from_toml_str(&invalid).expect_err("must require explicit enable");
        assert!(matches!(
            err,
            ConfigError::NonLoopbackExplicitEnableRequired(_)
        ));

        invalid = invalid.replace("allow_non_loopback = false", "allow_non_loopback = true");
        let err = CoreSettings::from_toml_str(&invalid).expect_err("must require auth");
        assert!(matches!(err, ConfigError::NonLoopbackAuthRequired(_)));
    }
}
