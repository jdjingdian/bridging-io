use std::collections::HashMap;
use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bridgingio_app_api::{ApiRequest, ApiRequestContext, ApiResponse, AppCommand};
use bridgingio_connectors::{
    BuiltInBinarySpec, BuiltInDistributionKind, ExecutableResolver, ToolchainResolver,
};
use bridgingio_domain::{PolicyProfile, SessionReusePolicy, TargetKind};
use bridgingio_engine::{
    ConfigError, CoreSettings, StandaloneConnectionSection, StandaloneTargetProfile,
    StandaloneTerminalSection, TerminalProviderSection,
};
use bridgingio_mcp::{
    control_plane_socket_path, CoreHostMode, CoreRuntimeError, ModelPlaneHttpServer,
    StandaloneCoreRuntime,
};
use bridgingio_platform::{
    detect_host_platform_adapter, CapabilityStatus, HostPlatform, RuntimeLogCategory,
    RuntimeLogLevel,
};
use bridgingio_providers::TerminalProvider;
use bridgingio_secrets::{
    local_admin_create_token_target, local_admin_payload_digest_for_create_agent_token,
    local_admin_payload_digest_for_unlock_vault, local_admin_unlock_vault_target,
    normalize_credential_ref, CreateAgentTokenRequest, LocalAdminActionKind, SecretBytes,
    SecretVaultRouter, SshAgentBrokerPrepareRequest, SshHostKeyPolicy, SshKeyPassphraseHandling,
    TokenScopeInput, UnlockVaultRequest, VaultError, VaultReadinessState, VaultUnlockPolicy,
    VaultUnlockTriggerPolicy,
};
use serde_json::json;

