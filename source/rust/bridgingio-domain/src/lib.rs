use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

pub type MetadataMap = BTreeMap<String, String>;
pub const TARGET_TERMINAL_SHELL_METADATA_KEY: &str = "terminal.shell";
pub const TARGET_TERMINAL_FAMILY_METADATA_KEY: &str = "terminal.family";
pub const TARGET_TERMINAL_CONCURRENCY_METADATA_KEY: &str = "terminal.concurrency";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContractStatus {
    Ready,
    Degraded,
    Fallback,
    Unsupported,
    Locked,
    NotReady,
    MethodNotImplemented,
    Failed,
}

impl ContractStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Degraded => "degraded",
            Self::Fallback => "fallback",
            Self::Unsupported => "unsupported",
            Self::Locked => "locked",
            Self::NotReady => "not_ready",
            Self::MethodNotImplemented => "method_not_implemented",
            Self::Failed => "failed",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "ready" => Some(Self::Ready),
            "degraded" => Some(Self::Degraded),
            "fallback" => Some(Self::Fallback),
            "unsupported" => Some(Self::Unsupported),
            "locked" => Some(Self::Locked),
            "not_ready" | "notready" => Some(Self::NotReady),
            "method_not_implemented" | "methodnotimplemented" => Some(Self::MethodNotImplemented),
            "failed" | "error" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommonErrorCode {
    CredentialRejected,
    NotFound,
    PermissionDenied,
    ValidationFailed,
    DependencyUnavailable,
    Internal,
    MethodNotImplemented,
    NotReady,
    Unsupported,
    Degraded,
    Locked,
    VerificationRequired,
}

impl CommonErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CredentialRejected => "credential_rejected",
            Self::NotFound => "not_found",
            Self::PermissionDenied => "permission_denied",
            Self::ValidationFailed => "validation_failed",
            Self::DependencyUnavailable => "dependency_unavailable",
            Self::Internal => "internal",
            Self::MethodNotImplemented => "method_not_implemented",
            Self::NotReady => "not_ready",
            Self::Unsupported => "unsupported",
            Self::Degraded => "degraded",
            Self::Locked => "locked",
            Self::VerificationRequired => "verification_required",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "credential_rejected" | "credentialrejected" => Some(Self::CredentialRejected),
            "not_found" | "notfound" => Some(Self::NotFound),
            "permission_denied" | "permissiondenied" => Some(Self::PermissionDenied),
            "validation_failed" | "validationfailed" => Some(Self::ValidationFailed),
            "dependency_unavailable" | "dependencyunavailable" => Some(Self::DependencyUnavailable),
            "internal" => Some(Self::Internal),
            "method_not_implemented" | "methodnotimplemented" => Some(Self::MethodNotImplemented),
            "not_ready" | "notready" => Some(Self::NotReady),
            "unsupported" => Some(Self::Unsupported),
            "degraded" => Some(Self::Degraded),
            "locked" => Some(Self::Locked),
            "verification_required" | "verificationrequired" => Some(Self::VerificationRequired),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorDomain {
    Authn,
    AppApi,
    ControlPlane,
    Mcp,
    RuntimeLifecycle,
    Config,
    Vault,
    Platform,
}

impl ErrorDomain {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Authn => "authn",
            Self::AppApi => "app_api",
            Self::ControlPlane => "control_plane",
            Self::Mcp => "mcp",
            Self::RuntimeLifecycle => "runtime_lifecycle",
            Self::Config => "config",
            Self::Vault => "vault",
            Self::Platform => "platform",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "authn" => Some(Self::Authn),
            "app_api" | "appapi" => Some(Self::AppApi),
            "control_plane" | "controlplane" => Some(Self::ControlPlane),
            "mcp" => Some(Self::Mcp),
            "runtime_lifecycle" | "runtimelifecycle" => Some(Self::RuntimeLifecycle),
            "config" => Some(Self::Config),
            "vault" => Some(Self::Vault),
            "platform" => Some(Self::Platform),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedError {
    pub status: ContractStatus,
    pub domain: ErrorDomain,
    pub common_code: CommonErrorCode,
    pub module_code: Option<String>,
    pub message: String,
    pub retriable: bool,
    pub recovery_hint: Option<String>,
}

impl SharedError {
    pub fn new(
        status: ContractStatus,
        domain: ErrorDomain,
        common_code: CommonErrorCode,
        message: impl Into<String>,
    ) -> Self {
        Self {
            status,
            domain,
            common_code,
            module_code: None,
            message: message.into(),
            retriable: false,
            recovery_hint: None,
        }
    }

