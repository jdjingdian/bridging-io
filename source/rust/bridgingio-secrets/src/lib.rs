use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::time::{Duration, SystemTime};

pub const DEFAULT_CREDENTIAL_NAMESPACE: &str = "bridgingio";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalCredentialRef {
    pub namespace: String,
    pub kind: String,
    pub name: String,
}

impl CanonicalCredentialRef {
    pub fn parse(raw: &str) -> Result<Self, VaultError> {
        let raw = raw.trim();
        if let Some(body) = raw.strip_prefix("vault://") {
            let parts = body.split('/').collect::<Vec<_>>();
            if parts.len() != 3 {
                return Err(VaultError::InvalidCredentialRef(raw.to_string()));
            }
            return Ok(Self {
                namespace: normalize_ref_segment(parts[0])?,
                kind: normalize_kind_alias(parts[1]),
                name: normalize_ref_segment(parts[2])?,
            });
        }

        if let Some(body) = raw.strip_prefix("vault:") {
            let parts = body.split(':').collect::<Vec<_>>();
            match parts.as_slice() {
                [kind, name] => {
                    return Ok(Self {
                        namespace: DEFAULT_CREDENTIAL_NAMESPACE.to_string(),
                        kind: normalize_kind_alias(kind),
                        name: normalize_ref_segment(name)?,
                    });
                }
                [namespace, kind, name] => {
                    return Ok(Self {
                        namespace: normalize_ref_segment(namespace)?,
                        kind: normalize_kind_alias(kind),
                        name: normalize_ref_segment(name)?,
                    });
                }
                _ => return Err(VaultError::InvalidCredentialRef(raw.to_string())),
            }
        }

        Err(VaultError::InvalidCredentialRef(raw.to_string()))
    }

    pub fn to_uri(&self) -> String {
        format!("vault://{}/{}/{}", self.namespace, self.kind, self.name)
    }
}

pub fn normalize_credential_ref(raw: &str) -> Result<String, VaultError> {
    CanonicalCredentialRef::parse(raw).map(|value| value.to_uri())
}