#[cfg(unix)]
use bridgingio_mcp::ControlPlaneIpcServer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LaunchMode {
    SelfTest,
    UiManagedEphemeral,
    StandaloneRun,
    StandaloneDetachedLauncher,
    StandaloneDetachedChild,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ManagementCommand {
    VaultInit,
    VaultImport {
        reference: String,
        label: Option<String>,
        input: SecretInputRoute,
    },
    VaultUnlock {
        method: Option<String>,
        input: SecretInputRoute,
    },
    AuthTokenCreate {
        label: String,
        expires_in_seconds: Option<u64>,
    },
    AuthTokenRevoke {
        token_id: String,
        reason: Option<String>,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct SecretInputRoute {
    fd: Option<i32>,
    from_stdin: bool,
    file: Option<PathBuf>,
    from_tty_prompt: bool,
}

impl SecretInputRoute {
    fn explicit_count(&self) -> usize {
        let mut count = 0usize;
        if self.fd.is_some() {
            count += 1;
        }
        if self.from_stdin {
            count += 1;
        }
        if self.file.is_some() {
            count += 1;
        }
        if self.from_tty_prompt {
            count += 1;
        }
        count
    }

    fn has_explicit_source(&self) -> bool {
        self.explicit_count() > 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SecretSourceKind {
    Fd,
    Stdin,
    File,
    TtyPrompt,
}

impl SecretSourceKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Fd => "fd",
            Self::Stdin => "stdin",
            Self::File => "file",
            Self::TtyPrompt => "tty-prompt",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SecretInputCapture {
    secret: String,
    source_kind: SecretSourceKind,
    source_summary: String,
    source_locator_digest: String,
    byte_length: usize,
}

#[derive(Debug)]
struct CliArgs {
    config_path: Option<PathBuf>,
    runtime_root: Option<PathBuf>,
    control_plane_socket_override: Option<PathBuf>,
    mode: LaunchMode,
    management_command: Option<ManagementCommand>,
}

fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            print_usage();
            std::process::exit(2);
        }
    };

    if let Err(err) = run(args) {
        eprintln!("failed to start bridgingio-core: {err}");
        std::process::exit(1);
    }
}

fn run(args: CliArgs) -> Result<(), String> {
    if args.mode == LaunchMode::SelfTest {
        return run_self_test();
    }
    if let Some(command) = args.management_command.clone() {
        let config_path = resolve_config_path(&args)?;
        return run_management_command(&config_path, command);
    }
    let config_path = resolve_config_path(&args)?;
    match args.mode {
        LaunchMode::StandaloneDetachedLauncher => {
            spawn_detached_child(&config_path, args.control_plane_socket_override.as_deref())
        }
        LaunchMode::UiManagedEphemeral => run_core(
            config_path,
            args.mode,
            CoreHostMode::UiManagedEphemeral,
            args.control_plane_socket_override.as_deref(),
        ),
        LaunchMode::StandaloneRun => run_core(
            config_path,
            args.mode,
            CoreHostMode::StandaloneRun,
            args.control_plane_socket_override.as_deref(),
        ),
        LaunchMode::StandaloneDetachedChild => run_core(
            config_path,
            args.mode,
            CoreHostMode::StandaloneDetached,
            args.control_plane_socket_override.as_deref(),
        ),
        LaunchMode::SelfTest => unreachable!("self-test handled before config resolution"),
    }
}

fn run_management_command(config_path: &Path, command: ManagementCommand) -> Result<(), String> {
    let (settings, mut router) = load_management_router(config_path)?;
    let operator_principal = management_operator_principal();
    match command {
        ManagementCommand::VaultInit => {
            let lock_state = router.vault_lock_state().as_str().to_string();
            let secret_count = router.list_secret_summaries().len();
            println!(
                "vault initialized (data_dir={}, backend={}, lock_state={}, secret_count={secret_count})",
                settings.core.data_dir,
                router.active_backend(),
                lock_state,
            );
            emit_management_audit("vault.init", &operator_principal, None, None, "ok");
            Ok(())
        }
        ManagementCommand::VaultImport {
            reference,
            label,
            input,
        } => {
            validate_secret_input_route(&input)?;
            let capture = read_secret_input(&input, "Enter secret to import: ")?;
            let final_label = label.unwrap_or_else(|| reference.clone());
            let imported = router
                .put_with_actor(
                    &reference,
                    SecretBytes::from_utf8(&capture.secret),
                    final_label,
                    &operator_principal,
                    Some("standalone-cli import".to_string()),
                )
                .map_err(|err| format!("vault import failed: {err:?}"))?;
            println!(
                "vault secret imported: reference={} status={} version={}",
                imported.reference,
                imported.status.as_str(),
                imported.version
            );
            emit_management_audit(
                "vault.import",
                &operator_principal,
                None,
                Some(&capture),
                "ok",
            );
            Ok(())
        }
        ManagementCommand::VaultUnlock { method, input } => {
            validate_secret_input_route(&input)?;
            let unlock_method = normalize_cli_vault_method(
                &method.unwrap_or_else(|| router.unlock_policy().preferred_method.clone()),
            );
            let passphrase_capture = if unlock_method == "passphrase" {
                Some(read_secret_input(
                    &input,
                    "Enter vault passphrase for unlock: ",
                )?)
            } else {
                if input.has_explicit_source() {
                    return Err(format!(
                        "vault unlock method `{unlock_method}` does not consume secret input sources"
                    ));
                }
                None
            };
            let payload_digest = local_admin_payload_digest_for_unlock_vault(&unlock_method);
            let intent = router
                .create_local_admin_intent_with_digest(
                    LocalAdminActionKind::UnlockVault,
                    local_admin_unlock_vault_target(),
                    &payload_digest,
                    &operator_principal,
                    Duration::from_secs(300),
                )
                .map_err(|err| format!("create unlock intent failed: {err:?}"))?;
            let attestation = router
                .complete_local_admin_attestation(
                    &intent.intent_id,
                    &operator_principal,
                    "standalone-cli",
                    Duration::from_secs(120),
                )
                .map_err(|err| format!("complete unlock attestation failed: {err:?}"))?;
            router
                .unlock_vault_with_attestation(UnlockVaultRequest {
                    method: unlock_method.clone(),
                    passphrase: passphrase_capture
                        .as_ref()
                        .map(|capture| trim_single_trailing_newline(capture.secret.clone())),
                    requested_by: operator_principal.clone(),
                    attestation_id: attestation.attestation_id.clone(),
                })
                .map_err(|err| format!("unlock vault failed: {err:?}"))?;
            let state = router.vault_lock_state().as_str().to_string();
            println!("vault unlock completed: method={unlock_method} lock_state={state}");
            emit_management_audit(
                "vault.unlock",
                &operator_principal,
                Some(&intent.intent_id),
                passphrase_capture.as_ref(),
                "ok",
            );
            Ok(())
        }
        ManagementCommand::AuthTokenCreate {
            label,
            expires_in_seconds,
        } => {
            let mut create_request = CreateAgentTokenRequest {
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
            let payload_digest = local_admin_payload_digest_for_create_agent_token(&create_request);
            let intent = router
                .create_local_admin_intent_with_digest(
                    LocalAdminActionKind::CreateAgentToken,
                    local_admin_create_token_target(),
                    &payload_digest,
                    &operator_principal,
                    Duration::from_secs(300),
                )
                .map_err(|err| format!("create token intent failed: {err:?}"))?;
            let attestation = router
                .complete_local_admin_attestation(
                    &intent.intent_id,
                    &operator_principal,
                    "standalone-cli",
                    Duration::from_secs(120),
                )
                .map_err(|err| format!("complete token attestation failed: {err:?}"))?;
            create_request.attestation_id = Some(attestation.attestation_id.clone());
            let created = router
                .create_agent_token(create_request)
                .map_err(|err| format!("create token failed: {err:?}"))?;
            println!("token created (one-time reveal): {}", created.plaintext_token);
            println!(
                "token summary: id={} status={} scope={} targets={}",
                created.summary.token_id,
                created.summary.status.as_str(),
                created.summary.scope_profile,
                created.summary.target_scope_summary
            );
            emit_management_audit(
                "auth.token.create",
                &operator_principal,
                Some(&intent.intent_id),
                None,
                "ok",
            );
            Ok(())
        }
        ManagementCommand::AuthTokenRevoke { token_id, reason } => {
            let summary = router
                .revoke_agent_token(&token_id, reason)
                .map_err(|err| format!("revoke token failed: {err:?}"))?;
            println!(
                "token revoked: id={} status={} reason={}",
                summary.token_id,
                summary.status.as_str(),
                summary.revoke_reason.as_deref().unwrap_or("none")
            );
            emit_management_audit("auth.token.revoke", &operator_principal, None, None, "ok");
            Ok(())
        }
    }
}

fn run_self_test() -> Result<(), String> {
    println!("running bridgingio-core self-test...");

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| format!("clock error: {err}"))?
        .as_nanos();
    let self_test_root = env::temp_dir().join(format!("bridgingio-self-test-{stamp}"));
    fs::create_dir_all(&self_test_root)
        .map_err(|err| format!("create self-test root failed: {err}"))?;

    let marker_one_shot = "BRIDGINGIO_SELFTEST_ONE_SHOT_OK";
    let marker_interactive = "BRIDGINGIO_SELFTEST_INTERACTIVE_OK";
    let marker_env = "BRIDGINGIO_SELFTEST_ENV_OK";
    let marker_space = "BRIDGINGIO_SELFTEST_SPACE_OK";
    let marker_arg = "BRIDGINGIO SELFTEST ARG OK";
    let marker_runtime = "BRIDGINGIO_SELFTEST_RUNTIME_OK";
    let cwd_hint = "bridgingio selftest cwd";

    println!("self-test [1/8] validating terminal provider one-shot execution...");
    let mut provider = TerminalProvider::default();
    let one_shot_artifact = provider
        .exec_local(
            "self-test-session",
            Some("self-test-channel"),
            Some("self-test-transport"),
            &format!("echo {marker_one_shot}"),
            "self-test-artifact-one-shot",
            &PolicyProfile::default(),
        )
        .map_err(|err| format!("one-shot exec failed: {}", err.message))?;
    let one_shot_chunks = provider.artifacts.read_chunks(&one_shot_artifact.id, 0, 20);
    if !one_shot_chunks
        .iter()
        .any(|line| line.contains(marker_one_shot))
    {
        return Err(format!(
            "one-shot exec output missing marker {marker_one_shot}: {one_shot_chunks:?}"
        ));
    }
    println!("self-test [ok] terminal provider one-shot execution");

    println!("self-test [2/8] validating interactive shell open/write/read/interrupt/close...");
    let shell = provider
        .open_interactive_shell(
            "self-test-session",
            "self-test-channel-interactive",
            Some("self-test-transport-interactive"),
            "self-test",
        )
        .map_err(|err| format!("interactive open failed: {}", err.message))?;
    let write = provider
        .write_interactive_shell(
            &shell.shell_id,
            &format!("echo {marker_interactive}"),
            "self-test-artifact-interactive",
            &PolicyProfile::default(),
        )
        .map_err(|err| format!("interactive write failed: {}", err.message))?;
    if !write.output.contains(marker_interactive) {
        return Err(format!(
            "interactive output missing marker {marker_interactive}: {}",
            write.output
        ));
    }
    println!("self-test [ok] interactive shell lifecycle");

    println!("self-test [3/8] validating interactive cwd/env semantics...");
    let interactive_cwd = self_test_root.join(cwd_hint);
    fs::create_dir_all(&interactive_cwd)
        .map_err(|err| format!("create self-test cwd dir failed: {err}"))?;

    #[cfg(windows)]
    {
        let change_dir_command = format!("cd /d {}", shell_quote_path(&interactive_cwd));
        provider
            .write_interactive_shell(
                &shell.shell_id,
                &change_dir_command,
                "self-test-artifact-cwd-change",
                &PolicyProfile::default(),
            )
            .map_err(|err| format!("interactive cd /d failed: {}", err.message))?;

        let cwd_output = provider
            .write_interactive_shell(
                &shell.shell_id,
                "cd",
                "self-test-artifact-cwd-read",
                &PolicyProfile::default(),
            )
            .map_err(|err| format!("interactive cwd read failed: {}", err.message))?;
        if !cwd_output.output.to_ascii_lowercase().contains(cwd_hint) {
            return Err(format!(
                "interactive cwd output missing expected folder hint ({cwd_hint}): {}",
                cwd_output.output
            ));
        }

        provider
            .write_interactive_shell(
                &shell.shell_id,
                &format!("set BRIDGINGIO_SELFTEST_ENV={marker_env}"),
                "self-test-artifact-env-set",
                &PolicyProfile::default(),
            )
            .map_err(|err| format!("interactive env set failed: {}", err.message))?;
        let env_output = provider
            .write_interactive_shell(
                &shell.shell_id,
                "echo %BRIDGINGIO_SELFTEST_ENV%",
                "self-test-artifact-env-read",
                &PolicyProfile::default(),
            )
            .map_err(|err| format!("interactive env read failed: {}", err.message))?;
        if !env_output.output.contains(marker_env) {
            return Err(format!(
                "interactive env output missing marker {marker_env}: {}",
                env_output.output
            ));
        }
    }

    #[cfg(not(windows))]
    {
        let change_dir_command = format!("cd {}", shell_quote_path(&interactive_cwd));
        provider
            .write_interactive_shell(
                &shell.shell_id,
                &change_dir_command,
                "self-test-artifact-cwd-change",
                &PolicyProfile::default(),
            )
            .map_err(|err| format!("interactive cd failed: {}", err.message))?;

        let cwd_output = provider
            .write_interactive_shell(
                &shell.shell_id,
                "pwd",
                "self-test-artifact-cwd-read",
                &PolicyProfile::default(),
            )
            .map_err(|err| format!("interactive cwd read failed: {}", err.message))?;
        if !cwd_output.output.contains(cwd_hint) {
            return Err(format!(
                "interactive cwd output missing expected folder hint ({cwd_hint}): {}",
                cwd_output.output
            ));
        }

        provider
            .write_interactive_shell(
                &shell.shell_id,
                &format!("export BRIDGINGIO_SELFTEST_ENV={marker_env}"),
                "self-test-artifact-env-set",
                &PolicyProfile::default(),
            )
            .map_err(|err| format!("interactive env set failed: {}", err.message))?;
        let env_output = provider
            .write_interactive_shell(
                &shell.shell_id,
                "printf \"%s\\n\" \"$BRIDGINGIO_SELFTEST_ENV\"",
                "self-test-artifact-env-read",
                &PolicyProfile::default(),
            )
            .map_err(|err| format!("interactive env read failed: {}", err.message))?;
        if !env_output.output.contains(marker_env) {
            return Err(format!(
                "interactive env output missing marker {marker_env}: {}",
                env_output.output
            ));
        }
    }
    println!("self-test [ok] interactive cwd/env semantics");

    println!("self-test [4/8] validating space-containing path/argument handling...");
    let spaced_file = interactive_cwd.join("file with space.txt");
    fs::write(&spaced_file, format!("{marker_space}\n"))
        .map_err(|err| format!("write self-test spaced file failed: {err}"))?;

    #[cfg(windows)]
    {
        let read_command = format!("type {}", shell_quote_path(&spaced_file));
        let read_output = provider
            .write_interactive_shell(
                &shell.shell_id,
                &read_command,
                "self-test-artifact-space-read",
                &PolicyProfile::default(),
            )
            .map_err(|err| format!("interactive spaced file read failed: {}", err.message))?;
        if !read_output.output.contains(marker_space) {
            return Err(format!(
                "interactive spaced-path output missing marker {marker_space}: {}",
                read_output.output
            ));
        }

        let arg_output = provider
            .write_interactive_shell(
                &shell.shell_id,
                &format!("echo \"{marker_arg}\""),
                "self-test-artifact-space-arg",
                &PolicyProfile::default(),
            )
            .map_err(|err| format!("interactive spaced-arg echo failed: {}", err.message))?;
        if !arg_output.output.contains(marker_arg) {
            return Err(format!(
                "interactive spaced-arg output missing marker {marker_arg}: {}",
                arg_output.output
            ));
        }
    }

    #[cfg(not(windows))]
    {
        let read_command = format!("cat {}", shell_quote_path(&spaced_file));
        let read_output = provider
            .write_interactive_shell(
                &shell.shell_id,
                &read_command,
                "self-test-artifact-space-read",
                &PolicyProfile::default(),
            )
            .map_err(|err| format!("interactive spaced file read failed: {}", err.message))?;
        if !read_output.output.contains(marker_space) {
            return Err(format!(
                "interactive spaced-path output missing marker {marker_space}: {}",
                read_output.output
            ));
        }

        let arg_output = provider
            .write_interactive_shell(
                &shell.shell_id,
                &format!("printf \"%s\\n\" \"{marker_arg}\""),
                "self-test-artifact-space-arg",
                &PolicyProfile::default(),
            )
            .map_err(|err| format!("interactive spaced-arg echo failed: {}", err.message))?;
        if !arg_output.output.contains(marker_arg) {
            return Err(format!(
                "interactive spaced-arg output missing marker {marker_arg}: {}",
                arg_output.output
            ));
        }
    }
    println!("self-test [ok] space-containing path/argument handling");

    provider
        .interrupt_interactive_shell(&shell.shell_id)
        .map_err(|err| format!("interactive interrupt failed: {}", err.message))?;
    provider
        .close_interactive_shell(&shell.shell_id)
        .map_err(|err| format!("interactive close failed: {}", err.message))?;
    let transcript = provider
        .read_interactive_transcript(&shell.shell_id, 0, 50)
        .map_err(|err| format!("interactive transcript read failed: {}", err.message))?;
    if !transcript
        .iter()
        .any(|line| line.contains(marker_interactive))
    {
        return Err(format!(
            "interactive transcript missing marker {marker_interactive}: {transcript:?}"
        ));
    }
    println!("self-test [ok] interactive transcript/checkpoint");

    println!("self-test [5/8] validating vault and auth contract smoke...");
    validate_vault_and_auth_contract_smoke()?;
    println!("self-test [ok] vault and auth contract smoke");

    println!("self-test [6/8] validating default model-plane bind probe...");
    let runtime_root = self_test_root.join("runtime");
    let state_dir = runtime_root.join("state");
    let artifacts_dir = runtime_root.join("artifacts");
    fs::create_dir_all(&state_dir)
        .map_err(|err| format!("create self-test state dir failed: {err}"))?;
    fs::create_dir_all(&artifacts_dir)
        .map_err(|err| format!("create self-test artifacts dir failed: {err}"))?;

    let mut settings = CoreSettings::from_toml_str(CoreSettings::minimal_example())
        .map_err(|err| format!("parse minimal settings failed: {err:?}"))?;
    settings.core.instance_name = "bridgingio-self-test".into();
    settings.core.data_dir = runtime_root.to_string_lossy().to_string();
    settings.storage.metadata_path = state_dir
        .join("metadata.sqlite3")
        .to_string_lossy()
        .to_string();
    settings.storage.artifacts.backend = "memory".into();
    settings.storage.artifacts.root = artifacts_dir.to_string_lossy().to_string();
    settings.control_plane.enabled = false;

    let bind_probe_host = settings.model_plane.http.host.clone();
    let bind_probe_port = settings.model_plane.http.port;
    let config_hint = runtime_root.join("self-test.toml");
    probe_model_plane_bind(&settings, &config_hint)?;
    println!(
        "self-test [ok] default model-plane bind probe on {}:{}",
        bind_probe_host, bind_probe_port
    );

    println!("self-test [7/8] validating standalone runtime execute path without config file...");
    settings.model_plane.http.enabled = false;
    settings.targets = vec![StandaloneTargetProfile {
        id: "self-test-local".into(),
        display_name: "Self-Test Local".into(),
        kind: TargetKind::Other("self-test".into()),
        enabled: true,
        aliases: vec!["local".into()],
        storage_class: "plain".into(),
        access_class: "anonymous-local".into(),
        sealed_profile_ref: None,
        credential_ref: None,
        notes: Some("self-test synthetic local target".into()),
        connection: StandaloneConnectionSection::default(),
        terminal: StandaloneTerminalSection::default(),
        toolchains: HashMap::new(),
        terminal_provider: TerminalProviderSection {
            enabled: true,
            shell: None,
        },
        git_repositories: Vec::new(),
    }];

    let resolver = default_toolchain_resolver(&settings, &config_hint);
    let mut runtime = StandaloneCoreRuntime::from_settings_with_mode(
        settings,
        resolver,
        CoreHostMode::StandaloneRun,
    )
    .map_err(|err| format!("create runtime for self-test failed: {err:?}"))?;

    let context = ApiRequestContext {
        agent_id: "self-test-agent".into(),
        run_id: "self-test-run".into(),
        client_session_id: "self-test-client".into(),
        reuse_policy: SessionReusePolicy::ReuseIfAlive,
    };

    let targets = runtime.handle_app_request(ApiRequest {
        request_id: "self-test-list-targets".into(),
        context: context.clone(),
        command: AppCommand::ListTargets,
    });
    let target_id = match targets {
        ApiResponse::Targets { items, .. } => items
            .into_iter()
            .find(|item| item.id == "self-test-local")
            .map(|item| item.id)
            .ok_or_else(|| "self-test target not found in runtime".to_string())?,
        other => return Err(format!("unexpected response for list targets: {other:?}")),
    };

    let open_session = runtime.handle_app_request(ApiRequest {
        request_id: "self-test-open-session".into(),
        context: context.clone(),
        command: AppCommand::OpenSession { target_id },
    });
    let session_id = match open_session {
        ApiResponse::Session { session, .. } => session.id,
        ApiResponse::Error { error, .. } => {
            return Err(format!("open session failed: {}", error.message))
        }
        other => return Err(format!("unexpected response for open session: {other:?}")),
    };

    let execution = runtime.handle_app_request(ApiRequest {
        request_id: "self-test-exec".into(),
        context: context.clone(),
        command: AppCommand::Execute {
            session_id,
            command: format!("echo {marker_runtime}"),
            stream: false,
        },
    });
    let artifact_id = match execution {
        ApiResponse::Execution { artifact, .. } => artifact.id,
        ApiResponse::Error { error, .. } => {
            return Err(format!("execute failed: {}", error.message))
        }
        other => return Err(format!("unexpected response for execute: {other:?}")),
    };

    let artifact = runtime.handle_app_request(ApiRequest {
        request_id: "self-test-read-artifact".into(),
        context,
        command: AppCommand::ReadArtifact {
            artifact_id,
            offset: 0,
            limit: 50,
        },
    });
    match artifact {
        ApiResponse::Artifact { artifact, .. } => {
            if !artifact
                .chunks
                .iter()
                .any(|line| line.contains(marker_runtime))
            {
                return Err(format!(
                    "runtime execute output missing marker {marker_runtime}: {:?}",
                    artifact.chunks
                ));
            }
        }
        ApiResponse::Error { error, .. } => {
            return Err(format!("read artifact failed: {}", error.message))
        }
        other => return Err(format!("unexpected response for read artifact: {other:?}")),
    }

    println!("self-test [ok] standalone runtime execute path");

    println!("self-test [8/8] validating host platform contract snapshot...");
    let host_platform_adapter = detect_host_platform_adapter("info");
    let snapshot = host_platform_adapter.snapshot();
    if snapshot.host_platform == HostPlatform::Unknown {
        return Err("host platform is unknown in self-test contract snapshot".to_string());
    }
    if snapshot.control_plane_transport_status == CapabilityStatus::Unsupported
        && snapshot.host_platform != HostPlatform::Unknown
    {
        return Err(format!(
            "unexpected unsupported control-plane transport status on host platform {:?}",
            snapshot.host_platform
        ));
    }
    let contract_paths = host_platform_adapter
        .runtime_paths()
        .runtime_paths("bridgingio-self-test", &runtime_root);
    match snapshot.host_platform {
        HostPlatform::Windows => {
            if !contract_paths
                .control_plane_endpoint
                .starts_with(r"\\.\pipe\bridgingio-")
            {
                return Err(format!(
                    "windows control-plane endpoint contract mismatch: {}",
                    contract_paths.control_plane_endpoint
                ));
            }
        }
        HostPlatform::Unix => {
            if !contract_paths
                .control_plane_endpoint
                .ends_with("control-plane.sock")
            {
                return Err(format!(
                    "unix control-plane endpoint contract mismatch: {}",
                    contract_paths.control_plane_endpoint
                ));
            }
        }
        HostPlatform::Unknown => {}
    }
    if host_platform_adapter.native_vault_binding().backend_label() != "os-native" {
        return Err("native vault backend label must remain os-native".to_string());
    }
    if !host_platform_adapter
        .toolchain_locator()
        .resolution_order()
        .contains(&"builtin_fallback")
    {
        return Err("toolchain resolution order missing builtin_fallback".to_string());
    }
    host_platform_adapter.runtime_logger().log(
        RuntimeLogLevel::Info,
        RuntimeLogCategory::Decode,
        "self-test validating platform output decoder",
    );
    let decoded = host_platform_adapter
        .output_decoder()
        .decode(b"line-1\r\nline-2\r");
    if decoded.text != "line-1\nline-2\n" || !decoded.normalized_newlines {
        return Err(format!(
            "platform output decoder contract mismatch: {:?}",
            decoded
        ));
    }
    println!("self-test [ok] host platform contract snapshot");

    let _ = fs::remove_dir_all(&self_test_root);
    println!("self-test passed");
    Ok(())
}

fn probe_model_plane_bind(settings: &CoreSettings, config_hint: &Path) -> Result<(), String> {
    let host = settings.model_plane.http.host.clone();
    let port = settings.model_plane.http.port;
    let resolver = default_toolchain_resolver(settings, config_hint);
    let runtime = StandaloneCoreRuntime::from_settings_with_mode(
        settings.clone(),
        resolver,
        CoreHostMode::StandaloneRun,
    )
    .map_err(|err| format!("create runtime for model-plane bind probe failed: {err:?}"))?
    .shared();

    let server = ModelPlaneHttpServer::bind(runtime, settings).map_err(|err| {
        format!("self-test model-plane bind probe failed at {host}:{port}: {err:?}")
    })?;
    drop(server);
    Ok(())
}

fn validate_vault_and_auth_contract_smoke() -> Result<(), String> {
    let canonical = normalize_credential_ref("vault:ssh-key:ops_prod")
        .map_err(|err| format!("normalize credential ref failed: {err:?}"))?;
    if canonical != "vault://bridgingio/ssh-private-key/ops-prod" {
        return Err(format!(
            "credential ref canonicalization mismatch: {canonical}"
        ));
    }
    let legacy = CoreSettings::minimal_example().replace(
        "credential_ref = \"vault://bridgingio/ssh-private-key/local\"",
        "credential_ref = \"vault:ssh-key:local\"",
    );
    let parsed_legacy = CoreSettings::from_toml_str(&legacy)
        .map_err(|err| format!("parse legacy vault config failed: {err:?}"))?;
    if parsed_legacy.vault.backend != "builtin-encrypted" {
        return Err(format!(
            "legacy vault backend must canonicalize to builtin-encrypted, got {}",
            parsed_legacy.vault.backend
        ));
    }
    if parsed_legacy.vault.protectors.primary.kind != "os-native" {
        return Err(format!(
            "legacy vault primary protector must canonicalize to os-native, got {}",
            parsed_legacy.vault.protectors.primary.kind
        ));
    }
    if parsed_legacy
        .targets
        .first()
        .and_then(|target| target.credential_ref.as_deref())
        != Some("vault://bridgingio/ssh-private-key/local")
    {
        return Err("legacy credential_ref must canonicalize to vault://...".to_string());
    }
    let rewritten_legacy = parsed_legacy.to_toml_string();
    if !rewritten_legacy.contains("[vault.unlock]")
        || !rewritten_legacy.contains("[vault.protectors.primary]")
        || !rewritten_legacy.contains("[vault.ssh]")
        || rewritten_legacy.contains("backend = \"os-native\"")
    {
        return Err("legacy vault config rewrite must emit canonical vault sections".to_string());
    }

    let mut router = SecretVaultRouter::default();
    let diag = router
        .active_backend_diagnostics()
        .map_err(|err| format!("read os-native backend diagnostics failed: {err:?}"))?;
    match diag.status {
        VaultReadinessState::Ready => {
            router
                .put("vault:ssh-key:dev", "SELF-TEST-DEV-KEY", "dev key")
                .map_err(|err| format!("ready os-native backend should allow writes: {err:?}"))?;
        }
        VaultReadinessState::Degraded => {
            match router.put("vault:ssh-key:dev", "SELF-TEST-DEV-KEY", "dev key") {
                Err(VaultError::FailClosed(_)) => {}
                other => {
                    return Err(format!(
                        "degraded backend must fail closed before explicit allow: {other:?}"
                    ))
                }
            }
            if !diag.fail_closed {
                return Err("degraded vault backend diagnostics must remain fail-closed".into());
            }
        }
        other => {
            return Err(format!(
                "unexpected os-native readiness status during self-test: {other:?}"
            ));
        }
    }

    router
        .set_active_backend("builtin-encrypted")
        .map_err(|err| format!("switch builtin-encrypted backend failed: {err:?}"))?;
    router
        .put(
            "vault:infra:token:self_test",
            "SELF-TEST-HTTP-TOKEN",
            "self-test token",
        )
        .map_err(|err| format!("store self-test http token failed: {err:?}"))?;
    router
        .put(
            "vault:ssh-key:self-test",
            "SELF-TEST-SSH-KEY",
            "self-test ssh key",
        )
        .map_err(|err| format!("store self-test ssh key failed: {err:?}"))?;

    match router.get("vault:infra:token:self_test") {
        Err(VaultError::PlaintextAccessDisabled(_)) => {}
        other => {
            return Err(format!(
                "plaintext get must remain disabled on canonical vault refs: {other:?}"
            ))
        }
    }

    let lease = router
        .use_for_http_auth("vault:infra:token:self_test", "provider:self-test")
        .map_err(|err| format!("issue broker lease for http auth failed: {err:?}"))?;
    let leased_secret =
        lease.with_secret_bytes(|bytes| std::str::from_utf8(bytes).unwrap_or_default().to_string());
    if leased_secret != "SELF-TEST-HTTP-TOKEN" {
        return Err(format!(
            "brokered http secret mismatch: expected SELF-TEST-HTTP-TOKEN, got {leased_secret}"
        ));
    }
    let redacted = router
        .redaction_registry()
        .redact("Authorization: Bearer SELF-TEST-HTTP-TOKEN");
    if redacted.contains("SELF-TEST-HTTP-TOKEN") || !redacted.contains("[REDACTED]") {
        return Err(format!(
            "redaction registry failed to mask brokered token: {redacted}"
        ));
    }

    let intent = router
        .create_local_admin_intent(
            LocalAdminActionKind::RevealSecret,
            "vault:ssh-key:self-test",
            "self-test-admin",
            Duration::from_secs(60),
        )
        .map_err(|err| format!("create local admin intent failed: {err:?}"))?;
    let attestation = router
        .verify_local_admin_intent(
            &intent.intent_id,
            "self-test-admin",
            "passkey",
            Duration::from_secs(30),
        )
        .map_err(|err| format!("verify local admin intent failed: {err:?}"))?;
    let revealed = router
        .reveal_for_local_admin(
            "vault:ssh-key:self-test",
            &intent.intent_id,
            &attestation.attestation_id,
            "self-test-admin",
        )
        .map_err(|err| format!("reveal for local admin failed: {err:?}"))?;
    if revealed.expose_utf8_for_use() != Some("SELF-TEST-SSH-KEY") {
        return Err("local admin reveal returned unexpected secret".into());
    }
    match router.reveal_for_local_admin(
        "vault:ssh-key:self-test",
        &intent.intent_id,
        &attestation.attestation_id,
        "self-test-admin",
    ) {
        Err(VaultError::LocalAdminVerificationRequired(_))
        | Err(VaultError::LocalAdminAttestationMismatch(_)) => {}
        Err(err) => {
            return Err(format!(
                "local admin intent/attestation returned unexpected error on second use: {err:?}"
            ))
        }
        Ok(_) => return Err("local admin intent/attestation must be single-use".into()),
    }
    match router.create_agent_token(CreateAgentTokenRequest {
        label: "self-test-synthetic".into(),
        created_by: "self-test-admin".into(),
        expires_in: None,
        idle_timeout_sec: None,
        scope: TokenScopeInput {
            scope_profile: Some("strict-default".into()),
            target_ids: Vec::new(),
            tool_ids: Vec::new(),
            max_risk_envelope: Some("deny-all".into()),
            allow_open_shell: Some(false),
            allow_write_shell_input: Some(false),
            allow_artifact_cross_principal: Some(false),
            allow_delegation: Some(false),
            allow_admin_actions: Some(false),
        },
        attestation_id: Some("attn-placeholder".into()),
    }) {
        Err(VaultError::LocalAdminVerificationRequired(_))
        | Err(VaultError::LocalAdminAttestationMismatch(_)) => {}
        other => {
            return Err(format!(
                "synthetic attestation id must be rejected for token create: {other:?}"
            ))
        }
    }
    match router.unlock_vault_with_attestation(UnlockVaultRequest {
        method: "os-native".into(),
        passphrase: None,
        requested_by: "self-test-admin".into(),
        attestation_id: "attn-placeholder".into(),
    }) {
        Err(VaultError::LocalAdminVerificationRequired(_))
        | Err(VaultError::LocalAdminAttestationMismatch(_)) => {}
        other => {
            return Err(format!(
                "synthetic attestation id must be rejected for vault unlock: {other:?}"
            ))
        }
    }

    let host_platform = if cfg!(windows) { "windows" } else { "unix" };
    let prepared = router
        .prepare_ssh_agent_broker_session(SshAgentBrokerPrepareRequest {
            target_id: "self-test-target".into(),
            credential_ref: "vault:ssh-key:self-test".into(),
            principal_id: "self-test-agent".into(),
            logical_session_id: Some("self-test-logical-session".into()),
            host_platform: host_platform.into(),
            allow_identity_fallback: true,
            host_key_policy: SshHostKeyPolicy::AcceptNew,
            key_passphrase_handling: SshKeyPassphraseHandling::ImportTimeOnly,
            runtime_passphrase_requested: false,
            session_ttl: Some(Duration::from_secs(30)),
        })
        .map_err(|err| format!("prepare ssh broker session failed: {err:?}"))?;
    if !prepared
        .ssh_option_args
        .iter()
        .any(|arg| arg.contains("IdentityAgent=") || arg == "-i")
    {
        return Err(format!(
            "ssh broker smoke did not emit broker/fallback ssh args: {:?}",
            prepared.ssh_option_args
        ));
    }
    let attached = router
        .attach_ssh_agent_broker_session(&prepared.session.broker_session_id, "channel-self-test")
        .map_err(|err| format!("attach ssh broker session failed: {err:?}"))?;
    if attached.attached_channel_count != 1 {
        return Err(format!(
            "ssh broker attach count mismatch: {}",
            attached.attached_channel_count
        ));
    }
    let detached = router
        .detach_ssh_agent_broker_session(&prepared.session.broker_session_id, "channel-self-test")
        .map_err(|err| format!("detach ssh broker session failed: {err:?}"))?;
    if detached.attached_channel_count != 0 {
        return Err(format!(
            "ssh broker detach count mismatch: {}",
            detached.attached_channel_count
        ));
    }
    let closed = router
        .close_ssh_agent_broker_session(&prepared.session.broker_session_id, "self-test cleanup")
        .map_err(|err| format!("close ssh broker session failed: {err:?}"))?;
    if closed.state.as_str() != "closed" {
        return Err(format!(
            "ssh broker session must close after cleanup, got {}",
            closed.state.as_str()
        ));
    }
    let ssh_diag = router
        .ssh_agent_broker_diagnostics(&prepared.session.broker_session_id)
        .map_err(|err| format!("read ssh broker diagnostics failed: {err:?}"))?;
    if !ssh_diag
        .iter()
        .any(|line| line.contains("principal scope: self-test-agent"))
    {
        return Err(format!(
            "ssh broker diagnostics missing principal scope: {ssh_diag:?}"
        ));
    }
    if !ssh_diag
        .iter()
        .any(|line| line.contains("logical session scope: self-test-logical-session"))
    {
        return Err(format!(
            "ssh broker diagnostics missing logical session scope: {ssh_diag:?}"
        ));
    }

    let mut invalid =
        CoreSettings::minimal_example().replace("host = \"127.0.0.1\"", "host = \"0.0.0.0\"");
    match CoreSettings::from_toml_str(&invalid) {
        Err(ConfigError::NonLoopbackExplicitEnableRequired(_)) => {}
        other => {
            return Err(format!(
                "non-loopback host must require explicit enable: {other:?}"
            ))
        }
    }
    invalid = invalid.replace("allow_non_loopback = false", "allow_non_loopback = true");
    match CoreSettings::from_toml_str(&invalid) {
        Err(ConfigError::NonLoopbackAuthRequired(_)) => {}
        other => {
            return Err(format!(
                "non-loopback host must require auth mode when protection is enabled: {other:?}"
            ))
        }
    }

    Ok(())
}

#[cfg(windows)]
fn shell_quote_path(path: &Path) -> String {
    format!("\"{}\"", path.to_string_lossy().replace('"', "\"\""))
}

#[cfg(not(windows))]
fn shell_quote_path(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
}

fn run_core(
    config_path: PathBuf,
    mode: LaunchMode,
    host_mode: CoreHostMode,
    control_plane_socket_override: Option<&Path>,
) -> Result<(), String> {
    let mut settings = CoreSettings::load_from_file(&config_path)
        .map_err(|err| format!("invalid config: {err:?}"))?;
    let host_platform_adapter = detect_host_platform_adapter(&settings.core.log_level);
    host_platform_adapter.runtime_logger().log(
        RuntimeLogLevel::Info,
        RuntimeLogCategory::Startup,
        &format!(
            "starting bridgingio-core (mode={mode:?}, host_mode={host_mode:?}, config={})",
            config_path.to_string_lossy()
        ),
    );
    apply_control_plane_socket_override(&mut settings, control_plane_socket_override)?;
    if control_plane_socket_override.is_some() {
        let warning =
            "using --control-plane-socket-override escape hatch (intended for debug/testing)";
        println!("{warning}");
        host_platform_adapter.runtime_logger().log(
            RuntimeLogLevel::Warn,
            RuntimeLogCategory::Transport,
            warning,
        );
    }
    let toolchain_resolver = default_toolchain_resolver(&settings, &config_path);
    let runtime = StandaloneCoreRuntime::from_settings_with_mode(
        settings.clone(),
        toolchain_resolver,
        host_mode,
    )
    .map_err(|err| format!("{err:?}"))?
    .shared();
    {
        let mut locked = runtime
            .lock()
            .map_err(|_| "runtime lock poisoned while setting config path".to_string())?;
        locked.settings_store.runtime_metadata.config_path =
            Some(config_path.to_string_lossy().to_string());
        settings = locked.settings_store.settings.clone();
    }

    let runtime_paths = host_platform_adapter.runtime_paths().runtime_paths(
        &settings.core.instance_name,
        Path::new(&settings.core.data_dir),
    );
    write_managed_instance_metadata(&runtime, &settings, host_mode, &runtime_paths)?;
    let transport = host_platform_adapter.control_plane_transport();
    let transport_lifecycle = transport.lifecycle_semantics();
    host_platform_adapter.runtime_logger().log(
        RuntimeLogLevel::Info,
        RuntimeLogCategory::Transport,
        &format!(
            "control-plane transport selected: kind={}, attach='{}', request_response='{}', lifecycle='{}'",
            transport.transport_kind(),
            transport_lifecycle.attach,
            transport_lifecycle.request_response,
            transport_lifecycle.lifecycle
        ),
    );
    for diagnostic in transport.diagnostics(&settings.core.instance_name, &runtime_paths) {
        let level = if diagnostic.status == bridgingio_platform::CapabilityStatus::Ready {
            RuntimeLogLevel::Info
        } else {
            RuntimeLogLevel::Warn
        };
        host_platform_adapter.runtime_logger().log(
            level,
            RuntimeLogCategory::Transport,
            &format!(
                "[{}] {} (hint: {})",
                diagnostic.code, diagnostic.message, diagnostic.recovery_hint
            ),
        );
    }

    let mut handles = Vec::new();
    if mcp_trace_enabled() {
        println!("mcp trace enabled via BRIDGINGIO_MCP_TRACE");
        host_platform_adapter.runtime_logger().log(
            RuntimeLogLevel::Info,
            RuntimeLogCategory::Mcp,
            "mcp trace enabled via BRIDGINGIO_MCP_TRACE",
        );
    }

    if settings.model_plane.http.enabled {
        let server = ModelPlaneHttpServer::bind(runtime.clone(), &settings)
            .map_err(|err| format!("bind model-plane http failed: {err:?}"))?;
        let http_addr = server
            .local_addr()
            .map_err(|err| format!("read model-plane address failed: {err:?}"))?;
        println!(
            "model-plane http listening on {}:{}",
            http_addr.ip(),
            http_addr.port()
        );
        handles.push(thread::spawn(move || loop {
            if let Err(err) = server.serve_once() {
                detect_host_platform_adapter("info").runtime_logger().log(
                    RuntimeLogLevel::Warn,
                    RuntimeLogCategory::Transport,
                    &format!("model-plane http stopped: {err:?}"),
                );
                break;
            }
        }));
    } else {
        println!("model-plane http disabled by config");
        host_platform_adapter.runtime_logger().log(
            RuntimeLogLevel::Info,
            RuntimeLogCategory::Transport,
            "model-plane http disabled by config",
        );
    }

    #[cfg(unix)]
    if settings.control_plane.enabled {
        let socket = control_plane_socket_path(&settings);
        let server = ControlPlaneIpcServer::bind(runtime.clone(), &socket)
            .map_err(|err| map_control_plane_bind_error(err, &socket))?;
        println!(
            "control-plane ipc listening on {}",
            socket.to_string_lossy()
        );
        handles.push(thread::spawn(move || loop {
            if let Err(err) = server.serve_once() {
                detect_host_platform_adapter("info").runtime_logger().log(
                    RuntimeLogLevel::Warn,
                    RuntimeLogCategory::Transport,
                    &format!("control-plane ipc stopped: {err:?}"),
                );
                break;
            }
        }));
    } else {
        println!("control-plane ipc disabled by config");
        host_platform_adapter.runtime_logger().log(
            RuntimeLogLevel::Info,
            RuntimeLogCategory::Transport,
            "control-plane ipc disabled by config",
        );
    }

    #[cfg(not(unix))]
    if settings.control_plane.enabled {
        let endpoint_semantics =
            transport.endpoint_semantics(&settings.core.instance_name, &runtime_paths);
        let message = format!(
            "control-plane transport is not wired on this build (kind={}, endpoint={}, naming_rule={})",
            transport.transport_kind(),
            endpoint_semantics.endpoint,
            endpoint_semantics.naming_rule
        );
        println!("{message}");
        host_platform_adapter.runtime_logger().log(
            RuntimeLogLevel::Warn,
            RuntimeLogCategory::Transport,
            &message,
        );
        if host_mode == CoreHostMode::UiManagedEphemeral {
            return Err(
                "ui-managed-ephemeral requires local control-plane attach, but the current host transport is deferred. Recovery: run on unix host for now or switch to standalone run mode.".to_string(),
            );
        }
    }

    if handles.is_empty() {
        return Err("no endpoint enabled (both control-plane and model-plane are disabled)".into());
    }

    let startup_state = {
        let runtime = runtime
            .lock()
            .map_err(|_| "runtime lock poisoned at startup".to_string())?;
        runtime.readiness_state_label().to_string()
    };
    println!(
        "bridgingio-core started in mode={:?} (host_mode={:?}, readiness={})",
        mode, host_mode, startup_state
    );
    host_platform_adapter.runtime_logger().log(
        RuntimeLogLevel::Info,
        RuntimeLogCategory::Startup,
        &format!(
            "bridgingio-core started (mode={mode:?}, host_mode={host_mode:?}, readiness={startup_state})"
        ),
    );
    if host_mode == CoreHostMode::UiManagedEphemeral {
        println!("waiting for ui attach before model-plane becomes ready");
    } else {
        println!("model-plane is ready immediately in standalone mode");
    }

    loop {
        thread::sleep(Duration::from_millis(300));
        let shutdown_requested = {
            let runtime = runtime
                .lock()
                .map_err(|_| "runtime lock poisoned while polling shutdown".to_string())?;
            runtime.shutdown_requested()
        };
        if shutdown_requested {
            println!("shutdown requested from control-plane, exiting core process");
            host_platform_adapter.runtime_logger().log(
                RuntimeLogLevel::Info,
                RuntimeLogCategory::Startup,
                "shutdown requested from control-plane; exiting core process",
            );
            break;
        }
    }
    cleanup_managed_instance_metadata(&runtime_paths);
    Ok(())
}

fn managed_instance_metadata_path(runtime_paths: &bridgingio_platform::RuntimePaths) -> PathBuf {
    runtime_paths.state_dir.join("managed-instance.json")
}

fn write_managed_instance_metadata(
    runtime: &bridgingio_mcp::SharedRuntime,
    settings: &CoreSettings,
    host_mode: CoreHostMode,
    runtime_paths: &bridgingio_platform::RuntimePaths,
) -> Result<(), String> {
    let host_mode_label = match host_mode {
        CoreHostMode::UiManagedEphemeral => "ui-managed-ephemeral",
        CoreHostMode::StandaloneRun => "standalone-run",
        CoreHostMode::StandaloneDetached => "standalone-detached",
    };
    let (core_instance_id, started_at_ms, readiness_state) = {
        let locked = runtime
            .lock()
            .map_err(|_| "runtime lock poisoned while writing instance metadata".to_string())?;
        (
            locked.core_instance_id().to_string(),
            locked
                .settings_store
                .runtime_metadata
                .started_at
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_millis() as u64)
                .unwrap_or(0),
            locked.readiness_state_label().to_string(),
        )
    };

    let payload = json!({
        "core_instance_id": core_instance_id,
        "host_mode": host_mode_label,
        "pid": std::process::id(),
        "started_at_ms": started_at_ms,
        "runtime_root": settings.core.data_dir,
        "control_plane_endpoint": runtime_paths.control_plane_endpoint,
        "model_plane": {
            "enabled": settings.model_plane.http.enabled,
            "host": settings.model_plane.http.host,
            "port": settings.model_plane.http.port,
        },
        "readiness_state": readiness_state,
    });
    let metadata_path = managed_instance_metadata_path(runtime_paths);
    let bytes = serde_json::to_vec_pretty(&payload)
        .map_err(|err| format!("encode managed instance metadata failed: {err}"))?;
    fs::write(&metadata_path, bytes).map_err(|err| {
        format!(
            "write managed instance metadata failed: {} ({err})",
            metadata_path.display()
        )
    })?;
    Ok(())
}

fn cleanup_managed_instance_metadata(runtime_paths: &bridgingio_platform::RuntimePaths) {
    let metadata_path = managed_instance_metadata_path(runtime_paths);
    let _ = fs::remove_file(metadata_path);
}

#[cfg(unix)]
fn map_control_plane_bind_error(err: CoreRuntimeError, endpoint: &Path) -> String {
    let detail = format!("{err:?}");
    if detail.contains("Permission denied") || detail.contains("Operation not permitted") {
        return format!(
            "bind control-plane ipc failed at {}: {detail}. Recovery: choose a writable runtime root or use --control-plane-socket-override to point to a writable location.",
            endpoint.display()
        );
    }
    if detail.contains("Address already in use") {
        return format!(
            "bind control-plane ipc failed at {}: {detail}. Recovery: stop the existing core process or use --control-plane-socket-override with a different endpoint.",
            endpoint.display()
        );
    }
    format!(
        "bind control-plane ipc failed at {}: {detail}",
        endpoint.display()
    )
}

fn spawn_detached_child(
    config_path: &Path,
    control_plane_socket_override: Option<&Path>,
) -> Result<(), String> {
    let exe = env::current_exe().map_err(|err| format!("resolve current_exe failed: {err}"))?;
    let mut command = Command::new(exe);
    command
        .arg("run")
        .arg("--config")
        .arg(config_path)
        .arg("--detached-child");
    if let Some(path) = control_plane_socket_override {
        command.arg("--control-plane-socket-override").arg(path);
    }
    let child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| format!("spawn detached child failed: {err}"))?;
    println!(
        "bridgingio-core detached in background (pid={})",
        child.id()
    );
    Ok(())
}

