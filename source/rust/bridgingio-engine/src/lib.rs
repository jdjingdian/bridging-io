use std::collections::HashMap;
use std::fs;
use std::net::IpAddr;
use std::path::Path;
use std::time::SystemTime;

use bridgingio_domain::{
    build_logical_session_key, AccessScope, ApprovalRequestRecord, ArtifactRecord, AuditEvent,
    ChannelKind, ChannelRecord, ChannelStatus, CommonErrorCode, ContractStatus,
    EnvironmentFingerprint, ErrorDomain, LogicalSessionRecord, LogicalSessionStatus,
    SessionRecord, SessionReusePolicy, SharedError, TargetKind, TargetProfile,
    TerminalConcurrencyPolicy, TerminalTargetFamily, TransportSessionRecord, TransportSessionStatus,
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
    pub exclusive_target_leases: HashMap<String, String>,
    next_logical_session_seq: u64,
    next_transport_session_seq: u64,
    next_channel_seq: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MetadataStoreError {
    TargetBusy {
        target_id: String,
        holder_transport_session_id: String,
    },
}

impl MetadataStoreError {
    pub fn message(&self) -> String {
        match self {
            Self::TargetBusy {
                target_id,
                holder_transport_session_id,
            } => format!(
                "target `{target_id}` is busy; held by transport session `{holder_transport_session_id}`"
            ),
        }
    }
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
    ) -> Result<TransportSessionRecord, MetadataStoreError> {
        let exclusive_mode = self
            .profiles
            .get(target_id)
            .and_then(bridgingio_domain::terminal_concurrency_policy_for)
            == Some(TerminalConcurrencyPolicy::Exclusive);
        if exclusive_mode {
            self.exclusive_target_leases.retain(|_, holder_id| {
                self.transport_sessions
                    .get(holder_id)
                    .map(|record| transport_status_is_active(&record.status))
                    .unwrap_or(false)
            });
            if let Some(holder) = self.exclusive_target_leases.get(target_id) {
                return Err(MetadataStoreError::TargetBusy {
                    target_id: target_id.to_string(),
                    holder_transport_session_id: holder.clone(),
                });
            }
            if let Some(existing) = self
                .transport_sessions
                .values()
                .find(|record| {
                    record.target_id == target_id && transport_status_is_active(&record.status)
                })
                .map(|record| record.transport_session_id.clone())
            {
                self.exclusive_target_leases
                    .insert(target_id.to_string(), existing.clone());
                return Err(MetadataStoreError::TargetBusy {
                    target_id: target_id.to_string(),
                    holder_transport_session_id: existing,
                });
            }
        }

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
        if exclusive_mode {
            self.exclusive_target_leases
                .insert(target_id.to_string(), record.transport_session_id.clone());
        }
        Ok(record)
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

    pub fn close_transport_session(
        &mut self,
        transport_session_id: &str,
        reason: impl Into<String>,
        now: SystemTime,
    ) -> Option<TransportSessionRecord> {
        let record = self.transport_sessions.get_mut(transport_session_id)?;
        record.status = TransportSessionStatus::Closed;
        record.last_activity_at = now;
        record.close_reason = Some(reason.into());
        if self
            .exclusive_target_leases
            .get(&record.target_id)
            .map(|holder| holder == transport_session_id)
            .unwrap_or(false)
        {
            self.exclusive_target_leases.remove(&record.target_id);
        }
        Some(record.clone())
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

fn transport_status_is_active(status: &TransportSessionStatus) -> bool {
    matches!(
        status,
        TransportSessionStatus::Connecting
            | TransportSessionStatus::Connected
            | TransportSessionStatus::Degraded
    )
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
    pub unlock: VaultUnlockSection,
    pub protectors: VaultProtectorsSection,
    pub ssh: VaultSshSection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultUnlockSection {
    pub trigger_policy: String,
    pub allowed_methods: Vec<String>,
    pub preferred_method: String,
    pub cache_ttl_sec: u64,
    pub require_fresh_user_verification: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultProtectorsSection {
    pub primary: VaultProtectorConfig,
    pub recovery: Vec<VaultProtectorConfig>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultProtectorConfig {
    pub kind: String,
    pub kdf: Option<String>,
    pub profile: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultSshSection {
    pub delivery_mode: String,
    pub fallback_delivery_mode: String,
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
    pub allow_loopback_anonymous_compat: bool,
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
    pub mcp_target_resolution_policy: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StandaloneTargetProfile {
    pub id: String,
    pub display_name: String,
    pub kind: TargetKind,
    pub enabled: bool,
    pub aliases: Vec<String>,
    pub storage_class: String,
    pub access_class: String,
    pub sealed_profile_ref: Option<String>,
    pub credential_ref: Option<String>,
    pub notes: Option<String>,
    pub connection: StandaloneConnectionSection,
    pub terminal: StandaloneTerminalSection,
    pub toolchains: HashMap<String, ToolchainSection>,
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

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct StandaloneTerminalSection {
    pub family: Option<String>,
    pub concurrency_policy: Option<String>,
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

fn default_vault_unlock_section() -> VaultUnlockSection {
    VaultUnlockSection {
        trigger_policy: "on-first-secret-access".into(),
        allowed_methods: vec!["os-native".into(), "passphrase".into()],
        preferred_method: "os-native".into(),
        cache_ttl_sec: 600,
        require_fresh_user_verification: true,
    }
}

fn default_vault_protectors_section() -> VaultProtectorsSection {
    VaultProtectorsSection {
        primary: VaultProtectorConfig {
            kind: "os-native".into(),
            kdf: None,
            profile: None,
        },
        recovery: vec![VaultProtectorConfig {
            kind: "passphrase".into(),
            kdf: Some("argon2id".into()),
            profile: Some("interactive-default".into()),
        }],
    }
}

fn default_vault_ssh_section() -> VaultSshSection {
    VaultSshSection {
        delivery_mode: "ssh-agent-broker".into(),
        fallback_delivery_mode: "ephemeral-identity-file".into(),
    }
}

fn default_vault_section() -> VaultSection {
    VaultSection {
        backend: "builtin-encrypted".into(),
        namespace: "io.bridgingio".into(),
        unlock: default_vault_unlock_section(),
        protectors: default_vault_protectors_section(),
        ssh: default_vault_ssh_section(),
    }
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

impl ConfigError {
    pub fn shared_error(&self) -> SharedError {
        let (common_code, module_code, message, recovery_hint) = match self {
            Self::MissingSection(section) => (
                CommonErrorCode::ValidationFailed,
                format!("config.missing_section.{section}"),
                format!("missing config section `{section}`"),
                "add the missing section to the config and retry".to_string(),
            ),
            Self::MissingField(field) => (
                CommonErrorCode::ValidationFailed,
                format!("config.missing_field.{field}"),
                format!("missing config field `{field}`"),
                "fill in the missing config field and retry".to_string(),
            ),
            Self::InvalidValue { field, reason } => (
                CommonErrorCode::ValidationFailed,
                format!("config.invalid_value.{field}"),
                format!("invalid config value for `{field}`: {reason}"),
                "fix the invalid config value and retry".to_string(),
            ),
            Self::UnsupportedSchemaVersion(version) => (
                CommonErrorCode::Unsupported,
                "config.unsupported_schema_version".to_string(),
                format!("unsupported config schema version `{version}`"),
                "migrate the config to a supported schema version".to_string(),
            ),
            Self::SensitiveFieldInConfig(field) => (
                CommonErrorCode::ValidationFailed,
                format!("config.sensitive_field.{field}"),
                format!("sensitive field `{field}` is not allowed in config"),
                "remove secret material from the config file and use a managed secret route"
                    .to_string(),
            ),
            Self::NonLoopbackExplicitEnableRequired(host) => (
                CommonErrorCode::NotReady,
                "config.non_loopback_explicit_enable_required".to_string(),
                format!("non-loopback model-plane host `{host}` requires explicit enable"),
                "set allow_non_loopback explicitly before exposing the model-plane"
                    .to_string(),
            ),
            Self::NonLoopbackAuthRequired(host) => (
                CommonErrorCode::NotReady,
                "config.non_loopback_auth_required".to_string(),
                format!("non-loopback model-plane host `{host}` requires authentication"),
                "configure a protected auth mode before exposing the model-plane".to_string(),
            ),
            Self::Io(_) => (
                CommonErrorCode::DependencyUnavailable,
                "config.io".to_string(),
                "config file could not be read".to_string(),
                "verify the config path is readable and try again".to_string(),
            ),
        };
        let status = match common_code {
            CommonErrorCode::Unsupported => ContractStatus::Unsupported,
            CommonErrorCode::NotReady => ContractStatus::NotReady,
            CommonErrorCode::DependencyUnavailable => ContractStatus::NotReady,
            _ => ContractStatus::Failed,
        };
        SharedError::new(status, ErrorDomain::Config, common_code, message)
            .with_module_code(module_code)
            .with_recovery_hint(recovery_hint)
    }
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
            VaultUnlock,
            VaultProtectorsPrimary,
            VaultProtectorsRecovery(usize),
            VaultSsh,
            ControlPlane,
            ModelPlaneHttp,
            ModelPlaneHttpAuth,
            Toolchain(String),
            PoliciesDefaults,
            Target,
            TargetConnection,
            TargetTerminalConfig,
            TargetToolchain(String),
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
        let mut vault = default_vault_section();
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
                    allow_loopback_anonymous_compat: false,
                },
            },
        };
        let mut toolchains = HashMap::<String, ToolchainSection>::new();
        let mut policies = PolicyDefaultsSection {
            reuse_policy: SessionReusePolicy::ResumeOrCreate,
            approval_mode: "on-risk".into(),
            capture_env_fingerprint: true,
            mcp_target_resolution_policy: "confirm_if_family".into(),
        };
        let mut targets = Vec::<StandaloneTargetProfile>::new();
        let mut seen_vault_recovery_array = false;

        for raw_line in input.lines() {
            let line = strip_comment(raw_line).trim();
            if line.is_empty() {
                continue;
            }

            if line.starts_with("[[") && line.ends_with("]]") {
                let section_name = &line[2..line.len() - 2];
                match section_name {
                    "vault.protectors.recovery" => {
                        if !seen_vault_recovery_array {
                            vault.protectors.recovery.clear();
                            seen_vault_recovery_array = true;
                        }
                        vault.protectors.recovery.push(VaultProtectorConfig {
                            kind: String::new(),
                            kdf: None,
                            profile: None,
                        });
                        section = Section::VaultProtectorsRecovery(
                            vault.protectors.recovery.len().saturating_sub(1),
                        );
                    }
                    "targets" => {
                        targets.push(StandaloneTargetProfile {
                            id: String::new(),
                            display_name: String::new(),
                            kind: TargetKind::Other("unknown".into()),
                            enabled: true,
                            aliases: Vec::new(),
                            storage_class: "plain".into(),
                            access_class: "anonymous-local".into(),
                            sealed_profile_ref: None,
                            credential_ref: None,
                            notes: None,
                            connection: StandaloneConnectionSection::default(),
                            terminal: StandaloneTerminalSection::default(),
                            toolchains: HashMap::new(),
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
                    "vault.unlock" => Section::VaultUnlock,
                    "vault.protectors.primary" => Section::VaultProtectorsPrimary,
                    "vault.ssh" => Section::VaultSsh,
                    "control_plane" => Section::ControlPlane,
                    "model_plane.http" => Section::ModelPlaneHttp,
                    "model_plane.http.auth" => Section::ModelPlaneHttpAuth,
                    "policies.defaults" => Section::PoliciesDefaults,
                    "targets.connection" => Section::TargetConnection,
                    "targets.terminal" => Section::TargetTerminalConfig,
                    _ if section_name.starts_with("targets.toolchains.") => {
                        let name = section_name
                            .trim_start_matches("targets.toolchains.")
                            .to_string();
                        Section::TargetToolchain(name)
                    }
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
                Section::VaultUnlock => match key {
                    "trigger_policy" => vault.unlock.trigger_policy = parse_string(key, value)?,
                    "allowed_methods" => vault.unlock.allowed_methods = parse_string_array(key, value)?,
                    "preferred_method" => vault.unlock.preferred_method = parse_string(key, value)?,
                    "cache_ttl_sec" => vault.unlock.cache_ttl_sec = parse_u64(key, value)?,
                    "require_fresh_user_verification" => {
                        vault.unlock.require_fresh_user_verification = parse_bool(key, value)?
                    }
                    _ => return Err(invalid_field(key, "vault.unlock")),
                },
                Section::VaultProtectorsPrimary => match key {
                    "kind" => vault.protectors.primary.kind = parse_string(key, value)?,
                    "kdf" => vault.protectors.primary.kdf = Some(parse_string(key, value)?),
                    "profile" => vault.protectors.primary.profile = Some(parse_string(key, value)?),
                    _ => return Err(invalid_field(key, "vault.protectors.primary")),
                },
                Section::VaultProtectorsRecovery(index) => {
                    let Some(recovery) = vault.protectors.recovery.get_mut(*index) else {
                        return Err(ConfigError::MissingSection("vault.protectors.recovery"));
                    };
                    match key {
                        "kind" => recovery.kind = parse_string(key, value)?,
                        "kdf" => recovery.kdf = Some(parse_string(key, value)?),
                        "profile" => recovery.profile = Some(parse_string(key, value)?),
                        _ => return Err(invalid_field(key, "vault.protectors.recovery")),
                    }
                }
                Section::VaultSsh => match key {
                    "delivery_mode" => vault.ssh.delivery_mode = parse_string(key, value)?,
                    "fallback_delivery_mode" => {
                        vault.ssh.fallback_delivery_mode = parse_string(key, value)?
                    }
                    _ => return Err(invalid_field(key, "vault.ssh")),
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
                    "allow_loopback_anonymous_compat" => {
                        model_plane.http.auth.allow_loopback_anonymous_compat =
                            parse_bool(key, value)?
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
                    "mcp_target_resolution_policy" => {
                        policies.mcp_target_resolution_policy =
                            parse_mcp_target_resolution_policy(value)?;
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
                        "storage_class" => {
                            target.storage_class = parse_target_storage_class(value)?;
                        }
                        "access_class" => {
                            target.access_class = parse_target_access_class(value)?;
                        }
                        "sealed_profile_ref" => {
                            target.sealed_profile_ref = Some(parse_string(key, value)?);
                        }
                        "credential_ref" => {
                            let raw_ref = parse_string(key, value)?;
                            target.credential_ref =
                                Some(canonicalize_credential_ref_input(&raw_ref)?);
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
                Section::TargetTerminalConfig => {
                    let target = targets
                        .last_mut()
                        .ok_or(ConfigError::MissingSection("targets"))?;
                    match key {
                        "family" => target.terminal.family = Some(parse_string(key, value)?),
                        "concurrency_policy" => {
                            target.terminal.concurrency_policy = Some(parse_string(key, value)?)
                        }
                        _ => return Err(invalid_field(key, "targets.terminal")),
                    }
                }
                Section::TargetToolchain(name) => {
                    let target = targets
                        .last_mut()
                        .ok_or(ConfigError::MissingSection("targets"))?;
                    let entry = target
                        .toolchains
                        .entry(name.clone())
                        .or_insert(ToolchainSection {
                            path_override: String::new(),
                            prefer_builtin_fallback: false,
                        });
                    match key {
                        "path_override" => entry.path_override = parse_string(key, value)?,
                        "prefer_builtin_fallback" => {
                            entry.prefer_builtin_fallback = parse_bool(key, value)?
                        }
                        _ => return Err(invalid_field(key, "targets.toolchains.<name>")),
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

        canonicalize_vault_section(&mut vault)?;

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
        if self.vault.namespace.trim().is_empty() {
            return Err(ConfigError::MissingField("vault.namespace"));
        }
        match self.vault.backend.as_str() {
            "builtin-encrypted" | "file-vault" | "in-memory" => {}
            other => {
                return Err(ConfigError::InvalidValue {
                    field: "vault.backend".into(),
                    reason: format!("unsupported vault backend: {other}"),
                })
            }
        }
        match self.vault.unlock.trigger_policy.as_str() {
            "on-core-start" | "on-first-secret-access" | "on-every-secret-access" | "manual-only" => {}
            other => {
                return Err(ConfigError::InvalidValue {
                    field: "vault.unlock.trigger_policy".into(),
                    reason: format!("unsupported vault unlock trigger policy: {other}"),
                })
            }
        }
        if self.vault.unlock.allowed_methods.is_empty() {
            return Err(ConfigError::MissingField("vault.unlock.allowed_methods"));
        }
        if !self
            .vault
            .unlock
            .allowed_methods
            .iter()
            .any(|method| method == &self.vault.unlock.preferred_method)
        {
            return Err(ConfigError::InvalidValue {
                field: "vault.unlock.preferred_method".into(),
                reason: "preferred method must exist in allowed_methods".into(),
            });
        }
        if self.vault.protectors.primary.kind.trim().is_empty() {
            return Err(ConfigError::MissingField("vault.protectors.primary.kind"));
        }
        if self.vault.ssh.delivery_mode.trim().is_empty() {
            return Err(ConfigError::MissingField("vault.ssh.delivery_mode"));
        }
        if self.vault.ssh.fallback_delivery_mode.trim().is_empty() {
            return Err(ConfigError::MissingField("vault.ssh.fallback_delivery_mode"));
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
            if !matches!(
                target.storage_class.trim(),
                "plain" | "sealed-overlay" | "sealed-full"
            ) {
                return Err(ConfigError::InvalidValue {
                    field: "targets[].storage_class".into(),
                    reason: format!(
                        "unsupported target storage class: {}",
                        target.storage_class.trim()
                    ),
                });
            }
            if !matches!(
                target.access_class.trim(),
                "anonymous-local" | "token-scoped"
            ) {
                return Err(ConfigError::InvalidValue {
                    field: "targets[].access_class".into(),
                    reason: format!(
                        "unsupported target access class: {}",
                        target.access_class.trim()
                    ),
                });
            }
            if target.storage_class == "plain" {
                if target
                    .sealed_profile_ref
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty())
                {
                    return Err(ConfigError::InvalidValue {
                        field: "targets[].sealed_profile_ref".into(),
                        reason: "sealed_profile_ref is only valid for sealed target storage_class"
                            .into(),
                    });
                }
            } else {
                let Some(sealed_profile_ref) = target.sealed_profile_ref.as_deref() else {
                    return Err(ConfigError::MissingField("targets[].sealed_profile_ref"));
                };
                if sealed_profile_ref.trim().is_empty() {
                    return Err(ConfigError::InvalidValue {
                        field: "targets[].sealed_profile_ref".into(),
                        reason: "sealed_profile_ref must be non-empty for sealed target"
                            .into(),
                    });
                }
                if target.access_class == "anonymous-local" {
                    return Err(ConfigError::InvalidValue {
                        field: "targets[].access_class".into(),
                        reason: "sealed target cannot use anonymous-local access_class".into(),
                    });
                }
            }
            if let Some(family) = target.terminal.family.as_deref() {
                if TerminalTargetFamily::parse(family).is_none() {
                    return Err(ConfigError::InvalidValue {
                        field: "targets.terminal.family".into(),
                        reason: format!("unsupported terminal family: {family}"),
                    });
                }
            }
            if let Some(policy) = target.terminal.concurrency_policy.as_deref() {
                if TerminalConcurrencyPolicy::parse(policy).is_none() {
                    return Err(ConfigError::InvalidValue {
                        field: "targets.terminal.concurrency_policy".into(),
                        reason: format!("unsupported terminal concurrency policy: {policy}"),
                    });
                }
            }
        }

        if !matches!(
            self.policies.mcp_target_resolution_policy.as_str(),
            "auto_execute" | "confirm_if_family" | "confirm_if_related" | "confirm_always"
        ) {
            return Err(ConfigError::InvalidValue {
                field: "policies.defaults.mcp_target_resolution_policy".into(),
                reason: format!(
                    "unsupported mcp target resolution policy: {}",
                    self.policies.mcp_target_resolution_policy
                ),
            });
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
                path: "vault.unlock.trigger_policy",
                description: "vault 解锁触发策略。",
            },
            ConfigFieldDescription {
                path: "vault.unlock.allowed_methods",
                description: "vault 允许的解锁方法集合。",
            },
            ConfigFieldDescription {
                path: "vault.protectors.primary.kind",
                description: "主 protector 类型。",
            },
            ConfigFieldDescription {
                path: "vault.ssh.delivery_mode",
                description: "secret-backed SSH 默认交付模式。",
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
                path: "model_plane.http.auth.allow_loopback_anonymous_compat",
                description:
                    "显式 loopback 匿名兼容开关，仅允许 plain + anonymous-local target 在无 token 时访问。",
            },
            ConfigFieldDescription {
                path: "toolchains.<name>.path_override",
                description: "工具用户覆盖路径。",
            },
            ConfigFieldDescription {
                path: "targets[].toolchains.<name>.path_override",
                description: "目标级工具覆盖路径，优先于全局 toolchains。",
            },
            ConfigFieldDescription {
                path: "policies.defaults.reuse_policy",
                description: "逻辑会话复用策略。",
            },
            ConfigFieldDescription {
                path: "policies.defaults.mcp_target_resolution_policy",
                description:
                    "MCP target 解析策略，支持 auto_execute/confirm_if_family/confirm_if_related/confirm_always。",
            },
            ConfigFieldDescription {
                path: "targets[].credential_ref",
                description: "凭据引用，只允许引用，不允许明文敏感字段。",
            },
            ConfigFieldDescription {
                path: "targets[].storage_class",
                description:
                    "target 存储分层：plain / sealed-overlay / sealed-full（legacy 默认 plain）。",
            },
            ConfigFieldDescription {
                path: "targets[].access_class",
                description:
                    "target 访问分层：anonymous-local / token-scoped（legacy 默认 anonymous-local）。",
            },
            ConfigFieldDescription {
                path: "targets[].sealed_profile_ref",
                description: "sealed target 对应的 vault profile 引用，仅 sealed-* 生效。",
            },
            ConfigFieldDescription {
                path: "targets.terminal.family",
                description: "target terminal family 元数据，首轮支持 terminal。",
            },
            ConfigFieldDescription {
                path: "targets.terminal.concurrency_policy",
                description: "target terminal 并发策略，支持 multiplexed 或 exclusive。",
            },
        ]
    }

    pub fn minimal_example() -> &'static str {
        include_str!("../tests/fixtures/standalone-minimal.toml")
    }

    pub fn complete_example() -> &'static str {
        include_str!("../tests/fixtures/standalone-complete.toml")
    }

    pub fn to_toml_string(&self) -> String {
        let mut lines = vec![
            format!("schema_version = {}", self.schema_version),
            String::new(),
            "[core]".to_string(),
            format!("instance_name = {}", toml_quote(&self.core.instance_name)),
            format!("data_dir = {}", toml_quote(&self.core.data_dir)),
            format!("log_level = {}", toml_quote(&self.core.log_level)),
            String::new(),
            "[storage]".to_string(),
            format!(
                "metadata_backend = {}",
                toml_quote(&self.storage.metadata_backend)
            ),
            format!(
                "metadata_path = {}",
                toml_quote(&self.storage.metadata_path)
            ),
            String::new(),
            "[storage.artifacts]".to_string(),
            format!("backend = {}", toml_quote(&self.storage.artifacts.backend)),
            format!("root = {}", toml_quote(&self.storage.artifacts.root)),
            format!("max_bytes = {}", self.storage.artifacts.max_bytes),
            format!(
                "eviction_policy = {}",
                toml_quote(&self.storage.artifacts.eviction_policy)
            ),
            String::new(),
            "[vault]".to_string(),
            format!("backend = {}", toml_quote(&self.vault.backend)),
            format!("namespace = {}", toml_quote(&self.vault.namespace)),
            String::new(),
            "[vault.unlock]".to_string(),
            format!(
                "trigger_policy = {}",
                toml_quote(&self.vault.unlock.trigger_policy)
            ),
            format!(
                "allowed_methods = {}",
                toml_string_array(&self.vault.unlock.allowed_methods)
            ),
            format!(
                "preferred_method = {}",
                toml_quote(&self.vault.unlock.preferred_method)
            ),
            format!("cache_ttl_sec = {}", self.vault.unlock.cache_ttl_sec),
            format!(
                "require_fresh_user_verification = {}",
                self.vault.unlock.require_fresh_user_verification
            ),
            String::new(),
            "[vault.protectors.primary]".to_string(),
            format!(
                "kind = {}",
                toml_quote(&self.vault.protectors.primary.kind)
            ),
            String::new(),
            "[vault.ssh]".to_string(),
            format!("delivery_mode = {}", toml_quote(&self.vault.ssh.delivery_mode)),
            format!(
                "fallback_delivery_mode = {}",
                toml_quote(&self.vault.ssh.fallback_delivery_mode)
            ),
            String::new(),
            "[control_plane]".to_string(),
            format!("enabled = {}", self.control_plane.enabled),
            format!("transport = {}", toml_quote(&self.control_plane.transport)),
            format!("endpoint = {}", toml_quote(&self.control_plane.endpoint)),
            String::new(),
            "[model_plane.http]".to_string(),
            format!("enabled = {}", self.model_plane.http.enabled),
            format!("host = {}", toml_quote(&self.model_plane.http.host)),
            format!("port = {}", self.model_plane.http.port),
            format!(
                "allow_non_loopback = {}",
                self.model_plane.http.allow_non_loopback
            ),
            String::new(),
            "[model_plane.http.auth]".to_string(),
            format!("mode = {}", toml_quote(&self.model_plane.http.auth.mode)),
            format!(
                "required_when_non_loopback = {}",
                self.model_plane.http.auth.required_when_non_loopback
            ),
            format!(
                "allow_loopback_anonymous_compat = {}",
                self.model_plane.http.auth.allow_loopback_anonymous_compat
            ),
            String::new(),
            "[policies.defaults]".to_string(),
            format!(
                "reuse_policy = {}",
                toml_quote(reuse_policy_to_str(&self.policies.reuse_policy))
            ),
            format!(
                "approval_mode = {}",
                toml_quote(&self.policies.approval_mode)
            ),
            format!(
                "capture_env_fingerprint = {}",
                self.policies.capture_env_fingerprint
            ),
            format!(
                "mcp_target_resolution_policy = {}",
                toml_quote(&self.policies.mcp_target_resolution_policy)
            ),
            String::new(),
        ];

        if let Some(kdf) = self.vault.protectors.primary.kdf.as_ref() {
            lines.insert(
                lines
                    .iter()
                    .position(|line| line == "[vault.ssh]")
                    .unwrap_or(lines.len()),
                format!("kdf = {}", toml_quote(kdf)),
            );
        }
        if let Some(profile) = self.vault.protectors.primary.profile.as_ref() {
            lines.insert(
                lines
                    .iter()
                    .position(|line| line == "[vault.ssh]")
                    .unwrap_or(lines.len()),
                format!("profile = {}", toml_quote(profile)),
            );
        }
        for recovery in &self.vault.protectors.recovery {
            lines.push("[[vault.protectors.recovery]]".to_string());
            lines.push(format!("kind = {}", toml_quote(&recovery.kind)));
            if let Some(kdf) = recovery.kdf.as_ref() {
                lines.push(format!("kdf = {}", toml_quote(kdf)));
            }
            if let Some(profile) = recovery.profile.as_ref() {
                lines.push(format!("profile = {}", toml_quote(profile)));
            }
            lines.push(String::new());
        }

        let mut toolchain_keys = self.toolchains.keys().cloned().collect::<Vec<_>>();
        toolchain_keys.sort();
        for key in toolchain_keys {
            let section = self
                .toolchains
                .get(&key)
                .expect("toolchain key from sorted iteration");
            lines.push(format!("[toolchains.{}]", key));
            lines.push(format!(
                "path_override = {}",
                toml_quote(&section.path_override)
            ));
            lines.push(format!(
                "prefer_builtin_fallback = {}",
                section.prefer_builtin_fallback
            ));
            lines.push(String::new());
        }

        for target in &self.targets {
            lines.push("[[targets]]".to_string());
            lines.push(format!("id = {}", toml_quote(&target.id)));
            lines.push(format!(
                "display_name = {}",
                toml_quote(&target.display_name)
            ));
            lines.push(format!(
                "kind = {}",
                toml_quote(target_kind_to_str(&target.kind))
            ));
            lines.push(format!("enabled = {}", target.enabled));
            lines.push(format!("aliases = {}", toml_string_array(&target.aliases)));
            lines.push(format!(
                "storage_class = {}",
                toml_quote(target.storage_class.trim())
            ));
            lines.push(format!(
                "access_class = {}",
                toml_quote(target.access_class.trim())
            ));
            if let Some(sealed_profile_ref) = target.sealed_profile_ref.as_ref() {
                lines.push(format!(
                    "sealed_profile_ref = {}",
                    toml_quote(sealed_profile_ref)
                ));
            }
            if let Some(credential_ref) = target.credential_ref.as_ref() {
                lines.push(format!("credential_ref = {}", toml_quote(credential_ref)));
            }
            if let Some(notes) = target.notes.as_ref() {
                lines.push(format!("notes = {}", toml_quote(notes)));
            }
            lines.push(String::new());

            lines.push("[targets.connection]".to_string());
            if let Some(host) = target.connection.host.as_ref() {
                lines.push(format!("host = {}", toml_quote(host)));
            }
            if let Some(port) = target.connection.port {
                lines.push(format!("port = {}", port));
            }
            if let Some(username) = target.connection.username.as_ref() {
                lines.push(format!("username = {}", toml_quote(username)));
            }
            if let Some(policy) = target.connection.known_hosts_policy.as_ref() {
                lines.push(format!("known_hosts_policy = {}", toml_quote(policy)));
            }
            if let Some(selector_kind) = target.connection.selector_kind.as_ref() {
                lines.push(format!("selector_kind = {}", toml_quote(selector_kind)));
            }
            if let Some(selector_value) = target.connection.selector_value.as_ref() {
                lines.push(format!("selector_value = {}", toml_quote(selector_value)));
            }
            lines.push(String::new());

            if target.terminal.family.is_some() || target.terminal.concurrency_policy.is_some() {
                lines.push("[targets.terminal]".to_string());
                if let Some(family) = target.terminal.family.as_ref() {
                    lines.push(format!("family = {}", toml_quote(family)));
                }
                if let Some(policy) = target.terminal.concurrency_policy.as_ref() {
                    lines.push(format!("concurrency_policy = {}", toml_quote(policy)));
                }
                lines.push(String::new());
            }

            let mut target_toolchain_keys = target.toolchains.keys().cloned().collect::<Vec<_>>();
            target_toolchain_keys.sort();
            for key in target_toolchain_keys {
                let section = target
                    .toolchains
                    .get(&key)
                    .expect("target toolchain key from sorted iteration");
                lines.push(format!("[targets.toolchains.{key}]"));
                lines.push(format!(
                    "path_override = {}",
                    toml_quote(&section.path_override)
                ));
                lines.push(format!(
                    "prefer_builtin_fallback = {}",
                    section.prefer_builtin_fallback
                ));
                lines.push(String::new());
            }

            lines.push("[targets.providers.terminal]".to_string());
            lines.push(format!("enabled = {}", target.terminal_provider.enabled));
            if let Some(shell) = target.terminal_provider.shell.as_ref() {
                lines.push(format!("shell = {}", toml_quote(shell)));
            }
            lines.push(String::new());

            for repo in &target.git_repositories {
                lines.push("[[targets.providers.git.repositories]]".to_string());
                lines.push(format!("id = {}", toml_quote(&repo.id)));
                lines.push(format!("path = {}", toml_quote(&repo.path)));
                if let Some(remote_name) = repo.remote_name.as_ref() {
                    lines.push(format!("remote_name = {}", toml_quote(remote_name)));
                }
                if let Some(web_url) = repo.web_url.as_ref() {
                    lines.push(format!("web_url = {}", toml_quote(web_url)));
                }
                lines.push(String::new());
                if let Some(review) = repo.review.as_ref() {
                    lines.push("[targets.providers.git.repositories.review]".to_string());
                    lines.push(format!("kind = {}", toml_quote(&review.kind)));
                    lines.push(format!("base_url = {}", toml_quote(&review.base_url)));
                    lines.push(format!("project = {}", toml_quote(&review.project)));
                    lines.push(String::new());
                }
            }
        }

        lines.join("\n") + "\n"
    }
}

fn target_kind_to_str(kind: &TargetKind) -> &str {
    match kind {
        TargetKind::Ssh => "ssh",
        TargetKind::Adb => "adb",
        TargetKind::Serial => "serial",
        TargetKind::Docker => "docker",
        TargetKind::Other(value) => value.as_str(),
    }
}

fn reuse_policy_to_str(policy: &SessionReusePolicy) -> &'static str {
    match policy {
        SessionReusePolicy::AlwaysNew => "always_new",
        SessionReusePolicy::ReuseIfAlive => "reuse_if_alive",
        SessionReusePolicy::ResumeOrCreate => "resume_or_create",
    }
}

fn toml_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn toml_string_array(values: &[String]) -> String {
    if values.is_empty() {
        "[]".to_string()
    } else {
        let body = values
            .iter()
            .map(|value| toml_quote(value))
            .collect::<Vec<_>>()
            .join(", ");
        format!("[{body}]")
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

fn parse_target_storage_class(value: &str) -> Result<String, ConfigError> {
    let raw = parse_string("targets[].storage_class", value)?
        .trim()
        .to_ascii_lowercase();
    match raw.as_str() {
        "plain" | "sealed-overlay" | "sealed-full" => Ok(raw),
        _ => Err(ConfigError::InvalidValue {
            field: "targets[].storage_class".into(),
            reason: format!("unsupported target storage class: {raw}"),
        }),
    }
}

fn parse_target_access_class(value: &str) -> Result<String, ConfigError> {
    let raw = parse_string("targets[].access_class", value)?
        .trim()
        .to_ascii_lowercase();
    match raw.as_str() {
        "anonymous-local" | "token-scoped" => Ok(raw),
        _ => Err(ConfigError::InvalidValue {
            field: "targets[].access_class".into(),
            reason: format!("unsupported target access class: {raw}"),
        }),
    }
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

fn parse_mcp_target_resolution_policy(value: &str) -> Result<String, ConfigError> {
    let raw = parse_string("policies.defaults.mcp_target_resolution_policy", value)?;
    match raw.as_str() {
        "auto_execute" | "confirm_if_family" | "confirm_if_related" | "confirm_always" => Ok(raw),
        _ => Err(ConfigError::InvalidValue {
            field: "policies.defaults.mcp_target_resolution_policy".into(),
            reason: format!("unsupported mcp target resolution policy: {raw}"),
        }),
    }
}

fn canonicalize_credential_ref_input(raw: &str) -> Result<String, ConfigError> {
    let trimmed = raw.trim();
    if trimmed.starts_with("vault:") || trimmed.starts_with("vault://") {
        return bridgingio_secrets::normalize_credential_ref(trimmed).map_err(|err| {
            ConfigError::InvalidValue {
                field: "targets[].credential_ref".into(),
                reason: format!("invalid vault credential ref: {err:?}"),
            }
        });
    }
    Ok(trimmed.to_string())
}

fn normalize_vault_method(raw: &str) -> String {
    raw.trim().to_ascii_lowercase().replace('_', "-")
}

fn push_unique_string(list: &mut Vec<String>, value: String) {
    if value.is_empty() {
        return;
    }
    if !list.iter().any(|existing| existing == &value) {
        list.push(value);
    }
}

fn canonicalize_vault_section(vault: &mut VaultSection) -> Result<(), ConfigError> {
    let mut backend = vault.backend.trim().to_ascii_lowercase();
    let legacy_os_native = backend == "os-native";
    if legacy_os_native {
        backend = "builtin-encrypted".into();
    }
    vault.backend = backend;

    if vault.namespace.trim().is_empty() {
        vault.namespace = "io.bridgingio".into();
    } else {
        vault.namespace = vault.namespace.trim().to_string();
    }

    let primary_kind = normalize_vault_method(&vault.protectors.primary.kind);
    vault.protectors.primary.kind = if primary_kind.is_empty() {
        if legacy_os_native {
            "os-native".into()
        } else {
            "passphrase".into()
        }
    } else {
        primary_kind
    };
    vault.protectors.primary.kdf = vault
        .protectors
        .primary
        .kdf
        .as_ref()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());
    vault.protectors.primary.profile = vault
        .protectors
        .primary
        .profile
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let mut canonical_recovery = Vec::new();
    for mut recovery in vault.protectors.recovery.clone() {
        recovery.kind = normalize_vault_method(&recovery.kind);
        recovery.kdf = recovery
            .kdf
            .as_ref()
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty());
        recovery.profile = recovery
            .profile
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        if recovery.kind.is_empty() {
            continue;
        }
        if !canonical_recovery
            .iter()
            .any(|existing: &VaultProtectorConfig| existing.kind == recovery.kind)
        {
            canonical_recovery.push(recovery);
        }
    }
    if canonical_recovery.is_empty() {
        canonical_recovery.push(VaultProtectorConfig {
            kind: "passphrase".into(),
            kdf: Some("argon2id".into()),
            profile: Some("interactive-default".into()),
        });
    }
    vault.protectors.recovery = canonical_recovery;

    vault.unlock.trigger_policy = normalize_vault_method(&vault.unlock.trigger_policy);
    if vault.unlock.trigger_policy.is_empty() {
        vault.unlock.trigger_policy = "on-first-secret-access".into();
    }
    let mut allowed_methods = Vec::new();
    for method in &vault.unlock.allowed_methods {
        push_unique_string(&mut allowed_methods, normalize_vault_method(method));
    }
    push_unique_string(&mut allowed_methods, vault.protectors.primary.kind.clone());
    for method in &vault.protectors.recovery {
        push_unique_string(&mut allowed_methods, method.kind.clone());
    }
    if legacy_os_native {
        push_unique_string(&mut allowed_methods, "os-native".into());
    }
    if allowed_methods.is_empty() {
        return Err(ConfigError::InvalidValue {
            field: "vault.unlock.allowed_methods".into(),
            reason: "at least one unlock method is required".into(),
        });
    }
    vault.unlock.allowed_methods = allowed_methods;
    let preferred = normalize_vault_method(&vault.unlock.preferred_method);
    vault.unlock.preferred_method = if preferred.is_empty() {
        vault.protectors.primary.kind.clone()
    } else {
        preferred
    };
    if !vault
        .unlock
        .allowed_methods
        .iter()
        .any(|method| method == &vault.unlock.preferred_method)
    {
        vault
            .unlock
            .allowed_methods
            .insert(0, vault.unlock.preferred_method.clone());
    }
    if vault.unlock.cache_ttl_sec == 0 {
        vault.unlock.cache_ttl_sec = 600;
    }

    vault.ssh.delivery_mode = normalize_vault_method(&vault.ssh.delivery_mode);
    if vault.ssh.delivery_mode.is_empty() {
        vault.ssh.delivery_mode = "ssh-agent-broker".into();
    }
    vault.ssh.fallback_delivery_mode = normalize_vault_method(&vault.ssh.fallback_delivery_mode);
    if vault.ssh.fallback_delivery_mode.is_empty() {
        vault.ssh.fallback_delivery_mode = "ephemeral-identity-file".into();
    }
    Ok(())
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
        SessionRecord, SessionReusePolicy, TargetKind, TARGET_TERMINAL_CONCURRENCY_METADATA_KEY,
    };

    use super::{InMemoryMetadataStore, MetadataStoreError};

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
            toolchains: Default::default(),
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
        let transport = store
            .open_transport_session(
                &logical.logical_session_id,
                "target-1",
                TargetKind::Ssh,
                Some("/usr/bin/ssh".into()),
                Some("system_path".into()),
                now,
            )
            .expect("open transport session");

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

    #[test]
    fn multiplexed_target_allows_multiple_active_transport_sessions() {
        let mut store = InMemoryMetadataStore::default();
        store.upsert_profile(bridgingio_domain::TargetProfile {
            id: "target-ssh".into(),
            name: "ssh".into(),
            kind: TargetKind::Ssh,
            connection: ConnectionConfig::Ssh {
                host: "127.0.0.1".into(),
                port: 22,
                username: "dev".into(),
            },
            credential_ref: None,
            default_policy: PolicyProfile::default(),
            notes: None,
            metadata: Default::default(),
            toolchains: Default::default(),
        });

        let now = SystemTime::now();
        let logical = store.resolve_logical_session(
            &scope("agent-a", "client-1"),
            "target-ssh",
            SessionReusePolicy::ReuseIfAlive,
            now,
        );

        let first = store
            .open_transport_session(
                &logical.logical_session_id,
                "target-ssh",
                TargetKind::Ssh,
                Some("/usr/bin/ssh".into()),
                Some("system_path".into()),
                now,
            )
            .expect("first transport session");
        let second = store
            .open_transport_session(
                &logical.logical_session_id,
                "target-ssh",
                TargetKind::Ssh,
                Some("/usr/bin/ssh".into()),
                Some("system_path".into()),
                now,
            )
            .expect("second transport session");

        assert_ne!(first.transport_session_id, second.transport_session_id);
    }

    #[test]
    fn exclusive_target_lease_is_target_wide_and_releases_after_close() {
        let mut store = InMemoryMetadataStore::default();
        let mut metadata = std::collections::BTreeMap::new();
        metadata.insert(
            TARGET_TERMINAL_CONCURRENCY_METADATA_KEY.to_string(),
            "exclusive".to_string(),
        );
        store.upsert_profile(bridgingio_domain::TargetProfile {
            id: "target-localshell".into(),
            name: "future localshell".into(),
            kind: TargetKind::Other("localshell".into()),
            connection: ConnectionConfig::Custom {
                description: "future localshell transport".into(),
            },
            credential_ref: None,
            default_policy: PolicyProfile::default(),
            notes: None,
            metadata,
            toolchains: Default::default(),
        });

        let now = SystemTime::now();
        let logical_a = store.resolve_logical_session(
            &scope("agent-a", "client-a"),
            "target-localshell",
            SessionReusePolicy::ReuseIfAlive,
            now,
        );
        let held_transport = store
            .open_transport_session(
                &logical_a.logical_session_id,
                "target-localshell",
                TargetKind::Other("localshell".into()),
                Some("/usr/bin/env".into()),
                Some("system_path".into()),
                now,
            )
            .expect("first exclusive transport");

        let logical_b = store.resolve_logical_session(
            &scope("agent-b", "client-b"),
            "target-localshell",
            SessionReusePolicy::ReuseIfAlive,
            now,
        );
        let err = store
            .open_transport_session(
                &logical_b.logical_session_id,
                "target-localshell",
                TargetKind::Other("localshell".into()),
                Some("/usr/bin/env".into()),
                Some("system_path".into()),
                now,
            )
            .expect_err("exclusive target should reject concurrent holder");
        assert_eq!(
            err,
            MetadataStoreError::TargetBusy {
                target_id: "target-localshell".into(),
                holder_transport_session_id: held_transport.transport_session_id.clone(),
            }
        );

        let closed = store.close_transport_session(
            &held_transport.transport_session_id,
            "interactive shell closed",
            now,
        );
        assert!(closed.is_some());

        let next_transport = store
            .open_transport_session(
                &logical_b.logical_session_id,
                "target-localshell",
                TargetKind::Other("localshell".into()),
                Some("/usr/bin/env".into()),
                Some("system_path".into()),
                now,
            )
            .expect("lease should be acquirable after release");
        assert_ne!(
            next_transport.transport_session_id,
            held_transport.transport_session_id
        );
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
        assert_eq!(config.vault.backend, "builtin-encrypted");
        assert_eq!(config.vault.protectors.primary.kind, "os-native");
        assert!(config
            .vault
            .unlock
            .allowed_methods
            .iter()
            .any(|method| method == "os-native"));
        assert_eq!(config.targets.len(), 1);
        assert_eq!(
            config.policies.mcp_target_resolution_policy,
            "confirm_if_family"
        );
    }

    #[test]
    fn parses_complete_standalone_example() {
        let config =
            CoreSettings::from_toml_str(CoreSettings::complete_example()).expect("parse complete");
        assert_eq!(config.targets.len(), 2);
        assert!(config.toolchains.contains_key("adb"));
        let adb_target = config
            .targets
            .iter()
            .find(|target| target.id == "android-emulator")
            .expect("android emulator target");
        assert_eq!(
            adb_target
                .toolchains
                .get("adb")
                .map(|section| section.path_override.as_str()),
            Some("__TARGET_ADB_OVERRIDE__")
        );
        assert_eq!(
            adb_target.credential_ref.as_deref(),
            Some("vault://bridgingio/adb/default")
        );
        assert_eq!(config.model_plane.http.host, "127.0.0.1");
        assert_eq!(config.storage.artifacts.backend, "filesystem");
        assert_eq!(
            config.policies.mcp_target_resolution_policy,
            "confirm_if_family"
        );
    }

    #[test]
    fn legacy_target_defaults_to_plain_and_anonymous_local() {
        let config =
            CoreSettings::from_toml_str(CoreSettings::minimal_example()).expect("parse minimal");
        let target = config
            .targets
            .iter()
            .find(|entry| entry.id == "local-ssh")
            .expect("local-ssh target");
        assert_eq!(target.storage_class, "plain");
        assert_eq!(target.access_class, "anonymous-local");
        assert!(target.sealed_profile_ref.is_none());
    }

    #[test]
    fn rejects_sealed_target_with_anonymous_local_access_class() {
        let invalid = format!(
            "{}\n\n[[targets]]\nid = \"sealed-bad\"\ndisplay_name = \"Sealed Bad\"\nkind = \"localshell\"\nenabled = true\naliases = []\nstorage_class = \"sealed-overlay\"\naccess_class = \"anonymous-local\"\nsealed_profile_ref = \"vault://bridgingio/targets/sealed-bad\"\n\n[targets.connection]\n\n[targets.providers.terminal]\nenabled = true\n",
            CoreSettings::minimal_example()
        );
        let err = CoreSettings::from_toml_str(&invalid).expect_err("must reject");
        assert!(matches!(
            err,
            ConfigError::InvalidValue { field, .. } if field == "targets[].access_class"
        ));
    }

    #[test]
    fn accepts_future_localshell_target_with_terminal_metadata() {
        let text = format!(
            "{}\n\n[[targets]]\nid = \"workspace-shell\"\ndisplay_name = \"Workspace LocalShell\"\nkind = \"localshell\"\nenabled = true\naliases = [\"ws\"]\n\n[targets.connection]\n\n[targets.terminal]\nfamily = \"terminal\"\nconcurrency_policy = \"exclusive\"\n\n[targets.providers.terminal]\nenabled = true\nshell = \"bash\"\n",
            CoreSettings::minimal_example()
        );

        let parsed = CoreSettings::from_toml_str(&text).expect("parse localshell style target");
        let target = parsed
            .targets
            .iter()
            .find(|entry| entry.id == "workspace-shell")
            .expect("workspace-shell target");
        assert_eq!(
            target.kind,
            bridgingio_domain::TargetKind::Other("localshell".into())
        );
        assert_eq!(target.terminal.family.as_deref(), Some("terminal"));
        assert_eq!(
            target.terminal.concurrency_policy.as_deref(),
            Some("exclusive")
        );
    }

    #[test]
    fn target_and_global_toolchain_roundtrip_preserves_scope() {
        let config = CoreSettings::from_toml_str(CoreSettings::complete_example())
            .expect("parse complete example");
        let serialized = config.to_toml_string();
        let reparsed = CoreSettings::from_toml_str(&serialized).expect("reparse serialized");

        assert_eq!(
            reparsed
                .toolchains
                .get("adb")
                .map(|section| section.path_override.as_str()),
            Some("__GLOBAL_ADB_OVERRIDE__")
        );

        let adb_target = reparsed
            .targets
            .iter()
            .find(|target| target.id == "android-emulator")
            .expect("android emulator target");
        assert_eq!(
            adb_target
                .toolchains
                .get("adb")
                .map(|section| section.path_override.as_str()),
            Some("__TARGET_ADB_OVERRIDE__")
        );
    }

    #[test]
    fn canonicalizes_legacy_vault_backend_and_reference_on_parse_and_rewrite() {
        let legacy = CoreSettings::minimal_example().replace(
            "credential_ref = \"vault://bridgingio/ssh-private-key/local\"",
            "credential_ref = \"vault:ssh-key:local\"",
        );
        let parsed = CoreSettings::from_toml_str(&legacy).expect("parse legacy vault config");
        assert_eq!(parsed.vault.backend, "builtin-encrypted");
        assert_eq!(parsed.vault.protectors.primary.kind, "os-native");
        assert_eq!(
            parsed.targets[0].credential_ref.as_deref(),
            Some("vault://bridgingio/ssh-private-key/local")
        );

        let rewritten = parsed.to_toml_string();
        assert!(rewritten.contains("[vault.unlock]"));
        assert!(rewritten.contains("[vault.protectors.primary]"));
        assert!(rewritten.contains("[vault.ssh]"));
        assert!(rewritten.contains("backend = \"builtin-encrypted\""));
        assert!(!rewritten.contains("backend = \"os-native\""));
        assert!(rewritten.contains("credential_ref = \"vault://bridgingio/ssh-private-key/local\""));
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

    #[test]
    fn rejects_unknown_mcp_target_resolution_policy() {
        let invalid = CoreSettings::minimal_example().replace(
            "mcp_target_resolution_policy = \"confirm_if_family\"",
            "mcp_target_resolution_policy = \"unknown\"",
        );
        let err = CoreSettings::from_toml_str(&invalid).expect_err("must reject unknown policy");
        assert!(matches!(err, ConfigError::InvalidValue { .. }));
    }

    #[test]
    fn config_error_maps_to_shared_error_contract() {
        let shared = ConfigError::NonLoopbackAuthRequired("0.0.0.0".into()).shared_error();
        assert_eq!(shared.status.as_str(), "not_ready");
        assert_eq!(shared.domain.as_str(), "config");
        assert_eq!(shared.common_code.as_str(), "not_ready");
        assert_eq!(
            shared.module_code.as_deref(),
            Some("config.non_loopback_auth_required")
        );
        assert!(shared.recovery_hint.is_some());
    }
}
