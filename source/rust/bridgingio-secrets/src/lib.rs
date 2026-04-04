use std::collections::{HashMap, HashSet};
use std::cell::RefCell;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Condvar, Mutex, OnceLock};
#[cfg(test)]
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use bridgingio_domain::{CommonErrorCode, ContractStatus, ErrorDomain, SharedError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const DEFAULT_CREDENTIAL_NAMESPACE: &str = "bridgingio";
const CANONICAL_METADATA_SCHEMA_VERSION: u32 = 2;
const LEGACY_METADATA_SCHEMA_VERSION: u32 = 1;
const VAULT_OBJECT_FORMAT_VERSION: u32 = 1;
const METADATA_DB_FILENAME: &str = "metadata.db";
const LEGACY_METADATA_FILENAME: &str = "legacy-vault-state.json";
const PASSPHRASE_KDF_FORMAT_VERSION: u32 = 1;
const LOCAL_ADMIN_INTENT_ID_PREFIX: &str = "intent-";
const LOCAL_ADMIN_ATTESTATION_ID_PREFIX: &str = "attest-";
const LOCAL_ADMIN_CREATE_TOKEN_TARGET: &str = "token-authority://create-agent-token";
const LOCAL_ADMIN_UNLOCK_VAULT_TARGET: &str = "vault://runtime/unlock";
const LOCAL_ADMIN_DELETE_VAULT_TARGET: &str = "vault://runtime/delete";

pub fn local_admin_create_token_target() -> &'static str {
    LOCAL_ADMIN_CREATE_TOKEN_TARGET
}

pub fn local_admin_unlock_vault_target() -> &'static str {
    LOCAL_ADMIN_UNLOCK_VAULT_TARGET
}

pub fn local_admin_delete_vault_target() -> &'static str {
    LOCAL_ADMIN_DELETE_VAULT_TARGET
}

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