fn mcp_trace_enabled() -> bool {
    match env::var("BRIDGINGIO_MCP_TRACE") {
        Ok(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        }
        Err(_) => false,
    }
}

fn apply_control_plane_socket_override(
    settings: &mut CoreSettings,
    control_plane_socket_override: Option<&Path>,
) -> Result<(), String> {
    let Some(override_path) = control_plane_socket_override else {
        return Ok(());
    };
    if settings.control_plane.transport != "platform-ipc" {
        return Ok(());
    }
    let normalized = normalize_control_plane_socket_override(override_path)?;
    if let Some(parent) = normalized.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "create control-plane socket override dir failed: {} ({err})",
                parent.display()
            )
        })?;
    }
    settings.control_plane.endpoint = normalized.to_string_lossy().to_string();
    Ok(())
}

fn normalize_control_plane_socket_override(path: &Path) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() {
        return Err("--control-plane-socket-override requires a non-empty path".to_string());
    }
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        let cwd = env::current_dir().map_err(|err| format!("resolve current dir failed: {err}"))?;
        Ok(cwd.join(path))
    }
}

fn default_toolchain_resolver(settings: &CoreSettings, config_path: &Path) -> ToolchainResolver {
    let config_dir = config_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let bundled_root = config_dir.join("bin");
    let specs = vec![
        BuiltInBinarySpec {
            command: "ssh".into(),
            relative_path: PathBuf::from("ssh"),
            distribution: BuiltInDistributionKind::StandalonePackage,
        },
        BuiltInBinarySpec {
            command: "adb".into(),
            relative_path: PathBuf::from("adb"),
            distribution: BuiltInDistributionKind::StandalonePackage,
        },
    ];
    let _ = settings;
    ToolchainResolver::new(ExecutableResolver::from_system_path(), bundled_root, specs)
}