    pub fn with_module_code(mut self, module_code: impl Into<String>) -> Self {
        self.module_code = Some(module_code.into());
        self
    }

    pub fn with_retriable(mut self, retriable: bool) -> Self {
        self.retriable = retriable;
        self
    }

    pub fn with_recovery_hint(mut self, recovery_hint: impl Into<String>) -> Self {
        self.recovery_hint = Some(recovery_hint.into());
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeBootstrapStatus {
    Ready,
    NeedsRelocate,
    NeedsPermissionFix,
    NeedsMigration,
    Failed,
}

impl RuntimeBootstrapStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::NeedsRelocate => "needs_relocate",
            Self::NeedsPermissionFix => "needs_permission_fix",
            Self::NeedsMigration => "needs_migration",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeRecoveryAction {
    Retry,
    ChooseRuntimeRoot,
    FixPermissions,
    MigrateConfig,
}

impl RuntimeRecoveryAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Retry => "retry",
            Self::ChooseRuntimeRoot => "choose_runtime_root",
            Self::FixPermissions => "fix_permissions",
            Self::MigrateConfig => "migrate_config",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StartupUnlockCarrierKind {
    HiddenPrompt,
    ParentStdin,
    TrustedLocalVerification,
}

impl StartupUnlockCarrierKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HiddenPrompt => "hidden_prompt",
            Self::ParentStdin => "parent_stdin",
            Self::TrustedLocalVerification => "trusted_local_verification",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetKind {
    Ssh,
    Adb,
    Serial,
    Docker,
    Other(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SshAuthKind {
    None,
    Password,
    PrivateKey,
}

impl SshAuthKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Password => "password",
            Self::PrivateKey => "private-key",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "password" => Some(Self::Password),
            "private-key" | "private_key" | "privatekey" => Some(Self::PrivateKey),
            _ => None,
        }
    }

    pub fn is_secret_backed(self) -> bool {
        matches!(self, Self::Password | Self::PrivateKey)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SshPrivateKeySource {
    LocalPath,
    VaultRef,
}

impl SshPrivateKeySource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LocalPath => "local-path",
            Self::VaultRef => "vault-ref",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "local-path" | "local_path" | "localpath" => Some(Self::LocalPath),
            "vault-ref" | "vault_ref" | "vaultref" => Some(Self::VaultRef),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshAuthConfig {
    pub kind: SshAuthKind,
    pub secure_access: bool,
    pub password: Option<String>,
    pub private_key_source: Option<SshPrivateKeySource>,
    pub key_locator: Option<String>,
}

impl Default for SshAuthConfig {
    fn default() -> Self {
        Self {
            kind: SshAuthKind::None,
            secure_access: false,
            password: None,
            private_key_source: None,
            key_locator: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SshAuthStorageClass {
    Plain,
    Sealed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SshAuthValidationContext {
    pub local_private_key_encrypted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SshAuthValidationCode {
    AuthCombinationDisallowed,
    SealedSecretBackedRequiresSecureAccess,
    PlainVaultKeyDisallowed,
    LocalKeyPassphraseRequiresVaultImport,
}

impl SshAuthValidationCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AuthCombinationDisallowed => "auth-combination-disallowed",
            Self::SealedSecretBackedRequiresSecureAccess => {
                "sealed-secret-backed-requires-secure-access"
            }
            Self::PlainVaultKeyDisallowed => "plain-vault-key-disallowed",
            Self::LocalKeyPassphraseRequiresVaultImport => {
                "local-key-passphrase-requires-vault-import"
            }
        }
    }
}

pub const SSH_ERROR_SUBCODE_AUTH_COMBINATION_DISALLOWED: &str = "auth-combination-disallowed";
pub const SSH_ERROR_SUBCODE_SEALED_SECRET_BACKED_REQUIRES_SECURE_ACCESS: &str =
    "sealed-secret-backed-requires-secure-access";
pub const SSH_ERROR_SUBCODE_PLAIN_VAULT_KEY_DISALLOWED: &str = "plain-vault-key-disallowed";
pub const SSH_ERROR_SUBCODE_LOCAL_KEY_PASSPHRASE_REQUIRES_VAULT_IMPORT: &str =
    "local-key-passphrase-requires-vault-import";
pub const SSH_ERROR_SUBCODE_PASSWORD_DELIVERY_UNAVAILABLE: &str =
    "password-delivery-unavailable";
pub const SSH_ERROR_SUBCODE_PASSWORD_DELIVERY_REJECTED: &str = "password-delivery-rejected";
pub const SSH_ERROR_SUBCODE_LOCAL_BROKERED_IDENTITY_UNAVAILABLE: &str =
    "local-brokered-identity-unavailable";

pub fn extract_ssh_error_subcode(raw: &str) -> Option<&'static str> {
    let normalized = raw.trim().to_ascii_lowercase();
    [
        SSH_ERROR_SUBCODE_AUTH_COMBINATION_DISALLOWED,
        SSH_ERROR_SUBCODE_SEALED_SECRET_BACKED_REQUIRES_SECURE_ACCESS,
        SSH_ERROR_SUBCODE_PLAIN_VAULT_KEY_DISALLOWED,
        SSH_ERROR_SUBCODE_LOCAL_KEY_PASSPHRASE_REQUIRES_VAULT_IMPORT,
        SSH_ERROR_SUBCODE_PASSWORD_DELIVERY_UNAVAILABLE,
        SSH_ERROR_SUBCODE_PASSWORD_DELIVERY_REJECTED,
        SSH_ERROR_SUBCODE_LOCAL_BROKERED_IDENTITY_UNAVAILABLE,
    ]
    .into_iter()
    .find(|code| normalized.contains(code))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshAuthValidationError {
    pub code: SshAuthValidationCode,
    pub message: String,
}

impl SshAuthValidationError {
    fn new(code: SshAuthValidationCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

pub fn validate_ssh_auth_config(
    storage_class: SshAuthStorageClass,
    auth: &SshAuthConfig,
    context: SshAuthValidationContext,
) -> Result<(), SshAuthValidationError> {
    match auth.kind {
        SshAuthKind::None => {
            if auth.secure_access
                || auth
                    .password
                    .as_ref()
                    .is_some_and(|value| !value.trim().is_empty())
                || auth.private_key_source.is_some()
                || auth
                    .key_locator
                    .as_ref()
                    .is_some_and(|value| !value.trim().is_empty())
            {
                return Err(SshAuthValidationError::new(
                    SshAuthValidationCode::AuthCombinationDisallowed,
                    "kind=none cannot carry password, private key source, key locator, or secure access",
                ));
            }
        }
        SshAuthKind::Password => {
            if auth.private_key_source.is_some()
                || auth
                    .key_locator
                    .as_ref()
                    .is_some_and(|value| !value.trim().is_empty())
            {
                return Err(SshAuthValidationError::new(
                    SshAuthValidationCode::AuthCombinationDisallowed,
                    "password auth cannot include private key source or key locator",
                ));
            }
        }
        SshAuthKind::PrivateKey => {
            let source = auth.private_key_source.ok_or_else(|| {
                SshAuthValidationError::new(
                    SshAuthValidationCode::AuthCombinationDisallowed,
                    "private-key auth requires private_key_source",
                )
            })?;
            if auth
                .password
                .as_ref()
                .is_some_and(|value| !value.trim().is_empty())
            {
                return Err(SshAuthValidationError::new(
                    SshAuthValidationCode::AuthCombinationDisallowed,
                    "private-key auth cannot include password",
                ));
            }
            let key_locator = auth
                .key_locator
                .as_ref()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    SshAuthValidationError::new(
                        SshAuthValidationCode::AuthCombinationDisallowed,
                        "private-key auth requires key_locator",
                    )
                })?;
            match source {
                SshPrivateKeySource::VaultRef => {
                    if storage_class == SshAuthStorageClass::Plain {
                        return Err(SshAuthValidationError::new(
                            SshAuthValidationCode::PlainVaultKeyDisallowed,
                            "plain target cannot use vault private key reference",
                        ));
                    }
                    if !key_locator.starts_with("vault://") {
                        return Err(SshAuthValidationError::new(
                            SshAuthValidationCode::AuthCombinationDisallowed,
                            "vault private key source requires canonical vault:// locator",
                        ));
                    }
                }
                SshPrivateKeySource::LocalPath => {
                    if context.local_private_key_encrypted {
                        return Err(SshAuthValidationError::new(
                            SshAuthValidationCode::LocalKeyPassphraseRequiresVaultImport,
                            "local passphrase-protected private key must be imported into vault",
                        ));
                    }
                }
            }
        }
    }

    if storage_class == SshAuthStorageClass::Sealed
        && auth.kind.is_secret_backed()
        && !auth.secure_access
    {
        return Err(SshAuthValidationError::new(
            SshAuthValidationCode::SealedSecretBackedRequiresSecureAccess,
            "sealed secret-backed auth requires secure_access=true",
        ));
    }

    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SshDeliveryPlan {
    None,
    PasswordDirectAskpass,
    PasswordManagedAskpass,
    DirectIdentityFile {
        identity_path: String,
    },
    LocalBrokeredIdentity {
        identity_path: String,
    },
    VaultBrokeredIdentity {
        credential_ref: String,
    },
}

impl SshDeliveryPlan {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::PasswordDirectAskpass => "password-direct-askpass",
            Self::PasswordManagedAskpass => "password-managed-askpass",
            Self::DirectIdentityFile { .. } => "direct-identity-file",
            Self::LocalBrokeredIdentity { .. } => "local-brokered-identity",
            Self::VaultBrokeredIdentity { .. } => "vault-brokered-identity",
        }
    }
}

pub fn derive_ssh_delivery_plan(
    storage_class: SshAuthStorageClass,
    auth: &SshAuthConfig,
    context: SshAuthValidationContext,
) -> Result<SshDeliveryPlan, SshAuthValidationError> {
    validate_ssh_auth_config(storage_class, auth, context)?;
    match auth.kind {
        SshAuthKind::None => Ok(SshDeliveryPlan::None),
        SshAuthKind::Password => {
            if auth.secure_access {
                Ok(SshDeliveryPlan::PasswordManagedAskpass)
            } else {
                Ok(SshDeliveryPlan::PasswordDirectAskpass)
            }
        }
        SshAuthKind::PrivateKey => {
            let key_locator = auth
                .key_locator
                .as_ref()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    SshAuthValidationError::new(
                        SshAuthValidationCode::AuthCombinationDisallowed,
                        "private-key auth requires key_locator",
                    )
                })?
                .to_string();
            match auth.private_key_source {
                Some(SshPrivateKeySource::VaultRef) => {
                    Ok(SshDeliveryPlan::VaultBrokeredIdentity {
                        credential_ref: key_locator,
                    })
                }
                Some(SshPrivateKeySource::LocalPath) => {
                    if auth.secure_access {
                        Ok(SshDeliveryPlan::LocalBrokeredIdentity {
                            identity_path: key_locator,
                        })
                    } else {
                        Ok(SshDeliveryPlan::DirectIdentityFile {
                            identity_path: key_locator,
                        })
                    }
                }
                None => Err(SshAuthValidationError::new(
                    SshAuthValidationCode::AuthCombinationDisallowed,
                    "private-key auth requires private_key_source",
                )),
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessScope {
    pub scope_id: String,
    pub workspace_id: String,
    pub principal_id: String,
    pub agent_id: String,
    pub run_id: String,
    pub thread_id: Option<String>,
    pub client_session_id: String,
    pub origin: String,
    pub created_at: SystemTime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialRef {
    pub id: String,
    pub provider: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyProfile {
    pub require_approval_for_write: bool,
    pub require_approval_for_delete: bool,
    pub require_approval_for_privileged: bool,
    pub require_approval_for_sensitive_read: bool,
}

impl Default for PolicyProfile {
    fn default() -> Self {
        Self {
            require_approval_for_write: true,
            require_approval_for_delete: true,
            require_approval_for_privileged: true,
            require_approval_for_sensitive_read: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionConfig {
    Ssh {
        host: String,
        port: u16,
        username: String,
    },
    Adb {
        serial: Option<String>,
        transport: Option<String>,
    },
    Serial {
        device: String,
        baud_rate: u32,
    },
    Docker {
        container: String,
        context: Option<String>,
    },
    Custom {
        description: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetProfile {
    pub id: String,
    pub name: String,
    pub kind: TargetKind,
    pub connection: ConnectionConfig,
    pub credential_ref: Option<CredentialRef>,
    pub ssh_auth: Option<SshAuthConfig>,
    pub default_policy: PolicyProfile,
    pub notes: Option<String>,
    pub metadata: MetadataMap,
    pub toolchains: MetadataMap,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalTargetFamily {
    Terminal,
    Other(String),
}

impl TerminalTargetFamily {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Terminal => "terminal",
            Self::Other(value) => value.as_str(),
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        let normalized = raw.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            return None;
        }
        match normalized.as_str() {
            "terminal" => Some(Self::Terminal),
            other => Some(Self::Other(other.to_string())),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalConcurrencyPolicy {
    Multiplexed,
    Exclusive,
}

impl TerminalConcurrencyPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Multiplexed => "multiplexed",
            Self::Exclusive => "exclusive",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "multiplexed" => Some(Self::Multiplexed),
            "exclusive" => Some(Self::Exclusive),
            _ => None,
        }
    }
}

pub fn default_terminal_target_family_for_kind(kind: &TargetKind) -> Option<TerminalTargetFamily> {
    match kind {
        TargetKind::Ssh | TargetKind::Adb | TargetKind::Serial => {
            Some(TerminalTargetFamily::Terminal)
        }
        _ => None,
    }
}

pub fn terminal_target_family_for(target: &TargetProfile) -> Option<TerminalTargetFamily> {
    if let Some(family) = target
        .metadata
        .get(TARGET_TERMINAL_FAMILY_METADATA_KEY)
        .and_then(|raw| TerminalTargetFamily::parse(raw))
    {
        return Some(family);
    }
    default_terminal_target_family_for_kind(&target.kind)
}

pub fn default_terminal_concurrency_policy_for_kind(
    kind: &TargetKind,
) -> Option<TerminalConcurrencyPolicy> {
    match kind {
        TargetKind::Ssh | TargetKind::Adb => Some(TerminalConcurrencyPolicy::Multiplexed),
        TargetKind::Serial => Some(TerminalConcurrencyPolicy::Exclusive),
        _ => None,
    }
}

pub fn terminal_concurrency_policy_for(
    target: &TargetProfile,
) -> Option<TerminalConcurrencyPolicy> {
    if let Some(policy) = target
        .metadata
        .get(TARGET_TERMINAL_CONCURRENCY_METADATA_KEY)
        .and_then(|raw| TerminalConcurrencyPolicy::parse(raw))
    {
        return Some(policy);
    }
    default_terminal_concurrency_policy_for_kind(&target.kind)
}

pub fn is_terminal_target(target: &TargetProfile) -> bool {
    matches!(
        terminal_target_family_for(target),
        Some(TerminalTargetFamily::Terminal)
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilitySummary {
    pub id: String,
    pub label: String,
    pub supports_streaming: bool,
    pub supports_file_transfer: bool,
    pub requires_approval: bool,
    pub typed_entrypoints: Vec<String>,
    pub raw_fallback: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionReusePolicy {
    AlwaysNew,
    ReuseIfAlive,
    ResumeOrCreate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogicalSessionStatus {
    Opening,
    Active,
    Idle,
    Degraded,
    Closed,
    Archived,
}

impl LogicalSessionStatus {
    pub fn is_alive(&self) -> bool {
        matches!(
            self,
            LogicalSessionStatus::Opening
                | LogicalSessionStatus::Active
                | LogicalSessionStatus::Idle
                | LogicalSessionStatus::Degraded
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogicalSessionRecord {
    pub logical_session_id: String,
    pub session_key: String,
    pub scope_id: String,
    pub target_id: String,
    pub reuse_policy: SessionReusePolicy,
    pub status: LogicalSessionStatus,
    pub created_at: SystemTime,
    pub last_activity_at: SystemTime,
    pub closed_at: Option<SystemTime>,
    pub close_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransportSessionStatus {
    Connecting,
    Connected,
    Degraded,
    Failed,
    Closed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportSessionRecord {
    pub transport_session_id: String,
    pub logical_session_id: String,
    pub target_id: String,
    pub connector_kind: TargetKind,
    pub status: TransportSessionStatus,
    pub created_at: SystemTime,
    pub last_activity_at: SystemTime,
    pub close_reason: Option<String>,
    pub resolved_executable_path: Option<String>,
    pub resolved_executable_source: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChannelKind {
    InteractiveShell,
    OneShotExec,
    LogStream,
    FileTransfer,
    Inspect,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChannelStatus {
    Pending,
    Opening,
    Active,
    Idle,
    Draining,
    Failed,
    Closed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelRecord {
    pub channel_id: String,
    pub logical_session_id: String,
    pub transport_session_id: String,
    pub target_id: String,
    pub channel_kind: ChannelKind,
    pub status: ChannelStatus,
    pub display_name: Option<String>,
    pub working_directory: Option<String>,
    pub foreground_command: Option<String>,
    pub created_at: SystemTime,
    pub last_activity_at: SystemTime,
    pub close_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvironmentFingerprint {
    pub os_type: String,
    pub arch: String,
    pub kernel_version: Option<String>,
    pub shell: Option<String>,
    pub detected_tools: Vec<String>,
    pub capabilities: Vec<CapabilitySummary>,
    pub captured_at: SystemTime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionState {
    Connecting,
    Connected,
    Degraded,
    Failed,
    Closed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionRecord {
    pub id: String,
    pub target_id: String,
    pub state: SessionState,
    pub started_at: SystemTime,
    pub last_activity_at: SystemTime,
    pub close_reason: Option<String>,
    pub fingerprint: Option<EnvironmentFingerprint>,
}

impl SessionRecord {
    pub fn new(id: impl Into<String>, target_id: impl Into<String>, now: SystemTime) -> Self {
        Self {
            id: id.into(),
            target_id: target_id.into(),
            state: SessionState::Connecting,
            started_at: now,
            last_activity_at: now,
            close_reason: None,
            fingerprint: None,
        }
    }

    pub fn transition(&mut self, state: SessionState, now: SystemTime, reason: Option<String>) {
        self.state = state;
        self.last_activity_at = now;
        self.close_reason = reason;
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtifactKind {
    RawCommandOutput,
    DerivedView,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactFilter {
    pub keyword: Option<String>,
    pub regex: Option<String>,
    pub line_start: Option<usize>,
    pub line_end: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub id: String,
    pub content_digest: String,
    pub logical_session_id: String,
    pub transport_session_id: Option<String>,
    pub channel_id: Option<String>,
    pub session_id: String,
    pub parent_id: Option<String>,
    pub kind: ArtifactKind,
    pub source_command: Option<String>,
    pub filter: Option<ArtifactFilter>,
    pub created_at: SystemTime,
    pub last_accessed_at: SystemTime,
    pub byte_count: u64,
    pub line_count: usize,
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Denied,
    Expired,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalRequestRecord {
    pub id: String,
    pub scope_id: Option<String>,
    pub logical_session_id: String,
    pub channel_id: Option<String>,
    pub session_id: String,
    pub reason: String,
    pub command_preview: String,
    pub requested_at: SystemTime,
    pub status: ApprovalStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditEvent {
    pub id: String,
    pub scope_id: Option<String>,
    pub logical_session_id: Option<String>,
    pub channel_id: Option<String>,
    pub session_id: Option<String>,
    pub event_type: String,
    pub detail: String,
    pub created_at: SystemTime,
}

pub fn build_logical_session_key(
    scope: &AccessScope,
    target_id: &str,
    client_session_id: &str,
) -> String {
    let input = format!(
        "{}:{}:{}:{}:{}:{}:{}",
        scope.workspace_id,
        scope.principal_id,
        scope.agent_id,
        scope.run_id,
        scope.thread_id.clone().unwrap_or_default(),
        target_id,
        client_session_id
    );
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    input.hash(&mut hasher);
    format!("lsk-{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_standardized_ssh_profile() {
        let profile = TargetProfile {
            id: "target-01".into(),
            name: "prod-linux".into(),
            kind: TargetKind::Ssh,
            connection: ConnectionConfig::Ssh {
                host: "10.0.0.8".into(),
                port: 22,
                username: "ops".into(),
            },
            credential_ref: Some(CredentialRef {
                id: "vault:ssh-key:ops-prod".into(),
                provider: "system-keychain".into(),
            }),
            ssh_auth: Some(SshAuthConfig {
                kind: SshAuthKind::PrivateKey,
                secure_access: true,
                password: None,
                private_key_source: Some(SshPrivateKeySource::VaultRef),
                key_locator: Some("vault://bridgingio/ssh-private-key/ops-prod".into()),
            }),
            default_policy: PolicyProfile::default(),
            notes: Some("production bastion".into()),
            metadata: MetadataMap::new(),
            toolchains: MetadataMap::new(),
        };

        assert!(matches!(profile.kind, TargetKind::Ssh));
        assert!(profile.credential_ref.is_some());
    }

    #[test]
    fn session_transition_updates_state_and_activity() {
        let now = SystemTime::now();
        let later = now.checked_add(std::time::Duration::from_secs(1)).unwrap();

        let mut session = SessionRecord::new("s1", "t1", now);
        session.transition(SessionState::Connected, later, None);

        assert!(matches!(session.state, SessionState::Connected));
        assert_eq!(session.last_activity_at, later);
        assert_eq!(session.close_reason, None);
    }

    #[test]
    fn logical_session_key_changes_by_agent() {
        let now = SystemTime::now();
        let scope_a = AccessScope {
            scope_id: "scope-a".into(),
            workspace_id: "ws-1".into(),
            principal_id: "user-1".into(),
            agent_id: "agent-a".into(),
            run_id: "run-1".into(),
            thread_id: Some("thread-1".into()),
            client_session_id: "client-1".into(),
            origin: "mcp".into(),
            created_at: now,
        };
        let mut scope_b = scope_a.clone();
        scope_b.agent_id = "agent-b".into();

        let key_a = build_logical_session_key(&scope_a, "target-1", "client-1");
        let key_b = build_logical_session_key(&scope_b, "target-1", "client-1");

        assert_ne!(key_a, key_b);
    }

    #[test]
    fn terminal_family_and_concurrency_defaults_follow_target_kind() {
        let make_target =
            |id: &str, kind: TargetKind, connection: ConnectionConfig| TargetProfile {
                id: id.into(),
                name: id.into(),
                kind,
                connection,
                credential_ref: None,
                ssh_auth: None,
                default_policy: PolicyProfile::default(),
                notes: None,
                metadata: MetadataMap::new(),
                toolchains: MetadataMap::new(),
            };

        let ssh = make_target(
            "t-ssh",
            TargetKind::Ssh,
            ConnectionConfig::Ssh {
                host: "127.0.0.1".into(),
                port: 22,
                username: "dev".into(),
            },
        );
        let adb = make_target(
            "t-adb",
            TargetKind::Adb,
            ConnectionConfig::Adb {
                serial: Some("emulator-5554".into()),
                transport: Some("serial".into()),
            },
        );
        let serial = make_target(
            "t-serial",
            TargetKind::Serial,
            ConnectionConfig::Serial {
                device: "/dev/tty.usbmodem01".into(),
                baud_rate: 115200,
            },
        );
        let docker = make_target(
            "t-docker",
            TargetKind::Docker,
            ConnectionConfig::Docker {
                container: "busybox".into(),
                context: None,
            },
        );

        assert_eq!(
            terminal_target_family_for(&ssh),
            Some(TerminalTargetFamily::Terminal)
        );
        assert_eq!(
            terminal_target_family_for(&adb),
            Some(TerminalTargetFamily::Terminal)
        );
        assert_eq!(
            terminal_target_family_for(&serial),
            Some(TerminalTargetFamily::Terminal)
        );
        assert_eq!(terminal_target_family_for(&docker), None);

        assert_eq!(
            terminal_concurrency_policy_for(&ssh),
            Some(TerminalConcurrencyPolicy::Multiplexed)
        );
        assert_eq!(
            terminal_concurrency_policy_for(&adb),
            Some(TerminalConcurrencyPolicy::Multiplexed)
        );
        assert_eq!(
            terminal_concurrency_policy_for(&serial),
            Some(TerminalConcurrencyPolicy::Exclusive)
        );
        assert_eq!(terminal_concurrency_policy_for(&docker), None);
    }

    #[test]
    fn metadata_can_override_terminal_family_and_concurrency_policy() {
        let mut target = TargetProfile {
            id: "t-localshell".into(),
            name: "future-localshell".into(),
            kind: TargetKind::Other("localshell".into()),
            connection: ConnectionConfig::Custom {
                description: "future localshell transport".into(),
            },
            credential_ref: None,
            ssh_auth: None,
            default_policy: PolicyProfile::default(),
            notes: None,
            metadata: MetadataMap::new(),
            toolchains: MetadataMap::new(),
        };
        target.metadata.insert(
            TARGET_TERMINAL_FAMILY_METADATA_KEY.to_string(),
            "terminal".into(),
        );
        target.metadata.insert(
            TARGET_TERMINAL_CONCURRENCY_METADATA_KEY.to_string(),
            "exclusive".into(),
        );

        assert_eq!(
            terminal_target_family_for(&target),
            Some(TerminalTargetFamily::Terminal)
        );
        assert_eq!(
            terminal_concurrency_policy_for(&target),
            Some(TerminalConcurrencyPolicy::Exclusive)
        );
        assert!(is_terminal_target(&target));
    }

    #[test]
    fn validator_rejects_plain_vault_key_and_sealed_insecure_secret_backed_auth() {
        let plain_vault = SshAuthConfig {
            kind: SshAuthKind::PrivateKey,
            secure_access: true,
            password: None,
            private_key_source: Some(SshPrivateKeySource::VaultRef),
            key_locator: Some("vault://bridgingio/ssh-private-key/lab".into()),
        };
        let err = validate_ssh_auth_config(
            SshAuthStorageClass::Plain,
            &plain_vault,
            SshAuthValidationContext::default(),
        )
        .expect_err("plain target should reject vault key");
        assert_eq!(err.code, SshAuthValidationCode::PlainVaultKeyDisallowed);

        let sealed_password = SshAuthConfig {
            kind: SshAuthKind::Password,
            secure_access: false,
            password: None,
            private_key_source: None,
            key_locator: None,
        };
        let err = validate_ssh_auth_config(
            SshAuthStorageClass::Sealed,
            &sealed_password,
            SshAuthValidationContext::default(),
        )
        .expect_err("sealed password must require secure access");
        assert_eq!(
            err.code,
            SshAuthValidationCode::SealedSecretBackedRequiresSecureAccess
        );
    }

    #[test]
    fn validator_rejects_local_encrypted_key_when_context_requires_vault_import() {
        let local_key = SshAuthConfig {
            kind: SshAuthKind::PrivateKey,
            secure_access: true,
            password: None,
            private_key_source: Some(SshPrivateKeySource::LocalPath),
            key_locator: Some("/tmp/id_ed25519".into()),
        };
        let err = validate_ssh_auth_config(
            SshAuthStorageClass::Sealed,
            &local_key,
            SshAuthValidationContext {
                local_private_key_encrypted: true,
            },
        )
        .expect_err("encrypted local key must be blocked");
        assert_eq!(
            err.code,
            SshAuthValidationCode::LocalKeyPassphraseRequiresVaultImport
        );
    }

    #[test]
    fn delivery_plan_derives_password_and_private_key_paths() {
        let password_managed = SshAuthConfig {
            kind: SshAuthKind::Password,
            secure_access: true,
            password: Some("demo".into()),
            private_key_source: None,
            key_locator: None,
        };
        let plan = derive_ssh_delivery_plan(
            SshAuthStorageClass::Sealed,
            &password_managed,
            SshAuthValidationContext::default(),
        )
        .expect("managed password plan");
        assert_eq!(plan, SshDeliveryPlan::PasswordManagedAskpass);

        let direct_identity = SshAuthConfig {
            kind: SshAuthKind::PrivateKey,
            secure_access: false,
            password: None,
            private_key_source: Some(SshPrivateKeySource::LocalPath),
            key_locator: Some("/tmp/id_ed25519".into()),
        };
        let plan = derive_ssh_delivery_plan(
            SshAuthStorageClass::Plain,
            &direct_identity,
            SshAuthValidationContext::default(),
        )
        .expect("direct identity plan");
        assert_eq!(
            plan,
            SshDeliveryPlan::DirectIdentityFile {
                identity_path: "/tmp/id_ed25519".into()
            }
        );
    }

    #[test]
    fn extracts_known_ssh_error_subcode_from_reason_text() {
        let code = extract_ssh_error_subcode(
            "validation failed: local-key-passphrase-requires-vault-import: import the key first",
        );
        assert_eq!(
            code,
            Some(SSH_ERROR_SUBCODE_LOCAL_KEY_PASSPHRASE_REQUIRES_VAULT_IMPORT)
        );

        let code = extract_ssh_error_subcode("password-delivery-unavailable");
        assert_eq!(code, Some(SSH_ERROR_SUBCODE_PASSWORD_DELIVERY_UNAVAILABLE));
    }
}