pub fn canonical_ssh_private_key_ref_from_key_name(raw: &str) -> Result<String, VaultError> {
    let canonical = canonical_ssh_private_key_name(raw)?;
    Ok(format!(
        "vault://{}/ssh-private-key/{}",
        DEFAULT_CREDENTIAL_NAMESPACE, canonical
    ))
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VaultSecretStatus {
    Active,
    Disabled,
    ScheduledDelete,
    Deleted,
}

impl VaultSecretStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
            Self::ScheduledDelete => "scheduled-delete",
            Self::Deleted => "deleted",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VaultSecretVersionState {
    Pending,
    Active,
    Superseded,
    Revoked,
    Destroyed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretRotationState {
    pub rotation_count: u32,
    pub last_rotated_by: Option<String>,
    pub last_rotation_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultSecretRecord {
    pub format_version: u32,
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
    pub format_version: u32,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VaultKeyEnvelopeState {
    Active,
    Rewrapping,
    Retired,
    Destroyed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultKeyEnvelopeRecord {
    pub format_version: u32,
    pub vault_key_id: String,
    pub state: VaultKeyEnvelopeState,
    pub active_wrap_set_id: String,
    pub created_at: SystemTime,
    pub rotated_at: Option<SystemTime>,
    pub last_unlocked_at: Option<SystemTime>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtectorWrapStatus {
    Ready,
    Fallback,
    Degraded,
    Unavailable,
    Retired,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtectorWrapManifest {
    pub format_version: u32,
    pub wrap_id: String,
    pub vault_key_id: String,
    pub protector_binding: String,
    pub wrap_format: String,
    pub wrapped_key_locator: String,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentTokenStatus {
    Active,
    Disabled,
    Revoked,
    Expired,
    Deleted,
}

impl AgentTokenStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
            Self::Revoked => "revoked",
            Self::Expired => "expired",
            Self::Deleted => "deleted",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    pub access_enabled: bool,
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
    pub deleted_at: Option<SystemTime>,
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
    pub token_fingerprint: String,
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
    pub deleted_at: Option<SystemTime>,
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
pub struct DeleteAgentTokenRequest {
    pub token_id: String,
    pub requested_by: String,
    pub attestation_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateAgentTokenLabelRequest {
    pub token_id: String,
    pub label: String,
    pub changed_by: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateAgentTokenAccessRequest {
    pub token_id: String,
    pub enabled: bool,
    pub changed_by: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnlockVaultRequest {
    pub method: String,
    pub passphrase: Option<String>,
    pub requested_by: String,
    pub attestation_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeleteVaultRequest {
    pub requested_by: String,
    pub attestation_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateAgentTokenResult {
    pub plaintext_token: String,
    pub summary: AgentTokenSummary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthenticatedAgentToken {
    pub token_id: String,
    pub label: String,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentTokenAuthRejectReason {
    Invalid,
    Disabled,
    Revoked,
    Expired,
}

impl AgentTokenAuthRejectReason {
    pub fn module_code(self) -> &'static str {
        match self {
            Self::Invalid => "agent_token_invalid",
            Self::Disabled => "agent_token_disabled",
            Self::Revoked => "agent_token_revoked",
            Self::Expired => "agent_token_expired",
        }
    }

    pub fn display_message(self) -> &'static str {
        match self {
            Self::Invalid => "Bearer token is invalid.",
            Self::Disabled => "Bearer token is disabled by local operator policy.",
            Self::Revoked => "Bearer token has been revoked.",
            Self::Expired => "Bearer token has expired.",
        }
    }

    pub fn recovery_hint(self) -> &'static str {
        match self {
            Self::Invalid => "Use a valid bearer token and retry.",
            Self::Disabled => "Ask a local operator to re-enable this token or issue a new one.",
            Self::Revoked => "Issue a new token from a trusted local management surface.",
            Self::Expired => "Issue a fresh token and retry the request.",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentTokenAuthResult {
    Authenticated(AuthenticatedAgentToken),
    Rejected(AgentTokenAuthRejectReason),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultSecretMetadataView {
    pub record: VaultSecretRecord,
    pub versions: Vec<VaultSecretVersionRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultSecretSummary {
    pub reference: String,
    pub kind: String,
    pub label: String,
    pub status: VaultSecretStatus,
    pub active_version_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VaultReadinessState {
    Ready,
    Fallback,
    Degraded,
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VaultLockState {
    Uninitialized,
    Locked,
    Unlocking,
    Unlocked,
    Unavailable,
}

impl VaultLockState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Uninitialized => "uninitialized",
            Self::Locked => "locked",
            Self::Unlocking => "unlocking",
            Self::Unlocked => "unlocked",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VaultUnlockTriggerPolicy {
    OnCoreStart,
    OnFirstSecretAccess,
    OnEverySecretAccess,
    ManualOnly,
}

impl VaultUnlockTriggerPolicy {
    fn as_str(&self) -> &'static str {
        match self {
            Self::OnCoreStart => "on-core-start",
            Self::OnFirstSecretAccess => "on-first-secret-access",
            Self::OnEverySecretAccess => "on-every-secret-access",
            Self::ManualOnly => "manual-only",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultUnlockPolicy {
    pub trigger_policy: VaultUnlockTriggerPolicy,
    pub allowed_methods: Vec<String>,
    pub preferred_method: String,
    pub cache_ttl_sec: u64,
    pub require_fresh_user_verification: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassphraseKdfParams {
    pub kdf: String,
    pub format_version: u32,
    pub m_cost_kib: u32,
    pub t_cost: u32,
    pub p_cost: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassphraseProtectorState {
    pub enabled: bool,
    pub wrap_id: String,
    pub salt_locator: String,
    pub params: PassphraseKdfParams,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct VaultUnlockSession {
    method: String,
    unlocked_at: SystemTime,
    expires_at: Option<SystemTime>,
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
pub struct VaultPassiveProjection {
    pub lock_state: VaultLockState,
    pub secret_count: usize,
    pub token_count: usize,
}

impl Default for VaultPassiveProjection {
    fn default() -> Self {
        Self {
            lock_state: VaultLockState::Uninitialized,
            secret_count: 0,
            token_count: 0,
        }
    }
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

#[derive(PartialEq, Eq)]
pub struct TrustedLocalSshKeyImportRequest {
    pub key_name: String,
    pub label: Option<String>,
    pub private_key: SecretBytes,
    pub passphrase: Option<String>,
    pub imported_by: String,
    pub rotation_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedLocalSshKeyImportResult {
    pub credential_ref: String,
    pub kind: String,
    pub label: String,
    pub status: String,
    pub active_version: String,
    pub encrypted_input: bool,
    pub last_rotated_at_unix_sec: Option<u64>,
    pub last_used_at_unix_sec: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetProfilePublicDescriptor {
    pub id: String,
    pub display_name: String,
    pub aliases: Vec<String>,
    pub kind: String,
    pub enabled: bool,
    pub storage_class: String,
    pub access_class: String,
    pub sealed_profile_ref: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TargetProfileSensitiveOverlay {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_ref: Option<String>,
    #[serde(default)]
    pub connection: serde_json::Value,
    #[serde(default)]
    pub toolchains: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub policy: serde_json::Map<String, serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetProfileSecretPayload {
    pub public_descriptor: TargetProfilePublicDescriptor,
    pub sensitive_overlay: TargetProfileSensitiveOverlay,
    pub public_descriptor_digest: String,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocalAdminActionKind {
    UnlockVault,
    DeleteVault,
    CreateAgentToken,
    DeleteAgentToken,
    UpdateAgentTokenScope,
    RevealSecret,
    ExportSecret,
}

impl LocalAdminActionKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UnlockVault => "unlock-vault",
            Self::DeleteVault => "delete-vault",
            Self::CreateAgentToken => "create-agent-token",
            Self::DeleteAgentToken => "delete-agent-token",
            Self::UpdateAgentTokenScope => "update-agent-token-scope",
            Self::RevealSecret => "reveal-secret",
            Self::ExportSecret => "export-secret",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        let normalized = raw.trim().to_ascii_lowercase().replace('_', "-");
        match normalized.as_str() {
            "unlock-vault" => Some(Self::UnlockVault),
            "delete-vault" => Some(Self::DeleteVault),
            "create-agent-token" => Some(Self::CreateAgentToken),
            "delete-agent-token" => Some(Self::DeleteAgentToken),
            "update-agent-token-scope" => Some(Self::UpdateAgentTokenScope),
            "reveal-secret" => Some(Self::RevealSecret),
            "export-secret" => Some(Self::ExportSecret),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
        if os_native_platform_binding_ready() {
            VaultReadinessState::Ready
        } else {
            VaultReadinessState::Degraded
        }
    }

    fn readiness_message(&self) -> &'static str {
        if os_native_platform_binding_ready() {
            "os-native protector is bound to platform credential storage"
        } else {
            "os-native protector binding is unavailable; falling back to degraded compatibility shim"
        }
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
    InvalidSshPrivateKey(String),
    SshKeyPassphraseRequired(String),
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
    VaultLocked(String),
    VaultUnavailable(String),
    UnlockMethodNotAllowed(String),
    UnlockFailed(String),
    PassphraseNotConfigured,
    PassphraseRejected,
    StorageIo(String),
    CryptoEnvelope(String),
}

impl VaultError {
    pub fn shared_error(&self) -> SharedError {
        let (status, common_code, module_code, message, recovery_hint) = match self {
            Self::BackendNotRegistered(_) => (
                ContractStatus::Unsupported,
                CommonErrorCode::DependencyUnavailable,
                "vault.backend_not_registered",
                "requested vault backend is not registered",
                "select a supported vault backend and retry",
            ),
            Self::InvalidCredentialRef(_) => (
                ContractStatus::Failed,
                CommonErrorCode::ValidationFailed,
                "vault.invalid_credential_ref",
                "credential reference is invalid",
                "use a canonical vault reference and retry",
            ),
            Self::InvalidSshPrivateKey(_) => (
                ContractStatus::Failed,
                CommonErrorCode::ValidationFailed,
                "vault.invalid_ssh_private_key",
                "ssh private key payload is invalid",
                "provide a valid private key file from trusted local storage and retry",
            ),
            Self::SshKeyPassphraseRequired(_) => (
                ContractStatus::Failed,
                CommonErrorCode::VerificationRequired,
                "vault.ssh_key_passphrase_required",
                "passphrase is required to import this ssh private key",
                "provide the key passphrase through trusted local input and retry",
            ),
            Self::PlaintextAccessDisabled(_) => (
                ContractStatus::Failed,
                CommonErrorCode::PermissionDenied,
                "vault.plaintext_access_disabled",
                "plaintext secret access is disabled",
                "use a brokered secret route instead of plaintext access",
            ),
            Self::SecretNotFound(_) => (
                ContractStatus::Failed,
                CommonErrorCode::NotFound,
                "vault.secret_not_found",
                "secret was not found",
                "verify the referenced secret exists and retry",
            ),
            Self::SecretVersionNotFound(_) => (
                ContractStatus::Failed,
                CommonErrorCode::NotFound,
                "vault.secret_version_not_found",
                "secret version was not found",
                "verify the active secret version exists and retry",
            ),
            Self::FailClosed(_) => (
                ContractStatus::NotReady,
                CommonErrorCode::DependencyUnavailable,
                "vault.fail_closed",
                "vault is in fail-closed state",
                "restore an allowed protector or unlock route before retrying",
            ),
            Self::LocalAdminVerificationRequired(_) => (
                ContractStatus::Locked,
                CommonErrorCode::VerificationRequired,
                "vault.local_admin_verification_required",
                "fresh local admin verification is required",
                "complete trusted local verification and retry",
            ),
            Self::LocalAdminIntentMismatch(_) => (
                ContractStatus::Failed,
                CommonErrorCode::ValidationFailed,
                "vault.local_admin_intent_mismatch",
                "local admin intent does not match the requested action",
                "create a new intent for the current action and retry",
            ),
            Self::LocalAdminIntentExpired(_) => (
                ContractStatus::Failed,
                CommonErrorCode::ValidationFailed,
                "vault.local_admin_intent_expired",
                "local admin intent has expired",
                "create a fresh intent and retry",
            ),
            Self::LocalAdminAttestationMismatch(_) => (
                ContractStatus::Failed,
                CommonErrorCode::ValidationFailed,
                "vault.local_admin_attestation_mismatch",
                "local admin attestation does not match the current action",
                "complete a new trusted verification flow and retry",
            ),
            Self::LocalAdminAttestationExpired(_) => (
                ContractStatus::Failed,
                CommonErrorCode::ValidationFailed,
                "vault.local_admin_attestation_expired",
                "local admin attestation has expired",
                "complete a fresh trusted verification flow and retry",
            ),
            Self::SshBrokerSessionNotFound(_) => (
                ContractStatus::Failed,
                CommonErrorCode::NotFound,
                "vault.ssh_broker_session_not_found",
                "ssh broker session was not found",
                "create a new ssh broker session and retry",
            ),
            Self::SshBrokerUnsupported(_) => (
                ContractStatus::Unsupported,
                CommonErrorCode::Unsupported,
                "vault.ssh_broker_unsupported",
                "ssh broker delivery is unsupported on the current route",
                "use a supported broker route or configured fallback",
            ),
            Self::SshRuntimePassphraseForbidden(_) => (
                ContractStatus::Failed,
                CommonErrorCode::ValidationFailed,
                "vault.ssh_runtime_passphrase_forbidden",
                "runtime ssh key passphrase entry is forbidden",
                "import signer material through a trusted management route instead",
            ),
            Self::AgentTokenNotFound(_) => (
                ContractStatus::Failed,
                CommonErrorCode::NotFound,
                "vault.agent_token_not_found",
                "agent token was not found",
                "verify the token id and retry",
            ),
            Self::AgentTokenRejected(_) => (
                ContractStatus::Failed,
                CommonErrorCode::ValidationFailed,
                "vault.agent_token_rejected",
                "agent token request was rejected",
                "review the requested token scope or verification state and retry",
            ),
            Self::VaultLocked(_) => (
                ContractStatus::Locked,
                CommonErrorCode::Locked,
                "vault.locked",
                "vault is locked",
                "unlock the vault through an allowed local route and retry",
            ),
            Self::VaultUnavailable(_) => (
                ContractStatus::NotReady,
                CommonErrorCode::DependencyUnavailable,
                "vault.unavailable",
                "vault is unavailable",
                "restore vault dependencies or unlock routes before retrying",
            ),
            Self::UnlockMethodNotAllowed(_) => (
                ContractStatus::Failed,
                CommonErrorCode::ValidationFailed,
                "vault.unlock_method_not_allowed",
                "unlock method is not allowed by policy",
                "use an allowed unlock method and retry",
            ),
            Self::UnlockFailed(_) => (
                ContractStatus::Failed,
                CommonErrorCode::ValidationFailed,
                "vault.unlock_failed",
                "vault unlock failed",
                "verify the provided unlock material and retry",
            ),
            Self::PassphraseNotConfigured => (
                ContractStatus::NotReady,
                CommonErrorCode::DependencyUnavailable,
                "vault.passphrase_not_configured",
                "passphrase protector is not configured",
                "configure a passphrase protector before using this unlock route",
            ),
            Self::PassphraseRejected => (
                ContractStatus::Failed,
                CommonErrorCode::ValidationFailed,
                "vault.passphrase_rejected",
                "passphrase was rejected",
                "verify the passphrase and retry",
            ),
            Self::StorageIo(_) => (
                ContractStatus::NotReady,
                CommonErrorCode::DependencyUnavailable,
                "vault.storage_io",
                "vault storage is unavailable",
                "verify the vault storage path is available and retry",
            ),
            Self::CryptoEnvelope(_) => (
                ContractStatus::Failed,
                CommonErrorCode::Internal,
                "vault.crypto_envelope",
                "vault encryption envelope operation failed",
                "inspect vault diagnostics before retrying the operation",
            ),
        };
        SharedError::new(status, ErrorDomain::Vault, common_code, message)
            .with_module_code(module_code)
            .with_recovery_hint(recovery_hint)
    }
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
    cleanup_paths: Vec<PathBuf>,
    diagnostics: Vec<String>,
    principal_id: String,
    logical_session_id: Option<String>,
    secret_reference: String,
    secret_version_id: String,
}

struct ValidatedLocalAdminAttestation {
    intent_id: String,
    attestation_id: String,
}

#[derive(Clone, Debug)]
struct VaultStorageLayout {
    root_dir: PathBuf,
    metadata_db_path: PathBuf,
    legacy_metadata_path: PathBuf,
}

impl VaultStorageLayout {
    fn open(root_dir: impl AsRef<Path>) -> Result<Self, VaultError> {
        let root_dir = root_dir.as_ref().to_path_buf();
        fs::create_dir_all(&root_dir).map_err(|err| {
            VaultError::StorageIo(format!(
                "failed to create vault root {}: {err}",
                root_dir.display()
            ))
        })?;
        let blobs_dir = root_dir.join("blobs");
        let secret_blob_dir = blobs_dir.join("secret");
        let wrap_blob_dir = blobs_dir.join("wrap");
        fs::create_dir_all(&secret_blob_dir).map_err(|err| {
            VaultError::StorageIo(format!(
                "failed to create secret blob dir {}: {err}",
                secret_blob_dir.display()
            ))
        })?;
        fs::create_dir_all(&wrap_blob_dir).map_err(|err| {
            VaultError::StorageIo(format!(
                "failed to create wrap blob dir {}: {err}",
                wrap_blob_dir.display()
            ))
        })?;
        Ok(Self {
            metadata_db_path: root_dir.join(METADATA_DB_FILENAME),
            legacy_metadata_path: root_dir.join(LEGACY_METADATA_FILENAME),
            root_dir,
        })
    }

    fn blob_uri_for_secret_version(&self, reference: &str, version_id: &str) -> (String, String) {
        let stable = short_digest(reference.as_bytes());
        let cipher = format!("secret/{stable}-{version_id}.cipher.bin");
        let wrap = format!("wrap/{stable}-{version_id}.dek.bin");
        (
            format!("blob://vault/{cipher}"),
            format!("blob://vault/{wrap}"),
        )
    }

    fn wrap_uri_for_manifest(&self, wrap_id: &str) -> String {
        format!("blob://vault/wrap/{wrap_id}.vrk.bin")
    }

    fn path_for_blob_uri(&self, uri: &str) -> Result<PathBuf, VaultError> {
        let Some(body) = uri.strip_prefix("blob://vault/") else {
            return Err(VaultError::StorageIo(format!(
                "unsupported blob uri for vault layout: {uri}"
            )));
        };
        if body.contains("..") {
            return Err(VaultError::StorageIo(format!(
                "refusing to resolve parent traversal in blob uri: {uri}"
            )));
        }
        Ok(self.root_dir.join("blobs").join(body))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct VaultMetadataDbV2 {
    schema_version: u32,
    active_backend: String,
    allow_degraded_mode: bool,
    #[serde(default = "default_persisted_lock_state")]
    lock_state: VaultLockState,
    #[serde(default = "default_persisted_unlock_policy")]
    unlock_policy: PersistedVaultUnlockPolicyV2,
    #[serde(default)]
    unlock_session: Option<PersistedVaultUnlockSessionV2>,
    #[serde(default = "default_passphrase_protector_state")]
    passphrase: PassphraseProtectorState,
    next_version_seq: u64,
    next_audit_seq: u64,
    next_agent_token_seq: u64,
    next_intent_seq: u64,
    next_attestation_seq: u64,
    next_ssh_broker_seq: u64,
    #[serde(default = "default_clock_watermark_unix_sec")]
    clock_watermark_unix_sec: u64,
    key_envelope: PersistedVaultKeyEnvelopeRecordV2,
    wrap_manifests: Vec<PersistedProtectorWrapManifestV2>,
    secrets: Vec<PersistedSecretStateV2>,
    #[serde(default)]
    agent_tokens: Vec<PersistedAgentTokenRecordV2>,
    #[serde(default)]
    token_scopes: Vec<PersistedTokenScopeRecordV2>,
    #[serde(default)]
    intents: Vec<PersistedLocalAdminActionIntentV2>,
    #[serde(default)]
    attestations: Vec<PersistedLocalAdminAttestationRecordV2>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedVaultUnlockPolicyV2 {
    trigger_policy: VaultUnlockTriggerPolicy,
    allowed_methods: Vec<String>,
    preferred_method: String,
    cache_ttl_sec: u64,
    require_fresh_user_verification: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedVaultUnlockSessionV2 {
    method: String,
    unlocked_at_unix_sec: u64,
    expires_at_unix_sec: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedVaultKeyEnvelopeRecordV2 {
    format_version: u32,
    vault_key_id: String,
    state: VaultKeyEnvelopeState,
    active_wrap_set_id: String,
    created_at_unix_sec: u64,
    rotated_at_unix_sec: Option<u64>,
    last_unlocked_at_unix_sec: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedProtectorWrapManifestV2 {
    format_version: u32,
    wrap_id: String,
    vault_key_id: String,
    protector_binding: String,
    wrap_format: String,
    wrapped_key_locator: String,
    wrapped_key_digest: String,
    created_at_unix_sec: u64,
    last_verified_at_unix_sec: Option<u64>,
    status: ProtectorWrapStatus,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedSecretStateV2 {
    record: PersistedVaultSecretRecordV2,
    versions: Vec<PersistedSecretVersionMaterialV2>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedVaultSecretRecordV2 {
    format_version: u32,
    reference: String,
    kind: String,
    label: String,
    status: VaultSecretStatus,
    active_version_id: Option<String>,
    created_by: String,
    created_at_unix_sec: u64,
    last_used_at_unix_sec: Option<u64>,
    last_rotated_at_unix_sec: Option<u64>,
    rotation: SecretRotationState,
    audit_chain_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedSecretVersionMaterialV2 {
    version: PersistedVaultSecretVersionRecordV2,
    ciphertext_blob_uri: String,
    wrapped_dek_blob_uri: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedVaultSecretVersionRecordV2 {
    format_version: u32,
    version_id: String,
    reference: String,
    version_seq: u32,
    state: VaultSecretVersionState,
    protector_binding: String,
    ciphertext_locator: String,
    ciphertext_digest: String,
    content_format: String,
    created_by: String,
    created_at_unix_sec: u64,
    activated_at_unix_sec: Option<u64>,
    superseded_at_unix_sec: Option<u64>,
    revoked_at_unix_sec: Option<u64>,
    destroy_after_unix_sec: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedAgentTokenRecordV2 {
    token_id: String,
    principal_id: String,
    label: String,
    status: AgentTokenStatus,
    #[serde(default = "default_agent_token_access_enabled")]
    access_enabled: bool,
    token_hash: String,
    hash_scheme: String,
    scope_profile: String,
    active_scope_version: u32,
    created_by: String,
    created_at_unix_sec: u64,
    last_used_at_unix_sec: Option<u64>,
    expires_at_unix_sec: Option<u64>,
    idle_timeout_sec: Option<u64>,
    revoked_at_unix_sec: Option<u64>,
    revoke_reason: Option<String>,
    deleted_at_unix_sec: Option<u64>,
    parent_token_id: Option<String>,
    issued_via_attestation_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedTokenScopeRecordV2 {
    token_id: String,
    version: u32,
    status: TokenScopeStatus,
    scope_profile: String,
    target_ids: Vec<String>,
    tool_ids: Vec<String>,
    max_risk_envelope: String,
    allow_open_shell: bool,
    allow_write_shell_input: bool,
    allow_artifact_cross_principal: bool,
    allow_delegation: bool,
    allow_admin_actions: bool,
    created_by: String,
    created_at_unix_sec: u64,
    superseded_at_unix_sec: Option<u64>,
    change_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedLocalAdminActionIntentV2 {
    intent_id: String,
    action_kind: LocalAdminActionKind,
    target_object_ref: String,
    requested_by_principal: String,
    requested_payload_digest: String,
    created_at_unix_sec: u64,
    expires_at_unix_sec: u64,
    status: LocalAdminIntentStatus,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedLocalAdminAttestationRecordV2 {
    attestation_id: String,
    intent_id: String,
    verified_principal: String,
    verification_method: String,
    issued_at_unix_sec: u64,
    expires_at_unix_sec: u64,
    consumed_at_unix_sec: Option<u64>,
    status: LocalAdminAttestationStatus,
}

#[derive(Debug, Serialize, Deserialize)]
struct VaultMetadataDbV1 {
    schema_version: u32,
    active_backend: String,
    allow_degraded_mode: bool,
    next_version_seq: u64,
    next_audit_seq: u64,
    next_agent_token_seq: u64,
    next_intent_seq: u64,
    next_attestation_seq: u64,
    next_ssh_broker_seq: u64,
    #[serde(default = "default_clock_watermark_unix_sec")]
    clock_watermark_unix_sec: u64,
    key_envelope: PersistedVaultKeyEnvelopeRecordV1,
    wrap_manifests: Vec<PersistedProtectorWrapManifestV1>,
    secrets: Vec<PersistedSecretStateV1>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedVaultKeyEnvelopeRecordV1 {
    vault_key_id: String,
    state: VaultKeyEnvelopeState,
    active_wrap_set_id: String,
    created_at_unix_sec: u64,
    rotated_at_unix_sec: Option<u64>,
    last_unlocked_at_unix_sec: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedProtectorWrapManifestV1 {
    wrap_id: String,
    vault_key_id: String,
    protector_binding: String,
    wrap_format: String,
    wrapped_key_digest: String,
    wrapped_key_hex: String,
    created_at_unix_sec: u64,
    last_verified_at_unix_sec: Option<u64>,
    status: ProtectorWrapStatus,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedSecretStateV1 {
    record: PersistedVaultSecretRecordV1,
    versions: Vec<PersistedSecretVersionMaterialV1>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedVaultSecretRecordV1 {
    reference: String,
    kind: String,
    label: String,
    status: VaultSecretStatus,
    active_version_id: Option<String>,
    created_by: String,
    created_at_unix_sec: u64,
    last_used_at_unix_sec: Option<u64>,
    last_rotated_at_unix_sec: Option<u64>,
    rotation: SecretRotationState,
    audit_chain_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedSecretVersionMaterialV1 {
    version: PersistedVaultSecretVersionRecordV1,
    ciphertext_hex: String,
    wrapped_dek_hex: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedVaultSecretVersionRecordV1 {
    version_id: String,
    reference: String,
    version_seq: u32,
    state: VaultSecretVersionState,
    protector_binding: String,
    ciphertext_locator: String,
    ciphertext_digest: String,
    content_format: String,
    created_by: String,
    created_at_unix_sec: u64,
    activated_at_unix_sec: Option<u64>,
    superseded_at_unix_sec: Option<u64>,
    revoked_at_unix_sec: Option<u64>,
    destroy_after_unix_sec: Option<u64>,
}

fn default_unlock_policy() -> VaultUnlockPolicy {
    VaultUnlockPolicy {
        trigger_policy: VaultUnlockTriggerPolicy::OnFirstSecretAccess,
        allowed_methods: vec!["os-native".into(), "passphrase".into()],
        preferred_method: "os-native".into(),
        cache_ttl_sec: 600,
        require_fresh_user_verification: true,
    }
}

fn default_persisted_lock_state() -> VaultLockState {
    VaultLockState::Locked
}

fn default_clock_watermark_unix_sec() -> u64 {
    0
}

fn default_agent_token_access_enabled() -> bool {
    true
}

fn default_persisted_unlock_policy() -> PersistedVaultUnlockPolicyV2 {
    PersistedVaultUnlockPolicyV2 {
        trigger_policy: VaultUnlockTriggerPolicy::OnFirstSecretAccess,
        allowed_methods: vec!["os-native".into(), "passphrase".into()],
        preferred_method: "os-native".into(),
        cache_ttl_sec: 600,
        require_fresh_user_verification: true,
    }
}

pub fn read_passive_vault_projection(
    root_dir: impl AsRef<Path>,
) -> Result<VaultPassiveProjection, VaultError> {
    let root_dir = root_dir.as_ref();
    let metadata_db_path = root_dir.join(METADATA_DB_FILENAME);
    if metadata_db_path.exists() {
        let raw = fs::read_to_string(&metadata_db_path).map_err(|err| {
            VaultError::StorageIo(format!(
                "failed to read metadata db {}: {err}",
                metadata_db_path.display()
            ))
        })?;
        let schema_version = detect_schema_version(&raw)?;
        let payload: serde_json::Value = serde_json::from_str(&raw).map_err(|err| {
            VaultError::StorageIo(format!(
                "failed to parse metadata db {}: {err}",
                metadata_db_path.display()
            ))
        })?;
        return match schema_version {
            CANONICAL_METADATA_SCHEMA_VERSION => Ok(passive_projection_from_v2_payload(&payload)),
            LEGACY_METADATA_SCHEMA_VERSION => Ok(passive_projection_from_v1_payload(&payload)),
            other => Err(VaultError::StorageIo(format!(
                "unsupported metadata schema version {other}; expected 1 or 2"
            ))),
        };
    }

    let legacy_metadata_path = root_dir.join(LEGACY_METADATA_FILENAME);
    if legacy_metadata_path.exists() {
        let raw = fs::read_to_string(&legacy_metadata_path).map_err(|err| {
            VaultError::StorageIo(format!(
                "failed to read legacy metadata {}: {err}",
                legacy_metadata_path.display()
            ))
        })?;
        let payload: serde_json::Value = serde_json::from_str(&raw).map_err(|err| {
            VaultError::StorageIo(format!(
                "failed to parse legacy metadata {}: {err}",
                legacy_metadata_path.display()
            ))
        })?;
        return Ok(passive_projection_from_v1_payload(&payload));
    }

    Ok(VaultPassiveProjection::default())
}

fn passive_projection_from_v2_payload(payload: &serde_json::Value) -> VaultPassiveProjection {
    let lock_state = payload
        .get("lock_state")
        .and_then(serde_json::Value::as_str)
        .and_then(parse_lock_state_label)
        .unwrap_or(VaultLockState::Locked);
    let secret_count = payload
        .get("secrets")
        .and_then(serde_json::Value::as_array)
        .map(|items| items.len())
        .unwrap_or(0);
    let token_count = payload
        .get("agent_tokens")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| {
                    let status = item
                        .get("status")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("active")
                        .to_ascii_lowercase();
                    status != "deleted"
                })
                .count()
        })
        .unwrap_or(0);
    VaultPassiveProjection {
        lock_state,
        secret_count,
        token_count,
    }
}

fn passive_projection_from_v1_payload(payload: &serde_json::Value) -> VaultPassiveProjection {
    let secret_count = payload
        .get("secrets")
        .and_then(serde_json::Value::as_array)
        .map(|items| items.len())
        .unwrap_or(0);
    VaultPassiveProjection {
        lock_state: VaultLockState::Locked,
        secret_count,
        token_count: 0,
    }
}

fn parse_lock_state_label(raw: &str) -> Option<VaultLockState> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "uninitialized" => Some(VaultLockState::Uninitialized),
        "locked" => Some(VaultLockState::Locked),
        "unlocking" => Some(VaultLockState::Unlocking),
        "unlocked" => Some(VaultLockState::Unlocked),
        "unavailable" => Some(VaultLockState::Unavailable),
        _ => None,
    }
}

fn default_passphrase_kdf_params() -> PassphraseKdfParams {
    PassphraseKdfParams {
        kdf: "argon2id".into(),
        format_version: PASSPHRASE_KDF_FORMAT_VERSION,
        m_cost_kib: 19_456,
        t_cost: 2,
        p_cost: 1,
    }
}

fn default_passphrase_protector_state() -> PassphraseProtectorState {
    PassphraseProtectorState {
        enabled: false,
        wrap_id: "wrap-passphrase-000001".into(),
        salt_locator: "blob://vault/wrap/passphrase-salt-000001.bin".into(),
        params: default_passphrase_kdf_params(),
    }
}

pub struct SecretVaultRouter {
    active_backend: String,
    backends: HashMap<String, Box<dyn SecretVaultBackend>>,
    secrets: HashMap<String, StoredSecretState>,
    agent_tokens: HashMap<String, AgentTokenRecord>,
    token_scopes: HashMap<String, Vec<TokenScopeRecord>>,
    token_hash_index: HashMap<String, String>,
    redaction_registry: RuntimeRedactionRegistry,
    root_key_cache: Option<SecretBytes>,
    passphrase: PassphraseProtectorState,
    unlock_policy: VaultUnlockPolicy,
    lock_state: VaultLockState,
    unlock_session: Option<VaultUnlockSession>,
    wrap_blob_cache: HashMap<String, Vec<u8>>,
    allow_degraded_mode: bool,
    next_version_seq: u64,
    next_audit_seq: u64,
    next_agent_token_seq: u64,
    next_intent_seq: u64,
    next_attestation_seq: u64,
    next_ssh_broker_seq: u64,
    clock_watermark: SystemTime,
    key_envelope: VaultKeyEnvelopeRecord,
    wrap_manifests: Vec<ProtectorWrapManifest>,
    intents: HashMap<String, LocalAdminActionIntent>,
    attestations: HashMap<String, LocalAdminAttestationRecord>,
    ssh_broker_sessions: HashMap<String, StoredSshAgentBrokerSession>,
    storage_layout: Option<VaultStorageLayout>,
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
            root_key_cache: None,
            passphrase: default_passphrase_protector_state(),
            unlock_policy: default_unlock_policy(),
            lock_state: VaultLockState::Locked,
            unlock_session: None,
            wrap_blob_cache: HashMap::new(),
            allow_degraded_mode: false,
            next_version_seq: 0,
            next_audit_seq: 0,
            next_agent_token_seq: 0,
            next_intent_seq: 0,
            next_attestation_seq: 0,
            next_ssh_broker_seq: 0,
            clock_watermark: SystemTime::UNIX_EPOCH,
            key_envelope: VaultKeyEnvelopeRecord {
                format_version: VAULT_OBJECT_FORMAT_VERSION,
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
            storage_layout: None,
        };
        router.register_backend(Box::<BuiltinEncryptedVaultBackend>::default());
        router.register_backend(Box::<OsNativeVaultBackend>::default());
        router.register_backend(Box::<FileVaultBackend>::default());
        router.register_backend(Box::<InMemoryVaultBackend>::default());
        router.wrap_manifests.push(ProtectorWrapManifest {
            format_version: VAULT_OBJECT_FORMAT_VERSION,
            wrap_id: "wrap-000001".into(),
            vault_key_id: router.key_envelope.vault_key_id.clone(),
            protector_binding: "os-native".into(),
            wrap_format: "sha256-stream-aead-v1".into(),
            wrapped_key_locator: "blob://vault/wrap/wrap-000001.vrk.bin".into(),
            wrapped_key_digest: String::new(),
            created_at: SystemTime::now(),
            last_verified_at: None,
            status: ProtectorWrapStatus::Degraded,
        });
        let bootstrap_root_key = SecretBytes::from_bytes(pseudo_random_bytes(32, "vault-root-key"));
        router
            .bootstrap_root_key_for_protectors_without_keyring(bootstrap_root_key.expose_for_use())
            .expect("bootstrap root wraps");
        router.lock_state = VaultLockState::Locked;
        router
    }
}

impl SecretVaultRouter {
    pub fn with_persistent_store(root_dir: impl AsRef<Path>) -> Result<Self, VaultError> {
        let mut router = Self::default();
        router.enable_persistent_store(root_dir)?;
        Ok(router)
    }

    pub fn register_backend(&mut self, backend: Box<dyn SecretVaultBackend>) {
        self.backends.insert(backend.kind().to_string(), backend);
    }

    pub fn enable_persistent_store(
        &mut self,
        root_dir: impl AsRef<Path>,
    ) -> Result<(), VaultError> {
        let layout = VaultStorageLayout::open(root_dir)?;
        self.storage_layout = Some(layout);
        self.load_or_initialize_metadata_db()?;
        self.persist_metadata_db()?;
        Ok(())
    }

    pub fn set_active_backend(&mut self, backend: &str) -> Result<(), VaultError> {
        if !self.backends.contains_key(backend) {
            return Err(VaultError::BackendNotRegistered(backend.to_string()));
        }
        self.active_backend = backend.to_string();
        self.persist_metadata_db()?;
        Ok(())
    }

    pub fn active_backend(&self) -> &str {
        &self.active_backend
    }

    pub fn set_allow_degraded_mode(&mut self, allow: bool) -> Result<(), VaultError> {
        self.allow_degraded_mode = allow;
        self.persist_metadata_db()?;
        Ok(())
    }

    pub fn vault_lock_state(&self) -> VaultLockState {
        self.lock_state.clone()
    }

    pub fn unlock_policy(&self) -> &VaultUnlockPolicy {
        &self.unlock_policy
    }

    pub fn set_unlock_policy(&mut self, policy: VaultUnlockPolicy) -> Result<(), VaultError> {
        self.unlock_policy = normalize_unlock_policy(policy);
        if matches!(self.lock_state, VaultLockState::Uninitialized) {
            self.persist_metadata_db()?;
            return Ok(());
        }
        if matches!(
            self.unlock_policy.trigger_policy,
            VaultUnlockTriggerPolicy::OnCoreStart
        ) {
            let preferred = self.unlock_policy.preferred_method.clone();
            if self.try_unlock_with_method(&preferred).is_err() {
                self.lock_state = default_persisted_lock_state();
            }
        }
        self.persist_metadata_db()?;
        Ok(())
    }

    pub fn configure_passphrase_protector(&mut self, passphrase: &str) -> Result<(), VaultError> {
        if passphrase.trim().is_empty() {
            return Err(VaultError::PassphraseRejected);
        }
        let root_key = self.ensure_unlocked_root_key()?;
        let salt = pseudo_random_bytes(16, "passphrase-salt");
        let salt_locator = self.passphrase.salt_locator.clone();
        self.write_wrap_blob(&salt_locator, salt.clone())?;
        let kek = derive_argon2id_kek(passphrase, &salt, &self.passphrase.params)?;
        let wrapped = seal_aead(
            &kek,
            format!(
                "root:{}:{}",
                self.key_envelope.vault_key_id, self.passphrase.wrap_id
            )
            .as_bytes(),
            &root_key,
        );
        let wrap_locator = format!("blob://vault/wrap/{}.vrk.bin", self.passphrase.wrap_id);
        self.write_wrap_blob(&wrap_locator, wrapped.clone())?;
        self.upsert_wrap_manifest(ProtectorWrapManifest {
            format_version: VAULT_OBJECT_FORMAT_VERSION,
            wrap_id: self.passphrase.wrap_id.clone(),
            vault_key_id: self.key_envelope.vault_key_id.clone(),
            protector_binding: "passphrase".into(),
            wrap_format: "argon2id-wrap-v1".into(),
            wrapped_key_locator: wrap_locator,
            wrapped_key_digest: short_digest(&wrapped),
            created_at: SystemTime::now(),
            last_verified_at: Some(SystemTime::now()),
            status: ProtectorWrapStatus::Ready,
        });
        self.passphrase.enabled = true;
        if !self
            .unlock_policy
            .allowed_methods
            .iter()
            .any(|method| method == "passphrase")
        {
            self.unlock_policy.allowed_methods.push("passphrase".into());
        }
        self.persist_metadata_db()?;
        Ok(())
    }

    pub fn unlock_with_passphrase(&mut self, passphrase: &str) -> Result<(), VaultError> {
        if !self.passphrase.enabled {
            return Err(VaultError::PassphraseNotConfigured);
        }
        self.try_unlock_with_method_and_secret("passphrase", Some(passphrase))
    }

    pub fn unlock_with_os_native(&mut self) -> Result<(), VaultError> {
        self.try_unlock_with_method("os-native")
    }

    pub fn unlock_with_os_native_verified(&mut self) -> Result<(), VaultError> {
        self.try_unlock_with_method_verified("os-native")
    }

    pub fn lock_vault(&mut self, reason: &str) -> Result<(), VaultError> {
        self.lock_vault_internal(reason);
        self.persist_metadata_db()?;
        Ok(())
    }

    pub fn shutdown_cleanup(&mut self) -> Result<(), VaultError> {
        self.lock_vault_internal("shutdown");
        for stored in self.ssh_broker_sessions.values_mut() {
            cleanup_stored_ssh_session_artifacts(stored);
        }
        self.ssh_broker_sessions.clear();
        self.persist_metadata_db()?;
        Ok(())
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
        let root_key = self.ensure_unlocked_root_key()?;

        let record = self
            .secrets
            .entry(canonical.clone())
            .or_insert_with(|| StoredSecretState {
                record: VaultSecretRecord {
                    format_version: VAULT_OBJECT_FORMAT_VERSION,
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
        let dek = pseudo_random_bytes(32, &format!("dek:{}:{version_id}", canonical));
        let wrapped_dek = seal_aead(
            &root_key,
            format!("vrk-wrap:{canonical}:{version_id}").as_bytes(),
            &dek,
        );
        let ciphertext = seal_aead(
            &dek,
            format!("secret:{canonical}:{version_id}").as_bytes(),
            plaintext,
        );
        let digest = short_digest(&ciphertext);
        let locator = format!(
            "blob://vault/{}/{}/{}",
            self.active_backend, canonical, version_id
        );

        record.versions.push(StoredSecretVersionMaterial {
            version: VaultSecretVersionRecord {
                format_version: VAULT_OBJECT_FORMAT_VERSION,
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
        let output =
            secret_record_from_metadata(&record.record, self.active_backend.clone(), version_seq);
        if matches!(
            self.unlock_policy.trigger_policy,
            VaultUnlockTriggerPolicy::OnEverySecretAccess
        ) {
            self.lock_vault_internal("on-every-secret-access");
        }
        self.persist_metadata_db()?;
        Ok(output)
    }

    pub fn import_ssh_private_key_trusted_local(
        &mut self,
        request: TrustedLocalSshKeyImportRequest,
    ) -> Result<TrustedLocalSshKeyImportResult, VaultError> {
        let canonical_ref = canonical_ssh_private_key_ref_from_key_name(&request.key_name)?;
        let key_payload = request
            .private_key
            .expose_utf8_for_use()
            .ok_or_else(|| VaultError::InvalidSshPrivateKey("key payload must be UTF-8".into()))?;
        let validation = validate_trusted_local_ssh_private_key_material(
            key_payload,
            request.passphrase.as_deref(),
        )?;
        let label = request
            .label
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| request.key_name.trim())
            .to_string();
        let actor = request.imported_by.trim();
        let actor = if actor.is_empty() {
            "trusted-local-admin"
        } else {
            actor
        };

        self.put_with_actor(
            &canonical_ref,
            request.private_key,
            label.clone(),
            actor,
            request.rotation_reason,
        )?;
        let metadata = self
            .secret_metadata(&canonical_ref)?
            .ok_or_else(|| VaultError::SecretNotFound(canonical_ref.clone()))?;
        let active_version = metadata
            .record
            .active_version_id
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        Ok(TrustedLocalSshKeyImportResult {
            credential_ref: canonical_ref,
            kind: metadata.record.kind.clone(),
            label,
            status: metadata.record.status.as_str().to_string(),
            active_version,
            encrypted_input: validation.encrypted_input,
            last_rotated_at_unix_sec: metadata
                .record
                .last_rotated_at
                .map(system_time_to_unix_secs),
            last_used_at_unix_sec: metadata.record.last_used_at.map(system_time_to_unix_secs),
        })
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

    pub fn delete_secret_trusted_local(&mut self, reference: &str) -> Result<(), VaultError> {
        let canonical = normalize_credential_ref(reference)?;
        self.ensure_backend_allows_secret_use()?;
        if self.secrets.remove(&canonical).is_none() {
            return Err(VaultError::SecretNotFound(canonical));
        }
        self.ssh_broker_sessions
            .retain(|_, stored| !stored.secret_reference.eq_ignore_ascii_case(&canonical));
        self.persist_metadata_db()?;
        Ok(())
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

    fn token_effective_now(&mut self) -> (SystemTime, bool) {
        let observed_now = SystemTime::now();
        if observed_now > self.clock_watermark {
            self.clock_watermark = observed_now;
            return (observed_now, false);
        }
        if observed_now < self.clock_watermark {
            return (self.clock_watermark, true);
        }
        (observed_now, false)
    }

    fn token_now_for_create(&mut self, has_expiry: bool) -> Result<SystemTime, VaultError> {
        let (effective_now, rollback_detected) = self.token_effective_now();
        if rollback_detected && has_expiry {
            return Err(VaultError::AgentTokenRejected(
                "system clock rollback detected; expiring token create is temporarily blocked"
                    .into(),
            ));
        }
        Ok(effective_now)
    }

    pub fn create_agent_token(
        &mut self,
        request: CreateAgentTokenRequest,
    ) -> Result<CreateAgentTokenResult, VaultError> {
        let label = normalize_agent_token_label_for_write(&request.label)?;
        let created_by = request.created_by.trim();
        if created_by.is_empty() {
            return Err(VaultError::AgentTokenRejected(
                "created_by principal must be non-empty".into(),
            ));
        }
        let attestation_id = request.attestation_id.as_deref().ok_or_else(|| {
            VaultError::LocalAdminVerificationRequired(
                "create_agent_token requires local admin attestation".into(),
            )
        })?;
        let payload_digest = payload_digest_for_token_create_request(&request);
        let validated = self.validate_local_admin_attestation(
            LocalAdminActionKind::CreateAgentToken,
            LOCAL_ADMIN_CREATE_TOKEN_TARGET,
            &payload_digest,
            created_by,
            attestation_id,
            None,
        )?;
        self.consume_local_admin_attestation(&validated)?;

        self.next_agent_token_seq += 1;
        let seq = self.next_agent_token_seq;
        let token_id = format!("token-{seq:06}");
        let principal_id = format!("principal-{seq:06}");
        let now = self.token_now_for_create(request.expires_in.is_some())?;
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
                label: label.clone(),
                status: AgentTokenStatus::Active,
                access_enabled: true,
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
                deleted_at: None,
                parent_token_id: None,
                issued_via_attestation_id: Some(attestation_id.to_string()),
            },
        );
        self.token_scopes
            .insert(token_id.clone(), vec![scope_record]);
        self.persist_metadata_db()?;

        let summary = self
            .agent_token_summary(&token_id)?
            .ok_or_else(|| VaultError::AgentTokenNotFound(token_id.clone()))?;
        Ok(CreateAgentTokenResult {
            plaintext_token,
            summary,
        })
    }

    pub fn init_vault_store(&mut self) -> Result<VaultLockState, VaultError> {
        if matches!(self.lock_state, VaultLockState::Uninitialized) {
            self.root_key_cache = None;
            self.unlock_session = None;
            self.lock_state = VaultLockState::Locked;
            self.persist_metadata_db()?;
        }
        Ok(self.lock_state.clone())
    }

    pub fn delete_vault_with_attestation(
        &mut self,
        request: DeleteVaultRequest,
    ) -> Result<VaultLockState, VaultError> {
        let requested_by = request.requested_by.trim();
        if requested_by.is_empty() {
            return Err(VaultError::LocalAdminIntentMismatch(
                "requested_by principal must be non-empty".into(),
            ));
        }
        let payload_digest = payload_digest_for_delete_vault_request();
        let validated = self.validate_local_admin_attestation(
            LocalAdminActionKind::DeleteVault,
            LOCAL_ADMIN_DELETE_VAULT_TARGET,
            &payload_digest,
            requested_by,
            &request.attestation_id,
            None,
        )?;
        self.consume_local_admin_attestation(&validated)?;

        let Some(layout) = self.storage_layout.clone() else {
            return Err(VaultError::VaultUnavailable(
                "vault delete requires a persistent vault store".into(),
            ));
        };
        if layout.root_dir.exists() {
            fs::remove_dir_all(&layout.root_dir).map_err(|err| {
                VaultError::StorageIo(format!(
                    "failed to delete vault root {}: {err}",
                    layout.root_dir.display()
                ))
            })?;
        }
        let rebuilt_layout = VaultStorageLayout::open(&layout.root_dir)?;
        let active_backend = self.active_backend.clone();
        let allow_degraded_mode = self.allow_degraded_mode;
        let unlock_policy = self.unlock_policy.clone();
        *self = SecretVaultRouter::default();
        self.storage_layout = Some(rebuilt_layout);
        self.active_backend = active_backend;
        self.allow_degraded_mode = allow_degraded_mode;
        self.unlock_policy = normalize_unlock_policy(unlock_policy);
        self.lock_state = VaultLockState::Uninitialized;
        self.persist_metadata_db()?;
        Ok(self.lock_state.clone())
    }

    pub fn list_agent_tokens(&mut self) -> Vec<AgentTokenSummary> {
        let mut token_ids = self.agent_tokens.keys().cloned().collect::<Vec<_>>();
        token_ids.sort();
        let (now, _) = self.token_effective_now();
        token_ids
            .iter()
            .filter_map(|token_id| {
                self.agent_token_summary_with_now(token_id, now)
                    .ok()
                    .flatten()
            })
            .filter(|summary| !matches!(summary.status, AgentTokenStatus::Deleted))
            .collect()
    }

    pub fn revoke_agent_token(
        &mut self,
        token_id: &str,
        reason: Option<String>,
    ) -> Result<AgentTokenSummary, VaultError> {
        let (now, _) = self.token_effective_now();
        let mut propagate_reason = None;
        {
            let record = self
                .agent_tokens
                .get_mut(token_id)
                .ok_or_else(|| VaultError::AgentTokenNotFound(token_id.to_string()))?;
            refresh_agent_token_status(record, now);
            if matches!(record.status, AgentTokenStatus::Deleted) {
                return Err(VaultError::AgentTokenRejected(
                    "cannot revoke a deleted token".into(),
                ));
            }
            if !matches!(record.status, AgentTokenStatus::Revoked) {
                record.status = AgentTokenStatus::Revoked;
                record.revoked_at = Some(now);
                record.revoke_reason = reason;
                propagate_reason = record.revoke_reason.clone();
            }
        }
        if propagate_reason.is_some() {
            self.propagate_revoke_to_descendants(token_id, now, propagate_reason);
        }
        self.persist_metadata_db()?;
        self.agent_token_summary(token_id)?
            .ok_or_else(|| VaultError::AgentTokenNotFound(token_id.to_string()))
    }

    pub fn update_agent_token_label(
        &mut self,
        request: UpdateAgentTokenLabelRequest,
    ) -> Result<AgentTokenSummary, VaultError> {
        let token_id = request.token_id.trim();
        if token_id.is_empty() {
            return Err(VaultError::AgentTokenRejected(
                "token_id must be non-empty".into(),
            ));
        }
        let changed_by = request.changed_by.trim();
        if changed_by.is_empty() {
            return Err(VaultError::AgentTokenRejected(
                "changed_by principal must be non-empty".into(),
            ));
        }
        let label = normalize_agent_token_label_for_write(&request.label)?;
        let (now, _) = self.token_effective_now();
        let token = self
            .agent_tokens
            .get_mut(token_id)
            .ok_or_else(|| VaultError::AgentTokenNotFound(token_id.to_string()))?;
        refresh_agent_token_status(token, now);
        if matches!(
            agent_token_projected_status(token),
            AgentTokenStatus::Deleted
        ) {
            return Err(VaultError::AgentTokenRejected(
                "cannot update label for a deleted token".into(),
            ));
        }
        token.label = label;
        self.persist_metadata_db()?;
        self.agent_token_summary_with_now(token_id, now)?
            .ok_or_else(|| VaultError::AgentTokenNotFound(token_id.to_string()))
    }

    pub fn update_agent_token_access(
        &mut self,
        request: UpdateAgentTokenAccessRequest,
    ) -> Result<AgentTokenSummary, VaultError> {
        let token_id = request.token_id.trim();
        if token_id.is_empty() {
            return Err(VaultError::AgentTokenRejected(
                "token_id must be non-empty".into(),
            ));
        }
        let changed_by = request.changed_by.trim();
        if changed_by.is_empty() {
            return Err(VaultError::AgentTokenRejected(
                "changed_by principal must be non-empty".into(),
            ));
        }
        let (now, _) = self.token_effective_now();
        let token = self
            .agent_tokens
            .get_mut(token_id)
            .ok_or_else(|| VaultError::AgentTokenNotFound(token_id.to_string()))?;
        refresh_agent_token_status(token, now);
        match agent_token_projected_status(token) {
            AgentTokenStatus::Active | AgentTokenStatus::Disabled => {
                token.access_enabled = request.enabled;
            }
            AgentTokenStatus::Revoked | AgentTokenStatus::Expired | AgentTokenStatus::Deleted => {
                return Err(VaultError::AgentTokenRejected(format!(
                    "token {} cannot change access in status {}",
                    token.token_id,
                    agent_token_projected_status(token).as_str()
                )));
            }
        }
        self.persist_metadata_db()?;
        self.agent_token_summary_with_now(token_id, now)?
            .ok_or_else(|| VaultError::AgentTokenNotFound(token_id.to_string()))
    }

    pub fn delete_agent_token_with_attestation(
        &mut self,
        request: DeleteAgentTokenRequest,
    ) -> Result<AgentTokenSummary, VaultError> {
        let token_id = request.token_id.trim().to_string();
        if token_id.is_empty() {
            return Err(VaultError::AgentTokenRejected(
                "token_id must be non-empty".into(),
            ));
        }
        let requested_by = request.requested_by.trim();
        if requested_by.is_empty() {
            return Err(VaultError::LocalAdminIntentMismatch(
                "requested_by principal must be non-empty".into(),
            ));
        }
        let payload_digest = payload_digest_for_token_delete_request(&token_id);
        let validated = self.validate_local_admin_attestation(
            LocalAdminActionKind::DeleteAgentToken,
            &token_id,
            &payload_digest,
            requested_by,
            &request.attestation_id,
            None,
        )?;
        self.consume_local_admin_attestation(&validated)?;

        let (now, _) = self.token_effective_now();
        let token = self
            .agent_tokens
            .get_mut(&token_id)
            .ok_or_else(|| VaultError::AgentTokenNotFound(token_id.clone()))?;
        refresh_agent_token_status(token, now);
        if !matches!(token.status, AgentTokenStatus::Revoked) {
            return Err(VaultError::AgentTokenRejected(
                "token delete requires token status revoked".into(),
            ));
        }
        token.status = AgentTokenStatus::Deleted;
        token.deleted_at = Some(now);
        self.token_hash_index.remove(&token.token_hash);
        self.persist_metadata_db()?;
        self.agent_token_summary_with_now(&token_id, now)?
            .ok_or_else(|| VaultError::AgentTokenNotFound(token_id))
    }

    pub fn update_agent_token_scope(
        &mut self,
        request: UpdateAgentTokenScopeRequest,
    ) -> Result<AgentTokenSummary, VaultError> {
        let (now, _) = self.token_effective_now();
        let token_id = request.token_id.clone();
        let changed_by = request.changed_by.trim();
        if changed_by.is_empty() {
            return Err(VaultError::AgentTokenRejected(
                "changed_by principal must be non-empty".into(),
            ));
        }
        {
            let token = self
                .agent_tokens
                .get_mut(&token_id)
                .ok_or_else(|| VaultError::AgentTokenNotFound(token_id.clone()))?;
            refresh_agent_token_status(token, now);
            if !matches!(
                agent_token_projected_status(token),
                AgentTokenStatus::Active
            ) {
                return Err(VaultError::AgentTokenRejected(format!(
                    "token {} is not active",
                    token.token_id
                )));
            }
        }
        let attestation_id = request.attestation_id.as_deref().ok_or_else(|| {
            VaultError::LocalAdminVerificationRequired(
                "update_agent_token_scope requires local admin attestation".into(),
            )
        })?;
        let payload_digest = payload_digest_for_token_scope_update_request(&request);
        let validated = self.validate_local_admin_attestation(
            LocalAdminActionKind::UpdateAgentTokenScope,
            &token_id,
            &payload_digest,
            changed_by,
            attestation_id,
            None,
        )?;
        self.consume_local_admin_attestation(&validated)?;

        let token = self
            .agent_tokens
            .get_mut(&token_id)
            .ok_or_else(|| VaultError::AgentTokenNotFound(token_id.clone()))?;
        refresh_agent_token_status(token, now);
        if !matches!(
            agent_token_projected_status(token),
            AgentTokenStatus::Active
        ) {
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
        token.issued_via_attestation_id = Some(attestation_id.to_string());

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
            changed_by,
            now,
            request.reason,
        ));
        self.persist_metadata_db()?;

        self.agent_token_summary(&token_id)?
            .ok_or_else(|| VaultError::AgentTokenNotFound(token_id))
    }

    pub fn authenticate_agent_token(&mut self, token: &str) -> AgentTokenAuthResult {
        let token_hash = short_digest(token.as_bytes());
        let Some(token_id) = self.token_hash_index.get(&token_hash).cloned() else {
            return AgentTokenAuthResult::Rejected(AgentTokenAuthRejectReason::Invalid);
        };
        let (now, _) = self.token_effective_now();
        let (label, principal_id, active_scope_version, scope_profile) = {
            let Some(record) = self.agent_tokens.get_mut(&token_id) else {
                return AgentTokenAuthResult::Rejected(AgentTokenAuthRejectReason::Invalid);
            };
            refresh_agent_token_status(record, now);
            match agent_token_projected_status(record) {
                AgentTokenStatus::Active => {}
                AgentTokenStatus::Disabled => {
                    return AgentTokenAuthResult::Rejected(AgentTokenAuthRejectReason::Disabled)
                }
                AgentTokenStatus::Revoked => {
                    return AgentTokenAuthResult::Rejected(AgentTokenAuthRejectReason::Revoked)
                }
                AgentTokenStatus::Expired => {
                    return AgentTokenAuthResult::Rejected(AgentTokenAuthRejectReason::Expired)
                }
                AgentTokenStatus::Deleted => {
                    return AgentTokenAuthResult::Rejected(AgentTokenAuthRejectReason::Invalid)
                }
            }
            record.last_used_at = Some(now);
            (
                record.label.clone(),
                record.principal_id.clone(),
                record.active_scope_version,
                record.scope_profile.clone(),
            )
        };
        let Some(scope) = self
            .scope_for_version(&token_id, active_scope_version)
            .cloned()
        else {
            return AgentTokenAuthResult::Rejected(AgentTokenAuthRejectReason::Invalid);
        };
        AgentTokenAuthResult::Authenticated(AuthenticatedAgentToken {
            token_id,
            label,
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

    fn propagate_revoke_to_descendants(
        &mut self,
        root_token_id: &str,
        revoked_at: SystemTime,
        reason: Option<String>,
    ) {
        let inherited_reason =
            reason.unwrap_or_else(|| format!("ancestor_revoked:{root_token_id}"));
        let mut queue = vec![root_token_id.to_string()];
        let mut visited = HashSet::<String>::new();
        while let Some(parent) = queue.pop() {
            if !visited.insert(parent.clone()) {
                continue;
            }
            let descendants = self
                .agent_tokens
                .iter()
                .filter_map(|(token_id, token)| {
                    if token.parent_token_id.as_deref() == Some(parent.as_str()) {
                        Some(token_id.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            for token_id in descendants {
                if let Some(token) = self.agent_tokens.get_mut(&token_id) {
                    refresh_agent_token_status(token, revoked_at);
                    if matches!(
                        token.status,
                        AgentTokenStatus::Revoked | AgentTokenStatus::Deleted
                    ) {
                        queue.push(token_id.clone());
                        continue;
                    }
                    token.status = AgentTokenStatus::Revoked;
                    token.revoked_at = Some(revoked_at);
                    token.revoke_reason = Some(inherited_reason.clone());
                }
                queue.push(token_id);
            }
        }
    }

    fn agent_token_summary(
        &mut self,
        token_id: &str,
    ) -> Result<Option<AgentTokenSummary>, VaultError> {
        let (now, _) = self.token_effective_now();
        self.agent_token_summary_with_now(token_id, now)
    }

    fn agent_token_summary_with_now(
        &mut self,
        token_id: &str,
        now: SystemTime,
    ) -> Result<Option<AgentTokenSummary>, VaultError> {
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
        let status = agent_token_projected_status(&record);
        let token_fingerprint = agent_token_display_fingerprint(&record);
        Ok(Some(AgentTokenSummary {
            token_id: record.token_id,
            label: record.label,
            token_fingerprint,
            principal_summary: record.principal_id,
            status,
            scope_profile: record.scope_profile,
            target_scope_summary: format!("targets:{}", scope.target_ids.len()),
            active_scope_version: record.active_scope_version,
            created_at: record.created_at,
            last_used_at: record.last_used_at,
            expires_at: record.expires_at,
            revoked_at: record.revoked_at,
            revoke_reason: record.revoke_reason,
            deleted_at: record.deleted_at,
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
        let canonical = normalize_local_admin_target(&action_kind, target_object_ref)?;
        let payload_digest = payload_digest_for_local_admin_action(&action_kind, &canonical, None);
        self.create_local_admin_intent_with_digest(
            action_kind,
            &canonical,
            &payload_digest,
            requested_by_principal,
            ttl,
        )
    }

    pub fn create_local_admin_intent_with_digest(
        &mut self,
        action_kind: LocalAdminActionKind,
        target_object_ref: &str,
        requested_payload_digest: &str,
        requested_by_principal: &str,
        ttl: Duration,
    ) -> Result<LocalAdminActionIntent, VaultError> {
        let target = normalize_local_admin_target(&action_kind, target_object_ref)?;
        let requested_by = requested_by_principal.trim();
        if requested_by.is_empty() {
            return Err(VaultError::LocalAdminIntentMismatch(
                "requested_by principal must be non-empty".into(),
            ));
        }
        let payload_digest =
            normalize_payload_digest(requested_payload_digest).ok_or_else(|| {
                VaultError::LocalAdminIntentMismatch("payload digest must be non-empty".into())
            })?;
        self.next_intent_seq += 1;
        let now = SystemTime::now();
        let intent = LocalAdminActionIntent {
            intent_id: format!("{LOCAL_ADMIN_INTENT_ID_PREFIX}{:06}", self.next_intent_seq),
            action_kind,
            target_object_ref: target,
            requested_by_principal: requested_by.to_string(),
            requested_payload_digest: payload_digest,
            created_at: now,
            expires_at: now + ttl,
            status: LocalAdminIntentStatus::Pending,
        };
        self.intents
            .insert(intent.intent_id.clone(), intent.clone());
        self.persist_metadata_db()?;
        Ok(intent)
    }

    pub fn complete_local_admin_attestation(
        &mut self,
        intent_id: &str,
        verified_principal: &str,
        verification_method: &str,
        ttl: Duration,
    ) -> Result<LocalAdminAttestationRecord, VaultError> {
        if !is_generated_local_admin_id(intent_id, LOCAL_ADMIN_INTENT_ID_PREFIX) {
            return Err(VaultError::LocalAdminIntentMismatch(
                "synthetic or placeholder intent id is not accepted".into(),
            ));
        }
        let verified_principal = verified_principal.trim();
        if verified_principal.is_empty() {
            return Err(VaultError::LocalAdminIntentMismatch(
                "verified_principal must be non-empty".into(),
            ));
        }
        let verification_method = verification_method.trim();
        if verification_method.is_empty() {
            return Err(VaultError::LocalAdminIntentMismatch(
                "verification_method must be non-empty".into(),
            ));
        }
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
            attestation_id: format!(
                "{LOCAL_ADMIN_ATTESTATION_ID_PREFIX}{:06}",
                self.next_attestation_seq
            ),
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
        self.persist_metadata_db()?;
        Ok(attestation)
    }

    pub fn verify_local_admin_intent(
        &mut self,
        intent_id: &str,
        verified_principal: &str,
        verification_method: &str,
        ttl: Duration,
    ) -> Result<LocalAdminAttestationRecord, VaultError> {
        self.complete_local_admin_attestation(
            intent_id,
            verified_principal,
            verification_method,
            ttl,
        )
    }

    pub fn unlock_vault_with_attestation(
        &mut self,
        request: UnlockVaultRequest,
    ) -> Result<(), VaultError> {
        let method = normalize_vault_method_label(&request.method);
        let requested_by = request.requested_by.trim();
        if requested_by.is_empty() {
            return Err(VaultError::LocalAdminIntentMismatch(
                "requested_by principal must be non-empty".into(),
            ));
        }
        let payload_digest = payload_digest_for_unlock_request(&method);
        let validated = self.validate_local_admin_attestation(
            LocalAdminActionKind::UnlockVault,
            LOCAL_ADMIN_UNLOCK_VAULT_TARGET,
            &payload_digest,
            requested_by,
            &request.attestation_id,
            None,
        )?;
        self.consume_local_admin_attestation(&validated)?;
        match method.as_str() {
            "os-native" => self.unlock_with_os_native(),
            "passphrase" => {
                self.unlock_with_passphrase(request.passphrase.as_deref().unwrap_or(""))
            }
            other => Err(VaultError::UnlockMethodNotAllowed(other.to_string())),
        }
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
        let payload_digest = payload_digest_for_local_admin_action(
            &LocalAdminActionKind::RevealSecret,
            &canonical,
            None,
        );
        let validated = self.validate_local_admin_attestation(
            LocalAdminActionKind::RevealSecret,
            &canonical,
            &payload_digest,
            requester_principal,
            attestation_id,
            Some(intent_id),
        )?;
        let secret = self.decrypt_active_secret(&canonical)?;
        self.consume_local_admin_attestation(&validated)?;
        Ok(secret)
    }

    pub fn export_for_local_admin(
        &mut self,
        reference: &str,
        intent_id: &str,
        attestation_id: &str,
        requester_principal: &str,
    ) -> Result<SecretBytes, VaultError> {
        let canonical = normalize_credential_ref(reference)?;
        self.ensure_backend_allows_secret_use()?;
        let payload_digest = payload_digest_for_local_admin_action(
            &LocalAdminActionKind::ExportSecret,
            &canonical,
            None,
        );
        let validated = self.validate_local_admin_attestation(
            LocalAdminActionKind::ExportSecret,
            &canonical,
            &payload_digest,
            requester_principal,
            attestation_id,
            Some(intent_id),
        )?;
        let secret = self.decrypt_active_secret(&canonical)?;
        self.consume_local_admin_attestation(&validated)?;
        Ok(secret)
    }

    fn validate_local_admin_attestation(
        &mut self,
        action_kind: LocalAdminActionKind,
        target_object_ref: &str,
        payload_digest: &str,
        requester_principal: &str,
        attestation_id: &str,
        expected_intent_id: Option<&str>,
    ) -> Result<ValidatedLocalAdminAttestation, VaultError> {
        if !is_generated_local_admin_id(attestation_id, LOCAL_ADMIN_ATTESTATION_ID_PREFIX) {
            return Err(VaultError::LocalAdminAttestationMismatch(
                "synthetic or placeholder attestation id is not accepted".into(),
            ));
        }
        let normalized_target = normalize_local_admin_target(&action_kind, target_object_ref)?;
        let normalized_digest = normalize_payload_digest(payload_digest).ok_or_else(|| {
            VaultError::LocalAdminIntentMismatch("payload digest must be non-empty".into())
        })?;
        let now = SystemTime::now();
        let attestation_snapshot = self
            .attestations
            .get(attestation_id)
            .cloned()
            .ok_or_else(|| VaultError::LocalAdminAttestationMismatch(attestation_id.to_string()))?;
        if let Some(expected) = expected_intent_id {
            if attestation_snapshot.intent_id != expected {
                return Err(VaultError::LocalAdminAttestationMismatch(
                    attestation_id.to_string(),
                ));
            }
        }
        if attestation_snapshot.verified_principal != requester_principal {
            return Err(VaultError::LocalAdminAttestationMismatch(
                attestation_id.to_string(),
            ));
        }
        if now > attestation_snapshot.expires_at {
            if let Some(attestation) = self.attestations.get_mut(attestation_id) {
                attestation.status = LocalAdminAttestationStatus::Expired;
            }
            self.persist_metadata_db()?;
            return Err(VaultError::LocalAdminAttestationExpired(
                attestation_id.to_string(),
            ));
        }
        if !matches!(
            attestation_snapshot.status,
            LocalAdminAttestationStatus::Active
        ) {
            return Err(VaultError::LocalAdminAttestationMismatch(
                attestation_id.to_string(),
            ));
        }

        let intent_id = attestation_snapshot.intent_id.clone();
        if !is_generated_local_admin_id(&intent_id, LOCAL_ADMIN_INTENT_ID_PREFIX) {
            return Err(VaultError::LocalAdminIntentMismatch(
                "synthetic or placeholder intent id is not accepted".into(),
            ));
        }
        let intent_snapshot = self
            .intents
            .get(&intent_id)
            .cloned()
            .ok_or_else(|| VaultError::LocalAdminIntentMismatch(intent_id.clone()))?;
        if intent_snapshot.requested_by_principal != requester_principal {
            return Err(VaultError::LocalAdminIntentMismatch(intent_id));
        }
        if now > intent_snapshot.expires_at {
            if let Some(intent) = self.intents.get_mut(&intent_id) {
                intent.status = LocalAdminIntentStatus::Expired;
            }
            self.persist_metadata_db()?;
            return Err(VaultError::LocalAdminIntentExpired(intent_id));
        }
        if !matches!(intent_snapshot.status, LocalAdminIntentStatus::Verified) {
            return Err(VaultError::LocalAdminVerificationRequired(
                "intent must be verified before action".into(),
            ));
        }
        if intent_snapshot.action_kind != action_kind
            || intent_snapshot.target_object_ref != normalized_target
            || intent_snapshot.requested_payload_digest != normalized_digest
        {
            return Err(VaultError::LocalAdminIntentMismatch(
                intent_snapshot.intent_id,
            ));
        }
        Ok(ValidatedLocalAdminAttestation {
            intent_id: intent_snapshot.intent_id,
            attestation_id: attestation_snapshot.attestation_id,
        })
    }

    fn consume_local_admin_attestation(
        &mut self,
        validated: &ValidatedLocalAdminAttestation,
    ) -> Result<(), VaultError> {
        let now = SystemTime::now();
        if let Some(intent) = self.intents.get_mut(&validated.intent_id) {
            intent.status = LocalAdminIntentStatus::Consumed;
        }
        if let Some(attestation) = self.attestations.get_mut(&validated.attestation_id) {
            attestation.status = LocalAdminAttestationStatus::Consumed;
            attestation.consumed_at = Some(now);
        }
        self.persist_metadata_db()
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

    pub fn list_secret_summaries(&self) -> Vec<VaultSecretSummary> {
        let mut items = self
            .secrets
            .values()
            .map(|state| VaultSecretSummary {
                reference: state.record.reference.clone(),
                kind: state.record.kind.clone(),
                label: state.record.label.clone(),
                status: state.record.status.clone(),
                active_version_id: state.record.active_version_id.clone(),
            })
            .collect::<Vec<_>>();
        items.sort_by(|left, right| left.reference.cmp(&right.reference));
        items
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
        let mut status = backend.readiness();
        if matches!(self.lock_state, VaultLockState::Unavailable) {
            status = VaultReadinessState::Unsupported;
        }
        let fail_closed = self.should_fail_closed(status.clone())
            || !matches!(self.lock_state, VaultLockState::Unlocked);
        let protectors = self.protector_assembly_for_backend(&self.active_backend);
        Ok(VaultReadinessDiagnostic {
            backend: self.active_backend.clone(),
            status,
            message: format!(
                "{}; lock_state={}; unlock_policy={}",
                backend.readiness_message(),
                self.lock_state.as_str(),
                self.unlock_policy.trigger_policy.as_str()
            ),
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
        self.prune_expired_ssh_broker_sessions();
        if request.runtime_passphrase_requested {
            return Err(VaultError::SshRuntimePassphraseForbidden(
                "runtime ssh key passphrase prompts are forbidden; passphrase must be handled during import/unlock".into(),
            ));
        }

        let canonical_ref = normalize_credential_ref(&request.credential_ref)?;
        self.refresh_unlock_session_ttl();
        match self.lock_state {
            VaultLockState::Unlocked => {}
            VaultLockState::Locked => {
                return Err(VaultError::VaultLocked(
                    "vault is locked; secret-backed ssh delivery requires explicit unlock".into(),
                ))
            }
            VaultLockState::Unlocking => {
                return Err(VaultError::VaultLocked(
                    "vault is currently unlocking; retry ssh delivery after unlock completes"
                        .into(),
                ))
            }
            VaultLockState::Unavailable | VaultLockState::Uninitialized => {
                return Err(VaultError::VaultUnavailable(
                    "vault is unavailable for secret-backed ssh delivery".into(),
                ))
            }
        }
        self.ensure_backend_allows_secret_use()?;
        let lease = self.use_for_ssh_auth(
            &canonical_ref,
            format!("target:{}:{}", request.target_id, request.principal_id),
        )?;
        let secret_reference = lease.reference().to_string();
        let secret_version_id = lease.version_id().to_string();

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
        let (endpoint_kind, degraded, endpoint_locator, cleanup_paths, mut warning_lines) = if matches!(
            preferred.status,
            VaultReadinessState::Ready
        ) {
            let endpoint = ssh_endpoint_locator_for_session(&session_id, &preferred.endpoint_kind);
            (
                preferred.endpoint_kind.clone(),
                false,
                Some(endpoint),
                Vec::new(),
                vec![format!(
                    "ssh-agent-compatible delivery is ready on platform {} via {}",
                    preferred.platform,
                    preferred.endpoint_kind.as_str()
                )],
            )
        } else if request.allow_identity_fallback {
            let fallback_kind = SshAgentBrokerEndpointKind::IdentityFileFallback;
            let (endpoint, fallback_cleanup_paths) =
                self.prepare_ephemeral_identity_file(&session_id, &lease)?;
            (
                fallback_kind,
                true,
                Some(endpoint),
                fallback_cleanup_paths,
                vec![
                    format!(
                        "ssh-agent-compatible delivery is {} on platform {}; falling back to ephemeral identity file",
                        readiness_label(&preferred.status),
                        preferred.platform
                    ),
                    "identity-file fallback is degraded by design and uses private runtime cleanup paths".to_string(),
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
            "vault secret binding: reference={} active_version={}",
            secret_reference, secret_version_id
        ));
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
                cleanup_paths,
                diagnostics: warning_lines.clone(),
                principal_id: request.principal_id,
                logical_session_id: request.logical_session_id,
                secret_reference,
                secret_version_id,
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
        self.prune_expired_ssh_broker_sessions();
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
        self.prune_expired_ssh_broker_sessions();
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
        self.prune_expired_ssh_broker_sessions();
        let stored = self
            .ssh_broker_sessions
            .get_mut(broker_session_id)
            .ok_or_else(|| VaultError::SshBrokerSessionNotFound(broker_session_id.into()))?;
        stored.attached_channels.clear();
        stored.session.attached_channel_count = 0;
        stored.session.state = SshAgentBrokerSessionState::Closed;
        stored.session.cleanup_deadline = Some(SystemTime::now());
        cleanup_stored_ssh_session_artifacts(stored);
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
        lines.push(format!(
            "vault binding: {}@{}",
            stored.secret_reference, stored.secret_version_id
        ));
        lines.push(format!("session state: {}", stored.session.state.as_str()));
        if let Some(deadline) = stored.session.cleanup_deadline {
            lines.push(format!(
                "cleanup deadline unix_sec={}",
                system_time_to_unix_secs(deadline)
            ));
        }
        lines.push(format!("principal scope: {}", stored.principal_id));
        if let Some(logical) = stored.logical_session_id.as_deref() {
            lines.push(format!("logical session scope: {logical}"));
        }
        Ok(lines)
    }

    fn prepare_ephemeral_identity_file(
        &self,
        session_id: &str,
        lease: &BrokeredSecretLease,
    ) -> Result<(String, Vec<PathBuf>), VaultError> {
        let runtime_root = self
            .storage_layout
            .as_ref()
            .map(|layout| layout.root_dir.join("runtime/ssh-broker").join(session_id))
            .unwrap_or_else(|| {
                std::env::temp_dir()
                    .join("bridgingio-vault-runtime")
                    .join("ssh-broker")
                    .join(session_id)
            });
        fs::create_dir_all(&runtime_root).map_err(|err| {
            VaultError::StorageIo(format!(
                "failed to create ssh broker runtime dir {}: {err}",
                runtime_root.display()
            ))
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&runtime_root, fs::Permissions::from_mode(0o700));
        }

        let identity_path = runtime_root.join("identity");
        let mut identity_bytes = Vec::new();
        lease.with_secret_bytes(|bytes| identity_bytes.extend_from_slice(bytes));
        fs::write(&identity_path, identity_bytes).map_err(|err| {
            VaultError::StorageIo(format!(
                "failed to write ephemeral identity file {}: {err}",
                identity_path.display()
            ))
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&identity_path, fs::Permissions::from_mode(0o600));
        }

        Ok((
            identity_path.to_string_lossy().to_string(),
            vec![identity_path, runtime_root],
        ))
    }

    fn prune_expired_ssh_broker_sessions(&mut self) {
        let now = SystemTime::now();
        let expired_ids = self
            .ssh_broker_sessions
            .iter()
            .filter_map(|(session_id, stored)| {
                let ttl_expired = stored
                    .session
                    .expires_at
                    .map(|expires_at| now > expires_at)
                    .unwrap_or(false);
                let cleanup_due = stored
                    .session
                    .cleanup_deadline
                    .map(|deadline| now >= deadline)
                    .unwrap_or(false);
                if cleanup_due || (ttl_expired && stored.session.attached_channel_count == 0) {
                    Some(session_id.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        for session_id in expired_ids {
            if let Some(stored) = self.ssh_broker_sessions.get_mut(&session_id) {
                if !matches!(stored.session.state, SshAgentBrokerSessionState::Closed) {
                    stored.session.state = SshAgentBrokerSessionState::Closed;
                }
                stored.session.cleanup_deadline = Some(now);
                cleanup_stored_ssh_session_artifacts(stored);
                stored
                    .diagnostics
                    .push("broker cleanup complete: deadline reached".to_string());
            }
        }
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
        self.enforce_unlock_gate_for_secret_use()?;
        let now = SystemTime::now();
        let root_key = self.ensure_unlocked_root_key()?;
        let plaintext = {
            let state = self
                .secrets
                .get_mut(canonical_reference)
                .ok_or_else(|| VaultError::SecretNotFound(canonical_reference.to_string()))?;
            let active_id = state.record.active_version_id.clone().ok_or_else(|| {
                VaultError::SecretVersionNotFound(canonical_reference.to_string())
            })?;
            let material = state
                .versions
                .iter()
                .find(|entry| entry.version.version_id == active_id)
                .ok_or_else(|| VaultError::SecretVersionNotFound(active_id.clone()))?;
            let dek = open_aead(
                &root_key,
                format!("vrk-wrap:{canonical_reference}:{active_id}").as_bytes(),
                &material.wrapped_dek,
            )?;
            let plaintext = open_aead(
                &dek,
                format!("secret:{canonical_reference}:{active_id}").as_bytes(),
                &material.ciphertext,
            )?;
            state.record.last_used_at = Some(now);
            plaintext
        };
        self.key_envelope.last_unlocked_at = Some(now);
        if matches!(
            self.unlock_policy.trigger_policy,
            VaultUnlockTriggerPolicy::OnEverySecretAccess
        ) {
            self.lock_vault_internal("on-every-secret-access");
        }
        self.persist_metadata_db()?;
        Ok(SecretBytes::from_bytes(plaintext))
    }

    fn ensure_backend_allows_secret_use(&mut self) -> Result<(), VaultError> {
        self.enforce_unlock_gate_for_secret_use()?;
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
            "builtin-encrypted" | "os-native" => {
                let os_native_status = if os_native_platform_binding_ready() {
                    VaultReadinessState::Ready
                } else {
                    VaultReadinessState::Degraded
                };
                let os_native_message = if matches!(os_native_status, VaultReadinessState::Ready) {
                    "primary protector is backed by platform-native credential storage"
                } else {
                    "primary protector is running in degraded compatibility mode"
                };
                vec![
                    ProtectorReadinessDiagnostic {
                        protector: "os-native".into(),
                        status: os_native_status,
                        message: os_native_message.into(),
                    },
                    ProtectorReadinessDiagnostic {
                        protector: "passphrase".into(),
                        status: VaultReadinessState::Fallback,
                        message: "recovery protector is available for headless flows".into(),
                    },
                ]
            }
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

    fn enforce_unlock_gate_for_secret_use(&mut self) -> Result<(), VaultError> {
        self.refresh_unlock_session_ttl();
        match self.lock_state {
            VaultLockState::Uninitialized => {
                return Err(VaultError::VaultUnavailable(
                    "vault is uninitialized; unlock is unavailable".into(),
                ))
            }
            VaultLockState::Unavailable => {
                return Err(VaultError::VaultUnavailable(
                    "vault protectors are unavailable and policy is fail-closed".into(),
                ))
            }
            VaultLockState::Unlocking => {
                return Err(VaultError::VaultLocked(
                    "vault is currently in unlocking transition".into(),
                ))
            }
            VaultLockState::Unlocked => return Ok(()),
            VaultLockState::Locked => {}
        }

        match self.unlock_policy.trigger_policy {
            VaultUnlockTriggerPolicy::ManualOnly => Err(VaultError::VaultLocked(
                "vault is locked and trigger policy is manual-only".into(),
            )),
            VaultUnlockTriggerPolicy::OnCoreStart => Err(VaultError::VaultLocked(
                "vault is locked; on-core-start unlock did not complete".into(),
            )),
            VaultUnlockTriggerPolicy::OnFirstSecretAccess
            | VaultUnlockTriggerPolicy::OnEverySecretAccess => {
                let preferred = self.unlock_policy.preferred_method.clone();
                self.try_unlock_with_method(&preferred)?;
                if matches!(self.lock_state, VaultLockState::Unlocked) {
                    Ok(())
                } else {
                    Err(VaultError::VaultLocked(
                        "vault remains locked after unlock trigger".into(),
                    ))
                }
            }
        }
    }

    fn refresh_unlock_session_ttl(&mut self) {
        let Some(session) = self.unlock_session.as_ref() else {
            return;
        };
        if let Some(expires_at) = session.expires_at {
            if SystemTime::now() >= expires_at {
                self.lock_vault_internal("unlock-cache-ttl-expired");
            }
        }
    }

    fn ensure_unlocked_root_key(&mut self) -> Result<Vec<u8>, VaultError> {
        self.refresh_unlock_session_ttl();
        if let Some(root_key) = self.root_key_cache.as_ref() {
            return Ok(root_key.expose_for_use().to_vec());
        }
        Err(VaultError::VaultLocked(
            "vault root key is not available in unlocked cache".into(),
        ))
    }

    fn try_unlock_with_method(&mut self, method: &str) -> Result<(), VaultError> {
        self.try_unlock_with_method_and_secret(method, None)
    }

    fn try_unlock_with_method_verified(&mut self, method: &str) -> Result<(), VaultError> {
        self.try_unlock_with_method_and_secret_mode(method, None, true)
    }

    fn try_unlock_with_method_and_secret(
        &mut self,
        method: &str,
        secret: Option<&str>,
    ) -> Result<(), VaultError> {
        self.try_unlock_with_method_and_secret_mode(method, secret, false)
    }

    fn try_unlock_with_method_and_secret_mode(
        &mut self,
        method: &str,
        secret: Option<&str>,
        require_verified_os_native: bool,
    ) -> Result<(), VaultError> {
        if matches!(self.lock_state, VaultLockState::Uninitialized) {
            return Err(VaultError::VaultUnavailable(
                "vault is uninitialized; run vault init before unlock".into(),
            ));
        }
        let method = normalize_vault_method_label(method);
        if !self
            .unlock_policy
            .allowed_methods
            .iter()
            .any(|allowed| normalize_vault_method_label(allowed) == method)
        {
            return Err(VaultError::UnlockMethodNotAllowed(method));
        }
        self.lock_state = VaultLockState::Unlocking;
        let now = SystemTime::now();
        let unlock_result = match method.as_str() {
            "os-native" => {
                if require_verified_os_native {
                    self.try_unlock_with_os_native_verified_internal()
                } else {
                    self.try_unlock_with_os_native_internal()
                }
            }
            "passphrase" => {
                let passphrase = secret.ok_or(VaultError::PassphraseRejected)?;
                self.try_unlock_with_passphrase_internal(passphrase)
            }
            other => Err(VaultError::UnlockMethodNotAllowed(other.to_string())),
        };
        match unlock_result {
            Ok(root_key) => {
                self.root_key_cache = Some(SecretBytes::from_bytes(root_key));
                self.lock_state = VaultLockState::Unlocked;
                let expires_at = if self.unlock_policy.cache_ttl_sec == 0 {
                    None
                } else {
                    Some(now + Duration::from_secs(self.unlock_policy.cache_ttl_sec))
                };
                self.unlock_session = Some(VaultUnlockSession {
                    method,
                    unlocked_at: now,
                    expires_at,
                });
                self.key_envelope.last_unlocked_at = Some(now);
                Ok(())
            }
            Err(err) => {
                self.root_key_cache = None;
                self.unlock_session = None;
                self.lock_state = match err {
                    VaultError::VaultUnavailable(_) => VaultLockState::Unavailable,
                    _ => VaultLockState::Locked,
                };
                Err(err)
            }
        }
    }

    fn try_unlock_with_os_native_internal(&mut self) -> Result<Vec<u8>, VaultError> {
        let manifest = self
            .wrap_manifests
            .iter()
            .find(|item| {
                item.vault_key_id == self.key_envelope.vault_key_id
                    && normalize_vault_method_label(&item.protector_binding) == "os-native"
            })
            .cloned()
            .ok_or_else(|| {
                VaultError::VaultUnavailable(
                    "no os-native protector wrap is registered for vault root key".into(),
                )
            })?;
        let wrapped = self.read_wrap_blob(&manifest.wrapped_key_locator)?;
        let aad = format!("root:{}:{}", manifest.vault_key_id, manifest.wrap_id);
        let primary_kek = protector_kek("os-native");
        if let Ok(root_key) = open_aead(&primary_kek, aad.as_bytes(), &wrapped) {
            return Ok(root_key);
        }
        let fallback_kek = fallback_protector_kek("os-native");
        if fallback_kek != primary_kek {
            if let Ok(root_key) = open_aead(&fallback_kek, aad.as_bytes(), &wrapped) {
                return Ok(root_key);
            }
        }
        Err(VaultError::UnlockFailed(
            "os-native protector could not unwrap the canonical vault root key".into(),
        ))
    }

    fn try_unlock_with_os_native_verified_internal(&mut self) -> Result<Vec<u8>, VaultError> {
        let manifest = self
            .wrap_manifests
            .iter()
            .find(|item| {
                item.vault_key_id == self.key_envelope.vault_key_id
                    && normalize_vault_method_label(&item.protector_binding) == "os-native"
            })
            .cloned()
            .ok_or_else(|| {
                VaultError::VaultUnavailable(
                    "no os-native protector wrap is registered for vault root key".into(),
                )
            })?;
        let wrapped = self.read_wrap_blob(&manifest.wrapped_key_locator)?;
        let kek = os_native_protector_kek_verified().ok_or_else(|| {
            VaultError::UnlockFailed(
                "os-native unlock verification did not complete or was cancelled".into(),
            )
        })?;
        self.unwrap_os_native_root_key_with_verified_kek(&manifest, &wrapped, &kek)
    }

    fn unwrap_os_native_root_key_with_verified_kek(
        &mut self,
        manifest: &ProtectorWrapManifest,
        wrapped: &[u8],
        verified_kek: &[u8],
    ) -> Result<Vec<u8>, VaultError> {
        let aad = format!("root:{}:{}", manifest.vault_key_id, manifest.wrap_id);
        if let Ok(root_key) = open_aead(verified_kek, aad.as_bytes(), wrapped) {
            return Ok(root_key);
        }

        // Compatibility path:
        // Older wraps can be sealed with the deterministic fallback KEK when
        // os-native keyring probing timed out. If local verification succeeded,
        // we unwrap once with legacy fallback and rewrap using verified os-native.
        let legacy_root_key = open_aead(
            fallback_protector_kek("os-native").as_slice(),
            aad.as_bytes(),
            wrapped,
        )
        .map_err(|_| {
            VaultError::UnlockFailed(
                "os-native protector could not unwrap the canonical vault root key".into(),
            )
        })?;

        let rewrapped = seal_aead(verified_kek, aad.as_bytes(), &legacy_root_key);
        if self
            .write_wrap_blob(&manifest.wrapped_key_locator, rewrapped.clone())
            .is_ok()
        {
            self.upsert_wrap_manifest(ProtectorWrapManifest {
                wrapped_key_digest: short_digest(&rewrapped),
                last_verified_at: Some(SystemTime::now()),
                status: ProtectorWrapStatus::Ready,
                ..manifest.clone()
            });
        }

        Ok(legacy_root_key)
    }

    fn try_unlock_with_passphrase_internal(
        &mut self,
        passphrase: &str,
    ) -> Result<Vec<u8>, VaultError> {
        if !self.passphrase.enabled {
            return Err(VaultError::PassphraseNotConfigured);
        }
        let manifest = self
            .wrap_manifests
            .iter()
            .find(|item| {
                item.vault_key_id == self.key_envelope.vault_key_id
                    && item.wrap_id == self.passphrase.wrap_id
            })
            .cloned()
            .ok_or(VaultError::PassphraseNotConfigured)?;
        let salt_locator = self.passphrase.salt_locator.clone();
        let salt = self.read_wrap_blob(&salt_locator)?;
        let wrapped = self.read_wrap_blob(&manifest.wrapped_key_locator)?;
        let kek = derive_argon2id_kek(passphrase, &salt, &self.passphrase.params)?;
        open_aead(
            &kek,
            format!("root:{}:{}", manifest.vault_key_id, manifest.wrap_id).as_bytes(),
            &wrapped,
        )
        .map_err(|_| VaultError::PassphraseRejected)
    }

    fn lock_vault_internal(&mut self, _reason: &str) {
        if matches!(self.lock_state, VaultLockState::Uninitialized) {
            return;
        }
        self.root_key_cache = None;
        self.unlock_session = None;
        self.lock_state = VaultLockState::Locked;
    }

    fn upsert_wrap_manifest(&mut self, manifest: ProtectorWrapManifest) {
        if let Some(existing) = self
            .wrap_manifests
            .iter_mut()
            .find(|entry| entry.wrap_id == manifest.wrap_id)
        {
            *existing = manifest;
            return;
        }
        self.wrap_manifests.push(manifest);
    }

    fn read_wrap_blob(&mut self, locator: &str) -> Result<Vec<u8>, VaultError> {
        if let Some(blob) = self.wrap_blob_cache.get(locator) {
            return Ok(blob.clone());
        }
        if let Some(layout) = self.storage_layout.as_ref() {
            let path = layout.path_for_blob_uri(locator)?;
            let payload = fs::read(&path).map_err(|err| {
                VaultError::StorageIo(format!(
                    "failed to read wrap blob {}: {err}",
                    path.display()
                ))
            })?;
            self.wrap_blob_cache
                .insert(locator.to_string(), payload.clone());
            return Ok(payload);
        }
        Err(VaultError::StorageIo(format!(
            "wrap blob {locator} is unavailable in memory and no storage layout is attached"
        )))
    }

    fn write_wrap_blob(&mut self, locator: &str, payload: Vec<u8>) -> Result<(), VaultError> {
        self.wrap_blob_cache
            .insert(locator.to_string(), payload.clone());
        if let Some(layout) = self.storage_layout.as_ref() {
            write_blob(layout.path_for_blob_uri(locator)?, &payload)?;
        }
        Ok(())
    }

    fn rewrap_root_key_for_protectors(&mut self, root_key: &[u8]) -> Result<(), VaultError> {
        let now = SystemTime::now();
        let os_manifest = self
            .wrap_manifests
            .iter()
            .find(|manifest| {
                manifest.vault_key_id == self.key_envelope.vault_key_id
                    && normalize_vault_method_label(&manifest.protector_binding) == "os-native"
            })
            .cloned()
            .unwrap_or(ProtectorWrapManifest {
                format_version: VAULT_OBJECT_FORMAT_VERSION,
                wrap_id: "wrap-000001".into(),
                vault_key_id: self.key_envelope.vault_key_id.clone(),
                protector_binding: "os-native".into(),
                wrap_format: "sha256-stream-aead-v1".into(),
                wrapped_key_locator: "blob://vault/wrap/wrap-000001.vrk.bin".into(),
                wrapped_key_digest: String::new(),
                created_at: now,
                last_verified_at: None,
                status: ProtectorWrapStatus::Degraded,
            });
        let wrapped = seal_aead(
            &protector_kek("os-native"),
            format!("root:{}:{}", os_manifest.vault_key_id, os_manifest.wrap_id).as_bytes(),
            root_key,
        );
        self.write_wrap_blob(&os_manifest.wrapped_key_locator, wrapped.clone())?;
        self.upsert_wrap_manifest(ProtectorWrapManifest {
            wrapped_key_digest: short_digest(&wrapped),
            last_verified_at: Some(now),
            ..os_manifest
        });
        Ok(())
    }

    fn bootstrap_root_key_for_protectors_without_keyring(
        &mut self,
        root_key: &[u8],
    ) -> Result<(), VaultError> {
        let now = SystemTime::now();
        let os_manifest = self
            .wrap_manifests
            .iter()
            .find(|manifest| {
                manifest.vault_key_id == self.key_envelope.vault_key_id
                    && normalize_vault_method_label(&manifest.protector_binding) == "os-native"
            })
            .cloned()
            .unwrap_or(ProtectorWrapManifest {
                format_version: VAULT_OBJECT_FORMAT_VERSION,
                wrap_id: "wrap-000001".into(),
                vault_key_id: self.key_envelope.vault_key_id.clone(),
                protector_binding: "os-native".into(),
                wrap_format: "sha256-stream-aead-v1".into(),
                wrapped_key_locator: "blob://vault/wrap/wrap-000001.vrk.bin".into(),
                wrapped_key_digest: String::new(),
                created_at: now,
                last_verified_at: None,
                status: ProtectorWrapStatus::Degraded,
            });
        let wrapped = seal_aead(
            &fallback_protector_kek("os-native"),
            format!("root:{}:{}", os_manifest.vault_key_id, os_manifest.wrap_id).as_bytes(),
            root_key,
        );
        self.write_wrap_blob(&os_manifest.wrapped_key_locator, wrapped.clone())?;
        self.upsert_wrap_manifest(ProtectorWrapManifest {
            wrapped_key_digest: short_digest(&wrapped),
            last_verified_at: None,
            status: ProtectorWrapStatus::Degraded,
            ..os_manifest
        });
        Ok(())
    }

    fn load_or_initialize_metadata_db(&mut self) -> Result<(), VaultError> {
        let Some(layout) = self.storage_layout.clone() else {
            return Ok(());
        };
        if layout.metadata_db_path.exists() {
            let raw = fs::read_to_string(&layout.metadata_db_path).map_err(|err| {
                VaultError::StorageIo(format!(
                    "failed to read metadata db {}: {err}",
                    layout.metadata_db_path.display()
                ))
            })?;
            let schema_version = detect_schema_version(&raw)?;
            match schema_version {
                CANONICAL_METADATA_SCHEMA_VERSION => {
                    let db: VaultMetadataDbV2 = serde_json::from_str(&raw).map_err(|err| {
                        VaultError::StorageIo(format!(
                            "failed to parse metadata db v2 {}: {err}",
                            layout.metadata_db_path.display()
                        ))
                    })?;
                    self.apply_metadata_db_v2(&layout, db)?;
                    return Ok(());
                }
                LEGACY_METADATA_SCHEMA_VERSION => {
                    let db_v1: VaultMetadataDbV1 = serde_json::from_str(&raw).map_err(|err| {
                        VaultError::StorageIo(format!(
                            "failed to parse metadata db v1 {}: {err}",
                            layout.metadata_db_path.display()
                        ))
                    })?;
                    let upgraded = self.migrate_v1_to_v2(&layout, db_v1)?;
                    self.apply_metadata_db_v2(&layout, upgraded)?;
                    return Ok(());
                }
                other => {
                    return Err(VaultError::StorageIo(format!(
                        "unsupported metadata schema version {other}; expected 1 or 2"
                    )))
                }
            }
        }

        if layout.legacy_metadata_path.exists() {
            let raw = fs::read_to_string(&layout.legacy_metadata_path).map_err(|err| {
                VaultError::StorageIo(format!(
                    "failed to read legacy metadata {}: {err}",
                    layout.legacy_metadata_path.display()
                ))
            })?;
            let db_v1: VaultMetadataDbV1 = serde_json::from_str(&raw).map_err(|err| {
                VaultError::StorageIo(format!(
                    "failed to parse legacy metadata {}: {err}",
                    layout.legacy_metadata_path.display()
                ))
            })?;
            let upgraded = self.migrate_v1_to_v2(&layout, db_v1)?;
            self.apply_metadata_db_v2(&layout, upgraded)?;
            return Ok(());
        }
        self.lock_state = VaultLockState::Uninitialized;
        self.root_key_cache = None;
        self.unlock_session = None;
        Ok(())
    }

    fn apply_metadata_db_v2(
        &mut self,
        layout: &VaultStorageLayout,
        db: VaultMetadataDbV2,
    ) -> Result<(), VaultError> {
        let VaultMetadataDbV2 {
            schema_version: _,
            active_backend,
            allow_degraded_mode,
            lock_state,
            unlock_policy,
            unlock_session,
            passphrase,
            next_version_seq,
            next_audit_seq,
            next_agent_token_seq,
            next_intent_seq,
            next_attestation_seq,
            next_ssh_broker_seq,
            clock_watermark_unix_sec,
            key_envelope,
            wrap_manifests,
            secrets,
            agent_tokens,
            token_scopes,
            intents,
            attestations,
        } = db;
        self.active_backend = active_backend;
        self.allow_degraded_mode = allow_degraded_mode;
        self.lock_state = lock_state;
        self.unlock_policy = normalize_unlock_policy(VaultUnlockPolicy {
            trigger_policy: unlock_policy.trigger_policy,
            allowed_methods: unlock_policy.allowed_methods,
            preferred_method: unlock_policy.preferred_method,
            cache_ttl_sec: unlock_policy.cache_ttl_sec,
            require_fresh_user_verification: unlock_policy.require_fresh_user_verification,
        });
        self.unlock_session = unlock_session.map(|value| VaultUnlockSession {
            method: value.method,
            unlocked_at: unix_secs_to_system_time(value.unlocked_at_unix_sec),
            expires_at: value.expires_at_unix_sec.map(unix_secs_to_system_time),
        });
        self.passphrase = passphrase;
        self.next_version_seq = next_version_seq;
        self.next_audit_seq = next_audit_seq;
        self.next_agent_token_seq = next_agent_token_seq;
        self.next_intent_seq = next_intent_seq;
        self.next_attestation_seq = next_attestation_seq;
        self.next_ssh_broker_seq = next_ssh_broker_seq;
        self.clock_watermark = unix_secs_to_system_time(clock_watermark_unix_sec);
        self.key_envelope = VaultKeyEnvelopeRecord {
            format_version: key_envelope.format_version,
            vault_key_id: key_envelope.vault_key_id,
            state: key_envelope.state,
            active_wrap_set_id: key_envelope.active_wrap_set_id,
            created_at: unix_secs_to_system_time(key_envelope.created_at_unix_sec),
            rotated_at: key_envelope
                .rotated_at_unix_sec
                .map(unix_secs_to_system_time),
            last_unlocked_at: key_envelope
                .last_unlocked_at_unix_sec
                .map(unix_secs_to_system_time),
        };
        self.wrap_manifests = wrap_manifests
            .into_iter()
            .map(|manifest| ProtectorWrapManifest {
                format_version: manifest.format_version,
                wrap_id: manifest.wrap_id,
                vault_key_id: manifest.vault_key_id,
                protector_binding: manifest.protector_binding,
                wrap_format: manifest.wrap_format,
                wrapped_key_locator: manifest.wrapped_key_locator,
                wrapped_key_digest: manifest.wrapped_key_digest,
                created_at: unix_secs_to_system_time(manifest.created_at_unix_sec),
                last_verified_at: manifest
                    .last_verified_at_unix_sec
                    .map(unix_secs_to_system_time),
                status: manifest.status,
            })
            .collect();
        self.wrap_blob_cache.clear();

        for manifest in &self.wrap_manifests {
            let wrapped_root_key_path = layout.path_for_blob_uri(&manifest.wrapped_key_locator)?;
            if let Ok(payload) = fs::read(&wrapped_root_key_path) {
                self.wrap_blob_cache
                    .insert(manifest.wrapped_key_locator.clone(), payload);
            }
        }
        if let Ok(salt) = fs::read(layout.path_for_blob_uri(&self.passphrase.salt_locator)?) {
            self.wrap_blob_cache
                .insert(self.passphrase.salt_locator.clone(), salt);
        }
        self.root_key_cache = None;
        self.refresh_unlock_session_ttl();
        if matches!(self.lock_state, VaultLockState::Uninitialized) {
            self.root_key_cache = None;
            self.unlock_session = None;
        } else if matches!(self.lock_state, VaultLockState::Unlocked) {
            let unlock_method = self
                .unlock_session
                .as_ref()
                .map(|session| session.method.clone())
                .unwrap_or_else(|| self.unlock_policy.preferred_method.clone());
            let _ = self.try_unlock_with_method(&unlock_method);
        } else if matches!(
            self.unlock_policy.trigger_policy,
            VaultUnlockTriggerPolicy::OnCoreStart
        ) {
            let preferred = self.unlock_policy.preferred_method.clone();
            if self.try_unlock_with_method(&preferred).is_err() {
                self.lock_state = VaultLockState::Unavailable;
            }
        } else {
            self.lock_vault_internal("restore-locked");
        }

        self.secrets.clear();
        for persisted in secrets {
            let mut versions = Vec::new();
            for entry in persisted.versions {
                let ciphertext_path = layout.path_for_blob_uri(&entry.ciphertext_blob_uri)?;
                let wrapped_dek_path = layout.path_for_blob_uri(&entry.wrapped_dek_blob_uri)?;
                let ciphertext = fs::read(&ciphertext_path).map_err(|err| {
                    VaultError::StorageIo(format!(
                        "failed to read ciphertext blob {}: {err}",
                        ciphertext_path.display()
                    ))
                })?;
                let wrapped_dek = fs::read(&wrapped_dek_path).map_err(|err| {
                    VaultError::StorageIo(format!(
                        "failed to read wrapped dek blob {}: {err}",
                        wrapped_dek_path.display()
                    ))
                })?;
                versions.push(StoredSecretVersionMaterial {
                    version: VaultSecretVersionRecord {
                        format_version: entry.version.format_version,
                        version_id: entry.version.version_id,
                        reference: entry.version.reference,
                        version_seq: entry.version.version_seq,
                        state: entry.version.state,
                        protector_binding: entry.version.protector_binding,
                        ciphertext_locator: entry.version.ciphertext_locator,
                        ciphertext_digest: entry.version.ciphertext_digest,
                        content_format: entry.version.content_format,
                        created_by: entry.version.created_by,
                        created_at: unix_secs_to_system_time(entry.version.created_at_unix_sec),
                        activated_at: entry
                            .version
                            .activated_at_unix_sec
                            .map(unix_secs_to_system_time),
                        superseded_at: entry
                            .version
                            .superseded_at_unix_sec
                            .map(unix_secs_to_system_time),
                        revoked_at: entry
                            .version
                            .revoked_at_unix_sec
                            .map(unix_secs_to_system_time),
                        destroy_after: entry
                            .version
                            .destroy_after_unix_sec
                            .map(unix_secs_to_system_time),
                    },
                    ciphertext,
                    wrapped_dek,
                });
            }
            self.secrets.insert(
                persisted.record.reference.clone(),
                StoredSecretState {
                    record: VaultSecretRecord {
                        format_version: persisted.record.format_version,
                        reference: persisted.record.reference,
                        kind: persisted.record.kind,
                        label: persisted.record.label,
                        status: persisted.record.status,
                        active_version_id: persisted.record.active_version_id,
                        created_by: persisted.record.created_by,
                        created_at: unix_secs_to_system_time(persisted.record.created_at_unix_sec),
                        last_used_at: persisted
                            .record
                            .last_used_at_unix_sec
                            .map(unix_secs_to_system_time),
                        last_rotated_at: persisted
                            .record
                            .last_rotated_at_unix_sec
                            .map(unix_secs_to_system_time),
                        rotation: persisted.record.rotation,
                        audit_chain_id: persisted.record.audit_chain_id,
                    },
                    versions,
                },
            );
        }
        self.agent_tokens = agent_tokens
            .into_iter()
            .map(|record| {
                let (status, access_enabled) =
                    if matches!(record.status, AgentTokenStatus::Disabled) {
                        (AgentTokenStatus::Active, false)
                    } else {
                        (record.status.clone(), record.access_enabled)
                    };
                (
                    record.token_id.clone(),
                    AgentTokenRecord {
                        token_id: record.token_id,
                        principal_id: record.principal_id,
                        label: record.label,
                        status,
                        access_enabled,
                        token_hash: record.token_hash,
                        hash_scheme: record.hash_scheme,
                        scope_profile: record.scope_profile,
                        active_scope_version: record.active_scope_version,
                        created_by: record.created_by,
                        created_at: unix_secs_to_system_time(record.created_at_unix_sec),
                        last_used_at: record.last_used_at_unix_sec.map(unix_secs_to_system_time),
                        expires_at: record.expires_at_unix_sec.map(unix_secs_to_system_time),
                        idle_timeout_sec: record.idle_timeout_sec,
                        revoked_at: record.revoked_at_unix_sec.map(unix_secs_to_system_time),
                        revoke_reason: record.revoke_reason,
                        deleted_at: record.deleted_at_unix_sec.map(unix_secs_to_system_time),
                        parent_token_id: record.parent_token_id,
                        issued_via_attestation_id: record.issued_via_attestation_id,
                    },
                )
            })
            .collect();
        self.token_scopes.clear();
        for scope in token_scopes {
            self.token_scopes
                .entry(scope.token_id.clone())
                .or_default()
                .push(TokenScopeRecord {
                    token_id: scope.token_id,
                    version: scope.version,
                    status: scope.status,
                    scope_profile: scope.scope_profile,
                    target_ids: scope.target_ids,
                    tool_ids: scope.tool_ids,
                    max_risk_envelope: scope.max_risk_envelope,
                    allow_open_shell: scope.allow_open_shell,
                    allow_write_shell_input: scope.allow_write_shell_input,
                    allow_artifact_cross_principal: scope.allow_artifact_cross_principal,
                    allow_delegation: scope.allow_delegation,
                    allow_admin_actions: scope.allow_admin_actions,
                    created_by: scope.created_by,
                    created_at: unix_secs_to_system_time(scope.created_at_unix_sec),
                    superseded_at: scope.superseded_at_unix_sec.map(unix_secs_to_system_time),
                    change_reason: scope.change_reason,
                });
        }
        for scopes in self.token_scopes.values_mut() {
            scopes.sort_by(|a, b| a.version.cmp(&b.version));
        }
        self.token_hash_index = self
            .agent_tokens
            .iter()
            .filter(|(_, token)| {
                !matches!(
                    agent_token_projected_status(token),
                    AgentTokenStatus::Deleted
                )
            })
            .map(|(token_id, token)| (token.token_hash.clone(), token_id.clone()))
            .collect();
        self.intents = intents
            .into_iter()
            .map(|intent| {
                (
                    intent.intent_id.clone(),
                    LocalAdminActionIntent {
                        intent_id: intent.intent_id,
                        action_kind: intent.action_kind,
                        target_object_ref: intent.target_object_ref,
                        requested_by_principal: intent.requested_by_principal,
                        requested_payload_digest: intent.requested_payload_digest,
                        created_at: unix_secs_to_system_time(intent.created_at_unix_sec),
                        expires_at: unix_secs_to_system_time(intent.expires_at_unix_sec),
                        status: intent.status,
                    },
                )
            })
            .collect();
        self.attestations = attestations
            .into_iter()
            .map(|attestation| {
                (
                    attestation.attestation_id.clone(),
                    LocalAdminAttestationRecord {
                        attestation_id: attestation.attestation_id,
                        intent_id: attestation.intent_id,
                        verified_principal: attestation.verified_principal,
                        verification_method: attestation.verification_method,
                        issued_at: unix_secs_to_system_time(attestation.issued_at_unix_sec),
                        expires_at: unix_secs_to_system_time(attestation.expires_at_unix_sec),
                        consumed_at: attestation
                            .consumed_at_unix_sec
                            .map(unix_secs_to_system_time),
                        status: attestation.status,
                    },
                )
            })
            .collect();
        Ok(())
    }

    fn migrate_v1_to_v2(
        &self,
        layout: &VaultStorageLayout,
        db_v1: VaultMetadataDbV1,
    ) -> Result<VaultMetadataDbV2, VaultError> {
        let mut wrap_manifests_v2 = Vec::new();
        for manifest in db_v1.wrap_manifests {
            let wrapped_key = hex_decode(&manifest.wrapped_key_hex).map_err(|err| {
                VaultError::StorageIo(format!(
                    "failed to decode wrapped root key hex in v1 metadata for {}: {err}",
                    manifest.wrap_id
                ))
            })?;
            let canonical_binding = if normalize_vault_method_label(&manifest.protector_binding)
                == "builtin-encrypted"
            {
                "os-native".to_string()
            } else {
                normalize_vault_method_label(&manifest.protector_binding)
            };
            let canonical_wrapped =
                if canonical_binding != normalize_vault_method_label(&manifest.protector_binding) {
                    let root_key = open_aead(
                        &protector_kek(&manifest.protector_binding),
                        format!("root:{}:{}", manifest.vault_key_id, manifest.wrap_id).as_bytes(),
                        &wrapped_key,
                    )?;
                    seal_aead(
                        &protector_kek(&canonical_binding),
                        format!("root:{}:{}", manifest.vault_key_id, manifest.wrap_id).as_bytes(),
                        &root_key,
                    )
                } else {
                    wrapped_key
                };
            let locator = layout.wrap_uri_for_manifest(&manifest.wrap_id);
            write_blob(layout.path_for_blob_uri(&locator)?, &canonical_wrapped)?;
            wrap_manifests_v2.push(PersistedProtectorWrapManifestV2 {
                format_version: VAULT_OBJECT_FORMAT_VERSION,
                wrap_id: manifest.wrap_id,
                vault_key_id: manifest.vault_key_id,
                protector_binding: canonical_binding,
                wrap_format: manifest.wrap_format,
                wrapped_key_locator: locator,
                wrapped_key_digest: short_digest(&canonical_wrapped),
                created_at_unix_sec: manifest.created_at_unix_sec,
                last_verified_at_unix_sec: manifest.last_verified_at_unix_sec,
                status: manifest.status,
            });
        }

        let mut secrets_v2 = Vec::new();
        for state in db_v1.secrets {
            let mut versions_v2 = Vec::new();
            for version in state.versions {
                let (cipher_uri, wrap_uri) = layout.blob_uri_for_secret_version(
                    &state.record.reference,
                    &version.version.version_id,
                );
                let ciphertext = hex_decode(&version.ciphertext_hex).map_err(|err| {
                    VaultError::StorageIo(format!(
                        "failed to decode ciphertext hex for version {}: {err}",
                        version.version.version_id
                    ))
                })?;
                let wrapped_dek = hex_decode(&version.wrapped_dek_hex).map_err(|err| {
                    VaultError::StorageIo(format!(
                        "failed to decode wrapped dek hex for version {}: {err}",
                        version.version.version_id
                    ))
                })?;
                write_blob(layout.path_for_blob_uri(&cipher_uri)?, &ciphertext)?;
                write_blob(layout.path_for_blob_uri(&wrap_uri)?, &wrapped_dek)?;
                versions_v2.push(PersistedSecretVersionMaterialV2 {
                    version: PersistedVaultSecretVersionRecordV2 {
                        format_version: VAULT_OBJECT_FORMAT_VERSION,
                        version_id: version.version.version_id,
                        reference: version.version.reference,
                        version_seq: version.version.version_seq,
                        state: version.version.state,
                        protector_binding: version.version.protector_binding,
                        ciphertext_locator: version.version.ciphertext_locator,
                        ciphertext_digest: version.version.ciphertext_digest,
                        content_format: version.version.content_format,
                        created_by: version.version.created_by,
                        created_at_unix_sec: version.version.created_at_unix_sec,
                        activated_at_unix_sec: version.version.activated_at_unix_sec,
                        superseded_at_unix_sec: version.version.superseded_at_unix_sec,
                        revoked_at_unix_sec: version.version.revoked_at_unix_sec,
                        destroy_after_unix_sec: version.version.destroy_after_unix_sec,
                    },
                    ciphertext_blob_uri: cipher_uri,
                    wrapped_dek_blob_uri: wrap_uri,
                });
            }
            secrets_v2.push(PersistedSecretStateV2 {
                record: PersistedVaultSecretRecordV2 {
                    format_version: VAULT_OBJECT_FORMAT_VERSION,
                    reference: state.record.reference,
                    kind: state.record.kind,
                    label: state.record.label,
                    status: state.record.status,
                    active_version_id: state.record.active_version_id,
                    created_by: state.record.created_by,
                    created_at_unix_sec: state.record.created_at_unix_sec,
                    last_used_at_unix_sec: state.record.last_used_at_unix_sec,
                    last_rotated_at_unix_sec: state.record.last_rotated_at_unix_sec,
                    rotation: state.record.rotation,
                    audit_chain_id: state.record.audit_chain_id,
                },
                versions: versions_v2,
            });
        }
        Ok(VaultMetadataDbV2 {
            schema_version: CANONICAL_METADATA_SCHEMA_VERSION,
            active_backend: db_v1.active_backend,
            allow_degraded_mode: db_v1.allow_degraded_mode,
            lock_state: VaultLockState::Locked,
            unlock_policy: PersistedVaultUnlockPolicyV2 {
                trigger_policy: VaultUnlockTriggerPolicy::OnFirstSecretAccess,
                allowed_methods: vec!["os-native".into(), "passphrase".into()],
                preferred_method: "os-native".into(),
                cache_ttl_sec: 600,
                require_fresh_user_verification: true,
            },
            unlock_session: None,
            passphrase: default_passphrase_protector_state(),
            next_version_seq: db_v1.next_version_seq,
            next_audit_seq: db_v1.next_audit_seq,
            next_agent_token_seq: db_v1.next_agent_token_seq,
            next_intent_seq: db_v1.next_intent_seq,
            next_attestation_seq: db_v1.next_attestation_seq,
            next_ssh_broker_seq: db_v1.next_ssh_broker_seq,
            clock_watermark_unix_sec: db_v1.clock_watermark_unix_sec,
            key_envelope: PersistedVaultKeyEnvelopeRecordV2 {
                format_version: VAULT_OBJECT_FORMAT_VERSION,
                vault_key_id: db_v1.key_envelope.vault_key_id,
                state: db_v1.key_envelope.state,
                active_wrap_set_id: db_v1.key_envelope.active_wrap_set_id,
                created_at_unix_sec: db_v1.key_envelope.created_at_unix_sec,
                rotated_at_unix_sec: db_v1.key_envelope.rotated_at_unix_sec,
                last_unlocked_at_unix_sec: db_v1.key_envelope.last_unlocked_at_unix_sec,
            },
            wrap_manifests: wrap_manifests_v2,
            secrets: secrets_v2,
            agent_tokens: Vec::new(),
            token_scopes: Vec::new(),
            intents: Vec::new(),
            attestations: Vec::new(),
        })
    }

    fn persist_metadata_db(&mut self) -> Result<(), VaultError> {
        let Some(layout) = self.storage_layout.clone() else {
            return Ok(());
        };
        self.unlock_policy = normalize_unlock_policy(self.unlock_policy.clone());
        if self.wrap_manifests.is_empty() {
            self.wrap_manifests.push(ProtectorWrapManifest {
                format_version: VAULT_OBJECT_FORMAT_VERSION,
                wrap_id: "wrap-000001".into(),
                vault_key_id: self.key_envelope.vault_key_id.clone(),
                protector_binding: "os-native".into(),
                wrap_format: "sha256-stream-aead-v1".into(),
                wrapped_key_locator: layout.wrap_uri_for_manifest("wrap-000001"),
                wrapped_key_digest: String::new(),
                created_at: SystemTime::now(),
                last_verified_at: None,
                status: ProtectorWrapStatus::Degraded,
            });
        }
        if !self
            .wrap_blob_cache
            .contains_key(&self.passphrase.salt_locator)
        {
            self.wrap_blob_cache.insert(
                self.passphrase.salt_locator.clone(),
                pseudo_random_bytes(16, "passphrase-salt-seed"),
            );
        }
        if self.key_envelope.format_version == 0 {
            self.key_envelope.format_version = VAULT_OBJECT_FORMAT_VERSION;
        }
        let mut wrap_locators_to_write = Vec::<String>::new();
        for manifest in &mut self.wrap_manifests {
            if manifest.format_version == 0 {
                manifest.format_version = VAULT_OBJECT_FORMAT_VERSION;
            }
            if manifest.wrapped_key_locator.trim().is_empty() {
                manifest.wrapped_key_locator = layout.wrap_uri_for_manifest(&manifest.wrap_id);
            }
            wrap_locators_to_write.push(manifest.wrapped_key_locator.clone());
        }

        if let Some(root_key) = self.root_key_cache.as_ref() {
            let root_key_bytes = root_key.expose_for_use().to_vec();
            self.rewrap_root_key_for_protectors(&root_key_bytes)?;
        }
        if self.passphrase.enabled {
            let passphrase_wrap_locator =
                format!("blob://vault/wrap/{}.vrk.bin", self.passphrase.wrap_id);
            if !self
                .wrap_manifests
                .iter()
                .any(|manifest| manifest.wrap_id == self.passphrase.wrap_id)
            {
                self.wrap_manifests.push(ProtectorWrapManifest {
                    format_version: VAULT_OBJECT_FORMAT_VERSION,
                    wrap_id: self.passphrase.wrap_id.clone(),
                    vault_key_id: self.key_envelope.vault_key_id.clone(),
                    protector_binding: "passphrase".into(),
                    wrap_format: "argon2id-wrap-v1".into(),
                    wrapped_key_locator: passphrase_wrap_locator.clone(),
                    wrapped_key_digest: self
                        .wrap_blob_cache
                        .get(&passphrase_wrap_locator)
                        .map(|blob| short_digest(blob))
                        .unwrap_or_default(),
                    created_at: SystemTime::now(),
                    last_verified_at: None,
                    status: ProtectorWrapStatus::Ready,
                });
                wrap_locators_to_write.push(passphrase_wrap_locator);
            }
        }
        for locator in wrap_locators_to_write {
            if let Some(payload) = self.wrap_blob_cache.get(&locator) {
                write_blob(layout.path_for_blob_uri(&locator)?, payload)?;
            }
        }
        if let Some(salt) = self.wrap_blob_cache.get(&self.passphrase.salt_locator) {
            write_blob(
                layout.path_for_blob_uri(&self.passphrase.salt_locator)?,
                salt,
            )?;
        }
        for manifest in &mut self.wrap_manifests {
            if let Some(payload) = self.wrap_blob_cache.get(&manifest.wrapped_key_locator) {
                manifest.wrapped_key_digest = short_digest(payload);
                manifest.last_verified_at = Some(SystemTime::now());
            }
        }

        let mut persisted_secrets = Vec::new();
        for (reference, state) in &self.secrets {
            let mut versions = Vec::new();
            for material in &state.versions {
                let (cipher_uri, wrapped_uri) =
                    layout.blob_uri_for_secret_version(reference, &material.version.version_id);
                write_blob(layout.path_for_blob_uri(&cipher_uri)?, &material.ciphertext)?;
                write_blob(
                    layout.path_for_blob_uri(&wrapped_uri)?,
                    &material.wrapped_dek,
                )?;
                versions.push(PersistedSecretVersionMaterialV2 {
                    version: PersistedVaultSecretVersionRecordV2 {
                        format_version: if material.version.format_version == 0 {
                            VAULT_OBJECT_FORMAT_VERSION
                        } else {
                            material.version.format_version
                        },
                        version_id: material.version.version_id.clone(),
                        reference: material.version.reference.clone(),
                        version_seq: material.version.version_seq,
                        state: material.version.state.clone(),
                        protector_binding: material.version.protector_binding.clone(),
                        ciphertext_locator: material.version.ciphertext_locator.clone(),
                        ciphertext_digest: material.version.ciphertext_digest.clone(),
                        content_format: material.version.content_format.clone(),
                        created_by: material.version.created_by.clone(),
                        created_at_unix_sec: system_time_to_unix_secs(material.version.created_at),
                        activated_at_unix_sec: material
                            .version
                            .activated_at
                            .map(system_time_to_unix_secs),
                        superseded_at_unix_sec: material
                            .version
                            .superseded_at
                            .map(system_time_to_unix_secs),
                        revoked_at_unix_sec: material
                            .version
                            .revoked_at
                            .map(system_time_to_unix_secs),
                        destroy_after_unix_sec: material
                            .version
                            .destroy_after
                            .map(system_time_to_unix_secs),
                    },
                    ciphertext_blob_uri: cipher_uri,
                    wrapped_dek_blob_uri: wrapped_uri,
                });
            }
            persisted_secrets.push(PersistedSecretStateV2 {
                record: PersistedVaultSecretRecordV2 {
                    format_version: if state.record.format_version == 0 {
                        VAULT_OBJECT_FORMAT_VERSION
                    } else {
                        state.record.format_version
                    },
                    reference: state.record.reference.clone(),
                    kind: state.record.kind.clone(),
                    label: state.record.label.clone(),
                    status: state.record.status.clone(),
                    active_version_id: state.record.active_version_id.clone(),
                    created_by: state.record.created_by.clone(),
                    created_at_unix_sec: system_time_to_unix_secs(state.record.created_at),
                    last_used_at_unix_sec: state.record.last_used_at.map(system_time_to_unix_secs),
                    last_rotated_at_unix_sec: state
                        .record
                        .last_rotated_at
                        .map(system_time_to_unix_secs),
                    rotation: state.record.rotation.clone(),
                    audit_chain_id: state.record.audit_chain_id.clone(),
                },
                versions,
            });
        }
        let mut persisted_intents = self.intents.values().cloned().collect::<Vec<_>>();
        persisted_intents.sort_by(|a, b| a.intent_id.cmp(&b.intent_id));
        let mut persisted_attestations = self.attestations.values().cloned().collect::<Vec<_>>();
        persisted_attestations.sort_by(|a, b| a.attestation_id.cmp(&b.attestation_id));
        let mut persisted_agent_tokens = self.agent_tokens.values().cloned().collect::<Vec<_>>();
        persisted_agent_tokens.sort_by(|a, b| a.token_id.cmp(&b.token_id));
        let mut persisted_token_scopes = Vec::<PersistedTokenScopeRecordV2>::new();
        for scopes in self.token_scopes.values() {
            for scope in scopes {
                persisted_token_scopes.push(PersistedTokenScopeRecordV2 {
                    token_id: scope.token_id.clone(),
                    version: scope.version,
                    status: scope.status.clone(),
                    scope_profile: scope.scope_profile.clone(),
                    target_ids: scope.target_ids.clone(),
                    tool_ids: scope.tool_ids.clone(),
                    max_risk_envelope: scope.max_risk_envelope.clone(),
                    allow_open_shell: scope.allow_open_shell,
                    allow_write_shell_input: scope.allow_write_shell_input,
                    allow_artifact_cross_principal: scope.allow_artifact_cross_principal,
                    allow_delegation: scope.allow_delegation,
                    allow_admin_actions: scope.allow_admin_actions,
                    created_by: scope.created_by.clone(),
                    created_at_unix_sec: system_time_to_unix_secs(scope.created_at),
                    superseded_at_unix_sec: scope.superseded_at.map(system_time_to_unix_secs),
                    change_reason: scope.change_reason.clone(),
                });
            }
        }
        persisted_token_scopes.sort_by(|a, b| {
            a.token_id
                .cmp(&b.token_id)
                .then_with(|| a.version.cmp(&b.version))
        });

        let db = VaultMetadataDbV2 {
            schema_version: CANONICAL_METADATA_SCHEMA_VERSION,
            active_backend: self.active_backend.clone(),
            allow_degraded_mode: self.allow_degraded_mode,
            lock_state: self.lock_state.clone(),
            unlock_policy: PersistedVaultUnlockPolicyV2 {
                trigger_policy: self.unlock_policy.trigger_policy.clone(),
                allowed_methods: self.unlock_policy.allowed_methods.clone(),
                preferred_method: self.unlock_policy.preferred_method.clone(),
                cache_ttl_sec: self.unlock_policy.cache_ttl_sec,
                require_fresh_user_verification: self.unlock_policy.require_fresh_user_verification,
            },
            unlock_session: self.unlock_session.as_ref().map(|session| {
                PersistedVaultUnlockSessionV2 {
                    method: session.method.clone(),
                    unlocked_at_unix_sec: system_time_to_unix_secs(session.unlocked_at),
                    expires_at_unix_sec: session.expires_at.map(system_time_to_unix_secs),
                }
            }),
            passphrase: self.passphrase.clone(),
            next_version_seq: self.next_version_seq,
            next_audit_seq: self.next_audit_seq,
            next_agent_token_seq: self.next_agent_token_seq,
            next_intent_seq: self.next_intent_seq,
            next_attestation_seq: self.next_attestation_seq,
            next_ssh_broker_seq: self.next_ssh_broker_seq,
            clock_watermark_unix_sec: system_time_to_unix_secs(self.clock_watermark),
            key_envelope: PersistedVaultKeyEnvelopeRecordV2 {
                format_version: self.key_envelope.format_version,
                vault_key_id: self.key_envelope.vault_key_id.clone(),
                state: self.key_envelope.state.clone(),
                active_wrap_set_id: self.key_envelope.active_wrap_set_id.clone(),
                created_at_unix_sec: system_time_to_unix_secs(self.key_envelope.created_at),
                rotated_at_unix_sec: self.key_envelope.rotated_at.map(system_time_to_unix_secs),
                last_unlocked_at_unix_sec: self
                    .key_envelope
                    .last_unlocked_at
                    .map(system_time_to_unix_secs),
            },
            wrap_manifests: self
                .wrap_manifests
                .iter()
                .map(|manifest| PersistedProtectorWrapManifestV2 {
                    format_version: manifest.format_version,
                    wrap_id: manifest.wrap_id.clone(),
                    vault_key_id: manifest.vault_key_id.clone(),
                    protector_binding: manifest.protector_binding.clone(),
                    wrap_format: manifest.wrap_format.clone(),
                    wrapped_key_locator: manifest.wrapped_key_locator.clone(),
                    wrapped_key_digest: manifest.wrapped_key_digest.clone(),
                    created_at_unix_sec: system_time_to_unix_secs(manifest.created_at),
                    last_verified_at_unix_sec: manifest
                        .last_verified_at
                        .map(system_time_to_unix_secs),
                    status: manifest.status.clone(),
                })
                .collect(),
            secrets: persisted_secrets,
            agent_tokens: persisted_agent_tokens
                .into_iter()
                .map(|token| PersistedAgentTokenRecordV2 {
                    token_id: token.token_id,
                    principal_id: token.principal_id,
                    label: token.label,
                    status: token.status,
                    access_enabled: token.access_enabled,
                    token_hash: token.token_hash,
                    hash_scheme: token.hash_scheme,
                    scope_profile: token.scope_profile,
                    active_scope_version: token.active_scope_version,
                    created_by: token.created_by,
                    created_at_unix_sec: system_time_to_unix_secs(token.created_at),
                    last_used_at_unix_sec: token.last_used_at.map(system_time_to_unix_secs),
                    expires_at_unix_sec: token.expires_at.map(system_time_to_unix_secs),
                    idle_timeout_sec: token.idle_timeout_sec,
                    revoked_at_unix_sec: token.revoked_at.map(system_time_to_unix_secs),
                    revoke_reason: token.revoke_reason,
                    deleted_at_unix_sec: token.deleted_at.map(system_time_to_unix_secs),
                    parent_token_id: token.parent_token_id,
                    issued_via_attestation_id: token.issued_via_attestation_id,
                })
                .collect(),
            token_scopes: persisted_token_scopes,
            intents: persisted_intents
                .into_iter()
                .map(|intent| PersistedLocalAdminActionIntentV2 {
                    intent_id: intent.intent_id,
                    action_kind: intent.action_kind,
                    target_object_ref: intent.target_object_ref,
                    requested_by_principal: intent.requested_by_principal,
                    requested_payload_digest: intent.requested_payload_digest,
                    created_at_unix_sec: system_time_to_unix_secs(intent.created_at),
                    expires_at_unix_sec: system_time_to_unix_secs(intent.expires_at),
                    status: intent.status,
                })
                .collect(),
            attestations: persisted_attestations
                .into_iter()
                .map(|attestation| PersistedLocalAdminAttestationRecordV2 {
                    attestation_id: attestation.attestation_id,
                    intent_id: attestation.intent_id,
                    verified_principal: attestation.verified_principal,
                    verification_method: attestation.verification_method,
                    issued_at_unix_sec: system_time_to_unix_secs(attestation.issued_at),
                    expires_at_unix_sec: system_time_to_unix_secs(attestation.expires_at),
                    consumed_at_unix_sec: attestation.consumed_at.map(system_time_to_unix_secs),
                    status: attestation.status,
                })
                .collect(),
        };

        let encoded = serde_json::to_string_pretty(&db).map_err(|err| {
            VaultError::StorageIo(format!(
                "failed to encode metadata db {}: {err}",
                layout.metadata_db_path.display()
            ))
        })?;
        fs::write(&layout.metadata_db_path, encoded).map_err(|err| {
            VaultError::StorageIo(format!(
                "failed to write metadata db {}: {err}",
                layout.metadata_db_path.display()
            ))
        })?;
        Ok(())
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

fn cleanup_stored_ssh_session_artifacts(stored: &mut StoredSshAgentBrokerSession) {
    for path in &stored.cleanup_paths {
        let _ = if path.is_dir() {
            fs::remove_dir(path)
        } else {
            fs::remove_file(path)
        };
    }
    stored.cleanup_paths.clear();
    stored.endpoint_locator = None;
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

fn agent_token_projected_status(record: &AgentTokenRecord) -> AgentTokenStatus {
    if matches!(record.status, AgentTokenStatus::Deleted) {
        return AgentTokenStatus::Deleted;
    }
    if matches!(record.status, AgentTokenStatus::Revoked) {
        return AgentTokenStatus::Revoked;
    }
    if matches!(record.status, AgentTokenStatus::Expired) {
        return AgentTokenStatus::Expired;
    }
    if !record.access_enabled {
        return AgentTokenStatus::Disabled;
    }
    AgentTokenStatus::Active
}

fn normalize_agent_token_label_for_write(raw: &str) -> Result<String, VaultError> {
    let candidate = raw.trim();
    if candidate.is_empty() {
        return Err(VaultError::AgentTokenRejected(
            "token label must be non-empty".into(),
        ));
    }
    if candidate.len() > 64 {
        return Err(VaultError::AgentTokenRejected(
            "token label must be <= 64 ASCII characters".into(),
        ));
    }
    if !is_valid_agent_token_label(candidate) {
        return Err(VaultError::AgentTokenRejected(
            "token label must match [A-Za-z0-9]+([-_][A-Za-z0-9]+)* without spaces".into(),
        ));
    }
    Ok(candidate.to_string())
}

fn is_valid_agent_token_label(value: &str) -> bool {
    let mut prev_separator = false;
    let mut seen_any = false;
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
        return false;
    }
    seen_any && !prev_separator
}

fn agent_token_display_fingerprint(record: &AgentTokenRecord) -> String {
    let digest = short_digest(format!("{}:{}", record.token_id, record.token_hash).as_bytes());
    format!("fp-{}", &digest[..12.min(digest.len())])
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

fn normalize_vault_method_label(raw: &str) -> String {
    raw.trim().to_ascii_lowercase().replace('_', "-")
}

fn normalize_payload_digest(raw: &str) -> Option<String> {
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }
    Some(normalized)
}

fn is_generated_local_admin_id(value: &str, prefix: &str) -> bool {
    let Some(suffix) = value.strip_prefix(prefix) else {
        return false;
    };
    suffix.len() == 6 && suffix.chars().all(|ch| ch.is_ascii_digit())
}

fn normalize_local_admin_target(
    action_kind: &LocalAdminActionKind,
    target_object_ref: &str,
) -> Result<String, VaultError> {
    match action_kind {
        LocalAdminActionKind::RevealSecret | LocalAdminActionKind::ExportSecret => {
            normalize_credential_ref(target_object_ref)
        }
        LocalAdminActionKind::CreateAgentToken => Ok(LOCAL_ADMIN_CREATE_TOKEN_TARGET.to_string()),
        LocalAdminActionKind::UnlockVault => Ok(LOCAL_ADMIN_UNLOCK_VAULT_TARGET.to_string()),
        LocalAdminActionKind::DeleteVault => Ok(LOCAL_ADMIN_DELETE_VAULT_TARGET.to_string()),
        LocalAdminActionKind::UpdateAgentTokenScope | LocalAdminActionKind::DeleteAgentToken => {
            let normalized = target_object_ref.trim().to_ascii_lowercase();
            if normalized.is_empty() {
                return Err(VaultError::LocalAdminIntentMismatch(
                    "token id target must be non-empty".into(),
                ));
            }
            Ok(normalized)
        }
    }
}

fn payload_digest_for_local_admin_action(
    action_kind: &LocalAdminActionKind,
    target_object_ref: &str,
    payload_digest: Option<&str>,
) -> String {
    if let Some(value) = payload_digest.and_then(normalize_payload_digest) {
        return value;
    }
    match action_kind {
        LocalAdminActionKind::RevealSecret
        | LocalAdminActionKind::ExportSecret
        | LocalAdminActionKind::UpdateAgentTokenScope
        | LocalAdminActionKind::DeleteAgentToken => short_digest(target_object_ref.as_bytes()),
        LocalAdminActionKind::CreateAgentToken
        | LocalAdminActionKind::UnlockVault
        | LocalAdminActionKind::DeleteVault => {
            short_digest(format!("{}:{target_object_ref}", action_kind.as_str()).as_bytes())
        }
    }
}

fn payload_digest_for_unlock_request(method: &str) -> String {
    let payload = serde_json::json!({
        "action": LocalAdminActionKind::UnlockVault.as_str(),
        "target": LOCAL_ADMIN_UNLOCK_VAULT_TARGET,
        "method": normalize_vault_method_label(method),
    });
    short_digest(payload.to_string().as_bytes())
}

fn payload_digest_for_delete_vault_request() -> String {
    let payload = serde_json::json!({
        "action": LocalAdminActionKind::DeleteVault.as_str(),
        "target": LOCAL_ADMIN_DELETE_VAULT_TARGET,
    });
    short_digest(payload.to_string().as_bytes())
}

pub fn local_admin_payload_digest_for_create_agent_token(
    request: &CreateAgentTokenRequest,
) -> String {
    payload_digest_for_token_create_request(request)
}

pub fn local_admin_payload_digest_for_update_agent_token_scope(
    request: &UpdateAgentTokenScopeRequest,
) -> String {
    payload_digest_for_token_scope_update_request(request)
}

pub fn local_admin_payload_digest_for_unlock_vault(method: &str) -> String {
    payload_digest_for_unlock_request(method)
}

pub fn local_admin_payload_digest_for_delete_vault() -> String {
    payload_digest_for_delete_vault_request()
}

pub fn local_admin_payload_digest_for_delete_agent_token(token_id: &str) -> String {
    payload_digest_for_token_delete_request(token_id)
}

fn normalize_unlock_policy(mut policy: VaultUnlockPolicy) -> VaultUnlockPolicy {
    if policy.cache_ttl_sec == 0 {
        policy.cache_ttl_sec = 600;
    }
    if policy.allowed_methods.is_empty() {
        policy.allowed_methods = vec!["os-native".into(), "passphrase".into()];
    }
    let mut methods = policy
        .allowed_methods
        .iter()
        .map(|value| normalize_vault_method_label(value))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    methods.sort();
    methods.dedup();
    if methods.is_empty() {
        methods = vec!["os-native".into(), "passphrase".into()];
    }
    policy.allowed_methods = methods;
    policy.preferred_method = normalize_vault_method_label(&policy.preferred_method);
    if policy.preferred_method.is_empty() {
        policy.preferred_method = policy.allowed_methods[0].clone();
    }
    if !policy
        .allowed_methods
        .iter()
        .any(|value| value == &policy.preferred_method)
    {
        policy
            .allowed_methods
            .insert(0, policy.preferred_method.clone());
    }
    policy
}

fn derive_argon2id_kek(
    passphrase: &str,
    salt: &[u8],
    params: &PassphraseKdfParams,
) -> Result<Vec<u8>, VaultError> {
    if params.kdf != "argon2id" {
        return Err(VaultError::UnlockFailed(format!(
            "unsupported passphrase kdf: {}",
            params.kdf
        )));
    }
    let argon2_params = Params::new(params.m_cost_kib, params.t_cost, params.p_cost, Some(32))
        .map_err(|err| VaultError::UnlockFailed(format!("invalid Argon2id params: {err}")))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon2_params);
    let mut output = vec![0u8; 32];
    argon2
        .hash_password_into(passphrase.as_bytes(), salt, &mut output)
        .map_err(|err| VaultError::UnlockFailed(format!("Argon2id derivation failed: {err}")))?;
    Ok(output)
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

fn payload_digest_for_token_create_request(request: &CreateAgentTokenRequest) -> String {
    let scope_profile =
        normalized_scope_profile(request.scope.scope_profile.as_deref(), Some("default-deny"));
    let payload = serde_json::json!({
        "action": LocalAdminActionKind::CreateAgentToken.as_str(),
        "target": LOCAL_ADMIN_CREATE_TOKEN_TARGET,
        "label": request.label.trim(),
        "expires_in_sec": request.expires_in.map(|ttl| ttl.as_secs()),
        "idle_timeout_sec": request.idle_timeout_sec,
        "scope_profile": scope_profile,
        "target_ids": canonicalize_ids(&request.scope.target_ids),
        "tool_ids": canonicalize_ids(&request.scope.tool_ids),
        "max_risk_envelope": request.scope.max_risk_envelope.as_deref().map(|value| value.trim().to_ascii_lowercase()),
        "allow_open_shell": request.scope.allow_open_shell.unwrap_or(false),
        "allow_write_shell_input": request.scope.allow_write_shell_input.unwrap_or(false),
        "allow_artifact_cross_principal": request.scope.allow_artifact_cross_principal.unwrap_or(false),
        "allow_delegation": request.scope.allow_delegation.unwrap_or(false),
        "allow_admin_actions": request.scope.allow_admin_actions.unwrap_or(false),
    });
    short_digest(payload.to_string().as_bytes())
}

fn payload_digest_for_token_scope_update_request(request: &UpdateAgentTokenScopeRequest) -> String {
    let payload = serde_json::json!({
        "action": LocalAdminActionKind::UpdateAgentTokenScope.as_str(),
        "target": request.token_id.trim().to_ascii_lowercase(),
        "scope_profile": request.scope.scope_profile.as_deref().map(|value| value.trim().to_ascii_lowercase()),
        "target_ids": canonicalize_ids(&request.scope.target_ids),
        "tool_ids": canonicalize_ids(&request.scope.tool_ids),
        "max_risk_envelope": request.scope.max_risk_envelope.as_deref().map(|value| value.trim().to_ascii_lowercase()),
        "allow_open_shell": request.scope.allow_open_shell.unwrap_or(false),
        "allow_write_shell_input": request.scope.allow_write_shell_input.unwrap_or(false),
        "allow_artifact_cross_principal": request.scope.allow_artifact_cross_principal.unwrap_or(false),
        "allow_delegation": request.scope.allow_delegation.unwrap_or(false),
        "allow_admin_actions": request.scope.allow_admin_actions.unwrap_or(false),
        "reason": request.reason.as_deref().map(|value| value.trim()),
    });
    short_digest(payload.to_string().as_bytes())
}

fn payload_digest_for_token_delete_request(token_id: &str) -> String {
    let payload = serde_json::json!({
        "action": LocalAdminActionKind::DeleteAgentToken.as_str(),
        "target": token_id.trim().to_ascii_lowercase(),
    });
    short_digest(payload.to_string().as_bytes())
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SshPrivateKeyValidationSummary {
    encrypted_input: bool,
}

fn canonical_ssh_private_key_name(raw: &str) -> Result<String, VaultError> {
    let normalized = raw.trim().to_ascii_lowercase().replace('_', "-");
    if normalized.is_empty() {
        return Err(VaultError::InvalidCredentialRef(raw.to_string()));
    }
    let mut out = String::with_capacity(normalized.len());
    let mut prev_dash = false;
    for ch in normalized.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '.' {
            out.push(ch);
            prev_dash = false;
            continue;
        }
        if ch == '-' || ch.is_ascii_whitespace() {
            if !prev_dash && !out.is_empty() {
                out.push('-');
                prev_dash = true;
            }
            continue;
        }
        return Err(VaultError::InvalidCredentialRef(raw.to_string()));
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        return Err(VaultError::InvalidCredentialRef(raw.to_string()));
    }
    Ok(out)
}

fn validate_trusted_local_ssh_private_key_material(
    raw: &str,
    passphrase: Option<&str>,
) -> Result<SshPrivateKeyValidationSummary, VaultError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(VaultError::InvalidSshPrivateKey(
            "ssh private key payload is empty".into(),
        ));
    }
    let encrypted = if trimmed.contains("-----BEGIN OPENSSH PRIVATE KEY-----") {
        parse_openssh_private_key_encryption_state(trimmed)?
    } else if trimmed.contains("-----BEGIN ") && trimmed.contains(" PRIVATE KEY-----") {
        parse_pem_private_key_encryption_state(trimmed)?
    } else {
        return Err(VaultError::InvalidSshPrivateKey(
            "missing private key envelope markers".into(),
        ));
    };
    if encrypted
        && passphrase
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
    {
        return Err(VaultError::SshKeyPassphraseRequired(
            "encrypted ssh private key requires passphrase during import".into(),
        ));
    }
    Ok(SshPrivateKeyValidationSummary {
        encrypted_input: encrypted,
    })
}

fn parse_pem_private_key_encryption_state(raw: &str) -> Result<bool, VaultError> {
    let begin_marker = raw
        .lines()
        .find(|line| line.trim_start().starts_with("-----BEGIN "))
        .ok_or_else(|| VaultError::InvalidSshPrivateKey("missing PEM begin marker".into()))?;
    let end_marker = begin_marker
        .replace("BEGIN", "END")
        .replace("-----BEGIN ", "-----END ");
    if !raw.contains(&end_marker) {
        return Err(VaultError::InvalidSshPrivateKey(
            "missing PEM end marker".into(),
        ));
    }
    let encrypted = raw
        .lines()
        .any(|line| line.trim().eq_ignore_ascii_case("proc-type: 4,encrypted"))
        || raw.lines().any(|line| line.trim().starts_with("DEK-Info:"));
    Ok(encrypted)
}

fn parse_openssh_private_key_encryption_state(raw: &str) -> Result<bool, VaultError> {
    let begin = "-----BEGIN OPENSSH PRIVATE KEY-----";
    let end = "-----END OPENSSH PRIVATE KEY-----";
    let start = raw.find(begin).ok_or_else(|| {
        VaultError::InvalidSshPrivateKey("missing OpenSSH key begin marker".into())
    })?;
    let end_pos = raw
        .find(end)
        .ok_or_else(|| VaultError::InvalidSshPrivateKey("missing OpenSSH key end marker".into()))?;
    if end_pos <= start {
        return Err(VaultError::InvalidSshPrivateKey(
            "OpenSSH key envelope order is invalid".into(),
        ));
    }
    let body = &raw[start + begin.len()..end_pos];
    let encoded = body.lines().map(str::trim).collect::<String>();
    if encoded.is_empty() {
        return Err(VaultError::InvalidSshPrivateKey(
            "OpenSSH key payload is empty".into(),
        ));
    }
    let payload = BASE64_STANDARD.decode(encoded.as_bytes()).map_err(|_| {
        VaultError::InvalidSshPrivateKey("OpenSSH key payload is not valid base64".into())
    })?;
    let magic = b"openssh-key-v1\0";
    if payload.len() <= magic.len() || &payload[..magic.len()] != magic {
        return Err(VaultError::InvalidSshPrivateKey(
            "OpenSSH key payload has invalid magic header".into(),
        ));
    }
    let mut offset = magic.len();
    let ciphername = read_openssh_string(&payload, &mut offset, "ciphername")?;
    let kdfname = read_openssh_string(&payload, &mut offset, "kdfname")?;
    let _kdfoptions = read_openssh_string(&payload, &mut offset, "kdfoptions")?;
    let _nkeys = read_openssh_u32(&payload, &mut offset, "nkeys")?;
    Ok(!ciphername.eq_ignore_ascii_case("none") || !kdfname.eq_ignore_ascii_case("none"))
}

fn read_openssh_u32(payload: &[u8], offset: &mut usize, field: &str) -> Result<u32, VaultError> {
    if payload.len().saturating_sub(*offset) < 4 {
        return Err(VaultError::InvalidSshPrivateKey(format!(
            "OpenSSH key payload truncated while reading {field}"
        )));
    }
    let value = u32::from_be_bytes([
        payload[*offset],
        payload[*offset + 1],
        payload[*offset + 2],
        payload[*offset + 3],
    ]);
    *offset += 4;
    Ok(value)
}

fn read_openssh_string(
    payload: &[u8],
    offset: &mut usize,
    field: &str,
) -> Result<String, VaultError> {
    let len = read_openssh_u32(payload, offset, field)? as usize;
    if payload.len().saturating_sub(*offset) < len {
        return Err(VaultError::InvalidSshPrivateKey(format!(
            "OpenSSH key payload truncated while reading {field}"
        )));
    }
    let bytes = &payload[*offset..*offset + len];
    *offset += len;
    String::from_utf8(bytes.to_vec()).map_err(|_| {
        VaultError::InvalidSshPrivateKey(format!("OpenSSH key field `{field}` is not valid UTF-8"))
    })
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

fn seal_aead(key: &[u8], aad: &[u8], plaintext: &[u8]) -> Vec<u8> {
    let nonce = pseudo_random_bytes(24, "vault-aead-nonce");
    let ciphertext = stream_xor(key, &nonce, plaintext);
    let mut tag_hasher = Sha256::new();
    tag_hasher.update(b"bridgingio-aead-v1");
    tag_hasher.update(key);
    tag_hasher.update(aad);
    tag_hasher.update(&nonce);
    tag_hasher.update(&ciphertext);
    let tag = tag_hasher.finalize();
    let mut out = Vec::with_capacity(3 + nonce.len() + ciphertext.len() + tag.len());
    out.extend_from_slice(b"AE1");
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    out.extend_from_slice(&tag);
    out
}

fn open_aead(key: &[u8], aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>, VaultError> {
    if sealed.len() < 3 + 24 + 32 {
        return Err(VaultError::CryptoEnvelope(
            "sealed payload is too short for aead envelope".into(),
        ));
    }
    if &sealed[..3] != b"AE1" {
        return Err(VaultError::CryptoEnvelope(
            "unknown aead envelope version marker".into(),
        ));
    }
    let nonce = &sealed[3..27];
    let tag_start = sealed.len() - 32;
    let ciphertext = &sealed[27..tag_start];
    let expected_tag = &sealed[tag_start..];
    let mut tag_hasher = Sha256::new();
    tag_hasher.update(b"bridgingio-aead-v1");
    tag_hasher.update(key);
    tag_hasher.update(aad);
    tag_hasher.update(nonce);
    tag_hasher.update(ciphertext);
    let computed = tag_hasher.finalize();
    if !constant_time_eq(computed.as_slice(), expected_tag) {
        return Err(VaultError::CryptoEnvelope(
            "aead authentication tag mismatch".into(),
        ));
    }
    Ok(stream_xor(key, nonce, ciphertext))
}

fn stream_xor(key: &[u8], nonce: &[u8], input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut counter = 0u64;
    while output.len() < input.len() {
        let mut hasher = Sha256::new();
        hasher.update(b"bridgingio-stream-v1");
        hasher.update(key);
        hasher.update(nonce);
        hasher.update(counter.to_le_bytes());
        let block = hasher.finalize();
        for byte in block {
            if output.len() == input.len() {
                break;
            }
            output.push(byte);
        }
        counter = counter.saturating_add(1);
    }
    output
        .into_iter()
        .zip(input.iter().copied())
        .map(|(mask, value)| mask ^ value)
        .collect()
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

fn derive_pseudo_key(context: &str, len: usize) -> Vec<u8> {
    let mut output = Vec::with_capacity(len);
    let mut counter = 0u64;
    while output.len() < len {
        let mut hasher = Sha256::new();
        hasher.update(b"bridgingio-pseudo-rng-v1");
        hasher.update(context.as_bytes());
        hasher.update(counter.to_le_bytes());
        hasher.update(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|value| value.as_nanos().to_le_bytes().to_vec())
                .unwrap_or_else(|_| vec![0u8; 16]),
        );
        output.extend_from_slice(&hasher.finalize());
        counter = counter.saturating_add(1);
    }
    output.truncate(len);
    output
}

fn pseudo_random_bytes(len: usize, context: &str) -> Vec<u8> {
    derive_pseudo_key(context, len)
}

fn short_digest(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let digest = hasher.finalize();
    hex_encode(&digest[..8])
}

const OS_NATIVE_PROTECTOR_SERVICE: &str = "io.bridgingio.vault";
const OS_NATIVE_PROTECTOR_ACCOUNT: &str = "os-native-protector-kek-v1";
const OS_NATIVE_VERIFIED_TIMEOUT_SECS: u64 = 120;
static OS_NATIVE_PROTECTOR_KEK_CACHE: OnceLock<Option<Vec<u8>>> = OnceLock::new();
static OS_NATIVE_VERIFIED_KEK_CACHE: OnceLock<Mutex<Option<Vec<u8>>>> = OnceLock::new();
static OS_NATIVE_VERIFIED_FLOW_COUNTER: OnceLock<Mutex<u64>> = OnceLock::new();
static OS_NATIVE_VERIFIED_SINGLEFLIGHT: OnceLock<VerifiedOsNativeSingleflight> = OnceLock::new();
#[cfg(test)]
static TEST_OS_NATIVE_VERIFIED_LOADER: OnceLock<
    Mutex<Option<Arc<dyn Fn() -> Option<Vec<u8>> + Send + Sync>>>,
> = OnceLock::new();
#[cfg(test)]
static TEST_OS_NATIVE_VERIFIED_TIMEOUT_MS: OnceLock<Mutex<Option<u64>>> = OnceLock::new();

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedOsNativeEvent {
    pub flow_id: String,
    pub dedupe_state: String,
    pub phase: String,
    pub result: String,
    pub cache_state: String,
}

thread_local! {
    static LAST_VERIFIED_OS_NATIVE_EVENT: RefCell<Option<VerifiedOsNativeEvent>> = const { RefCell::new(None) };
}

pub fn take_last_verified_os_native_event() -> Option<VerifiedOsNativeEvent> {
    LAST_VERIFIED_OS_NATIVE_EVENT.with(|slot| slot.borrow_mut().take())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct VerifiedOsNativeAttempt {
    value: Option<Vec<u8>>,
    event: VerifiedOsNativeEvent,
}

#[derive(Clone, Debug)]
struct VerifiedOsNativeInFlight {
    flow_id: String,
    participants: usize,
    completed: bool,
    result: Option<Vec<u8>>,
}

struct VerifiedOsNativeSingleflight {
    state: Mutex<Option<VerifiedOsNativeInFlight>>,
    cv: Condvar,
}

fn os_native_platform_binding_ready() -> bool {
    // Readiness reporting must be side-effect free: do not touch keyring here,
    // otherwise diagnostics can trigger interactive system authorization prompts.
    os_native_platform_binding_supported()
}

fn os_native_platform_binding_supported() -> bool {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        !env_flag_enabled("BRIDGINGIO_DISABLE_OS_NATIVE_KEYRING")
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        false
    }
}

fn verified_os_native_kek_cache() -> &'static Mutex<Option<Vec<u8>>> {
    OS_NATIVE_VERIFIED_KEK_CACHE.get_or_init(|| Mutex::new(None))
}

fn cached_verified_os_native_kek() -> Option<Vec<u8>> {
    verified_os_native_kek_cache()
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
}

fn cache_verified_os_native_kek(kek: Option<Vec<u8>>) {
    if let Ok(mut guard) = verified_os_native_kek_cache().lock() {
        *guard = kek;
    }
}

fn os_native_protector_kek() -> Option<Vec<u8>> {
    if let Some(cached) = cached_verified_os_native_kek() {
        return Some(cached);
    }
    OS_NATIVE_PROTECTOR_KEK_CACHE
        .get_or_init(os_native_protector_kek_inner)
        .clone()
}

fn os_native_protector_kek_verified() -> Option<Vec<u8>> {
    let attempt = os_native_protector_kek_verified_attempt();
    if let Some(value) = attempt.value.clone() {
        cache_verified_os_native_kek(Some(value));
    }
    LAST_VERIFIED_OS_NATIVE_EVENT.with(|slot| {
        *slot.borrow_mut() = Some(attempt.event);
    });
    attempt.value
}

fn os_native_protector_kek_verified_attempt() -> VerifiedOsNativeAttempt {
    if let Some(cached) = cached_verified_os_native_kek() {
        return VerifiedOsNativeAttempt {
            value: Some(cached),
            event: VerifiedOsNativeEvent {
                flow_id: "cached-verified-os-native-kek".to_string(),
                dedupe_state: "not-applicable".to_string(),
                phase: "succeeded".to_string(),
                result: "ok".to_string(),
                cache_state: "hit".to_string(),
            },
        };
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        if !os_native_platform_binding_supported() {
            return VerifiedOsNativeAttempt {
                value: None,
                event: VerifiedOsNativeEvent {
                    flow_id: "unsupported-os-native".to_string(),
                    dedupe_state: "not-applicable".to_string(),
                    phase: "failed".to_string(),
                    result: "unsupported".to_string(),
                    cache_state: "skip".to_string(),
                },
            };
        }
        let singleflight = verified_os_native_singleflight();
        let timeout = verified_os_native_timeout();
        let joined_started_at = Instant::now();
        let mut guard = match singleflight.state.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return VerifiedOsNativeAttempt {
                    value: None,
                    event: VerifiedOsNativeEvent {
                        flow_id: "lock-poisoned-os-native".to_string(),
                        dedupe_state: "not-applicable".to_string(),
                        phase: "failed".to_string(),
                        result: "error".to_string(),
                        cache_state: "skip".to_string(),
                    },
                };
            }
        };
        if let Some(inflight) = guard.as_mut() {
            inflight.participants = inflight.participants.saturating_add(1);
            let flow_id = inflight.flow_id.clone();
            loop {
                if let Some(current) = guard.as_mut() {
                    if current.flow_id == flow_id && current.completed {
                        let value = current.result.clone();
                        current.participants = current.participants.saturating_sub(1);
                        let should_clear = current.participants == 0;
                        if should_clear {
                            *guard = None;
                        }
                        drop(guard);
                        return VerifiedOsNativeAttempt {
                            value: value.clone(),
                            event: VerifiedOsNativeEvent {
                                flow_id,
                                dedupe_state: "joined".to_string(),
                                phase: if value.is_some() {
                                    "succeeded".to_string()
                                } else {
                                    "failed".to_string()
                                },
                                result: if value.is_some() {
                                    "ok".to_string()
                                } else {
                                    "error".to_string()
                                },
                                cache_state: if value.is_some() {
                                    "stored".to_string()
                                } else {
                                    "skip".to_string()
                                },
                            },
                        };
                    }
                }
                let elapsed = joined_started_at.elapsed();
                if elapsed >= timeout {
                    if let Some(current) = guard.as_mut() {
                        if current.flow_id == flow_id {
                            current.participants = current.participants.saturating_sub(1);
                            if current.completed && current.participants == 0 {
                                *guard = None;
                            }
                        }
                    }
                    drop(guard);
                    return VerifiedOsNativeAttempt {
                        value: None,
                        event: VerifiedOsNativeEvent {
                            flow_id,
                            dedupe_state: "joined".to_string(),
                            phase: "cancelled".to_string(),
                            result: "cancelled".to_string(),
                            cache_state: "skip".to_string(),
                        },
                    };
                }
                let remaining = timeout.saturating_sub(elapsed);
                let (next_guard, _) = match singleflight.cv.wait_timeout(guard, remaining) {
                    Ok(outcome) => outcome,
                    Err(_) => {
                        return VerifiedOsNativeAttempt {
                            value: None,
                            event: VerifiedOsNativeEvent {
                                flow_id,
                                dedupe_state: "joined".to_string(),
                                phase: "failed".to_string(),
                                result: "error".to_string(),
                                cache_state: "skip".to_string(),
                            },
                        };
                    }
                };
                guard = next_guard;
            }
        }

        let flow_id = next_verified_os_native_flow_id();
        *guard = Some(VerifiedOsNativeInFlight {
            flow_id: flow_id.clone(),
            participants: 1,
            completed: false,
            result: None,
        });
        drop(guard);

        let (value, timed_out) = run_os_native_verified_loader(timeout);
        if let Ok(mut guard) = singleflight.state.lock() {
            if let Some(current) = guard.as_mut() {
                if current.flow_id == flow_id {
                    current.completed = true;
                    current.result = value.clone();
                    current.participants = current.participants.saturating_sub(1);
                    let should_clear = current.participants == 0;
                    singleflight.cv.notify_all();
                    if should_clear {
                        *guard = None;
                    }
                }
            }
        }
        return VerifiedOsNativeAttempt {
            value: value.clone(),
            event: VerifiedOsNativeEvent {
                flow_id,
                dedupe_state: "leader".to_string(),
                phase: if timed_out {
                    "cancelled".to_string()
                } else if value.is_some() {
                    "succeeded".to_string()
                } else {
                    "failed".to_string()
                },
                result: if timed_out {
                    "cancelled".to_string()
                } else if value.is_some() {
                    "ok".to_string()
                } else {
                    "error".to_string()
                },
                cache_state: if value.is_some() {
                    "stored".to_string()
                } else {
                    "skip".to_string()
                },
            },
        };
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        VerifiedOsNativeAttempt {
            value: None,
            event: VerifiedOsNativeEvent {
                flow_id: "unsupported-os-native".to_string(),
                dedupe_state: "not-applicable".to_string(),
                phase: "failed".to_string(),
                result: "unsupported".to_string(),
                cache_state: "skip".to_string(),
            },
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn run_os_native_verified_loader(timeout: Duration) -> (Option<Vec<u8>>, bool) {
    let (tx, rx) = mpsc::sync_channel::<Option<Vec<u8>>>(1);
    #[cfg(test)]
    let test_loader = TEST_OS_NATIVE_VERIFIED_LOADER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|guard| guard.clone());
    let spawned = std::thread::Builder::new()
        .name("bridgingio-os-native-kek-verified".into())
        .spawn(move || {
            #[cfg(test)]
            if let Some(loader) = test_loader {
                let _ = tx.send(loader());
                return;
            }
            let _ = tx.send(os_native_protector_kek_blocking());
        });
    if spawned.is_err() {
        return (None, false);
    }
    match rx.recv_timeout(timeout) {
        Ok(value) => (value, false),
        Err(mpsc::RecvTimeoutError::Timeout) => (None, true),
        Err(mpsc::RecvTimeoutError::Disconnected) => (None, false),
    }
}

fn verified_os_native_singleflight() -> &'static VerifiedOsNativeSingleflight {
    OS_NATIVE_VERIFIED_SINGLEFLIGHT.get_or_init(|| VerifiedOsNativeSingleflight {
        state: Mutex::new(None),
        cv: Condvar::new(),
    })
}

fn next_verified_os_native_flow_id() -> String {
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or(0);
    let counter = OS_NATIVE_VERIFIED_FLOW_COUNTER
        .get_or_init(|| Mutex::new(1))
        .lock()
        .map(|mut value| {
            let current = *value;
            *value = value.saturating_add(1);
            current
        })
        .unwrap_or(0);
    format!("verified-os-native-{stamp}-{counter:08x}")
}

fn verified_os_native_timeout() -> Duration {
    #[cfg(test)]
    if let Some(value) = TEST_OS_NATIVE_VERIFIED_TIMEOUT_MS
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|guard| *guard)
    {
        return Duration::from_millis(value);
    }
    Duration::from_secs(OS_NATIVE_VERIFIED_TIMEOUT_SECS)
}

#[cfg(test)]
fn set_test_os_native_verified_timeout_ms(timeout_ms: Option<u64>) {
    if let Ok(mut guard) = TEST_OS_NATIVE_VERIFIED_TIMEOUT_MS
        .get_or_init(|| Mutex::new(None))
        .lock()
    {
        *guard = timeout_ms;
    }
}

#[cfg(test)]
fn set_test_os_native_verified_loader(
    loader: Option<Arc<dyn Fn() -> Option<Vec<u8>> + Send + Sync>>,
) {
    if let Ok(mut guard) = TEST_OS_NATIVE_VERIFIED_LOADER
        .get_or_init(|| Mutex::new(None))
        .lock()
    {
        *guard = loader;
    }
}

#[cfg(test)]
fn reset_verified_os_native_state_for_tests() {
    cache_verified_os_native_kek(None);
    if let Ok(mut guard) = verified_os_native_singleflight().state.lock() {
        *guard = None;
    }
    if let Ok(mut guard) = OS_NATIVE_VERIFIED_FLOW_COUNTER
        .get_or_init(|| Mutex::new(1))
        .lock()
    {
        *guard = 1;
    }
    set_test_os_native_verified_timeout_ms(None);
    set_test_os_native_verified_loader(None);
    LAST_VERIFIED_OS_NATIVE_EVENT.with(|slot| {
        *slot.borrow_mut() = None;
    });
}

fn os_native_protector_kek_inner() -> Option<Vec<u8>> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        if !os_native_platform_binding_supported() {
            return None;
        }
        let (tx, rx) = mpsc::sync_channel::<Option<Vec<u8>>>(1);
        let spawned = std::thread::Builder::new()
            .name("bridgingio-os-native-kek-probe".into())
            .spawn(move || {
                let _ = tx.send(os_native_protector_kek_blocking());
            });
        if spawned.is_err() {
            return None;
        }
        match rx.recv_timeout(Duration::from_millis(300)) {
            Ok(value) => value,
            Err(_) => None,
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn os_native_protector_kek_blocking() -> Option<Vec<u8>> {
    let entry =
        keyring::Entry::new(OS_NATIVE_PROTECTOR_SERVICE, OS_NATIVE_PROTECTOR_ACCOUNT).ok()?;
    if let Ok(existing) = entry.get_password() {
        if let Ok(decoded) = hex_decode(existing.trim()) {
            if decoded.len() == 32 {
                return Some(decoded);
            }
        }
    }
    let generated = pseudo_random_bytes(32, "os-native-protector-kek");
    let encoded = hex_encode(&generated);
    if entry.set_password(&encoded).is_ok() {
        return Some(generated);
    }
    None
}

fn env_flag_enabled(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|raw| {
            matches!(
                raw.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn protector_kek(binding: &str) -> Vec<u8> {
    if normalize_vault_method_label(binding) == "os-native" {
        if let Some(platform_kek) = os_native_protector_kek() {
            return platform_kek;
        }
    }
    fallback_protector_kek(binding)
}

fn fallback_protector_kek(binding: &str) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(b"bridgingio-protector-kek-v1");
    hasher.update(binding.as_bytes());
    hasher.finalize().to_vec()
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

fn hex_decode(raw: &str) -> Result<Vec<u8>, String> {
    let raw = raw.trim();
    if raw.len() % 2 != 0 {
        return Err("hex payload length must be even".into());
    }
    let mut out = Vec::with_capacity(raw.len() / 2);
    let bytes = raw.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        let hi = decode_hex_nibble(bytes[index])
            .ok_or_else(|| format!("invalid hex character at byte {}", index))?;
        let lo = decode_hex_nibble(bytes[index + 1])
            .ok_or_else(|| format!("invalid hex character at byte {}", index + 1))?;
        out.push((hi << 4) | lo);
        index += 2;
    }
    Ok(out)
}

fn decode_hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn detect_schema_version(raw: &str) -> Result<u32, VaultError> {
    let parsed: serde_json::Value = serde_json::from_str(raw).map_err(|err| {
        VaultError::StorageIo(format!(
            "failed to parse metadata json for schema probe: {err}"
        ))
    })?;
    let Some(schema) = parsed
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
    else {
        return Err(VaultError::StorageIo(
            "metadata db missing schema_version".into(),
        ));
    };
    u32::try_from(schema).map_err(|_| {
        VaultError::StorageIo(format!("metadata schema_version {schema} overflows u32"))
    })
}

fn write_blob(path: PathBuf, payload: &[u8]) -> Result<(), VaultError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            VaultError::StorageIo(format!(
                "failed to create blob directory {}: {err}",
                parent.display()
            ))
        })?;
    }
    fs::write(&path, payload).map_err(|err| {
        VaultError::StorageIo(format!("failed to write blob {}: {err}", path.display()))
    })
}

fn system_time_to_unix_secs(value: SystemTime) -> u64 {
    value
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn unix_secs_to_system_time(value: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(value)
}

#[cfg(test)]
mod tests {
    use super::{
        canonical_ssh_private_key_ref_from_key_name, command_audit_preview,
        normalize_credential_ref, read_passive_vault_projection, AgentTokenAuthResult,
        AgentTokenStatus, CreateAgentTokenRequest, DeleteAgentTokenRequest, DeleteVaultRequest,
        LocalAdminActionKind, SecretBytes, SecretVaultRouter, SshAgentBrokerPrepareRequest,
        SshAgentBrokerSessionState, SshHostKeyPolicy, SshKeyPassphraseHandling, TokenScopeInput,
        TrustedLocalSshKeyImportRequest, UpdateAgentTokenAccessRequest,
        UpdateAgentTokenLabelRequest, UpdateAgentTokenScopeRequest, VaultError, VaultLockState,
        VaultReadinessState, VaultUnlockPolicy, VaultUnlockTriggerPolicy,
        os_native_protector_kek_verified_attempt, reset_verified_os_native_state_for_tests,
        set_test_os_native_verified_loader, set_test_os_native_verified_timeout_ms,
    };
    use base64::Engine as _;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Barrier,
    };
    use std::thread;
    use std::time::Duration;

    fn new_temp_vault_dir(label: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("bridgingio-vault-{label}-{nonce}"));
        fs::create_dir_all(&dir).expect("create temp vault dir");
        dir
    }

    fn mint_create_token_attestation(
        router: &mut SecretVaultRouter,
        request: &CreateAgentTokenRequest,
    ) -> String {
        let digest = super::payload_digest_for_token_create_request(request);
        let intent = router
            .create_local_admin_intent_with_digest(
                LocalAdminActionKind::CreateAgentToken,
                super::LOCAL_ADMIN_CREATE_TOKEN_TARGET,
                &digest,
                &request.created_by,
                Duration::from_secs(60),
            )
            .expect("create token intent");
        router
            .complete_local_admin_attestation(
                &intent.intent_id,
                &request.created_by,
                "passkey",
                Duration::from_secs(30),
            )
            .expect("create token attestation")
            .attestation_id
    }

    fn mint_scope_update_attestation(
        router: &mut SecretVaultRouter,
        request: &UpdateAgentTokenScopeRequest,
    ) -> String {
        let digest = super::payload_digest_for_token_scope_update_request(request);
        let intent = router
            .create_local_admin_intent_with_digest(
                LocalAdminActionKind::UpdateAgentTokenScope,
                &request.token_id,
                &digest,
                &request.changed_by,
                Duration::from_secs(60),
            )
            .expect("create scope intent");
        router
            .complete_local_admin_attestation(
                &intent.intent_id,
                &request.changed_by,
                "passkey",
                Duration::from_secs(30),
            )
            .expect("scope attestation")
            .attestation_id
    }

    fn mint_delete_token_attestation(
        router: &mut SecretVaultRouter,
        request: &DeleteAgentTokenRequest,
    ) -> String {
        let digest = super::payload_digest_for_token_delete_request(&request.token_id);
        let intent = router
            .create_local_admin_intent_with_digest(
                LocalAdminActionKind::DeleteAgentToken,
                &request.token_id,
                &digest,
                &request.requested_by,
                Duration::from_secs(60),
            )
            .expect("create delete-token intent");
        router
            .complete_local_admin_attestation(
                &intent.intent_id,
                &request.requested_by,
                "passkey",
                Duration::from_secs(30),
            )
            .expect("delete-token attestation")
            .attestation_id
    }

    fn mint_delete_vault_attestation(
        router: &mut SecretVaultRouter,
        request: &DeleteVaultRequest,
    ) -> String {
        let digest = super::payload_digest_for_delete_vault_request();
        let intent = router
            .create_local_admin_intent_with_digest(
                LocalAdminActionKind::DeleteVault,
                super::LOCAL_ADMIN_DELETE_VAULT_TARGET,
                &digest,
                &request.requested_by,
                Duration::from_secs(60),
            )
            .expect("create delete-vault intent");
        router
            .complete_local_admin_attestation(
                &intent.intent_id,
                &request.requested_by,
                "passkey",
                Duration::from_secs(30),
            )
            .expect("delete-vault attestation")
            .attestation_id
    }

    fn openssh_test_key(encrypted: bool) -> String {
        fn push_u32(bytes: &mut Vec<u8>, value: u32) {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        fn push_string(bytes: &mut Vec<u8>, value: &str) {
            push_u32(bytes, value.len() as u32);
            bytes.extend_from_slice(value.as_bytes());
        }

        let mut payload = b"openssh-key-v1\0".to_vec();
        if encrypted {
            push_string(&mut payload, "aes256-ctr");
            push_string(&mut payload, "bcrypt");
            push_string(&mut payload, "kdfopts");
        } else {
            push_string(&mut payload, "none");
            push_string(&mut payload, "none");
            push_string(&mut payload, "");
        }
        push_u32(&mut payload, 1);
        push_string(&mut payload, "ssh-ed25519");

        let encoded = base64::engine::general_purpose::STANDARD.encode(payload);
        format!(
            "-----BEGIN OPENSSH PRIVATE KEY-----\n{}\n-----END OPENSSH PRIVATE KEY-----\n",
            encoded
        )
    }

    #[test]
    fn normalizes_legacy_credential_refs_to_canonical_uri() {
        let canonical = normalize_credential_ref("vault:ssh-key:ops-prod").expect("canonical");
        assert_eq!(canonical, "vault://bridgingio/ssh-private-key/ops-prod");

        let namespaced = normalize_credential_ref("vault:infra:token:grok_default")
            .expect("canonical namespaced");
        assert_eq!(namespaced, "vault://infra/opaque-token/grok-default");
    }

    #[test]
    fn canonicalizes_key_name_to_ssh_private_key_ref() {
        let canonical =
            canonical_ssh_private_key_ref_from_key_name("Ops Prod_Main").expect("canonical ref");
        assert_eq!(
            canonical,
            "vault://bridgingio/ssh-private-key/ops-prod-main"
        );
    }

    #[test]
    fn trusted_local_ssh_import_creates_and_rotates_same_canonical_ref() {
        let mut router = SecretVaultRouter::default();
        router
            .set_active_backend("builtin-encrypted")
            .expect("switch backend");
        let first = router
            .import_ssh_private_key_trusted_local(TrustedLocalSshKeyImportRequest {
                key_name: "ops-prod".into(),
                label: Some("Ops Prod".into()),
                private_key: SecretBytes::from_utf8(openssh_test_key(false)),
                passphrase: None,
                imported_by: "menuconfig:local-operator".into(),
                rotation_reason: None,
            })
            .expect("first import");
        let second = router
            .import_ssh_private_key_trusted_local(TrustedLocalSshKeyImportRequest {
                key_name: "ops-prod".into(),
                label: Some("Ops Prod".into()),
                private_key: SecretBytes::from_utf8(openssh_test_key(false)),
                passphrase: None,
                imported_by: "menuconfig:local-operator".into(),
                rotation_reason: Some("import-new-version".into()),
            })
            .expect("second import");

        assert_eq!(first.credential_ref, second.credential_ref);
        assert_ne!(first.active_version, second.active_version);

        let metadata = router
            .secret_metadata(&first.credential_ref)
            .expect("metadata lookup")
            .expect("metadata exists");
        assert_eq!(metadata.record.rotation.rotation_count, 1);
        assert_eq!(metadata.record.label, "Ops Prod");
    }

    #[test]
    fn trusted_local_ssh_import_rejects_invalid_key_payload() {
        let mut router = SecretVaultRouter::default();
        router
            .set_active_backend("builtin-encrypted")
            .expect("switch backend");
        let err = router
            .import_ssh_private_key_trusted_local(TrustedLocalSshKeyImportRequest {
                key_name: "ops-prod".into(),
                label: Some("Ops Prod".into()),
                private_key: SecretBytes::from_utf8("not-a-private-key"),
                passphrase: None,
                imported_by: "menuconfig:local-operator".into(),
                rotation_reason: None,
            })
            .expect_err("invalid key must be rejected");
        assert!(matches!(err, VaultError::InvalidSshPrivateKey(_)));
    }

    #[test]
    fn trusted_local_ssh_import_requires_passphrase_for_encrypted_openssh_input() {
        let mut router = SecretVaultRouter::default();
        router
            .set_active_backend("builtin-encrypted")
            .expect("switch backend");
        let encrypted_key = openssh_test_key(true);
        let err = router
            .import_ssh_private_key_trusted_local(TrustedLocalSshKeyImportRequest {
                key_name: "ops-encrypted".into(),
                label: Some("Ops Encrypted".into()),
                private_key: SecretBytes::from_utf8(encrypted_key.clone()),
                passphrase: None,
                imported_by: "menuconfig:local-operator".into(),
                rotation_reason: None,
            })
            .expect_err("encrypted key requires passphrase");
        assert!(matches!(err, VaultError::SshKeyPassphraseRequired(_)));

        let imported = router
            .import_ssh_private_key_trusted_local(TrustedLocalSshKeyImportRequest {
                key_name: "ops-encrypted".into(),
                label: Some("Ops Encrypted".into()),
                private_key: SecretBytes::from_utf8(encrypted_key),
                passphrase: Some("correct horse battery staple".into()),
                imported_by: "menuconfig:local-operator".into(),
                rotation_reason: None,
            })
            .expect("encrypted key import");
        assert!(imported.encrypted_input);
    }

    #[test]
    fn trusted_local_ssh_import_exposes_display_safe_summary_and_detail_fields() {
        let mut router = SecretVaultRouter::default();
        router
            .set_active_backend("builtin-encrypted")
            .expect("switch backend");
        let imported = router
            .import_ssh_private_key_trusted_local(TrustedLocalSshKeyImportRequest {
                key_name: "display-safe".into(),
                label: Some("Display Safe".into()),
                private_key: SecretBytes::from_utf8(openssh_test_key(false)),
                passphrase: None,
                imported_by: "menuconfig:local-operator".into(),
                rotation_reason: None,
            })
            .expect("import key");

        let summaries = router.list_secret_summaries();
        let summary = summaries
            .iter()
            .find(|item| item.reference == imported.credential_ref)
            .expect("summary");
        assert_eq!(summary.kind, "ssh-private-key");
        assert_eq!(summary.label, "Display Safe");
        assert!(summary.active_version_id.is_some());

        let detail = router
            .secret_metadata(&imported.credential_ref)
            .expect("metadata")
            .expect("detail");
        assert_eq!(detail.record.reference, imported.credential_ref);
        assert_eq!(detail.record.kind, "ssh-private-key");
        assert_eq!(detail.record.label, "Display Safe");
        assert_eq!(
            detail.record.active_version_id.as_deref(),
            Some(imported.active_version.as_str())
        );
    }

    #[test]
    fn trusted_local_ssh_key_can_be_deleted_after_import() {
        let mut router = SecretVaultRouter::default();
        router
            .set_active_backend("builtin-encrypted")
            .expect("switch backend");
        let imported = router
            .import_ssh_private_key_trusted_local(TrustedLocalSshKeyImportRequest {
                key_name: "delete-me".into(),
                label: Some("Delete Me".into()),
                private_key: SecretBytes::from_utf8(openssh_test_key(false)),
                passphrase: None,
                imported_by: "menuconfig:local-operator".into(),
                rotation_reason: None,
            })
            .expect("import key");
        router
            .delete_secret_trusted_local(&imported.credential_ref)
            .expect("delete imported key");

        let summaries = router.list_secret_summaries();
        assert!(summaries
            .iter()
            .all(|item| item.reference != imported.credential_ref));
        let metadata = router
            .secret_metadata(&imported.credential_ref)
            .expect("metadata lookup");
        assert!(metadata.is_none());
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
        let diag = router.active_backend_diagnostics().expect("diag");
        if matches!(diag.status, VaultReadinessState::Degraded) {
            let err = router
                .put("vault:ssh-key:dev", "PRIVATE_KEY", "dev key")
                .expect_err("os-native degraded backend should fail-closed by default");
            assert!(matches!(err, VaultError::FailClosed(_)));

            router
                .set_allow_degraded_mode(true)
                .expect("enable degraded mode");
            let record = router
                .put("vault:ssh-key:dev", "PRIVATE_KEY", "dev key")
                .expect("allow degraded mode");
            assert_eq!(record.backend, "os-native");
        } else {
            assert_eq!(diag.status, VaultReadinessState::Ready);
            let record = router
                .put("vault:ssh-key:dev", "PRIVATE_KEY", "dev key")
                .expect("ready os-native backend should allow writes");
            assert_eq!(record.backend, "os-native");
        }
    }

    #[test]
    fn exposes_readiness_and_protector_diagnostics() {
        let router = SecretVaultRouter::default();
        let diag = router.active_backend_diagnostics().expect("diag");
        assert!(
            matches!(
                diag.status,
                VaultReadinessState::Ready | VaultReadinessState::Degraded
            ),
            "unexpected os-native status: {:?}",
            diag.status
        );
        assert!(diag.fail_closed);
        assert!(!diag.protectors.is_empty());
        assert!(diag.message.contains("lock_state="));
    }

    #[test]
    fn passphrase_unlock_requires_matching_secret_and_uses_argon2id_profile() {
        let mut router = SecretVaultRouter::default();
        router
            .set_active_backend("builtin-encrypted")
            .expect("switch backend");
        router.unlock_with_os_native().expect("unlock os-native");
        router
            .configure_passphrase_protector("correct horse battery staple")
            .expect("configure passphrase");
        assert!(router.passphrase.enabled);
        assert_eq!(router.passphrase.params.kdf, "argon2id");

        router.lock_vault("test lock").expect("lock");
        let wrong = router.unlock_with_passphrase("wrong passphrase");
        assert!(matches!(wrong, Err(VaultError::PassphraseRejected)));
        router
            .unlock_with_passphrase("correct horse battery staple")
            .expect("unlock with passphrase");
        assert_eq!(router.vault_lock_state(), VaultLockState::Unlocked);
    }

    #[test]
    fn verified_unlock_migrates_legacy_fallback_wrap_to_verified_kek() {
        let mut router = SecretVaultRouter::default();
        let manifest = router
            .protector_wrap_manifests()
            .iter()
            .find(|item| item.protector_binding == "os-native")
            .cloned()
            .expect("os-native wrap manifest");
        let aad = format!("root:{}:{}", manifest.vault_key_id, manifest.wrap_id);
        let legacy_root_key = super::pseudo_random_bytes(32, "legacy-root-key");
        let legacy_wrapped = super::seal_aead(
            &super::fallback_protector_kek("os-native"),
            aad.as_bytes(),
            &legacy_root_key,
        );
        router
            .write_wrap_blob(&manifest.wrapped_key_locator, legacy_wrapped.clone())
            .expect("write legacy wrap");

        let verified_kek = super::pseudo_random_bytes(32, "verified-kek");
        let unlocked = router
            .unwrap_os_native_root_key_with_verified_kek(&manifest, &legacy_wrapped, &verified_kek)
            .expect("verified unlock with legacy fallback wrap");
        assert_eq!(unlocked, legacy_root_key);

        let migrated_wrapped = router
            .read_wrap_blob(&manifest.wrapped_key_locator)
            .expect("read migrated wrap");
        let migrated_root_key = super::open_aead(&verified_kek, aad.as_bytes(), &migrated_wrapped)
            .expect("migrated wrap uses verified os-native key");
        assert_eq!(migrated_root_key, legacy_root_key);

        let manifest_after = router
            .protector_wrap_manifests()
            .iter()
            .find(|item| item.wrap_id == manifest.wrap_id)
            .expect("updated manifest");
        assert_eq!(manifest_after.status, super::ProtectorWrapStatus::Ready);
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn verified_os_native_singleflight_joins_and_calls_provider_once() {
        reset_verified_os_native_state_for_tests();
        set_test_os_native_verified_timeout_ms(Some(2_000));
        let provider_calls = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(2));
        let loader = {
            let provider_calls = provider_calls.clone();
            Arc::new(move || {
                provider_calls.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(120));
                Some(vec![7u8; 32])
            })
        };
        set_test_os_native_verified_loader(Some(loader));

        let handles = (0..2)
            .map(|_| {
                let barrier = barrier.clone();
                thread::spawn(move || {
                    barrier.wait();
                    os_native_protector_kek_verified_attempt()
                })
            })
            .collect::<Vec<_>>();
        let mut events = Vec::new();
        for handle in handles {
            let attempt = handle.join().expect("join verification thread");
            assert!(attempt.value.is_some());
            events.push(attempt.event.dedupe_state);
        }

        assert_eq!(provider_calls.load(Ordering::SeqCst), 1);
        events.sort();
        assert_eq!(events, vec!["joined".to_string(), "leader".to_string()]);
        reset_verified_os_native_state_for_tests();
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn verified_os_native_joined_waiter_can_timeout_and_cancel() {
        reset_verified_os_native_state_for_tests();
        set_test_os_native_verified_timeout_ms(Some(30));
        let provider_calls = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(2));
        let loader = {
            let provider_calls = provider_calls.clone();
            Arc::new(move || {
                provider_calls.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(200));
                Some(vec![9u8; 32])
            })
        };
        set_test_os_native_verified_loader(Some(loader));

        let handles = (0..2)
            .map(|_| {
                let barrier = barrier.clone();
                thread::spawn(move || {
                    barrier.wait();
                    os_native_protector_kek_verified_attempt()
                })
            })
            .collect::<Vec<_>>();

        let attempts = handles
            .into_iter()
            .map(|handle| handle.join().expect("join thread"))
            .collect::<Vec<_>>();
        assert_eq!(provider_calls.load(Ordering::SeqCst), 1);
        assert!(attempts
            .iter()
            .any(|attempt| attempt.event.phase == "cancelled"));
        reset_verified_os_native_state_for_tests();
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn verified_os_native_failure_is_not_cached_as_success() {
        reset_verified_os_native_state_for_tests();
        set_test_os_native_verified_timeout_ms(Some(1_000));
        let provider_calls = Arc::new(AtomicUsize::new(0));
        let loader = {
            let provider_calls = provider_calls.clone();
            Arc::new(move || {
                provider_calls.fetch_add(1, Ordering::SeqCst);
                None
            })
        };
        set_test_os_native_verified_loader(Some(loader));

        let first = os_native_protector_kek_verified_attempt();
        let second = os_native_protector_kek_verified_attempt();
        assert!(first.value.is_none());
        assert!(second.value.is_none());
        assert_eq!(provider_calls.load(Ordering::SeqCst), 2);
        reset_verified_os_native_state_for_tests();
    }

    #[test]
    fn manual_unlock_policy_blocks_secret_access_until_explicit_unlock() {
        let mut router = SecretVaultRouter::default();
        router
            .set_active_backend("builtin-encrypted")
            .expect("switch backend");
        router
            .put("vault:ssh-key:manual", "manual-key", "manual key")
            .expect("store");
        router
            .set_unlock_policy(VaultUnlockPolicy {
                trigger_policy: VaultUnlockTriggerPolicy::ManualOnly,
                allowed_methods: vec!["os-native".into(), "passphrase".into()],
                preferred_method: "os-native".into(),
                cache_ttl_sec: 600,
                require_fresh_user_verification: true,
            })
            .expect("set policy");
        router.lock_vault("manual gate").expect("lock");

        let blocked = router.use_for_http_auth("vault:ssh-key:manual", "scope");
        assert!(matches!(blocked, Err(VaultError::VaultLocked(_))));

        router.unlock_with_os_native().expect("unlock");
        let lease = router
            .use_for_http_auth("vault:ssh-key:manual", "scope")
            .expect("lease");
        let revealed = lease
            .with_secret_bytes(|bytes| std::str::from_utf8(bytes).unwrap_or_default().to_string());
        assert_eq!(revealed, "manual-key");
    }

    #[test]
    fn on_every_secret_access_policy_relocks_vault_after_each_operation() {
        let mut router = SecretVaultRouter::default();
        router
            .set_active_backend("builtin-encrypted")
            .expect("switch backend");
        router
            .set_unlock_policy(VaultUnlockPolicy {
                trigger_policy: VaultUnlockTriggerPolicy::OnEverySecretAccess,
                allowed_methods: vec!["os-native".into()],
                preferred_method: "os-native".into(),
                cache_ttl_sec: 600,
                require_fresh_user_verification: false,
            })
            .expect("set policy");
        router
            .put("vault:ssh-key:every", "every-key", "every key")
            .expect("store");
        assert_eq!(router.vault_lock_state(), VaultLockState::Locked);

        let lease = router
            .use_for_http_auth("vault:ssh-key:every", "scope")
            .expect("lease");
        assert_eq!(
            lease.with_secret_bytes(|bytes| std::str::from_utf8(bytes)
                .unwrap_or_default()
                .to_string()),
            "every-key"
        );
        assert_eq!(router.vault_lock_state(), VaultLockState::Locked);
    }

    #[test]
    fn unlock_cache_ttl_expires_and_shutdown_cleans_runtime_state() {
        let mut router = SecretVaultRouter::default();
        router
            .set_active_backend("builtin-encrypted")
            .expect("switch backend");
        router
            .put("vault:ssh-key:ttl", "ttl-key", "ttl key")
            .expect("store");
        router
            .set_unlock_policy(VaultUnlockPolicy {
                trigger_policy: VaultUnlockTriggerPolicy::ManualOnly,
                allowed_methods: vec!["os-native".into()],
                preferred_method: "os-native".into(),
                cache_ttl_sec: 1,
                require_fresh_user_verification: false,
            })
            .expect("set policy");
        router.unlock_with_os_native().expect("unlock");
        assert_eq!(router.vault_lock_state(), VaultLockState::Unlocked);
        std::thread::sleep(Duration::from_secs(2));
        let blocked = router.use_for_http_auth("vault:ssh-key:ttl", "scope");
        assert!(matches!(blocked, Err(VaultError::VaultLocked(_))));
        router.shutdown_cleanup().expect("shutdown");
        assert_eq!(router.vault_lock_state(), VaultLockState::Locked);
        assert!(router.unlock_session.is_none());
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
        let diagnostics = router
            .ssh_agent_broker_diagnostics(&prepared.session.broker_session_id)
            .expect("diagnostics");
        assert!(diagnostics
            .iter()
            .any(|line| line.contains("vault binding: vault://bridgingio/ssh-private-key/ops@")));

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
        let identity_path = PathBuf::from(prepared.ssh_option_args[1].clone());
        assert!(identity_path.exists());
        assert!(identity_path
            .to_string_lossy()
            .contains("bridgingio-vault-runtime/ssh-broker"));
        assert!(prepared
            .diagnostics
            .iter()
            .any(|line| line.contains("falling back to ephemeral identity file")));
        assert!(prepared
            .diagnostics
            .iter()
            .any(|line| line.contains("private runtime cleanup paths")));
        router
            .close_ssh_agent_broker_session(&prepared.session.broker_session_id, "fallback cleanup")
            .expect("close fallback");
        assert!(!identity_path.exists());
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
        let mut request = CreateAgentTokenRequest {
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
        };
        request.attestation_id = Some(mint_create_token_attestation(&mut router, &request));
        let created = router.create_agent_token(request).expect("create token");
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
    fn agent_token_create_rejects_missing_or_synthetic_attestation() {
        let mut router = SecretVaultRouter::default();
        let request = CreateAgentTokenRequest {
            label: "nightly-runner".into(),
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
        };
        let missing = router.create_agent_token(request.clone());
        assert!(matches!(
            missing,
            Err(VaultError::LocalAdminVerificationRequired(_))
        ));

        let synthetic = router.create_agent_token(CreateAgentTokenRequest {
            attestation_id: Some("attest-001".into()),
            ..request
        });
        assert!(matches!(
            synthetic,
            Err(VaultError::LocalAdminAttestationMismatch(_))
        ));
    }

    #[test]
    fn agent_token_plaintext_only_on_create_and_not_returned_by_listing() {
        let mut router = SecretVaultRouter::default();
        let mut request = CreateAgentTokenRequest {
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
        };
        request.attestation_id = Some(mint_create_token_attestation(&mut router, &request));
        let created = router.create_agent_token(request).expect("create token");

        let listed = router.list_agent_tokens();
        assert_eq!(listed.len(), 1);
        assert_ne!(listed[0].token_id, created.plaintext_token);
        assert!(matches!(
            router.authenticate_agent_token(&created.plaintext_token),
            AgentTokenAuthResult::Authenticated(_)
        ));
    }

    #[test]
    fn agent_token_scope_update_creates_new_version_and_preserves_superseded_history() {
        let mut router = SecretVaultRouter::default();
        let mut create_request = CreateAgentTokenRequest {
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
        };
        create_request.attestation_id =
            Some(mint_create_token_attestation(&mut router, &create_request));
        let created = router
            .create_agent_token(create_request)
            .expect("create token");

        let mut update_request = UpdateAgentTokenScopeRequest {
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
            attestation_id: None,
        };
        update_request.attestation_id =
            Some(mint_scope_update_attestation(&mut router, &update_request));
        let updated = router
            .update_agent_token_scope(update_request)
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
    fn token_records_and_scope_history_persist_across_reload() {
        let vault_dir = new_temp_vault_dir("token-persist");
        let mut router = SecretVaultRouter::with_persistent_store(&vault_dir).expect("open store");

        let mut create_request = CreateAgentTokenRequest {
            label: "persisted-token".into(),
            created_by: "local-operator".into(),
            expires_in: None,
            idle_timeout_sec: None,
            scope: TokenScopeInput {
                scope_profile: Some("strict-default".into()),
                target_ids: vec!["target-a".into()],
                tool_ids: Vec::new(),
                max_risk_envelope: Some("deny-all".into()),
                allow_open_shell: Some(false),
                allow_write_shell_input: Some(false),
                allow_artifact_cross_principal: Some(false),
                allow_delegation: Some(false),
                allow_admin_actions: Some(false),
            },
            attestation_id: None,
        };
        create_request.attestation_id =
            Some(mint_create_token_attestation(&mut router, &create_request));
        let created = router
            .create_agent_token(create_request)
            .expect("create token");

        let mut update_request = UpdateAgentTokenScopeRequest {
            token_id: created.summary.token_id.clone(),
            changed_by: "local-operator".into(),
            scope: TokenScopeInput {
                scope_profile: Some("strict-default".into()),
                target_ids: vec!["target-a".into(), "target-b".into()],
                tool_ids: vec!["terminal.exec".into()],
                max_risk_envelope: Some("deny-all".into()),
                allow_open_shell: Some(false),
                allow_write_shell_input: Some(false),
                allow_artifact_cross_principal: Some(false),
                allow_delegation: Some(false),
                allow_admin_actions: Some(false),
            },
            reason: Some("expand scope".into()),
            attestation_id: None,
        };
        update_request.attestation_id =
            Some(mint_scope_update_attestation(&mut router, &update_request));
        router
            .update_agent_token_scope(update_request)
            .expect("update scope");
        drop(router);

        let mut reloaded =
            SecretVaultRouter::with_persistent_store(&vault_dir).expect("reload store");
        let listed = reloaded.list_agent_tokens();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].active_scope_version, 2);
        let history = reloaded.token_scope_history(&listed[0].token_id);
        assert_eq!(history.len(), 2);
        assert!(history.iter().any(|scope| scope.version == 1
            && matches!(scope.status, super::TokenScopeStatus::Superseded)));
        assert!(history
            .iter()
            .any(|scope| scope.version == 2
                && matches!(scope.status, super::TokenScopeStatus::Active)));

        let _ = fs::remove_dir_all(vault_dir);
    }

    #[test]
    fn revoke_propagates_to_descendant_tokens_via_lineage() {
        let mut router = SecretVaultRouter::default();

        let mut root_request = CreateAgentTokenRequest {
            label: "root-token".into(),
            created_by: "local-operator".into(),
            expires_in: None,
            idle_timeout_sec: None,
            scope: TokenScopeInput {
                scope_profile: Some("strict-default".into()),
                target_ids: vec!["target-root".into()],
                tool_ids: Vec::new(),
                max_risk_envelope: None,
                allow_open_shell: None,
                allow_write_shell_input: None,
                allow_artifact_cross_principal: None,
                allow_delegation: None,
                allow_admin_actions: None,
            },
            attestation_id: None,
        };
        root_request.attestation_id =
            Some(mint_create_token_attestation(&mut router, &root_request));
        let root = router
            .create_agent_token(root_request)
            .expect("create root token");

        let mut child_request = CreateAgentTokenRequest {
            label: "child-token".into(),
            created_by: "local-operator".into(),
            expires_in: None,
            idle_timeout_sec: None,
            scope: TokenScopeInput {
                scope_profile: Some("strict-default".into()),
                target_ids: vec!["target-child".into()],
                tool_ids: Vec::new(),
                max_risk_envelope: None,
                allow_open_shell: None,
                allow_write_shell_input: None,
                allow_artifact_cross_principal: None,
                allow_delegation: None,
                allow_admin_actions: None,
            },
            attestation_id: None,
        };
        child_request.attestation_id =
            Some(mint_create_token_attestation(&mut router, &child_request));
        let child = router
            .create_agent_token(child_request)
            .expect("create child token");

        router
            .agent_tokens
            .get_mut(&child.summary.token_id)
            .expect("child token exists")
            .parent_token_id = Some(root.summary.token_id.clone());

        router
            .revoke_agent_token(&root.summary.token_id, Some("user delete".into()))
            .expect("revoke root");
        let listed = router.list_agent_tokens();
        let child_summary = listed
            .iter()
            .find(|summary| summary.token_id == child.summary.token_id)
            .expect("child summary");
        assert_eq!(child_summary.status, AgentTokenStatus::Revoked);
        assert_eq!(child_summary.revoke_reason.as_deref(), Some("user delete"));
    }

    #[test]
    fn agent_token_lifetime_expiration_blocks_authentication() {
        let mut router = SecretVaultRouter::default();
        let mut request = CreateAgentTokenRequest {
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
        };
        request.attestation_id = Some(mint_create_token_attestation(&mut router, &request));
        let created = router.create_agent_token(request).expect("create token");
        std::thread::sleep(Duration::from_millis(5));
        assert!(matches!(
            router.authenticate_agent_token(&created.plaintext_token),
            AgentTokenAuthResult::Rejected(_)
        ));
        let listed = router.list_agent_tokens();
        assert_eq!(listed[0].status, AgentTokenStatus::Expired);
    }

    #[test]
    fn expired_token_must_be_revoked_before_delete() {
        let mut router = SecretVaultRouter::default();
        let mut create_request = CreateAgentTokenRequest {
            label: "expires-then-delete".into(),
            created_by: "local-operator".into(),
            expires_in: Some(Duration::from_millis(1)),
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
        };
        create_request.attestation_id =
            Some(mint_create_token_attestation(&mut router, &create_request));
        let created = router
            .create_agent_token(create_request)
            .expect("create expiring token");
        std::thread::sleep(Duration::from_millis(5));
        let listed = router.list_agent_tokens();
        assert_eq!(listed[0].status, AgentTokenStatus::Expired);

        let mut delete_request = DeleteAgentTokenRequest {
            token_id: created.summary.token_id.clone(),
            requested_by: "local-operator".into(),
            attestation_id: String::new(),
        };
        delete_request.attestation_id = mint_delete_token_attestation(&mut router, &delete_request);
        let direct_delete = router.delete_agent_token_with_attestation(delete_request.clone());
        assert!(matches!(
            direct_delete,
            Err(VaultError::AgentTokenRejected(_))
        ));

        router
            .revoke_agent_token(
                &created.summary.token_id,
                Some("revoke before delete".into()),
            )
            .expect("revoke expired token");
        delete_request.attestation_id = mint_delete_token_attestation(&mut router, &delete_request);
        let deleted = router
            .delete_agent_token_with_attestation(delete_request)
            .expect("delete revoked token");
        assert_eq!(deleted.status, AgentTokenStatus::Deleted);
        assert!(deleted.deleted_at.is_some());
        assert!(router.list_agent_tokens().is_empty());
    }

    #[test]
    fn token_label_update_is_allowed_until_deleted() {
        let mut router = SecretVaultRouter::default();
        let mut create_request = CreateAgentTokenRequest {
            label: "initial-label".into(),
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
        };
        create_request.attestation_id =
            Some(mint_create_token_attestation(&mut router, &create_request));
        let created = router
            .create_agent_token(create_request)
            .expect("create token");

        let updated = router
            .update_agent_token_label(UpdateAgentTokenLabelRequest {
                token_id: created.summary.token_id.clone(),
                label: "Codex".into(),
                changed_by: "local-operator".into(),
            })
            .expect("update label");
        assert_eq!(updated.label, "Codex");

        router
            .revoke_agent_token(&created.summary.token_id, Some("cleanup".into()))
            .expect("revoke token");
        let mut delete_request = DeleteAgentTokenRequest {
            token_id: created.summary.token_id.clone(),
            requested_by: "local-operator".into(),
            attestation_id: String::new(),
        };
        delete_request.attestation_id = mint_delete_token_attestation(&mut router, &delete_request);
        router
            .delete_agent_token_with_attestation(delete_request)
            .expect("delete token");
        let update_deleted = router.update_agent_token_label(UpdateAgentTokenLabelRequest {
            token_id: created.summary.token_id,
            label: "after-delete".into(),
            changed_by: "local-operator".into(),
        });
        assert!(matches!(
            update_deleted,
            Err(VaultError::AgentTokenRejected(_))
        ));
    }

    #[test]
    fn token_label_write_contract_enforces_ascii_separator_rules_and_keeps_legacy_readable() {
        let mut router = SecretVaultRouter::default();
        let mut create_request = CreateAgentTokenRequest {
            label: "test_tok".into(),
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
        };
        create_request.attestation_id =
            Some(mint_create_token_attestation(&mut router, &create_request));
        let created = router
            .create_agent_token(create_request)
            .expect("create token");

        let invalid_update = router.update_agent_token_label(UpdateAgentTokenLabelRequest {
            token_id: created.summary.token_id.clone(),
            label: "bad label".into(),
            changed_by: "local-operator".into(),
        });
        assert!(matches!(
            invalid_update,
            Err(VaultError::AgentTokenRejected(_))
        ));

        router
            .agent_tokens
            .get_mut(&created.summary.token_id)
            .expect("token exists")
            .label = "legacy label with spaces".into();
        let listed = router.list_agent_tokens();
        assert_eq!(listed[0].label, "legacy label with spaces");

        let valid_update = router
            .update_agent_token_label(UpdateAgentTokenLabelRequest {
                token_id: created.summary.token_id,
                label: "legacy_fixed".into(),
                changed_by: "local-operator".into(),
            })
            .expect("valid update");
        assert_eq!(valid_update.label, "legacy_fixed");
    }

    #[test]
    fn token_access_toggle_supports_disabled_projection_but_not_terminal_reactivation() {
        let mut router = SecretVaultRouter::default();
        let mut create_request = CreateAgentTokenRequest {
            label: "toggle-access".into(),
            created_by: "local-operator".into(),
            expires_in: None,
            idle_timeout_sec: None,
            scope: TokenScopeInput {
                scope_profile: Some("strict-default".into()),
                target_ids: vec!["target-a".into()],
                tool_ids: vec!["terminal.exec".into()],
                max_risk_envelope: Some("allow-low".into()),
                allow_open_shell: Some(true),
                allow_write_shell_input: Some(true),
                allow_artifact_cross_principal: Some(false),
                allow_delegation: Some(false),
                allow_admin_actions: Some(false),
            },
            attestation_id: None,
        };
        create_request.attestation_id =
            Some(mint_create_token_attestation(&mut router, &create_request));
        let created = router
            .create_agent_token(create_request)
            .expect("create token");

        let disabled = router
            .update_agent_token_access(UpdateAgentTokenAccessRequest {
                token_id: created.summary.token_id.clone(),
                enabled: false,
                changed_by: "local-operator".into(),
            })
            .expect("disable token");
        assert_eq!(disabled.status, AgentTokenStatus::Disabled);
        assert!(matches!(
            router.authenticate_agent_token(&created.plaintext_token),
            AgentTokenAuthResult::Rejected(_)
        ));

        let enabled = router
            .update_agent_token_access(UpdateAgentTokenAccessRequest {
                token_id: created.summary.token_id.clone(),
                enabled: true,
                changed_by: "local-operator".into(),
            })
            .expect("enable token");
        assert_eq!(enabled.status, AgentTokenStatus::Active);
        assert!(matches!(
            router.authenticate_agent_token(&created.plaintext_token),
            AgentTokenAuthResult::Authenticated(_)
        ));

        let mut expiring_request = CreateAgentTokenRequest {
            label: "toggle-access-expiring".into(),
            created_by: "local-operator".into(),
            expires_in: Some(Duration::from_millis(1)),
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
        };
        expiring_request.attestation_id = Some(mint_create_token_attestation(
            &mut router,
            &expiring_request,
        ));
        let expiring = router
            .create_agent_token(expiring_request)
            .expect("create expiring token");
        router
            .update_agent_token_access(UpdateAgentTokenAccessRequest {
                token_id: expiring.summary.token_id.clone(),
                enabled: false,
                changed_by: "local-operator".into(),
            })
            .expect("disable expiring token");
        std::thread::sleep(Duration::from_millis(5));
        let cannot_reactivate = router.update_agent_token_access(UpdateAgentTokenAccessRequest {
            token_id: expiring.summary.token_id.clone(),
            enabled: true,
            changed_by: "local-operator".into(),
        });
        assert!(matches!(
            cannot_reactivate,
            Err(VaultError::AgentTokenRejected(_))
        ));
        let listed = router.list_agent_tokens();
        let expiring_summary = listed
            .iter()
            .find(|item| item.token_id == expiring.summary.token_id)
            .expect("expiring token listed");
        assert_eq!(expiring_summary.status, AgentTokenStatus::Expired);
    }

    #[test]
    fn rollback_detection_blocks_expiring_token_create_but_allows_long_lived() {
        let mut router = SecretVaultRouter::default();
        router.clock_watermark = std::time::SystemTime::now() + Duration::from_secs(120);

        let mut expiring = CreateAgentTokenRequest {
            label: "expiring-under-rollback".into(),
            created_by: "local-operator".into(),
            expires_in: Some(Duration::from_secs(60)),
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
        };
        expiring.attestation_id = Some(mint_create_token_attestation(&mut router, &expiring));
        let blocked = router.create_agent_token(expiring);
        assert!(matches!(blocked, Err(VaultError::AgentTokenRejected(_))));

        let mut long_lived = CreateAgentTokenRequest {
            label: "long-lived-under-rollback".into(),
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
        };
        long_lived.attestation_id = Some(mint_create_token_attestation(&mut router, &long_lived));
        let created = router
            .create_agent_token(long_lived)
            .expect("long-lived token should still be creatable");
        assert_eq!(created.summary.status, AgentTokenStatus::Active);
    }

    #[test]
    fn local_admin_intent_and_attestation_are_persisted_in_metadata_store() {
        let vault_dir = new_temp_vault_dir("local-admin-persist");
        let mut request = CreateAgentTokenRequest {
            label: "persisted-admin-token".into(),
            created_by: "local-operator".into(),
            expires_in: None,
            idle_timeout_sec: None,
            scope: TokenScopeInput {
                scope_profile: Some("strict-default".into()),
                target_ids: vec!["target-a".into()],
                tool_ids: Vec::new(),
                max_risk_envelope: None,
                allow_open_shell: Some(false),
                allow_write_shell_input: Some(false),
                allow_artifact_cross_principal: Some(false),
                allow_delegation: Some(false),
                allow_admin_actions: Some(false),
            },
            attestation_id: None,
        };
        let attestation_id = {
            let mut router = SecretVaultRouter::with_persistent_store(&vault_dir)
                .expect("open persistent store");
            mint_create_token_attestation(&mut router, &request)
        };

        let mut reloaded =
            SecretVaultRouter::with_persistent_store(&vault_dir).expect("reload persistent store");
        request.attestation_id = Some(attestation_id);
        let created = reloaded
            .create_agent_token(request)
            .expect("create token after reload");
        assert!(created.plaintext_token.starts_with("agt_"));

        let metadata = fs::read_to_string(vault_dir.join("metadata.db")).expect("read metadata");
        assert!(metadata.contains("\"intents\""));
        assert!(metadata.contains("\"attestations\""));

        let _ = fs::remove_dir_all(vault_dir);
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

    #[test]
    fn persistent_store_round_trip_restores_secret_state_and_blobs() {
        let vault_dir = new_temp_vault_dir("persist-roundtrip");
        let mut router = SecretVaultRouter::with_persistent_store(&vault_dir).expect("open store");
        router.init_vault_store().expect("init vault store");
        router
            .set_active_backend("builtin-encrypted")
            .expect("switch backend");
        router
            .put("vault:ssh-key:persisted", "PERSISTED-KEY", "persisted key")
            .expect("store secret");
        drop(router);

        let mut reloaded =
            SecretVaultRouter::with_persistent_store(&vault_dir).expect("reload store");
        let lease = reloaded
            .use_for_ssh_auth("vault:ssh-key:persisted", "target:persist")
            .expect("lease");
        let value = lease
            .with_secret_bytes(|bytes| std::str::from_utf8(bytes).unwrap_or_default().to_string());
        assert_eq!(value, "PERSISTED-KEY");

        let metadata_path = vault_dir.join("metadata.db");
        assert!(metadata_path.exists());
        let metadata = fs::read_to_string(&metadata_path).expect("read metadata");
        assert!(metadata.contains("\"schema_version\": 2"));
        assert!(metadata.contains("\"format_version\": 1"));

        let secret_blob_count = fs::read_dir(vault_dir.join("blobs/secret"))
            .expect("read secret blobs")
            .count();
        let wrap_blob_count = fs::read_dir(vault_dir.join("blobs/wrap"))
            .expect("read wrap blobs")
            .count();
        assert!(secret_blob_count > 0);
        assert!(wrap_blob_count > 0);

        let _ = fs::remove_dir_all(vault_dir);
    }

    #[test]
    fn passive_projection_defaults_when_no_metadata_exists() {
        let vault_dir = new_temp_vault_dir("passive-default");
        let projection = read_passive_vault_projection(&vault_dir).expect("read projection");
        assert_eq!(projection.lock_state, VaultLockState::Uninitialized);
        assert_eq!(projection.secret_count, 0);
        assert_eq!(projection.token_count, 0);
        let _ = fs::remove_dir_all(vault_dir);
    }

    #[test]
    fn vault_init_and_delete_returns_to_uninitialized_state() {
        let vault_dir = new_temp_vault_dir("vault-init-delete");
        let mut router = SecretVaultRouter::with_persistent_store(&vault_dir).expect("open store");
        assert_eq!(router.lock_state, VaultLockState::Uninitialized);

        let lock_state = router.init_vault_store().expect("init vault");
        assert_eq!(lock_state, VaultLockState::Locked);

        let mut delete_request = DeleteVaultRequest {
            requested_by: "local-operator".into(),
            attestation_id: String::new(),
        };
        delete_request.attestation_id = mint_delete_vault_attestation(&mut router, &delete_request);
        let deleted_state = router
            .delete_vault_with_attestation(delete_request)
            .expect("delete vault");
        assert_eq!(deleted_state, VaultLockState::Uninitialized);

        let projection = read_passive_vault_projection(&vault_dir).expect("read projection");
        assert_eq!(projection.lock_state, VaultLockState::Uninitialized);

        let unlock = router.unlock_with_os_native_verified();
        assert!(matches!(unlock, Err(VaultError::VaultUnavailable(_))));

        let _ = fs::remove_dir_all(vault_dir);
    }

    #[test]
    fn passive_projection_reads_v2_metadata_without_unlock_side_effects() {
        let vault_dir = new_temp_vault_dir("passive-v2");
        let metadata = r#"{
  "schema_version": 2,
  "lock_state": "Unlocked",
  "secrets": [{}, {}],
  "agent_tokens": [{}]
}"#;
        fs::write(vault_dir.join("metadata.db"), metadata).expect("write v2 metadata");
        let projection = read_passive_vault_projection(&vault_dir).expect("read projection");
        assert_eq!(projection.lock_state, VaultLockState::Unlocked);
        assert_eq!(projection.secret_count, 2);
        assert_eq!(projection.token_count, 1);
        let _ = fs::remove_dir_all(vault_dir);
    }

    #[test]
    fn passive_projection_reads_legacy_v1_metadata_counts() {
        let vault_dir = new_temp_vault_dir("passive-v1");
        let metadata = r#"{
  "schema_version": 1,
  "secrets": [{}, {}, {}]
}"#;
        fs::write(vault_dir.join("legacy-vault-state.json"), metadata).expect("write v1 metadata");
        let projection = read_passive_vault_projection(&vault_dir).expect("read projection");
        assert_eq!(projection.lock_state, VaultLockState::Locked);
        assert_eq!(projection.secret_count, 3);
        assert_eq!(projection.token_count, 0);
        let _ = fs::remove_dir_all(vault_dir);
    }

    #[test]
    fn migrates_legacy_v1_metadata_into_v2_layout() {
        let vault_dir = new_temp_vault_dir("legacy-migration");
        let reference = "vault://bridgingio/ssh-private-key/legacy";
        let version_id = "ver-000001";
        let root_key = vec![9u8; 32];
        let dek = vec![3u8; 32];
        let wrapped_root = super::seal_aead(
            &super::protector_kek("builtin-encrypted"),
            b"root:vrk-legacy:wrap-legacy",
            &root_key,
        );
        let wrapped_dek = super::seal_aead(
            &root_key,
            format!("vrk-wrap:{reference}:{version_id}").as_bytes(),
            &dek,
        );
        let ciphertext = super::seal_aead(
            &dek,
            format!("secret:{reference}:{version_id}").as_bytes(),
            b"LEGACY-KEY",
        );

        let legacy_json = format!(
            concat!(
                "{{\n",
                "  \"schema_version\": 1,\n",
                "  \"active_backend\": \"builtin-encrypted\",\n",
                "  \"allow_degraded_mode\": false,\n",
                "  \"next_version_seq\": 1,\n",
                "  \"next_audit_seq\": 1,\n",
                "  \"next_agent_token_seq\": 0,\n",
                "  \"next_intent_seq\": 0,\n",
                "  \"next_attestation_seq\": 0,\n",
                "  \"next_ssh_broker_seq\": 0,\n",
                "  \"key_envelope\": {{\"vault_key_id\":\"vrk-legacy\",\"state\":\"Active\",\"active_wrap_set_id\":\"wrap-set-legacy\",\"created_at_unix_sec\":1,\"rotated_at_unix_sec\":null,\"last_unlocked_at_unix_sec\":null}},\n",
                "  \"wrap_manifests\": [{{\"wrap_id\":\"wrap-legacy\",\"vault_key_id\":\"vrk-legacy\",\"protector_binding\":\"builtin-encrypted\",\"wrap_format\":\"sha256-stream-aead-v1\",\"wrapped_key_digest\":\"{wrapped_root_digest}\",\"wrapped_key_hex\":\"{wrapped_root_hex}\",\"created_at_unix_sec\":1,\"last_verified_at_unix_sec\":null,\"status\":\"Ready\"}}],\n",
                "  \"secrets\": [{{\"record\":{{\"reference\":\"{reference}\",\"kind\":\"ssh-private-key\",\"label\":\"legacy\",\"status\":\"Active\",\"active_version_id\":\"{version_id}\",\"created_by\":\"legacy\",\"created_at_unix_sec\":1,\"last_used_at_unix_sec\":null,\"last_rotated_at_unix_sec\":null,\"rotation\":{{\"rotation_count\":0,\"last_rotated_by\":null,\"last_rotation_reason\":null}},\"audit_chain_id\":\"audit-legacy\"}},",
                "\"versions\":[{{\"version\":{{\"version_id\":\"{version_id}\",\"reference\":\"{reference}\",\"version_seq\":1,\"state\":\"Active\",\"protector_binding\":\"builtin-encrypted\",\"ciphertext_locator\":\"blob://vault/builtin-encrypted/{reference}/{version_id}\",\"ciphertext_digest\":\"{ciphertext_digest}\",\"content_format\":\"ssh-private-key\",\"created_by\":\"legacy\",\"created_at_unix_sec\":1,\"activated_at_unix_sec\":1,\"superseded_at_unix_sec\":null,\"revoked_at_unix_sec\":null,\"destroy_after_unix_sec\":null}},\"ciphertext_hex\":\"{ciphertext_hex}\",\"wrapped_dek_hex\":\"{wrapped_dek_hex}\"}}]}}]\n",
                "}}\n"
            ),
            wrapped_root_digest = super::short_digest(&wrapped_root),
            wrapped_root_hex = super::hex_encode(&wrapped_root),
            reference = reference,
            version_id = version_id,
            ciphertext_digest = super::short_digest(&ciphertext),
            ciphertext_hex = super::hex_encode(&ciphertext),
            wrapped_dek_hex = super::hex_encode(&wrapped_dek),
        );

        fs::write(vault_dir.join("legacy-vault-state.json"), legacy_json).expect("write legacy");

        let mut migrated =
            SecretVaultRouter::with_persistent_store(&vault_dir).expect("load migrated store");
        let lease = migrated
            .use_for_ssh_auth(reference, "target:legacy")
            .expect("lease after migration");
        let value = lease
            .with_secret_bytes(|bytes| std::str::from_utf8(bytes).unwrap_or_default().to_string());
        assert_eq!(value, "LEGACY-KEY");

        let metadata_path = vault_dir.join("metadata.db");
        let metadata = fs::read_to_string(metadata_path).expect("read migrated metadata");
        assert!(metadata.contains("\"schema_version\": 2"));
        assert!(metadata.contains("\"wrapped_key_locator\""));

        let _ = fs::remove_file(vault_dir.join("legacy-vault-state.json"));
        let _ = fs::remove_dir_all(vault_dir);
    }

    #[test]
    fn vault_error_maps_to_shared_error_contract() {
        let shared =
            VaultError::LocalAdminVerificationRequired("fresh verification".into()).shared_error();
        assert_eq!(shared.status.as_str(), "locked");
        assert_eq!(shared.domain.as_str(), "vault");
        assert_eq!(shared.common_code.as_str(), "verification_required");
        assert_eq!(
            shared.module_code.as_deref(),
            Some("vault.local_admin_verification_required")
        );
        assert!(shared.recovery_hint.is_some());
    }
}