fn parse_args() -> Result<CliArgs, String> {
    parse_args_from(env::args().skip(1))
}

fn parse_args_from<I>(args: I) -> Result<CliArgs, String>
where
    I: IntoIterator<Item = String>,
{
    let args = args.into_iter().collect::<Vec<_>>();
    reject_plaintext_secret_argv(&args)?;
    if let Some(management) = parse_management_cli_args(&args)? {
        return Ok(management);
    }

    let mut mode = LaunchMode::StandaloneRun;
    let mut config_path = None::<PathBuf>;
    let mut runtime_root = None::<PathBuf>;
    let mut control_plane_socket_override = None::<PathBuf>;
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--self-test" => mode = LaunchMode::SelfTest,
            "run" => mode = LaunchMode::StandaloneRun,
            "ui-managed-ephemeral" => mode = LaunchMode::UiManagedEphemeral,
            "-d" => mode = LaunchMode::StandaloneDetachedLauncher,
            "--detached-child" => mode = LaunchMode::StandaloneDetachedChild,
            "--config" => {
                let path = iter
                    .next()
                    .ok_or_else(|| "--config requires a path".to_string())?;
                config_path = Some(PathBuf::from(path));
            }
            "--runtime-root" => {
                let path = iter
                    .next()
                    .ok_or_else(|| "--runtime-root requires a path".to_string())?;
                runtime_root = Some(PathBuf::from(path));
            }
            "--control-plane-socket-override" => {
                let path = iter
                    .next()
                    .ok_or_else(|| "--control-plane-socket-override requires a path".to_string())?;
                control_plane_socket_override = Some(PathBuf::from(path));
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            _ => {
                return Err(format!("unknown argument: {arg}"));
            }
        }
    }

    match mode {
        LaunchMode::SelfTest => {
            if config_path.is_some()
                || runtime_root.is_some()
                || control_plane_socket_override.is_some()
            {
                return Err(
                    "--self-test does not accept --config/--runtime-root/--control-plane-socket-override"
                        .to_string(),
                );
            }
        }
        LaunchMode::UiManagedEphemeral => {
            if runtime_root.is_none() && config_path.is_none() {
                return Err(
                    "ui-managed-ephemeral requires --runtime-root <dir> or --config <path>"
                        .to_string(),
                );
            }
            if runtime_root.is_some() && config_path.is_some() {
                return Err(
                    "ui-managed-ephemeral accepts either --runtime-root or --config, not both"
                        .to_string(),
                );
            }
        }
        _ => {
            if config_path.is_none() {
                return Err("missing required argument: --config <path>".to_string());
            }
            if runtime_root.is_some() {
                return Err("--runtime-root is only supported in ui-managed-ephemeral mode".into());
            }
        }
    }
    Ok(CliArgs {
        config_path,
        runtime_root,
        control_plane_socket_override,
        mode,
        management_command: None,
    })
}