pub fn command_audit_preview(command: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    command.hash(&mut hasher);
    format!(
        "cmd#{:016x} len={}",
        hasher.finish(),
        command.chars().count()
    )
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuntimeRedactionRegistry {
    entries: Vec<String>,
}

impl RuntimeRedactionRegistry {
    pub fn register(&mut self, value: &str) {
        let candidate = value.trim().to_string();
        if candidate.len() < 4 {
            return;
        }
        if !self.entries.contains(&candidate) {
            self.entries.push(candidate);
            self.entries
                .sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        }
    }

    pub fn tracked_entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn redact(&self, text: &str) -> String {
        let mut redacted = text.to_string();
        for entry in &self.entries {
            redacted = redacted.replace(entry, "[REDACTED]");
        }
        redacted
    }
}

pub struct SecretBytes {
    bytes: Vec<u8>,
}

impl SecretBytes {
    pub fn from_utf8(value: impl AsRef<str>) -> Self {
        Self {
            bytes: value.as_ref().as_bytes().to_vec(),
        }
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    pub fn expose_for_use(&self) -> &[u8] {
        &self.bytes
    }

    pub fn expose_utf8_for_use(&self) -> Option<&str> {
        std::str::from_utf8(&self.bytes).ok()
    }

    pub fn into_bytes(mut self) -> Vec<u8> {
        let mut out = Vec::new();
        std::mem::swap(&mut out, &mut self.bytes);
        out
    }
}

impl Drop for SecretBytes {
    fn drop(&mut self) {
        for byte in &mut self.bytes {
            *byte = 0;
        }
    }
}

impl PartialEq for SecretBytes {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes
    }
}

impl Eq for SecretBytes {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VaultSecretStatus {
    Active,
    Disabled,
    ScheduledDelete,
    Deleted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VaultSecretVersionState {
    Pending,
    Active,
    Superseded,
    Revoked,
    Destroyed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecretRotationState {
    pub rotation_count: u32,
    pub last_rotated_by: Option<String>,
    pub last_rotation_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultSecretRecord {
    pub reference: String,
    pub kind: String,
    pub label: String,
    pub status: VaultSecretStatus,
    pub active_version_id: Option<String>,
    pub created_by: String,
    pub created_at: SystemTime,
    pub last_used_at: Option<SystemTime>,
    pub last_rotated_at: Option<SystemTime>,
    pub rotation: SecretRotationState,
    pub audit_chain_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultSecretVersionRecord {
    pub version_id: String,
    pub reference: String,
    pub version_seq: u32,
    pub state: VaultSecretVersionState,
    pub protector_binding: String,
    pub ciphertext_locator: String,
    pub ciphertext_digest: String,
    pub content_format: String,
    pub created_by: String,
    pub created_at: SystemTime,
    pub activated_at: Option<SystemTime>,
    pub superseded_at: Option<SystemTime>,
    pub revoked_at: Option<SystemTime>,
    pub destroy_after: Option<SystemTime>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VaultKeyEnvelopeState {
    Active,
    Rewrapping,
    Retired,
    Destroyed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultKeyEnvelopeRecord {
    pub vault_key_id: String,
    pub state: VaultKeyEnvelopeState,
    pub active_wrap_set_id: String,
    pub created_at: SystemTime,
    pub rotated_at: Option<SystemTime>,
    pub last_unlocked_at: Option<SystemTime>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProtectorWrapStatus {
    Ready,
    Fallback,
    Degraded,
    Unavailable,
    Retired,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtectorWrapManifest {
    pub wrap_id: String,
    pub vault_key_id: String,
    pub protector_binding: String,
    pub wrap_format: String,
    pub wrapped_key_digest: String,
    pub created_at: SystemTime,
    pub last_verified_at: Option<SystemTime>,
    pub status: ProtectorWrapStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecretRecord {
    pub reference: String,
    pub label: String,
    pub backend: String,
    pub kind: String,
    pub version: u32,
    pub status: VaultSecretStatus,
    pub created_by: String,
    pub created_at: SystemTime,
    pub last_rotated_at: Option<SystemTime>,
    pub audit_chain_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentTokenStatus {
    Active,
    Revoked,
    Expired,
}

impl AgentTokenStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Revoked => "revoked",
            Self::Expired => "expired",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenScopeStatus {
    Active,
    Superseded,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentTokenRecord {
    pub token_id: String,
    pub principal_id: String,
    pub label: String,
    pub status: AgentTokenStatus,
    pub token_hash: String,
    pub hash_scheme: String,
    pub scope_profile: String,
    pub active_scope_version: u32,
    pub created_by: String,
    pub created_at: SystemTime,
    pub last_used_at: Option<SystemTime>,
    pub expires_at: Option<SystemTime>,
    pub idle_timeout_sec: Option<u64>,
    pub revoked_at: Option<SystemTime>,
    pub revoke_reason: Option<String>,
    pub parent_token_id: Option<String>,
    pub issued_via_attestation_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenScopeRecord {
    pub token_id: String,
    pub version: u32,
    pub status: TokenScopeStatus,
    pub scope_profile: String,
    pub target_ids: Vec<String>,
    pub tool_ids: Vec<String>,
    pub max_risk_envelope: String,
    pub allow_open_shell: bool,
    pub allow_write_shell_input: bool,
    pub allow_artifact_cross_principal: bool,
    pub allow_delegation: bool,
    pub allow_admin_actions: bool,
    pub created_by: String,
    pub created_at: SystemTime,
    pub superseded_at: Option<SystemTime>,
    pub change_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentTokenSummary {
    pub token_id: String,
    pub label: String,
    pub principal_summary: String,
    pub status: AgentTokenStatus,
    pub scope_profile: String,
    pub target_scope_summary: String,
    pub active_scope_version: u32,
    pub created_at: SystemTime,
    pub last_used_at: Option<SystemTime>,
    pub expires_at: Option<SystemTime>,
    pub revoked_at: Option<SystemTime>,
    pub revoke_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenScopeInput {
    pub scope_profile: Option<String>,
    pub target_ids: Vec<String>,
    pub tool_ids: Vec<String>,
    pub max_risk_envelope: Option<String>,
    pub allow_open_shell: Option<bool>,
    pub allow_write_shell_input: Option<bool>,
    pub allow_artifact_cross_principal: Option<bool>,
    pub allow_delegation: Option<bool>,
    pub allow_admin_actions: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateAgentTokenRequest {
    pub label: String,
    pub created_by: String,
    pub expires_in: Option<Duration>,
    pub idle_timeout_sec: Option<u64>,
    pub scope: TokenScopeInput,
    pub attestation_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateAgentTokenScopeRequest {
    pub token_id: String,
    pub changed_by: String,
    pub scope: TokenScopeInput,
    pub reason: Option<String>,
    pub attestation_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateAgentTokenResult {
    pub plaintext_token: String,
    pub summary: AgentTokenSummary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthenticatedAgentToken {
    pub token_id: String,
    pub principal_id: String,
    pub active_scope_version: u32,
    pub scope_profile: String,
    pub target_ids: Vec<String>,
    pub tool_ids: Vec<String>,
    pub max_risk_envelope: String,
    pub allow_open_shell: bool,
    pub allow_write_shell_input: bool,
    pub allow_artifact_cross_principal: bool,
    pub allow_delegation: bool,
    pub allow_admin_actions: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultSecretMetadataView {
    pub record: VaultSecretRecord,
    pub versions: Vec<VaultSecretVersionRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VaultReadinessState {
    Ready,
    Fallback,
    Degraded,
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtectorReadinessDiagnostic {
    pub protector: String,
    pub status: VaultReadinessState,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultReadinessDiagnostic {
    pub backend: String,
    pub status: VaultReadinessState,
    pub message: String,
    pub fail_closed: bool,
    pub protectors: Vec<ProtectorReadinessDiagnostic>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtectorAssembly {
    pub primary: String,
    pub recovery: Vec<String>,
    pub retired: Vec<String>,
    pub fail_closed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrokerUseCase {
    SshAuth { target_scope: String },
    HttpAuth { request_scope: String },
    Signing { signing_scope: String },
    ModelPlaneToken { token_scope: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SshAgentBrokerSessionState {
    Preparing,
    Ready,
    Attached,
    Draining,
    Closed,
    Failed,
}

impl SshAgentBrokerSessionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Preparing => "preparing",
            Self::Ready => "ready",
            Self::Attached => "attached",
            Self::Draining => "draining",
            Self::Closed => "closed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SshAgentBrokerEndpointKind {
    UnixSocket,
    NamedPipe,
    PlatformLocalEndpoint,
    IdentityFileFallback,
}

impl SshAgentBrokerEndpointKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UnixSocket => "unix-socket",
            Self::NamedPipe => "named-pipe",
            Self::PlatformLocalEndpoint => "platform-local-endpoint",
            Self::IdentityFileFallback => "identity-file-fallback",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SshHostKeyPolicy {
    Strict,
    AcceptNew,
    InsecureNoCheck,
}

impl SshHostKeyPolicy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::AcceptNew => "accept-new",
            Self::InsecureNoCheck => "insecure-no-check",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SshKeyPassphraseHandling {
    ImportTimeOnly,
    RuntimePromptForbidden,
}

impl SshKeyPassphraseHandling {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ImportTimeOnly => "import-time-only",
            Self::RuntimePromptForbidden => "runtime-prompt-forbidden",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshAgentCompatibilityDiagnostic {
    pub platform: String,
    pub endpoint_kind: SshAgentBrokerEndpointKind,
    pub status: VaultReadinessState,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshAgentBrokerSession {
    pub broker_session_id: String,
    pub target_id: String,
    pub credential_ref: String,
    pub state: SshAgentBrokerSessionState,
    pub endpoint_kind: SshAgentBrokerEndpointKind,
    pub degraded: bool,
    pub created_at: SystemTime,
    pub expires_at: Option<SystemTime>,
    pub attached_channel_count: u32,
    pub cleanup_deadline: Option<SystemTime>,
    pub host_key_policy: SshHostKeyPolicy,
    pub key_passphrase_handling: SshKeyPassphraseHandling,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshAgentBrokerSessionSummary {
    pub broker_session_id: String,
    pub target_id: String,
    pub credential_ref: String,
    pub state: SshAgentBrokerSessionState,
    pub endpoint_kind: SshAgentBrokerEndpointKind,
    pub degraded: bool,
    pub created_at: SystemTime,
    pub expires_at: Option<SystemTime>,
    pub attached_channel_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshAgentBrokerPrepareRequest {
    pub target_id: String,
    pub credential_ref: String,
    pub principal_id: String,
    pub logical_session_id: Option<String>,
    pub host_platform: String,
    pub allow_identity_fallback: bool,
    pub host_key_policy: SshHostKeyPolicy,
    pub key_passphrase_handling: SshKeyPassphraseHandling,
    pub runtime_passphrase_requested: bool,
    pub session_ttl: Option<Duration>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshAgentBrokerPreparedSession {
    pub session: SshAgentBrokerSessionSummary,
    pub ssh_option_args: Vec<String>,
    pub diagnostics: Vec<String>,
}

pub struct BrokeredSecretLease {
    reference: String,
    version_id: String,
    use_case: BrokerUseCase,
    redacted_preview: String,
    secret: SecretBytes,
}

impl BrokeredSecretLease {
    pub fn reference(&self) -> &str {
        &self.reference
    }

    pub fn version_id(&self) -> &str {
        &self.version_id
    }

    pub fn use_case(&self) -> &BrokerUseCase {
        &self.use_case
    }

    pub fn redacted_preview(&self) -> &str {
        &self.redacted_preview
    }

    pub fn with_secret_bytes<R>(&self, map: impl FnOnce(&[u8]) -> R) -> R {
        map(self.secret.expose_for_use())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalAdminActionKind {
    RevealSecret,
    ExportSecret,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalAdminIntentStatus {
    Pending,
    Verified,
    Consumed,
    Expired,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalAdminActionIntent {
    pub intent_id: String,
    pub action_kind: LocalAdminActionKind,
    pub target_object_ref: String,
    pub requested_by_principal: String,
    pub requested_payload_digest: String,
    pub created_at: SystemTime,
    pub expires_at: SystemTime,
    pub status: LocalAdminIntentStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalAdminAttestationStatus {
    Active,
    Consumed,
    Expired,
    Revoked,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalAdminAttestationRecord {
    pub attestation_id: String,
    pub intent_id: String,
    pub verified_principal: String,
    pub verification_method: String,
    pub issued_at: SystemTime,
    pub expires_at: SystemTime,
    pub consumed_at: Option<SystemTime>,
    pub status: LocalAdminAttestationStatus,
}

pub trait SecretVaultBackend: Send + Sync {
    fn kind(&self) -> &'static str;
    fn readiness(&self) -> VaultReadinessState;
    fn readiness_message(&self) -> &'static str;
}

#[derive(Default)]
pub struct BuiltinEncryptedVaultBackend;

impl SecretVaultBackend for BuiltinEncryptedVaultBackend {
    fn kind(&self) -> &'static str {
        "builtin-encrypted"
    }

    fn readiness(&self) -> VaultReadinessState {
        VaultReadinessState::Ready
    }

    fn readiness_message(&self) -> &'static str {
        "builtin encrypted vault is available"
    }
}

#[derive(Default)]
pub struct InMemoryVaultBackend;

impl SecretVaultBackend for InMemoryVaultBackend {
    fn kind(&self) -> &'static str {
        "in-memory"
    }

    fn readiness(&self) -> VaultReadinessState {
        VaultReadinessState::Degraded
    }

    fn readiness_message(&self) -> &'static str {
        "in-memory backend is dev-only and cannot be treated as production vault"
    }
}

#[derive(Default)]
pub struct OsNativeVaultBackend;

impl SecretVaultBackend for OsNativeVaultBackend {
    fn kind(&self) -> &'static str {
        "os-native"
    }

    fn readiness(&self) -> VaultReadinessState {
        VaultReadinessState::Degraded
    }

    fn readiness_message(&self) -> &'static str {
        "os-native backend currently runs as degraded memory shim"
    }
}

#[derive(Default)]
pub struct FileVaultBackend;

impl SecretVaultBackend for FileVaultBackend {
    fn kind(&self) -> &'static str {
        "file-vault"
    }

    fn readiness(&self) -> VaultReadinessState {
        VaultReadinessState::Fallback
    }

    fn readiness_message(&self) -> &'static str {
        "file vault is available as fallback with strict local protection requirements"
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum VaultError {
    BackendNotRegistered(String),
    InvalidCredentialRef(String),
    PlaintextAccessDisabled(String),
    SecretNotFound(String),
    SecretVersionNotFound(String),
    FailClosed(String),
    LocalAdminVerificationRequired(String),
    LocalAdminIntentMismatch(String),
    LocalAdminIntentExpired(String),
    LocalAdminAttestationMismatch(String),
    LocalAdminAttestationExpired(String),
    SshBrokerSessionNotFound(String),
    SshBrokerUnsupported(String),
    SshRuntimePassphraseForbidden(String),
    AgentTokenNotFound(String),
    AgentTokenRejected(String),
}

struct StoredSecretVersionMaterial {
    version: VaultSecretVersionRecord,
    ciphertext: Vec<u8>,
    wrapped_dek: Vec<u8>,
}

struct StoredSecretState {
    record: VaultSecretRecord,
    versions: Vec<StoredSecretVersionMaterial>,
}

struct StoredSshAgentBrokerSession {
    session: SshAgentBrokerSession,
    attached_channels: HashSet<String>,
    endpoint_locator: Option<String>,
    diagnostics: Vec<String>,
    principal_id: String,
    logical_session_id: Option<String>,
}

pub struct SecretVaultRouter {
    active_backend: String,
    backends: HashMap<String, Box<dyn SecretVaultBackend>>,
    secrets: HashMap<String, StoredSecretState>,
    agent_tokens: HashMap<String, AgentTokenRecord>,
    token_scopes: HashMap<String, Vec<TokenScopeRecord>>,
    token_hash_index: HashMap<String, String>,
    redaction_registry: RuntimeRedactionRegistry,
    root_key: SecretBytes,
    allow_degraded_mode: bool,
    next_version_seq: u64,
    next_audit_seq: u64,
    next_agent_token_seq: u64,
    next_intent_seq: u64,
    next_attestation_seq: u64,
    next_ssh_broker_seq: u64,
    key_envelope: VaultKeyEnvelopeRecord,
    wrap_manifests: Vec<ProtectorWrapManifest>,
    intents: HashMap<String, LocalAdminActionIntent>,
    attestations: HashMap<String, LocalAdminAttestationRecord>,
    ssh_broker_sessions: HashMap<String, StoredSshAgentBrokerSession>,
}

impl Default for SecretVaultRouter {
    fn default() -> Self {
        let mut router = Self {
            active_backend: "os-native".into(),
            backends: HashMap::new(),
            secrets: HashMap::new(),
            agent_tokens: HashMap::new(),
            token_scopes: HashMap::new(),
            token_hash_index: HashMap::new(),
            redaction_registry: RuntimeRedactionRegistry::default(),
            root_key: SecretBytes::from_bytes(pseudo_random_bytes(32, "vault-root-key")),
            allow_degraded_mode: false,
            next_version_seq: 0,
            next_audit_seq: 0,
            next_agent_token_seq: 0,
            next_intent_seq: 0,
            next_attestation_seq: 0,
            next_ssh_broker_seq: 0,
            key_envelope: VaultKeyEnvelopeRecord {
                vault_key_id: "vrk-000001".into(),
                state: VaultKeyEnvelopeState::Active,
                active_wrap_set_id: "wrap-set-000001".into(),
                created_at: SystemTime::now(),
                rotated_at: None,
                last_unlocked_at: None,
            },
            wrap_manifests: Vec::new(),
            intents: HashMap::new(),
            attestations: HashMap::new(),
            ssh_broker_sessions: HashMap::new(),
        };
        router.register_backend(Box::<BuiltinEncryptedVaultBackend>::default());
        router.register_backend(Box::<OsNativeVaultBackend>::default());
        router.register_backend(Box::<FileVaultBackend>::default());
        router.register_backend(Box::<InMemoryVaultBackend>::default());
        router.wrap_manifests.push(ProtectorWrapManifest {
            wrap_id: "wrap-000001".into(),
            vault_key_id: router.key_envelope.vault_key_id.clone(),
            protector_binding: "os-native".into(),
            wrap_format: "xor-envelope-v1".into(),
            wrapped_key_digest: short_digest(router.root_key.expose_for_use()),
            created_at: SystemTime::now(),
            last_verified_at: None,
            status: ProtectorWrapStatus::Degraded,
        });
        router
    }
}

impl SecretVaultRouter {
    pub fn register_backend(&mut self, backend: Box<dyn SecretVaultBackend>) {
        self.backends.insert(backend.kind().to_string(), backend);
    }

    pub fn set_active_backend(&mut self, backend: &str) -> Result<(), VaultError> {
        if !self.backends.contains_key(backend) {
            return Err(VaultError::BackendNotRegistered(backend.to_string()));
        }
        self.active_backend = backend.to_string();
        Ok(())
    }

    pub fn active_backend(&self) -> &str {
        &self.active_backend
    }

    pub fn set_allow_degraded_mode(&mut self, allow: bool) {
        self.allow_degraded_mode = allow;
    }

    pub fn redaction_registry(&self) -> &RuntimeRedactionRegistry {
        &self.redaction_registry
    }

    pub fn redaction_registry_mut(&mut self) -> &mut RuntimeRedactionRegistry {
        &mut self.redaction_registry
    }

    pub fn put(
        &mut self,
        reference: impl AsRef<str>,
        secret: impl AsRef<str>,
        label: impl Into<String>,
    ) -> Result<SecretRecord, VaultError> {
        self.put_with_actor(
            reference.as_ref(),
            SecretBytes::from_utf8(secret.as_ref()),
            label.into(),
            "local-admin",
            None,
        )
    }

    pub fn put_with_actor(
        &mut self,
        reference: &str,
        secret: SecretBytes,
        label: String,
        actor: &str,
        rotation_reason: Option<String>,
    ) -> Result<SecretRecord, VaultError> {
        let canonical = normalize_credential_ref(reference)?;
        self.ensure_backend_allows_secret_use()?;
        let now = SystemTime::now();
        let canonical_parts = CanonicalCredentialRef::parse(&canonical)?;
        let version_id = self.allocate_version_id();
        let audit_chain_id = self.allocate_audit_id();

        let record = self
            .secrets
            .entry(canonical.clone())
            .or_insert_with(|| StoredSecretState {
                record: VaultSecretRecord {
                    reference: canonical.clone(),
                    kind: canonical_parts.kind.clone(),
                    label: label.clone(),
                    status: VaultSecretStatus::Active,
                    active_version_id: None,
                    created_by: actor.to_string(),
                    created_at: now,
                    last_used_at: None,
                    last_rotated_at: None,
                    rotation: SecretRotationState {
                        rotation_count: 0,
                        last_rotated_by: None,
                        last_rotation_reason: None,
                    },
                    audit_chain_id,
                },
                versions: Vec::new(),
            });

        if let Some(previous_active) = record.record.active_version_id.clone() {
            for version in &mut record.versions {
                if version.version.version_id == previous_active
                    && version.version.state == VaultSecretVersionState::Active
                {
                    version.version.state = VaultSecretVersionState::Superseded;
                    version.version.superseded_at = Some(now);
                }
            }
        }

        let version_seq = record.versions.len() as u32 + 1;
        let plaintext = secret.expose_for_use();
        let dek = derive_pseudo_key(&(canonical.clone() + &version_id), 32);
        let wrapped_dek = xor_bytes(&dek, self.root_key.expose_for_use());
        let ciphertext = xor_bytes(plaintext, &dek);
        let digest = short_digest(&ciphertext);
        let locator = format!(
            "blob://vault/{}/{}/{}",
            self.active_backend, canonical, version_id
        );

        record.versions.push(StoredSecretVersionMaterial {
            version: VaultSecretVersionRecord {
                version_id: version_id.clone(),
                reference: canonical.clone(),
                version_seq,
                state: VaultSecretVersionState::Active,
                protector_binding: self.active_backend.clone(),
                ciphertext_locator: locator,
                ciphertext_digest: digest,
                content_format: canonical_parts.kind,
                created_by: actor.to_string(),
                created_at: now,
                activated_at: Some(now),
                superseded_at: None,
                revoked_at: None,
                destroy_after: None,
            },
            ciphertext,
            wrapped_dek,
        });

        record.record.label = label;
        record.record.active_version_id = Some(version_id);
        if version_seq > 1 {
            record.record.last_rotated_at = Some(now);
            record.record.rotation.rotation_count += 1;
            record.record.rotation.last_rotated_by = Some(actor.to_string());
            record.record.rotation.last_rotation_reason = rotation_reason;
        }

        if let Some(plain) = secret.expose_utf8_for_use() {
            self.redaction_registry.register(plain);
        }
        Ok(secret_record_from_metadata(
            &record.record,
            self.active_backend.clone(),
            version_seq,
        ))
    }

    pub fn get(&self, reference: &str) -> Result<Option<String>, VaultError> {
        let canonical = normalize_credential_ref(reference)?;
        if self.secrets.contains_key(&canonical) {
            return Err(VaultError::PlaintextAccessDisabled(
                "plaintext get() is disabled; use broker APIs or reveal_for_local_admin".into(),
            ));
        }
        Ok(None)
    }

    pub fn use_for_ssh_auth(
        &mut self,
        reference: &str,
        target_scope: impl Into<String>,
    ) -> Result<BrokeredSecretLease, VaultError> {
        self.issue_broker_lease(
            reference,
            BrokerUseCase::SshAuth {
                target_scope: target_scope.into(),
            },
        )
    }

    pub fn use_for_http_auth(
        &mut self,
        reference: &str,
        request_scope: impl Into<String>,
    ) -> Result<BrokeredSecretLease, VaultError> {
        self.issue_broker_lease(
            reference,
            BrokerUseCase::HttpAuth {
                request_scope: request_scope.into(),
            },
        )
    }

    pub fn use_for_signing(
        &mut self,
        reference: &str,
        signing_scope: impl Into<String>,
    ) -> Result<BrokeredSecretLease, VaultError> {
        self.issue_broker_lease(
            reference,
            BrokerUseCase::Signing {
                signing_scope: signing_scope.into(),
            },
        )
    }

    pub fn create_agent_token(
        &mut self,
        request: CreateAgentTokenRequest,
    ) -> Result<CreateAgentTokenResult, VaultError> {
        let label = request.label.trim();
        if label.is_empty() {
            return Err(VaultError::AgentTokenRejected(
                "token label must be non-empty".into(),
            ));
        }
        let created_by = request.created_by.trim();
        if created_by.is_empty() {
            return Err(VaultError::AgentTokenRejected(
                "created_by principal must be non-empty".into(),
            ));
        }

        self.next_agent_token_seq += 1;
        let seq = self.next_agent_token_seq;
        let token_id = format!("token-{seq:06}");
        let principal_id = format!("principal-{seq:06}");
        let now = SystemTime::now();
        let expires_at = request.expires_in.map(|ttl| now + ttl);
        let scope_profile =
            normalized_scope_profile(request.scope.scope_profile.as_deref(), Some("default-deny"));
        let scope_record = build_scope_record(
            &token_id,
            1,
            &scope_profile,
            TokenScopeStatus::Active,
            &request.scope,
            created_by,
            now,
            None,
        );

        let mut attempt = 0u32;
        let (plaintext_token, token_hash) = loop {
            let plaintext = format!(
                "agt_{}",
                hex_encode(&pseudo_random_bytes(
                    24,
                    &format!("agent-token:{token_id}:{attempt}")
                ))
            );
            let digest = short_digest(plaintext.as_bytes());
            if !self.token_hash_index.contains_key(&digest) {
                break (plaintext, digest);
            }
            attempt = attempt.saturating_add(1);
        };

        self.redaction_registry.register(&plaintext_token);
        self.token_hash_index
            .insert(token_hash.clone(), token_id.clone());
        self.agent_tokens.insert(
            token_id.clone(),
            AgentTokenRecord {
                token_id: token_id.clone(),
                principal_id,
                label: label.to_string(),
                status: AgentTokenStatus::Active,
                token_hash,
                hash_scheme: "siphash-64".into(),
                scope_profile: scope_profile.clone(),
                active_scope_version: 1,
                created_by: created_by.to_string(),
                created_at: now,
                last_used_at: None,
                expires_at,
                idle_timeout_sec: request.idle_timeout_sec,
                revoked_at: None,
                revoke_reason: None,
                parent_token_id: None,
                issued_via_attestation_id: request.attestation_id,
            },
        );
        self.token_scopes
            .insert(token_id.clone(), vec![scope_record]);

        let summary = self
            .agent_token_summary(&token_id)?
            .ok_or_else(|| VaultError::AgentTokenNotFound(token_id.clone()))?;
        Ok(CreateAgentTokenResult {
            plaintext_token,
            summary,
        })
    }

    pub fn list_agent_tokens(&mut self) -> Vec<AgentTokenSummary> {
        let mut token_ids = self.agent_tokens.keys().cloned().collect::<Vec<_>>();
        token_ids.sort();
        token_ids
            .iter()
            .filter_map(|token_id| self.agent_token_summary(token_id).ok().flatten())
            .collect()
    }

    pub fn revoke_agent_token(
        &mut self,
        token_id: &str,
        reason: Option<String>,
    ) -> Result<AgentTokenSummary, VaultError> {
        let now = SystemTime::now();
        let record = self
            .agent_tokens
            .get_mut(token_id)
            .ok_or_else(|| VaultError::AgentTokenNotFound(token_id.to_string()))?;
        refresh_agent_token_status(record, now);
        if !matches!(record.status, AgentTokenStatus::Revoked) {
            record.status = AgentTokenStatus::Revoked;
            record.revoked_at = Some(now);
            record.revoke_reason = reason;
        }
        self.agent_token_summary(token_id)?
            .ok_or_else(|| VaultError::AgentTokenNotFound(token_id.to_string()))
    }

    pub fn update_agent_token_scope(
        &mut self,
        request: UpdateAgentTokenScopeRequest,
    ) -> Result<AgentTokenSummary, VaultError> {
        let now = SystemTime::now();
        let token_id = request.token_id.clone();
        let token = self
            .agent_tokens
            .get_mut(&token_id)
            .ok_or_else(|| VaultError::AgentTokenNotFound(token_id.clone()))?;
        refresh_agent_token_status(token, now);
        if !matches!(token.status, AgentTokenStatus::Active) {
            return Err(VaultError::AgentTokenRejected(format!(
                "token {} is not active",
                token.token_id
            )));
        }

        let active_scope_version = token.active_scope_version;
        let next_scope_version = token.active_scope_version.saturating_add(1);
        let scope_profile = normalized_scope_profile(
            request.scope.scope_profile.as_deref(),
            Some(&token.scope_profile),
        );
        token.active_scope_version = next_scope_version;
        token.scope_profile = scope_profile.clone();
        if request.attestation_id.is_some() {
            token.issued_via_attestation_id = request.attestation_id.clone();
        }

        let scopes = self
            .token_scopes
            .get_mut(&token_id)
            .ok_or_else(|| VaultError::AgentTokenNotFound(token_id.clone()))?;
        for scope in scopes.iter_mut() {
            if scope.version == active_scope_version
                && matches!(scope.status, TokenScopeStatus::Active)
            {
                scope.status = TokenScopeStatus::Superseded;
                scope.superseded_at = Some(now);
                scope.change_reason = request.reason.clone();
            }
        }
        scopes.push(build_scope_record(
            &token_id,
            next_scope_version,
            &scope_profile,
            TokenScopeStatus::Active,
            &request.scope,
            &request.changed_by,
            now,
            request.reason,
        ));

        self.agent_token_summary(&token_id)?
            .ok_or_else(|| VaultError::AgentTokenNotFound(token_id))
    }

    pub fn authenticate_agent_token(&mut self, token: &str) -> Option<AuthenticatedAgentToken> {
        let token_hash = short_digest(token.as_bytes());
        let token_id = self.token_hash_index.get(&token_hash)?.clone();
        let now = SystemTime::now();
        let (principal_id, active_scope_version, scope_profile) = {
            let record = self.agent_tokens.get_mut(&token_id)?;
            refresh_agent_token_status(record, now);
            if !matches!(record.status, AgentTokenStatus::Active) {
                return None;
            }
            record.last_used_at = Some(now);
            (
                record.principal_id.clone(),
                record.active_scope_version,
                record.scope_profile.clone(),
            )
        };
        let scope = self
            .scope_for_version(&token_id, active_scope_version)?
            .clone();
        Some(AuthenticatedAgentToken {
            token_id,
            principal_id,
            active_scope_version,
            scope_profile,
            target_ids: scope.target_ids,
            tool_ids: scope.tool_ids,
            max_risk_envelope: scope.max_risk_envelope,
            allow_open_shell: scope.allow_open_shell,
            allow_write_shell_input: scope.allow_write_shell_input,
            allow_artifact_cross_principal: scope.allow_artifact_cross_principal,
            allow_delegation: scope.allow_delegation,
            allow_admin_actions: scope.allow_admin_actions,
        })
    }

    pub fn active_scope_record(&self, token_id: &str) -> Option<TokenScopeRecord> {
        let record = self.agent_tokens.get(token_id)?;
        self.scope_for_version(token_id, record.active_scope_version)
            .cloned()
    }

    pub fn token_scope_history(&self, token_id: &str) -> Vec<TokenScopeRecord> {
        self.token_scopes.get(token_id).cloned().unwrap_or_default()
    }

    fn agent_token_summary(
        &mut self,
        token_id: &str,
    ) -> Result<Option<AgentTokenSummary>, VaultError> {
        let now = SystemTime::now();
        let record = match self.agent_tokens.get_mut(token_id) {
            Some(record) => {
                refresh_agent_token_status(record, now);
                record.clone()
            }
            None => return Ok(None),
        };
        let scope = self
            .scope_for_version(token_id, record.active_scope_version)
            .ok_or_else(|| VaultError::AgentTokenNotFound(token_id.to_string()))?;
        Ok(Some(AgentTokenSummary {
            token_id: record.token_id,
            label: record.label,
            principal_summary: record.principal_id,
            status: record.status,
            scope_profile: record.scope_profile,
            target_scope_summary: format!("targets:{}", scope.target_ids.len()),
            active_scope_version: record.active_scope_version,
            created_at: record.created_at,
            last_used_at: record.last_used_at,
            expires_at: record.expires_at,
            revoked_at: record.revoked_at,
            revoke_reason: record.revoke_reason,
        }))
    }

    fn scope_for_version(&self, token_id: &str, version: u32) -> Option<&TokenScopeRecord> {
        self.token_scopes
            .get(token_id)?
            .iter()
            .find(|scope| scope.version == version)
    }

    pub fn create_local_admin_intent(
        &mut self,
        action_kind: LocalAdminActionKind,
        target_object_ref: &str,
        requested_by_principal: &str,
        ttl: Duration,
    ) -> Result<LocalAdminActionIntent, VaultError> {
        let canonical = normalize_credential_ref(target_object_ref)?;
        self.next_intent_seq += 1;
        let now = SystemTime::now();
        let intent = LocalAdminActionIntent {
            intent_id: format!("intent-{:06}", self.next_intent_seq),
            action_kind,
            target_object_ref: canonical.clone(),
            requested_by_principal: requested_by_principal.to_string(),
            requested_payload_digest: short_digest(canonical.as_bytes()),
            created_at: now,
            expires_at: now + ttl,
            status: LocalAdminIntentStatus::Pending,
        };
        self.intents
            .insert(intent.intent_id.clone(), intent.clone());
        Ok(intent)
    }

    pub fn verify_local_admin_intent(
        &mut self,
        intent_id: &str,
        verified_principal: &str,
        verification_method: &str,
        ttl: Duration,
    ) -> Result<LocalAdminAttestationRecord, VaultError> {
        let now = SystemTime::now();
        let intent = self
            .intents
            .get_mut(intent_id)
            .ok_or_else(|| VaultError::LocalAdminIntentMismatch(intent_id.to_string()))?;
        if intent.status != LocalAdminIntentStatus::Pending {
            return Err(VaultError::LocalAdminIntentMismatch(intent_id.to_string()));
        }
        if now > intent.expires_at {
            intent.status = LocalAdminIntentStatus::Expired;
            return Err(VaultError::LocalAdminIntentExpired(intent_id.to_string()));
        }
        if intent.requested_by_principal != verified_principal {
            return Err(VaultError::LocalAdminIntentMismatch(intent_id.to_string()));
        }

        intent.status = LocalAdminIntentStatus::Verified;
        self.next_attestation_seq += 1;
        let attestation = LocalAdminAttestationRecord {
            attestation_id: format!("attest-{:06}", self.next_attestation_seq),
            intent_id: intent_id.to_string(),
            verified_principal: verified_principal.to_string(),
            verification_method: verification_method.to_string(),
            issued_at: now,
            expires_at: now + ttl,
            consumed_at: None,
            status: LocalAdminAttestationStatus::Active,
        };
        self.attestations
            .insert(attestation.attestation_id.clone(), attestation.clone());
        Ok(attestation)
    }

    pub fn reveal_for_local_admin(
        &mut self,
        reference: &str,
        intent_id: &str,
        attestation_id: &str,
        requester_principal: &str,
    ) -> Result<SecretBytes, VaultError> {
        let canonical = normalize_credential_ref(reference)?;
        self.ensure_backend_allows_secret_use()?;
        let now = SystemTime::now();

        {
            let intent = self
                .intents
                .get_mut(intent_id)
                .ok_or_else(|| VaultError::LocalAdminIntentMismatch(intent_id.to_string()))?;
            if intent.target_object_ref != canonical {
                return Err(VaultError::LocalAdminIntentMismatch(intent_id.to_string()));
            }
            if intent.requested_by_principal != requester_principal {
                return Err(VaultError::LocalAdminIntentMismatch(intent_id.to_string()));
            }
            if now > intent.expires_at {
                intent.status = LocalAdminIntentStatus::Expired;
                return Err(VaultError::LocalAdminIntentExpired(intent_id.to_string()));
            }
            if intent.status != LocalAdminIntentStatus::Verified {
                return Err(VaultError::LocalAdminVerificationRequired(
                    "intent must be verified before reveal".into(),
                ));
            }
            if !matches!(
                intent.action_kind,
                LocalAdminActionKind::RevealSecret | LocalAdminActionKind::ExportSecret
            ) {
                return Err(VaultError::LocalAdminIntentMismatch(intent_id.to_string()));
            }
        }

        {
            let attestation = self.attestations.get_mut(attestation_id).ok_or_else(|| {
                VaultError::LocalAdminAttestationMismatch(attestation_id.to_string())
            })?;
            if attestation.intent_id != intent_id
                || attestation.verified_principal != requester_principal
            {
                return Err(VaultError::LocalAdminAttestationMismatch(
                    attestation_id.to_string(),
                ));
            }
            if now > attestation.expires_at {
                attestation.status = LocalAdminAttestationStatus::Expired;
                return Err(VaultError::LocalAdminAttestationExpired(
                    attestation_id.to_string(),
                ));
            }
            if attestation.status != LocalAdminAttestationStatus::Active {
                return Err(VaultError::LocalAdminAttestationMismatch(
                    attestation_id.to_string(),
                ));
            }
        }

        let secret = self.decrypt_active_secret(&canonical)?;
        if let Some(intent) = self.intents.get_mut(intent_id) {
            intent.status = LocalAdminIntentStatus::Consumed;
        }
        if let Some(attestation) = self.attestations.get_mut(attestation_id) {
            attestation.status = LocalAdminAttestationStatus::Consumed;
            attestation.consumed_at = Some(now);
        }
        Ok(secret)
    }

    pub fn export_for_local_admin(
        &mut self,
        reference: &str,
        intent_id: &str,
        attestation_id: &str,
        requester_principal: &str,
    ) -> Result<SecretBytes, VaultError> {
        self.reveal_for_local_admin(reference, intent_id, attestation_id, requester_principal)
    }

    pub fn secret_metadata(
        &self,
        reference: &str,
    ) -> Result<Option<VaultSecretMetadataView>, VaultError> {
        let canonical = normalize_credential_ref(reference)?;
        Ok(self
            .secrets
            .get(&canonical)
            .map(|state| VaultSecretMetadataView {
                record: state.record.clone(),
                versions: state
                    .versions
                    .iter()
                    .map(|value| value.version.clone())
                    .collect(),
            }))
    }

    pub fn key_envelope_record(&self) -> &VaultKeyEnvelopeRecord {
        &self.key_envelope
    }

    pub fn protector_wrap_manifests(&self) -> &[ProtectorWrapManifest] {
        &self.wrap_manifests
    }

    pub fn active_backend_diagnostics(&self) -> Result<VaultReadinessDiagnostic, VaultError> {
        let backend = self
            .backends
            .get(&self.active_backend)
            .ok_or_else(|| VaultError::BackendNotRegistered(self.active_backend.clone()))?;
        let status = backend.readiness();
        let fail_closed = self.should_fail_closed(status.clone());
        let protectors = self.protector_assembly_for_backend(&self.active_backend);
        Ok(VaultReadinessDiagnostic {
            backend: self.active_backend.clone(),
            status,
            message: backend.readiness_message().to_string(),
            fail_closed,
            protectors,
        })
    }

    pub fn protector_assembly(&self) -> Result<ProtectorAssembly, VaultError> {
        self.backends
            .get(&self.active_backend)
            .ok_or_else(|| VaultError::BackendNotRegistered(self.active_backend.clone()))?;
        Ok(ProtectorAssembly {
            primary: "os-native".into(),
            recovery: vec!["passphrase".into()],
            retired: vec!["legacy-memory-shim".into()],
            fail_closed: self
                .active_backend_diagnostics()
                .map(|diag| diag.fail_closed)
                .unwrap_or(true),
        })
    }

    pub fn ssh_agent_compatibility(
        &self,
        host_platform: &str,
    ) -> Vec<SshAgentCompatibilityDiagnostic> {
        ssh_agent_compatibility_for_host(host_platform)
    }

    pub fn prepare_ssh_agent_broker_session(
        &mut self,
        request: SshAgentBrokerPrepareRequest,
    ) -> Result<SshAgentBrokerPreparedSession, VaultError> {
        if request.runtime_passphrase_requested {
            return Err(VaultError::SshRuntimePassphraseForbidden(
                "runtime ssh key passphrase prompts are forbidden; passphrase must be handled during import/unlock".into(),
            ));
        }

        let canonical_ref = normalize_credential_ref(&request.credential_ref)?;
        self.ensure_backend_allows_secret_use()?;
        let _lease = self.use_for_ssh_auth(
            &canonical_ref,
            format!("target:{}:{}", request.target_id, request.principal_id),
        )?;

        let diagnostics = ssh_agent_compatibility_for_host(&request.host_platform);
        let preferred = diagnostics
            .first()
            .cloned()
            .unwrap_or(SshAgentCompatibilityDiagnostic {
                platform: normalize_platform_label(&request.host_platform).to_string(),
                endpoint_kind: SshAgentBrokerEndpointKind::IdentityFileFallback,
                status: VaultReadinessState::Unsupported,
                message: "host platform is not recognized for ssh-agent-compatible delivery".into(),
            });

        let now = SystemTime::now();
        let session_id = self.allocate_ssh_broker_session_id();
        let (endpoint_kind, degraded, endpoint_locator, mut warning_lines) = if matches!(
            preferred.status,
            VaultReadinessState::Ready
        ) {
            let endpoint = ssh_endpoint_locator_for_session(&session_id, &preferred.endpoint_kind);
            (
                preferred.endpoint_kind.clone(),
                false,
                Some(endpoint),
                vec![format!(
                    "ssh-agent-compatible delivery is ready on platform {} via {}",
                    preferred.platform,
                    preferred.endpoint_kind.as_str()
                )],
            )
        } else if request.allow_identity_fallback {
            let fallback_kind = SshAgentBrokerEndpointKind::IdentityFileFallback;
            let endpoint = ssh_endpoint_locator_for_session(&session_id, &fallback_kind);
            (
                    fallback_kind,
                    true,
                    Some(endpoint),
                    vec![
                        format!(
                            "ssh-agent-compatible delivery is {} on platform {}; falling back to ephemeral identity file",
                            readiness_label(&preferred.status),
                            preferred.platform
                        ),
                        preferred.message.clone(),
                    ],
                )
        } else {
            return Err(VaultError::SshBrokerUnsupported(format!(
                    "ssh-agent-compatible delivery is {} on platform {} and identity-file fallback is disabled",
                    readiness_label(&preferred.status),
                    preferred.platform
                )));
        };
        warning_lines.push(format!(
            "ssh key passphrase handling: {}",
            request.key_passphrase_handling.as_str()
        ));
        warning_lines.push(format!(
            "host key policy is enforced by ssh policy layer: {}",
            request.host_key_policy.as_str()
        ));

        let mut session = SshAgentBrokerSession {
            broker_session_id: session_id.clone(),
            target_id: request.target_id.clone(),
            credential_ref: canonical_ref,
            state: SshAgentBrokerSessionState::Preparing,
            endpoint_kind,
            degraded,
            created_at: now,
            expires_at: request.session_ttl.map(|ttl| now + ttl),
            attached_channel_count: 0,
            cleanup_deadline: None,
            host_key_policy: request.host_key_policy.clone(),
            key_passphrase_handling: request.key_passphrase_handling.clone(),
        };
        session.state = SshAgentBrokerSessionState::Ready;

        let ssh_option_args = ssh_delivery_option_args(
            session.endpoint_kind.clone(),
            endpoint_locator.as_deref(),
            &request.host_key_policy,
        );
        let summary = ssh_broker_session_summary(&session);
        self.ssh_broker_sessions.insert(
            session_id.clone(),
            StoredSshAgentBrokerSession {
                session,
                attached_channels: HashSet::new(),
                endpoint_locator,
                diagnostics: warning_lines.clone(),
                principal_id: request.principal_id,
                logical_session_id: request.logical_session_id,
            },
        );
        Ok(SshAgentBrokerPreparedSession {
            session: summary,
            ssh_option_args,
            diagnostics: warning_lines,
        })
    }

    pub fn attach_ssh_agent_broker_session(
        &mut self,
        broker_session_id: &str,
        channel_id: &str,
    ) -> Result<SshAgentBrokerSessionSummary, VaultError> {
        let now = SystemTime::now();
        let stored = self
            .ssh_broker_sessions
            .get_mut(broker_session_id)
            .ok_or_else(|| VaultError::SshBrokerSessionNotFound(broker_session_id.into()))?;
        if matches!(
            stored.session.state,
            SshAgentBrokerSessionState::Closed | SshAgentBrokerSessionState::Failed
        ) {
            return Err(VaultError::SshBrokerUnsupported(format!(
                "ssh broker session is not attachable in state {}",
                stored.session.state.as_str()
            )));
        }

        stored.session.state = SshAgentBrokerSessionState::Attached;
        stored.session.cleanup_deadline = None;
        stored.attached_channels.insert(channel_id.to_string());
        stored.session.attached_channel_count = stored.attached_channels.len() as u32;
        if let Some(expires_at) = stored.session.expires_at {
            if now > expires_at {
                stored.session.state = SshAgentBrokerSessionState::Failed;
                stored.session.cleanup_deadline = Some(now);
                return Err(VaultError::SshBrokerUnsupported(
                    "ssh broker session expired before channel attach".into(),
                ));
            }
        }
        Ok(ssh_broker_session_summary(&stored.session))
    }

    pub fn detach_ssh_agent_broker_session(
        &mut self,
        broker_session_id: &str,
        channel_id: &str,
    ) -> Result<SshAgentBrokerSessionSummary, VaultError> {
        let stored = self
            .ssh_broker_sessions
            .get_mut(broker_session_id)
            .ok_or_else(|| VaultError::SshBrokerSessionNotFound(broker_session_id.into()))?;
        stored.attached_channels.remove(channel_id);
        stored.session.attached_channel_count = stored.attached_channels.len() as u32;
        if stored.session.attached_channel_count == 0
            && matches!(stored.session.state, SshAgentBrokerSessionState::Attached)
        {
            stored.session.state = SshAgentBrokerSessionState::Draining;
            stored.session.cleanup_deadline = Some(SystemTime::now() + Duration::from_secs(5));
        }
        Ok(ssh_broker_session_summary(&stored.session))
    }

    pub fn close_ssh_agent_broker_session(
        &mut self,
        broker_session_id: &str,
        reason: &str,
    ) -> Result<SshAgentBrokerSessionSummary, VaultError> {
        let stored = self
            .ssh_broker_sessions
            .get_mut(broker_session_id)
            .ok_or_else(|| VaultError::SshBrokerSessionNotFound(broker_session_id.into()))?;
        stored.attached_channels.clear();
        stored.session.attached_channel_count = 0;
        stored.session.state = SshAgentBrokerSessionState::Closed;
        stored.session.cleanup_deadline = Some(SystemTime::now());
        stored.endpoint_locator = None;
        stored
            .diagnostics
            .push(format!("broker cleanup complete: {reason}"));
        Ok(ssh_broker_session_summary(&stored.session))
    }

    pub fn ssh_agent_broker_session_summary(
        &self,
        broker_session_id: &str,
    ) -> Option<SshAgentBrokerSessionSummary> {
        self.ssh_broker_sessions
            .get(broker_session_id)
            .map(|stored| ssh_broker_session_summary(&stored.session))
    }

    pub fn ssh_agent_broker_diagnostics(
        &self,
        broker_session_id: &str,
    ) -> Result<Vec<String>, VaultError> {
        let stored = self
            .ssh_broker_sessions
            .get(broker_session_id)
            .ok_or_else(|| VaultError::SshBrokerSessionNotFound(broker_session_id.into()))?;
        let mut lines = stored.diagnostics.clone();
        lines.push(format!("principal scope: {}", stored.principal_id));
        if let Some(logical) = stored.logical_session_id.as_deref() {
            lines.push(format!("logical session scope: {logical}"));
        }
        Ok(lines)
    }

    fn issue_broker_lease(
        &mut self,
        reference: &str,
        use_case: BrokerUseCase,
    ) -> Result<BrokeredSecretLease, VaultError> {
        let canonical = normalize_credential_ref(reference)?;
        self.ensure_backend_allows_secret_use()?;
        let secret = self.decrypt_active_secret(&canonical)?;
        let active_version_id = self
            .secrets
            .get(&canonical)
            .and_then(|value| value.record.active_version_id.clone())
            .ok_or_else(|| VaultError::SecretVersionNotFound(canonical.clone()))?;
        Ok(BrokeredSecretLease {
            reference: canonical.clone(),
            version_id: active_version_id,
            use_case,
            redacted_preview: command_audit_preview(&self.redaction_registry.redact(&canonical)),
            secret,
        })
    }

    fn decrypt_active_secret(
        &mut self,
        canonical_reference: &str,
    ) -> Result<SecretBytes, VaultError> {
        let now = SystemTime::now();
        let state = self
            .secrets
            .get_mut(canonical_reference)
            .ok_or_else(|| VaultError::SecretNotFound(canonical_reference.to_string()))?;
        let active_id =
            state.record.active_version_id.clone().ok_or_else(|| {
                VaultError::SecretVersionNotFound(canonical_reference.to_string())
            })?;
        let material = state
            .versions
            .iter()
            .find(|entry| entry.version.version_id == active_id)
            .ok_or_else(|| VaultError::SecretVersionNotFound(active_id.clone()))?;
        let dek = xor_bytes(&material.wrapped_dek, self.root_key.expose_for_use());
        let plaintext = xor_bytes(&material.ciphertext, &dek);
        state.record.last_used_at = Some(now);
        self.key_envelope.last_unlocked_at = Some(now);
        Ok(SecretBytes::from_bytes(plaintext))
    }

    fn ensure_backend_allows_secret_use(&self) -> Result<(), VaultError> {
        let diag = self.active_backend_diagnostics()?;
        if diag.fail_closed {
            return Err(VaultError::FailClosed(format!(
                "backend `{}` is `{}` and fail-closed policy blocks secret use",
                diag.backend,
                readiness_label(&diag.status)
            )));
        }
        Ok(())
    }

    fn should_fail_closed(&self, readiness: VaultReadinessState) -> bool {
        matches!(readiness, VaultReadinessState::Unsupported)
            || (matches!(readiness, VaultReadinessState::Degraded) && !self.allow_degraded_mode)
    }

    fn protector_assembly_for_backend(&self, backend: &str) -> Vec<ProtectorReadinessDiagnostic> {
        match backend {
            "builtin-encrypted" | "os-native" => vec![
                ProtectorReadinessDiagnostic {
                    protector: "os-native".into(),
                    status: VaultReadinessState::Degraded,
                    message: "primary protector is present but still degraded".into(),
                },
                ProtectorReadinessDiagnostic {
                    protector: "passphrase".into(),
                    status: VaultReadinessState::Fallback,
                    message: "recovery protector is available for headless flows".into(),
                },
            ],
            "file-vault" => vec![ProtectorReadinessDiagnostic {
                protector: "passphrase".into(),
                status: VaultReadinessState::Fallback,
                message: "file-vault requires explicit passphrase policy".into(),
            }],
            _ => vec![ProtectorReadinessDiagnostic {
                protector: "none".into(),
                status: VaultReadinessState::Unsupported,
                message: "no usable protector assembly".into(),
            }],
        }
    }

    fn allocate_version_id(&mut self) -> String {
        self.next_version_seq += 1;
        format!("ver-{:06}", self.next_version_seq)
    }

    fn allocate_audit_id(&mut self) -> String {
        self.next_audit_seq += 1;
        format!("audit-{:06}", self.next_audit_seq)
    }

    fn allocate_ssh_broker_session_id(&mut self) -> String {
        self.next_ssh_broker_seq += 1;
        format!("ssh-broker-{:06}", self.next_ssh_broker_seq)
    }
}

fn refresh_agent_token_status(record: &mut AgentTokenRecord, now: SystemTime) {
    if matches!(record.status, AgentTokenStatus::Active)
        && record
            .expires_at
            .map(|expires_at| now >= expires_at)
            .unwrap_or(false)
    {
        record.status = AgentTokenStatus::Expired;
    }
}

fn normalized_scope_profile(value: Option<&str>, fallback: Option<&str>) -> String {
    value
        .map(|raw| raw.trim())
        .filter(|raw| !raw.is_empty())
        .map(|raw| raw.to_ascii_lowercase())
        .or_else(|| {
            fallback
                .map(|raw| raw.trim())
                .filter(|raw| !raw.is_empty())
                .map(|raw| raw.to_ascii_lowercase())
        })
        .unwrap_or_else(|| "default-deny".to_string())
}

fn canonicalize_ids(values: &[String]) -> Vec<String> {
    let mut canonical = values
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    canonical.sort();
    canonical.dedup();
    canonical
}

fn build_scope_record(
    token_id: &str,
    version: u32,
    scope_profile: &str,
    status: TokenScopeStatus,
    input: &TokenScopeInput,
    actor: &str,
    now: SystemTime,
    reason: Option<String>,
) -> TokenScopeRecord {
    TokenScopeRecord {
        token_id: token_id.to_string(),
        version,
        status,
        scope_profile: scope_profile.to_string(),
        target_ids: canonicalize_ids(&input.target_ids),
        tool_ids: canonicalize_ids(&input.tool_ids),
        max_risk_envelope: input
            .max_risk_envelope
            .as_deref()
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "deny-all".to_string()),
        allow_open_shell: input.allow_open_shell.unwrap_or(false),
        allow_write_shell_input: input.allow_write_shell_input.unwrap_or(false),
        allow_artifact_cross_principal: input.allow_artifact_cross_principal.unwrap_or(false),
        allow_delegation: input.allow_delegation.unwrap_or(false),
        allow_admin_actions: input.allow_admin_actions.unwrap_or(false),
        created_by: actor.to_string(),
        created_at: now,
        superseded_at: None,
        change_reason: reason,
    }
}

fn readiness_label(status: &VaultReadinessState) -> &'static str {
    match status {
        VaultReadinessState::Ready => "ready",
        VaultReadinessState::Fallback => "fallback",
        VaultReadinessState::Degraded => "degraded",
        VaultReadinessState::Unsupported => "unsupported",
    }
}

fn secret_record_from_metadata(
    record: &VaultSecretRecord,
    backend: String,
    version: u32,
) -> SecretRecord {
    SecretRecord {
        reference: record.reference.clone(),
        label: record.label.clone(),
        backend,
        kind: record.kind.clone(),
        version,
        status: record.status.clone(),
        created_by: record.created_by.clone(),
        created_at: record.created_at,
        last_rotated_at: record.last_rotated_at,
        audit_chain_id: record.audit_chain_id.clone(),
    }
}

fn normalize_ref_segment(raw: &str) -> Result<String, VaultError> {
    let normalized = raw.trim().to_ascii_lowercase().replace('_', "-");
    if normalized.is_empty() {
        return Err(VaultError::InvalidCredentialRef(raw.to_string()));
    }
    if normalized
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '-' | '.'))
    {
        return Ok(normalized);
    }
    Err(VaultError::InvalidCredentialRef(raw.to_string()))
}

fn normalize_kind_alias(raw: &str) -> String {
    let normalized = raw.trim().to_ascii_lowercase().replace('_', "-");
    match normalized.as_str() {
        "ssh" | "ssh-key" | "ssh-private" => "ssh-private-key".into(),
        "token" | "grok-token" | "api-token" => "opaque-token".into(),
        "agent-token" | "model-token" => "model-plane-token".into(),
        _ => normalized,
    }
}

fn normalize_platform_label(raw: &str) -> &str {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "macos" | "darwin" => "macos",
        "windows" | "win32" => "windows",
        "openharmony" | "ohos" | "open-harmony" => "openharmony",
        "linux" => "linux",
        "unix" => "unix",
        _ => "unknown",
    }
}

fn ssh_agent_compatibility_for_host(host_platform: &str) -> Vec<SshAgentCompatibilityDiagnostic> {
    let platform = normalize_platform_label(host_platform).to_string();
    match platform.as_str() {
        "macos" | "linux" | "unix" => vec![SshAgentCompatibilityDiagnostic {
            platform,
            endpoint_kind: SshAgentBrokerEndpointKind::UnixSocket,
            status: VaultReadinessState::Ready,
            message: "OpenSSH-compatible unix socket delivery is available".into(),
        }],
        "windows" => vec![SshAgentCompatibilityDiagnostic {
            platform,
            endpoint_kind: SshAgentBrokerEndpointKind::NamedPipe,
            status: VaultReadinessState::Degraded,
            message: "named pipe endpoint semantics are defined; OpenSSH compatibility requires platform-specific broker wiring".into(),
        }],
        "openharmony" => vec![SshAgentCompatibilityDiagnostic {
            platform,
            endpoint_kind: SshAgentBrokerEndpointKind::PlatformLocalEndpoint,
            status: VaultReadinessState::Degraded,
            message:
                "OpenHarmony PC agent-compatible endpoint semantics are pending platform spike".into(),
        }],
        _ => vec![SshAgentCompatibilityDiagnostic {
            platform,
            endpoint_kind: SshAgentBrokerEndpointKind::IdentityFileFallback,
            status: VaultReadinessState::Unsupported,
            message:
                "host platform has no verified ssh-agent-compatible endpoint in current runtime".into(),
        }],
    }
}

fn ssh_endpoint_locator_for_session(
    session_id: &str,
    endpoint_kind: &SshAgentBrokerEndpointKind,
) -> String {
    match endpoint_kind {
        SshAgentBrokerEndpointKind::UnixSocket => {
            format!("/tmp/bridgingio-{session_id}.sock")
        }
        SshAgentBrokerEndpointKind::NamedPipe => {
            format!(r"\\.\pipe\bridgingio-{session_id}-ssh-agent")
        }
        SshAgentBrokerEndpointKind::PlatformLocalEndpoint => {
            format!("platform://bridgingio/{session_id}/ssh-agent")
        }
        SshAgentBrokerEndpointKind::IdentityFileFallback => {
            format!("/tmp/bridgingio-{session_id}.identity")
        }
    }
}

fn ssh_delivery_option_args(
    endpoint_kind: SshAgentBrokerEndpointKind,
    endpoint_locator: Option<&str>,
    host_key_policy: &SshHostKeyPolicy,
) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(endpoint) = endpoint_locator {
        match endpoint_kind {
            SshAgentBrokerEndpointKind::IdentityFileFallback => {
                args.push("-i".into());
                args.push(endpoint.to_string());
            }
            _ => {
                args.push("-o".into());
                args.push(format!("IdentityAgent={endpoint}"));
            }
        }
    }

    match host_key_policy {
        SshHostKeyPolicy::Strict => {}
        SshHostKeyPolicy::AcceptNew => {
            args.push("-o".into());
            args.push("StrictHostKeyChecking=accept-new".into());
        }
        SshHostKeyPolicy::InsecureNoCheck => {
            args.push("-o".into());
            args.push("StrictHostKeyChecking=no".into());
            args.push("-o".into());
            args.push(format!("UserKnownHostsFile={}", known_hosts_null_device()));
        }
    }
    args
}

fn known_hosts_null_device() -> &'static str {
    #[cfg(windows)]
    {
        "NUL"
    }
    #[cfg(not(windows))]
    {
        "/dev/null"
    }
}

fn ssh_broker_session_summary(session: &SshAgentBrokerSession) -> SshAgentBrokerSessionSummary {
    SshAgentBrokerSessionSummary {
        broker_session_id: session.broker_session_id.clone(),
        target_id: session.target_id.clone(),
        credential_ref: session.credential_ref.clone(),
        state: session.state.clone(),
        endpoint_kind: session.endpoint_kind.clone(),
        degraded: session.degraded,
        created_at: session.created_at,
        expires_at: session.expires_at,
        attached_channel_count: session.attached_channel_count,
    }
}

fn xor_bytes(input: &[u8], key: &[u8]) -> Vec<u8> {
    if key.is_empty() {
        return input.to_vec();
    }
    input
        .iter()
        .enumerate()
        .map(|(index, byte)| byte ^ key[index % key.len()])
        .collect()
}

fn derive_pseudo_key(context: &str, len: usize) -> Vec<u8> {
    let mut output = Vec::with_capacity(len);
    let mut seen = HashSet::<u64>::new();
    let mut counter = 0u64;
    while output.len() < len {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        context.hash(&mut hasher);
        counter.hash(&mut hasher);
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or(0)
            .hash(&mut hasher);
        let digest = hasher.finish();
        if seen.insert(digest) {
            output.extend_from_slice(&digest.to_le_bytes());
        }
        counter += 1;
    }
    output.truncate(len);
    output
}

fn pseudo_random_bytes(len: usize, context: &str) -> Vec<u8> {
    derive_pseudo_key(context, len)
}

fn short_digest(data: &[u8]) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn hex_encode(data: &[u8]) -> String {
    const LUT: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(data.len() * 2);
    for byte in data {
        out.push(LUT[(byte >> 4) as usize] as char);
        out.push(LUT[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        command_audit_preview, normalize_credential_ref, AgentTokenStatus, CreateAgentTokenRequest,
        LocalAdminActionKind, SecretBytes, SecretVaultRouter, SshAgentBrokerPrepareRequest,
        SshAgentBrokerSessionState, SshHostKeyPolicy, SshKeyPassphraseHandling, TokenScopeInput,
        UpdateAgentTokenScopeRequest, VaultError, VaultReadinessState,
    };
    use std::time::Duration;

    #[test]
    fn normalizes_legacy_credential_refs_to_canonical_uri() {
        let canonical = normalize_credential_ref("vault:ssh-key:ops-prod").expect("canonical");
        assert_eq!(canonical, "vault://bridgingio/ssh-private-key/ops-prod");

        let namespaced = normalize_credential_ref("vault:infra:token:grok_default")
            .expect("canonical namespaced");
        assert_eq!(namespaced, "vault://infra/opaque-token/grok-default");
    }

    #[test]
    fn builds_secret_metadata_and_version_state_for_rotation() {
        let mut router = SecretVaultRouter::default();
        router
            .set_active_backend("builtin-encrypted")
            .expect("switch backend");
        router
            .put("vault:ssh-key:lab", "secret-v1", "lab key")
            .expect("v1");
        router
            .put("vault:ssh-key:lab", "secret-v2", "lab key")
            .expect("v2");

        let view = router
            .secret_metadata("vault://bridgingio/ssh-private-key/lab")
            .expect("metadata")
            .expect("existing secret");
        assert_eq!(view.versions.len(), 2);
        assert_eq!(view.record.rotation.rotation_count, 1);
        assert!(view.record.active_version_id.is_some());
        assert_eq!(
            view.versions[0].state,
            super::VaultSecretVersionState::Superseded
        );
        assert_eq!(
            view.versions[1].state,
            super::VaultSecretVersionState::Active
        );
    }

    #[test]
    fn exposes_key_envelope_and_wrap_metadata_for_layered_encryption() {
        let mut router = SecretVaultRouter::default();
        router
            .set_active_backend("builtin-encrypted")
            .expect("switch backend");
        router
            .put(
                "vault://bridgingio/opaque-token/default",
                "TOKEN-VALUE",
                "default token",
            )
            .expect("store token");
        let key = router.key_envelope_record();
        assert_eq!(key.state, super::VaultKeyEnvelopeState::Active);
        assert!(!router.protector_wrap_manifests().is_empty());

        let view = router
            .secret_metadata("vault://bridgingio/opaque-token/default")
            .expect("metadata")
            .expect("secret");
        let version = &view.versions[0];
        assert!(version.ciphertext_locator.starts_with("blob://vault/"));
        assert!(!version.ciphertext_digest.is_empty());
    }

    #[test]
    fn fail_closed_blocks_degraded_backend_unless_explicitly_enabled() {
        let mut router = SecretVaultRouter::default();
        let err = router
            .put("vault:ssh-key:dev", "PRIVATE_KEY", "dev key")
            .expect_err("os-native degraded backend should fail-closed by default");
        assert!(matches!(err, VaultError::FailClosed(_)));

        router.set_allow_degraded_mode(true);
        let record = router
            .put("vault:ssh-key:dev", "PRIVATE_KEY", "dev key")
            .expect("allow degraded mode");
        assert_eq!(record.backend, "os-native");
    }

    #[test]
    fn exposes_readiness_and_protector_diagnostics() {
        let router = SecretVaultRouter::default();
        let diag = router.active_backend_diagnostics().expect("diag");
        assert_eq!(diag.status, VaultReadinessState::Degraded);
        assert!(diag.fail_closed);
        assert!(!diag.protectors.is_empty());
    }

    #[test]
    fn broker_path_replaces_plaintext_get() {
        let mut router = SecretVaultRouter::default();
        router
            .set_active_backend("builtin-encrypted")
            .expect("switch backend");
        router
            .put(
                "vault:grok-token:default",
                "grok-secret-token",
                "grok token",
            )
            .expect("store");

        let get_err = router
            .get("vault:grok-token:default")
            .expect_err("plaintext get must be disabled");
        assert!(matches!(get_err, VaultError::PlaintextAccessDisabled(_)));

        let lease = router
            .use_for_http_auth("vault:grok-token:default", "provider:grok")
            .expect("lease");
        let secret =
            lease.with_secret_bytes(|bytes| std::str::from_utf8(bytes).expect("utf8").to_string());
        assert_eq!(secret, "grok-secret-token");
    }

    #[test]
    fn reveal_requires_verified_local_admin_intent_and_single_use_attestation() {
        let mut router = SecretVaultRouter::default();
        router
            .set_active_backend("builtin-encrypted")
            .expect("switch backend");
        router
            .put("vault:ssh-key:ops", "OPS-KEY", "ops key")
            .expect("store");

        let intent = router
            .create_local_admin_intent(
                LocalAdminActionKind::RevealSecret,
                "vault:ssh-key:ops",
                "alice",
                Duration::from_secs(60),
            )
            .expect("intent");
        let attestation = router
            .verify_local_admin_intent(
                &intent.intent_id,
                "alice",
                "passkey",
                Duration::from_secs(30),
            )
            .expect("attestation");

        let reveal = router
            .reveal_for_local_admin(
                "vault:ssh-key:ops",
                &intent.intent_id,
                &attestation.attestation_id,
                "alice",
            )
            .expect("reveal");
        assert_eq!(reveal.expose_utf8_for_use(), Some("OPS-KEY"));

        let second = router.reveal_for_local_admin(
            "vault:ssh-key:ops",
            &intent.intent_id,
            &attestation.attestation_id,
            "alice",
        );
        assert!(matches!(
            second,
            Err(VaultError::LocalAdminVerificationRequired(_))
                | Err(VaultError::LocalAdminAttestationMismatch(_))
        ));
    }

    #[test]
    fn redaction_registry_masks_registered_sensitive_values() {
        let mut router = SecretVaultRouter::default();
        router
            .redaction_registry_mut()
            .register("secret-token-123456");
        let redacted = router
            .redaction_registry()
            .redact("Authorization: Bearer secret-token-123456");
        assert_eq!(redacted, "Authorization: Bearer [REDACTED]");
    }

    #[test]
    fn command_preview_uses_digest_without_raw_command_text() {
        let preview = command_audit_preview("curl -H 'Authorization: Bearer foo' https://api");
        assert!(preview.contains("cmd#"));
        assert!(preview.contains("len="));
        assert!(!preview.contains("Authorization"));
    }

    #[test]
    fn ssh_broker_session_supports_attach_detach_and_cleanup() {
        let mut router = SecretVaultRouter::default();
        router
            .set_active_backend("builtin-encrypted")
            .expect("switch backend");
        router
            .put("vault:ssh-key:ops", "OPS-KEY", "ops key")
            .expect("store");

        let prepared = router
            .prepare_ssh_agent_broker_session(SshAgentBrokerPrepareRequest {
                target_id: "target-ops".into(),
                credential_ref: "vault:ssh-key:ops".into(),
                principal_id: "agent-a".into(),
                logical_session_id: Some("ls-001".into()),
                host_platform: "macos".into(),
                allow_identity_fallback: true,
                host_key_policy: SshHostKeyPolicy::Strict,
                key_passphrase_handling: SshKeyPassphraseHandling::ImportTimeOnly,
                runtime_passphrase_requested: false,
                session_ttl: Some(Duration::from_secs(30)),
            })
            .expect("prepare broker");
        assert_eq!(prepared.session.state, SshAgentBrokerSessionState::Ready);
        assert!(!prepared.session.degraded);
        assert!(prepared
            .ssh_option_args
            .iter()
            .any(|value| value.contains("IdentityAgent=")));

        let attached = router
            .attach_ssh_agent_broker_session(&prepared.session.broker_session_id, "channel-1")
            .expect("attach");
        assert_eq!(attached.state, SshAgentBrokerSessionState::Attached);
        assert_eq!(attached.attached_channel_count, 1);

        let detached = router
            .detach_ssh_agent_broker_session(&prepared.session.broker_session_id, "channel-1")
            .expect("detach");
        assert_eq!(detached.state, SshAgentBrokerSessionState::Draining);
        assert_eq!(detached.attached_channel_count, 0);

        let closed = router
            .close_ssh_agent_broker_session(&prepared.session.broker_session_id, "one-shot done")
            .expect("close");
        assert_eq!(closed.state, SshAgentBrokerSessionState::Closed);
    }

    #[test]
    fn ssh_broker_can_fallback_to_identity_file_with_degraded_diagnostics() {
        let mut router = SecretVaultRouter::default();
        router
            .set_active_backend("builtin-encrypted")
            .expect("switch backend");
        router
            .put("vault:ssh-key:ops", "OPS-KEY", "ops key")
            .expect("store");

        let prepared = router
            .prepare_ssh_agent_broker_session(SshAgentBrokerPrepareRequest {
                target_id: "target-ops".into(),
                credential_ref: "vault:ssh-key:ops".into(),
                principal_id: "agent-a".into(),
                logical_session_id: None,
                host_platform: "unknown".into(),
                allow_identity_fallback: true,
                host_key_policy: SshHostKeyPolicy::AcceptNew,
                key_passphrase_handling: SshKeyPassphraseHandling::RuntimePromptForbidden,
                runtime_passphrase_requested: false,
                session_ttl: None,
            })
            .expect("prepare fallback");
        assert!(prepared.session.degraded);
        assert!(prepared.ssh_option_args.len() >= 2);
        assert_eq!(prepared.ssh_option_args[0], "-i");
        assert!(prepared
            .diagnostics
            .iter()
            .any(|line| line.contains("falling back to ephemeral identity file")));
    }

    #[test]
    fn ssh_broker_rejects_runtime_key_passphrase_prompt() {
        let mut router = SecretVaultRouter::default();
        router
            .set_active_backend("builtin-encrypted")
            .expect("switch backend");
        router
            .put("vault:ssh-key:ops", "OPS-KEY", "ops key")
            .expect("store");

        let err = router
            .prepare_ssh_agent_broker_session(SshAgentBrokerPrepareRequest {
                target_id: "target-ops".into(),
                credential_ref: "vault:ssh-key:ops".into(),
                principal_id: "agent-a".into(),
                logical_session_id: None,
                host_platform: "macos".into(),
                allow_identity_fallback: true,
                host_key_policy: SshHostKeyPolicy::Strict,
                key_passphrase_handling: SshKeyPassphraseHandling::RuntimePromptForbidden,
                runtime_passphrase_requested: true,
                session_ttl: None,
            })
            .expect_err("must reject runtime passphrase prompt");
        assert!(matches!(err, VaultError::SshRuntimePassphraseForbidden(_)));
    }

    #[test]
    fn ssh_agent_compatibility_matrix_covers_windows_and_openharmony() {
        let router = SecretVaultRouter::default();
        let windows = router.ssh_agent_compatibility("windows");
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].status, VaultReadinessState::Degraded);

        let openharmony = router.ssh_agent_compatibility("openharmony");
        assert_eq!(openharmony.len(), 1);
        assert_eq!(openharmony[0].status, VaultReadinessState::Degraded);
    }

    #[test]
    fn agent_token_create_list_revoke_contract() {
        let mut router = SecretVaultRouter::default();
        let created = router
            .create_agent_token(CreateAgentTokenRequest {
                label: "nightly-runner".into(),
                created_by: "local-operator".into(),
                expires_in: None,
                idle_timeout_sec: None,
                scope: TokenScopeInput {
                    scope_profile: Some("strict-default".into()),
                    target_ids: vec!["TARGET-A".into(), "target-a".into(), "target-b".into()],
                    tool_ids: Vec::new(),
                    max_risk_envelope: None,
                    allow_open_shell: None,
                    allow_write_shell_input: None,
                    allow_artifact_cross_principal: None,
                    allow_delegation: None,
                    allow_admin_actions: None,
                },
                attestation_id: None,
            })
            .expect("create token");
        assert!(created.plaintext_token.starts_with("agt_"));
        assert_eq!(created.summary.status, AgentTokenStatus::Active);

        let listed = router.list_agent_tokens();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].token_id, created.summary.token_id);
        assert_eq!(listed[0].target_scope_summary, "targets:2");

        let revoked = router
            .revoke_agent_token(&created.summary.token_id, Some("user delete".into()))
            .expect("revoke");
        assert_eq!(revoked.status, AgentTokenStatus::Revoked);
        assert_eq!(revoked.revoke_reason.as_deref(), Some("user delete"));
    }

    #[test]
    fn agent_token_plaintext_only_on_create_and_not_returned_by_listing() {
        let mut router = SecretVaultRouter::default();
        let created = router
            .create_agent_token(CreateAgentTokenRequest {
                label: "ci-runner".into(),
                created_by: "local-operator".into(),
                expires_in: None,
                idle_timeout_sec: None,
                scope: TokenScopeInput {
                    scope_profile: None,
                    target_ids: vec!["local-ssh".into()],
                    tool_ids: Vec::new(),
                    max_risk_envelope: None,
                    allow_open_shell: Some(false),
                    allow_write_shell_input: Some(false),
                    allow_artifact_cross_principal: Some(false),
                    allow_delegation: Some(false),
                    allow_admin_actions: Some(false),
                },
                attestation_id: None,
            })
            .expect("create token");

        let listed = router.list_agent_tokens();
        assert_eq!(listed.len(), 1);
        assert_ne!(listed[0].token_id, created.plaintext_token);
        assert!(router
            .authenticate_agent_token(&created.plaintext_token)
            .is_some());
    }

    #[test]
    fn agent_token_scope_update_creates_new_version_and_preserves_superseded_history() {
        let mut router = SecretVaultRouter::default();
        let created = router
            .create_agent_token(CreateAgentTokenRequest {
                label: "scope-updater".into(),
                created_by: "local-operator".into(),
                expires_in: None,
                idle_timeout_sec: None,
                scope: TokenScopeInput {
                    scope_profile: Some("strict-default".into()),
                    target_ids: vec!["target-a".into()],
                    tool_ids: Vec::new(),
                    max_risk_envelope: None,
                    allow_open_shell: None,
                    allow_write_shell_input: None,
                    allow_artifact_cross_principal: None,
                    allow_delegation: None,
                    allow_admin_actions: None,
                },
                attestation_id: None,
            })
            .expect("create token");

        let updated = router
            .update_agent_token_scope(UpdateAgentTokenScopeRequest {
                token_id: created.summary.token_id.clone(),
                changed_by: "local-operator".into(),
                scope: TokenScopeInput {
                    scope_profile: Some("strict-default".into()),
                    target_ids: vec!["target-a".into(), "target-b".into()],
                    tool_ids: Vec::new(),
                    max_risk_envelope: Some("deny-all".into()),
                    allow_open_shell: Some(false),
                    allow_write_shell_input: Some(false),
                    allow_artifact_cross_principal: Some(false),
                    allow_delegation: Some(false),
                    allow_admin_actions: Some(false),
                },
                reason: Some("expand targets".into()),
                attestation_id: Some("attest-001".into()),
            })
            .expect("update scope");
        assert_eq!(updated.active_scope_version, 2);

        let scopes = router.token_scope_history(&created.summary.token_id);
        assert_eq!(scopes.len(), 2);
        assert!(scopes.iter().any(|scope| scope.version == 1
            && matches!(scope.status, super::TokenScopeStatus::Superseded)));
        assert!(scopes
            .iter()
            .any(|scope| scope.version == 2
                && matches!(scope.status, super::TokenScopeStatus::Active)));
    }

    #[test]
    fn agent_token_lifetime_expiration_blocks_authentication() {
        let mut router = SecretVaultRouter::default();
        let created = router
            .create_agent_token(CreateAgentTokenRequest {
                label: "expiring-token".into(),
                created_by: "local-operator".into(),
                expires_in: Some(Duration::from_millis(1)),
                idle_timeout_sec: None,
                scope: TokenScopeInput {
                    scope_profile: None,
                    target_ids: vec!["local-ssh".into()],
                    tool_ids: Vec::new(),
                    max_risk_envelope: None,
                    allow_open_shell: None,
                    allow_write_shell_input: None,
                    allow_artifact_cross_principal: None,
                    allow_delegation: None,
                    allow_admin_actions: None,
                },
                attestation_id: None,
            })
            .expect("create token");
        std::thread::sleep(Duration::from_millis(5));
        assert!(router
            .authenticate_agent_token(&created.plaintext_token)
            .is_none());
        let listed = router.list_agent_tokens();
        assert_eq!(listed[0].status, AgentTokenStatus::Expired);
    }

    #[test]
    fn secret_container_is_not_copyable_and_still_readable_for_controlled_use() {
        let secret = SecretBytes::from_utf8("hello");
        assert_eq!(secret.expose_utf8_for_use(), Some("hello"));
    }

    #[test]
    fn errors_for_unknown_backend() {
        let mut router = SecretVaultRouter::default();
        let err = router
            .set_active_backend("future-vault")
            .expect_err("must fail");
        assert_eq!(err, VaultError::BackendNotRegistered("future-vault".into()));
    }
}