fn parse_management_cli_args(args: &[String]) -> Result<Option<CliArgs>, String> {
    let Some(route) = args.first().map(String::as_str) else {
        return Ok(None);
    };
    match route {
        "vault" => parse_vault_management_cli(args).map(Some),
        "auth" => parse_auth_management_cli(args).map(Some),
        _ => Ok(None),
    }
}

fn parse_vault_management_cli(args: &[String]) -> Result<CliArgs, String> {
    let subcommand = args
        .get(1)
        .map(String::as_str)
        .ok_or_else(|| "vault route requires subcommand: init|import|unlock".to_string())?;
    let mut config_path = None::<PathBuf>;
    let mut method = None::<String>;
    let mut reference = None::<String>;
    let mut label = None::<String>;
    let mut input = SecretInputRoute::default();
    let mut iter = args.iter().skip(2);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--config" => {
                let path = iter
                    .next()
                    .ok_or_else(|| "--config requires a path".to_string())?;
                config_path = Some(PathBuf::from(path));
            }
            "--method" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--method requires a value".to_string())?;
                method = Some(value.to_string());
            }
            "--reference" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--reference requires a value".to_string())?;
                reference = Some(value.to_string());
            }
            "--label" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--label requires a value".to_string())?;
                label = Some(value.to_string());
            }
            "--from-fd" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--from-fd requires an integer fd".to_string())?;
                let fd = value
                    .parse::<i32>()
                    .map_err(|_| "--from-fd requires an integer fd".to_string())?;
                if fd < 0 {
                    return Err("--from-fd requires a non-negative fd".to_string());
                }
                input.fd = Some(fd);
            }
            "--from-stdin" => input.from_stdin = true,
            "--from-file" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--from-file requires a path".to_string())?;
                input.file = Some(PathBuf::from(value));
            }
            "--from-tty-prompt" => input.from_tty_prompt = true,
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument for vault {subcommand}: {arg}")),
        }
    }
    validate_secret_input_route(&input)?;
    let management_command = match subcommand {
        "init" => {
            if method.is_some()
                || reference.is_some()
                || label.is_some()
                || input.has_explicit_source()
            {
                return Err(
                    "vault init only accepts --config and does not consume secret input flags"
                        .to_string(),
                );
            }
            ManagementCommand::VaultInit
        }
        "import" => ManagementCommand::VaultImport {
            reference: reference.ok_or_else(|| {
                "vault import requires --reference <credential-ref>".to_string()
            })?,
            label,
            input,
        },
        "unlock" => {
            if reference.is_some() || label.is_some() {
                return Err(
                    "vault unlock does not accept --reference/--label (use --method and secret input flags)"
                        .to_string(),
                );
            }
            ManagementCommand::VaultUnlock { method, input }
        }
        other => {
            return Err(format!(
                "unknown vault subcommand `{other}` (expected init|import|unlock)"
            ))
        }
    };

    if config_path.is_none() {
        return Err("missing required argument: --config <path>".to_string());
    }
    Ok(CliArgs {
        config_path,
        runtime_root: None,
        control_plane_socket_override: None,
        mode: LaunchMode::StandaloneRun,
        management_command: Some(management_command),
    })
}

fn parse_auth_management_cli(args: &[String]) -> Result<CliArgs, String> {
    let route = args
        .get(1)
        .map(String::as_str)
        .ok_or_else(|| "auth route requires subcommand group `token`".to_string())?;
    if route != "token" {
        return Err(format!(
            "unknown auth subcommand group `{route}` (expected `token`)"
        ));
    }
    let subcommand = args
        .get(2)
        .map(String::as_str)
        .ok_or_else(|| "auth token requires subcommand: create|revoke".to_string())?;
    let mut config_path = None::<PathBuf>;
    let mut label = None::<String>;
    let mut expires_in_seconds = None::<u64>;
    let mut token_id = None::<String>;
    let mut reason = None::<String>;
    let mut iter = args.iter().skip(3);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--config" => {
                let path = iter
                    .next()
                    .ok_or_else(|| "--config requires a path".to_string())?;
                config_path = Some(PathBuf::from(path));
            }
            "--label" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--label requires a value".to_string())?;
                label = Some(value.to_string());
            }
            "--expires-in-seconds" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--expires-in-seconds requires a u64".to_string())?;
                expires_in_seconds = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| "--expires-in-seconds requires a u64".to_string())?,
                );
            }
            "--token-id" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--token-id requires a value".to_string())?;
                token_id = Some(value.to_string());
            }
            "--reason" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--reason requires a value".to_string())?;
                reason = Some(value.to_string());
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            _ => {
                return Err(format!(
                    "unknown argument for auth token {subcommand}: {arg}"
                ))
            }
        }
    }
    let management_command = match subcommand {
        "create" => ManagementCommand::AuthTokenCreate {
            label: label.ok_or_else(|| "auth token create requires --label <name>".to_string())?,
            expires_in_seconds,
        },
        "revoke" => {
            if label.is_some() || expires_in_seconds.is_some() {
                return Err(
                    "auth token revoke does not accept --label/--expires-in-seconds".to_string(),
                );
            }
            ManagementCommand::AuthTokenRevoke {
                token_id: token_id
                    .ok_or_else(|| "auth token revoke requires --token-id <id>".to_string())?,
                reason,
            }
        }
        other => {
            return Err(format!(
                "unknown auth token subcommand `{other}` (expected create|revoke)"
            ))
        }
    };

    if config_path.is_none() {
        return Err("missing required argument: --config <path>".to_string());
    }
    Ok(CliArgs {
        config_path,
        runtime_root: None,
        control_plane_socket_override: None,
        mode: LaunchMode::StandaloneRun,
        management_command: Some(management_command),
    })
}

fn validate_secret_input_route(input: &SecretInputRoute) -> Result<(), String> {
    if input.explicit_count() > 1 {
        return Err(
            "secret input source conflict: choose exactly one of --from-fd/--from-stdin/--from-file/--from-tty-prompt"
                .to_string(),
        );
    }
    Ok(())
}

fn read_secret_input(input: &SecretInputRoute, prompt: &str) -> Result<SecretInputCapture, String> {
    validate_secret_input_route(input)?;
    let (kind, locator, summary, bytes) = if let Some(fd) = input.fd {
        let path = format!("/dev/fd/{fd}");
        let bytes = fs::read(&path).map_err(|err| format!("read --from-fd failed ({path}): {err}"))?;
        (SecretSourceKind::Fd, path, "explicit:fd".to_string(), bytes)
    } else if input.from_stdin {
        let bytes = read_all_stdin()?;
        (
            SecretSourceKind::Stdin,
            "stdin:explicit".to_string(),
            "explicit:stdin".to_string(),
            bytes,
        )
    } else if let Some(path) = input.file.as_ref() {
        let bytes = fs::read(path)
            .map_err(|err| format!("read --from-file failed ({}): {err}", path.display()))?;
        (
            SecretSourceKind::File,
            path.to_string_lossy().to_string(),
            "explicit:file".to_string(),
            bytes,
        )
    } else if input.from_tty_prompt {
        let bytes = read_secret_from_tty_prompt(prompt)?;
        (
            SecretSourceKind::TtyPrompt,
            "tty-prompt:explicit".to_string(),
            "explicit:tty-prompt".to_string(),
            bytes,
        )
    } else if !io::stdin().is_terminal() {
        let bytes = read_all_stdin()?;
        (
            SecretSourceKind::Stdin,
            "stdin:auto".to_string(),
            "auto:stdin".to_string(),
            bytes,
        )
    } else {
        let bytes = read_secret_from_tty_prompt(prompt)?;
        (
            SecretSourceKind::TtyPrompt,
            "tty-prompt:auto".to_string(),
            "auto:tty-prompt".to_string(),
            bytes,
        )
    };

    if bytes.is_empty() {
        return Err("secret input is empty".to_string());
    }
    let secret = String::from_utf8(bytes.clone())
        .map_err(|_| "secret input must be valid UTF-8".to_string())?;
    Ok(SecretInputCapture {
        secret,
        source_kind: kind,
        source_summary: summary,
        source_locator_digest: stable_locator_digest(&locator),
        byte_length: bytes.len(),
    })
}

fn read_all_stdin() -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    io::stdin()
        .read_to_end(&mut bytes)
        .map_err(|err| format!("read stdin failed: {err}"))?;
    Ok(bytes)
}

fn read_secret_from_tty_prompt(prompt: &str) -> Result<Vec<u8>, String> {
    if !io::stdin().is_terminal() {
        return Err("tty prompt requested but stdin is not a tty".to_string());
    }
    print!("{prompt}");
    io::stdout()
        .flush()
        .map_err(|err| format!("flush tty prompt failed: {err}"))?;
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|err| format!("read tty prompt input failed: {err}"))?;
    Ok(trim_single_trailing_newline(line).into_bytes())
}

fn trim_single_trailing_newline(value: String) -> String {
    let mut value = value;
    if value.ends_with('\n') {
        value.pop();
        if value.ends_with('\r') {
            value.pop();
        }
    }
    value
}

fn stable_locator_digest(locator: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    locator.hash(&mut hasher);
    format!("siphash64:{:016x}", hasher.finish())
}

fn emit_management_audit(
    action: &str,
    operator_principal: &str,
    intent_id: Option<&str>,
    input: Option<&SecretInputCapture>,
    status: &str,
) {
    let payload = json!({
        "kind": "standalone_management_audit",
        "action": action,
        "status": status,
        "operator_principal": operator_principal,
        "intent_id": intent_id,
        "source_kind": input.map(|capture| capture.source_kind.as_str()),
        "source_summary": input.map(|capture| capture.source_summary.clone()),
        "source_locator_digest": input.map(|capture| capture.source_locator_digest.clone()),
        "byte_length": input.map(|capture| capture.byte_length),
    });
    eprintln!(
        "{}",
        serde_json::to_string(&payload).unwrap_or_else(|_| "{\"kind\":\"standalone_management_audit\",\"status\":\"encode-failed\"}".to_string())
    );
}

fn management_operator_principal() -> String {
    let user = env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string());
    format!("standalone-cli:{user}")
}

fn normalize_cli_vault_method(method: &str) -> String {
    method.trim().to_ascii_lowercase()
}

fn load_management_router(config_path: &Path) -> Result<(CoreSettings, SecretVaultRouter), String> {
    let settings = CoreSettings::load_from_file(config_path)
        .map_err(|err| format!("invalid config: {err:?}"))?;
    let vault_root = Path::new(&settings.core.data_dir).join("vault");
    fs::create_dir_all(&vault_root).map_err(|err| {
        format!(
            "create vault store dir failed: {} ({err})",
            vault_root.display()
        )
    })?;
    let mut router = SecretVaultRouter::with_persistent_store(&vault_root)
        .map_err(|err| format!("open vault persistent store failed: {err:?}"))?;
    router
        .set_active_backend(&settings.vault.backend)
        .map_err(|err| format!("set active vault backend failed: {err:?}"))?;
    router
        .set_unlock_policy(vault_unlock_policy_from_settings(&settings)?)
        .map_err(|err| format!("set vault unlock policy failed: {err:?}"))?;
    Ok((settings, router))
}

fn vault_unlock_policy_from_settings(settings: &CoreSettings) -> Result<VaultUnlockPolicy, String> {
    Ok(VaultUnlockPolicy {
        trigger_policy: parse_vault_trigger_policy(&settings.vault.unlock.trigger_policy)?,
        allowed_methods: settings.vault.unlock.allowed_methods.clone(),
        preferred_method: settings.vault.unlock.preferred_method.clone(),
        cache_ttl_sec: settings.vault.unlock.cache_ttl_sec,
        require_fresh_user_verification: settings.vault.unlock.require_fresh_user_verification,
    })
}

fn parse_vault_trigger_policy(raw: &str) -> Result<VaultUnlockTriggerPolicy, String> {
    match normalize_cli_vault_method(raw).as_str() {
        "on-core-start" => Ok(VaultUnlockTriggerPolicy::OnCoreStart),
        "on-first-secret-access" => Ok(VaultUnlockTriggerPolicy::OnFirstSecretAccess),
        "on-every-secret-access" => Ok(VaultUnlockTriggerPolicy::OnEverySecretAccess),
        "manual-only" => Ok(VaultUnlockTriggerPolicy::ManualOnly),
        other => Err(format!("unsupported vault unlock trigger policy: {other}")),
    }
}

fn reject_plaintext_secret_argv(args: &[String]) -> Result<(), String> {
    const FORBIDDEN_FLAGS: [&str; 3] = ["--token", "--secret", "--access-token"];
    if let Some(flag) = args
        .iter()
        .map(String::as_str)
        .find(|arg| FORBIDDEN_FLAGS.contains(arg))
    {
        return Err(format!(
            "{flag} is forbidden: do not pass plaintext token or secret via argv"
        ));
    }
    Ok(())
}

fn print_usage() {
    eprintln!("usage:");
    eprintln!("  bridgingio-core --self-test");
    eprintln!("  bridgingio-core run --config <path-to-standalone.toml>");
    eprintln!("  bridgingio-core -d --config <path-to-standalone.toml>");
    eprintln!("  bridgingio-core ui-managed-ephemeral --runtime-root <runtime-root-dir>");
    eprintln!("  bridgingio-core ui-managed-ephemeral --config <path-to-managed-core.toml>");
    eprintln!("  bridgingio-core vault init --config <path-to-standalone.toml>");
    eprintln!("  bridgingio-core vault import --config <path> --reference <vault://...> [--label <label>] [--from-fd N|--from-stdin|--from-file <path>|--from-tty-prompt]");
    eprintln!("  bridgingio-core vault unlock --config <path> [--method os-native|passphrase] [--from-fd N|--from-stdin|--from-file <path>|--from-tty-prompt]");
    eprintln!("  bridgingio-core auth token create --config <path> --label <name> [--expires-in-seconds <u64>]");
    eprintln!("  bridgingio-core auth token revoke --config <path> --token-id <token-id> [--reason <reason>]");
    eprintln!("  debug/testing escape hatch: --control-plane-socket-override <path-or-endpoint>");
}

fn resolve_config_path(args: &CliArgs) -> Result<PathBuf, String> {
    if let Some(path) = args.config_path.clone() {
        return Ok(path);
    }
    if args.mode == LaunchMode::UiManagedEphemeral {
        if let Some(runtime_root) = args.runtime_root.as_ref() {
            return ensure_runtime_root_layout(runtime_root);
        }
    }
    Err("missing config path".to_string())
}

fn ensure_runtime_root_layout(runtime_root: &Path) -> Result<PathBuf, String> {
    ensure_runtime_dir(runtime_root, "runtime root")?;
    let host_platform_adapter = detect_host_platform_adapter("info");
    let runtime_paths = host_platform_adapter
        .runtime_paths()
        .runtime_paths("bridgingio-ui-managed", runtime_root);
    let config_dir = runtime_root.join("config");
    let state_dir = runtime_paths.state_dir.clone();
    let artifacts_dir = runtime_paths.artifact_root.clone();
    let logs_dir = runtime_paths.logs_dir.clone();
    ensure_runtime_dir(&config_dir, "config dir")?;
    ensure_runtime_dir(&state_dir, "state dir")?;
    ensure_runtime_dir(&artifacts_dir, "artifacts dir")?;
    ensure_runtime_dir(&logs_dir, "logs dir")?;

    if let Some(parent) = runtime_paths.metadata_path.parent() {
        ensure_runtime_dir(parent, "metadata parent dir")?;
        ensure_writable_probe(parent, "metadata parent dir")?;
    } else {
        return Err(format!(
            "metadata path has no parent: {}. Recovery: choose a different runtime root.",
            runtime_paths.metadata_path.display()
        ));
    }
    ensure_writable_probe(runtime_root, "runtime root")?;
    ensure_writable_probe(&state_dir, "state dir")?;
    ensure_writable_probe(&artifacts_dir, "artifacts dir")?;
    ensure_writable_probe(&logs_dir, "logs dir")?;

    let config_path = config_dir.join("managed-core.toml");
    if !config_path.exists() {
        fs::write(
            &config_path,
            default_managed_core_config(runtime_root, &runtime_paths),
        )
        .map_err(|err| {
            format!(
                "create managed config failed: {} ({err})",
                config_path.display()
            )
        })?;
    }
    Ok(config_path)
}

fn ensure_runtime_dir(path: &Path, label: &str) -> Result<(), String> {
    if path.exists() && !path.is_dir() {
        return Err(format!(
            "{label} is not a directory: {}. Recovery: remove/rename this path or choose another runtime root.",
            path.display()
        ));
    }
    fs::create_dir_all(path).map_err(|err| {
        format!(
            "create {label} failed: {} ({err}). Recovery: choose a writable runtime root and retry.",
            path.display()
        )
    })
}

fn ensure_writable_probe(path: &Path, label: &str) -> Result<(), String> {
    let probe = path.join(".bridgingio-write-probe");
    fs::write(&probe, b"ok").map_err(|err| {
        format!(
            "{label} is not writable: {} ({err}). Recovery: grant write permission or choose another runtime root.",
            path.display()
        )
    })?;
    let _ = fs::remove_file(probe);
    Ok(())
}

fn default_managed_core_config(
    runtime_root: &Path,
    runtime_paths: &bridgingio_platform::RuntimePaths,
) -> String {
    let data_dir = toml_escape_path(runtime_root);
    let metadata_path = toml_escape_path(&runtime_paths.metadata_path);
    let artifacts_path = toml_escape_path(&runtime_paths.artifact_root);
    let endpoint = toml_escape_string(&runtime_paths.control_plane_endpoint);
    format!(
        r#"schema_version = 1

[core]
instance_name = "bridgingio-ui-managed"
data_dir = "{data_dir}"
log_level = "info"

[storage]
metadata_backend = "sqlite"
metadata_path = "{metadata_path}"

[storage.artifacts]
backend = "filesystem"
root = "{artifacts_path}"
max_bytes = 268435456
eviction_policy = "lru"

[vault]
backend = "os-native"
namespace = "io.bridgingio"

[control_plane]
enabled = true
transport = "platform-ipc"
endpoint = "{endpoint}"

[model_plane.http]
enabled = true
host = "127.0.0.1"
port = 19718
allow_non_loopback = false

[model_plane.http.auth]
mode = "none"
required_when_non_loopback = true
allow_loopback_anonymous_compat = true

[policies.defaults]
reuse_policy = "resume_or_create"
approval_mode = "on-risk"
capture_env_fingerprint = true
"#
    )
}

fn toml_escape_path(path: &Path) -> String {
    toml_escape_string(&path.to_string_lossy())
}

fn toml_escape_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use bridgingio_engine::CoreSettings;

    use super::{parse_args_from, CliArgs, LaunchMode, ManagementCommand, SecretInputRoute};

    fn parse(items: &[&str]) -> LaunchMode {
        let args = items
            .iter()
            .map(|item| item.to_string())
            .collect::<Vec<_>>();
        parse_args_from(args).expect("parse").mode
    }

    fn parse_cli(items: &[&str]) -> CliArgs {
        let args = items
            .iter()
            .map(|item| item.to_string())
            .collect::<Vec<_>>();
        parse_args_from(args).expect("parse")
    }

    #[test]
    fn parses_default_as_standalone_run() {
        assert_eq!(
            parse(&["--config", "/tmp/standalone.toml"]),
            LaunchMode::StandaloneRun
        );
    }

    #[test]
    fn parses_detached_mode() {
        assert_eq!(
            parse(&["-d", "--config", "/tmp/standalone.toml"]),
            LaunchMode::StandaloneDetachedLauncher
        );
    }

    #[test]
    fn parses_ui_managed_mode() {
        assert_eq!(
            parse(&["ui-managed-ephemeral", "--runtime-root", "/tmp/runtime"]),
            LaunchMode::UiManagedEphemeral
        );
    }

    #[test]
    fn parses_self_test_mode() {
        assert_eq!(parse(&["--self-test"]), LaunchMode::SelfTest);
    }

    #[test]
    fn rejects_self_test_with_config() {
        let args = ["--self-test", "--config", "/tmp/standalone.toml"]
            .iter()
            .map(|item| item.to_string())
            .collect::<Vec<_>>();
        let err = parse_args_from(args).expect_err("must reject mixed self-test arguments");
        assert!(err.contains("--self-test does not accept"));
    }

    #[test]
    fn rejects_plaintext_token_argv_flags() {
        let args = [
            "run",
            "--config",
            "/tmp/standalone.toml",
            "--token",
            "raw-value",
        ]
        .iter()
        .map(|item| item.to_string())
        .collect::<Vec<_>>();
        let err = parse_args_from(args).expect_err("must reject plaintext token argv");
        assert!(err.contains("forbidden"));
    }

    #[test]
    fn parses_auth_token_create_management_route() {
        let parsed = parse_cli(&[
            "auth",
            "token",
            "create",
            "--config",
            "/tmp/standalone.toml",
            "--label",
            "nightly-runner",
            "--expires-in-seconds",
            "1800",
        ]);
        assert_eq!(parsed.mode, LaunchMode::StandaloneRun);
        assert!(parsed.control_plane_socket_override.is_none());
        assert_eq!(
            parsed.management_command,
            Some(ManagementCommand::AuthTokenCreate {
                label: "nightly-runner".to_string(),
                expires_in_seconds: Some(1800),
            })
        );
    }

    #[test]
    fn parses_vault_unlock_management_route_with_explicit_input_source() {
        let parsed = parse_cli(&[
            "vault",
            "unlock",
            "--config",
            "/tmp/standalone.toml",
            "--method",
            "passphrase",
            "--from-file",
            "/tmp/passphrase.txt",
        ]);
        assert_eq!(
            parsed.management_command,
            Some(ManagementCommand::VaultUnlock {
                method: Some("passphrase".to_string()),
                input: SecretInputRoute {
                    fd: None,
                    from_stdin: false,
                    file: Some(PathBuf::from("/tmp/passphrase.txt")),
                    from_tty_prompt: false,
                },
            })
        );
    }

    #[test]
    fn rejects_conflicting_secret_input_sources_in_management_route() {
        let args = [
            "vault",
            "unlock",
            "--config",
            "/tmp/standalone.toml",
            "--from-stdin",
            "--from-file",
            "/tmp/passphrase.txt",
        ]
        .iter()
        .map(|item| item.to_string())
        .collect::<Vec<_>>();
        let err = parse_args_from(args).expect_err("must reject conflicting secret sources");
        assert!(err.contains("secret input source conflict"));
    }

    #[test]
    fn parses_control_plane_socket_override_for_ui_mode() {
        let parsed = parse_cli(&[
            "ui-managed-ephemeral",
            "--runtime-root",
            "/tmp/runtime",
            "--control-plane-socket-override",
            "/tmp/bridgingio-ui.sock",
        ]);
        assert_eq!(
            parsed.control_plane_socket_override,
            Some(PathBuf::from("/tmp/bridgingio-ui.sock"))
        );
    }

    #[test]
    fn parses_control_plane_socket_override_for_standalone_mode() {
        let parsed = parse_cli(&[
            "run",
            "--config",
            "/tmp/standalone.toml",
            "--control-plane-socket-override",
            "relative.sock",
        ]);
        assert_eq!(
            parsed.control_plane_socket_override,
            Some(PathBuf::from("relative.sock"))
        );
    }

    #[test]
    fn creates_runtime_root_layout_and_default_config() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = PathBuf::from("/tmp").join(format!("bridgingio-runtime-{stamp}"));
        fs::create_dir_all(&root).expect("create runtime root");
        let config_path = super::ensure_runtime_root_layout(&root).expect("layout");
        assert!(root.join("config").is_dir());
        assert!(root.join("state").is_dir());
        assert!(root.join("artifacts").is_dir());
        assert!(root.join("logs").is_dir());
        assert!(config_path.exists());

        let settings = CoreSettings::load_from_file(&config_path).expect("parse config");
        assert_eq!(settings.model_plane.http.host, "127.0.0.1");
        assert_eq!(settings.model_plane.http.port, 19718);
        assert_eq!(settings.core.data_dir, root.to_string_lossy().to_string());
    }

    fn self_test_probe_settings(port: u16) -> (CoreSettings, PathBuf, PathBuf) {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = env::temp_dir().join(format!("bridgingio-probe-{stamp}"));
        let state_dir = root.join("state");
        let artifacts_dir = root.join("artifacts");
        fs::create_dir_all(&state_dir).expect("create state dir");
        fs::create_dir_all(&artifacts_dir).expect("create artifacts dir");

        let mut settings = CoreSettings::from_toml_str(CoreSettings::minimal_example())
            .expect("parse minimal settings");
        settings.core.instance_name = format!("bridgingio-self-test-{stamp}");
        settings.core.data_dir = root.to_string_lossy().to_string();
        settings.storage.metadata_path = state_dir
            .join("metadata.sqlite3")
            .to_string_lossy()
            .to_string();
        settings.storage.artifacts.backend = "memory".into();
        settings.storage.artifacts.root = artifacts_dir.to_string_lossy().to_string();
        settings.control_plane.enabled = false;
        settings.model_plane.http.host = "127.0.0.1".into();
        settings.model_plane.http.port = port;

        let config_hint = root.join("self-test.toml");
        (settings, config_hint, root)
    }

    #[test]
    fn model_plane_bind_probe_accepts_available_port() {
        let (settings, config_hint, root) = self_test_probe_settings(0);
        super::probe_model_plane_bind(&settings, &config_hint).expect("probe should bind");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn model_plane_bind_probe_reports_host_port_and_error_on_failure() {
        let occupied = TcpListener::bind("127.0.0.1:0").expect("bind occupied port");
        let port = occupied.local_addr().expect("read local addr").port();
        let (settings, config_hint, root) = self_test_probe_settings(port);
        let err =
            super::probe_model_plane_bind(&settings, &config_hint).expect_err("probe must fail");
        assert!(err.contains("127.0.0.1"));
        assert!(err.contains(&port.to_string()));
        assert!(err.contains("bind"));
        drop(occupied);
        let _ = fs::remove_dir_all(root);
    }
}
