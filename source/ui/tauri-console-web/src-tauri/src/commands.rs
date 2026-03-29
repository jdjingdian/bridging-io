use serde_json::{json, Value};
use tauri::{AppHandle, State};

use crate::app_state::{AppRuntimeState, RuntimeRootValidationDto, StartupStateDto};
use crate::windowing;
use crate::SharedHostState;

fn with_state<T>(
    state: State<'_, SharedHostState>,
    op: impl FnOnce(&mut AppRuntimeState) -> Result<T, String>,
) -> Result<T, String> {
    let mut guard = state
        .0
        .lock()
        .map_err(|_| "desktop host state lock poisoned".to_string())?;
    op(&mut guard)
}

async fn with_state_async<T>(
    state: State<'_, SharedHostState>,
    op: impl FnOnce(&mut AppRuntimeState) -> Result<T, String> + Send + 'static,
) -> Result<T, String>
where
    T: Send + 'static,
{
    let shared = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = shared
            .lock()
            .map_err(|_| "desktop host state lock poisoned".to_string())?;
        op(&mut guard)
    })
    .await
    .map_err(|err| format!("desktop host worker join failed: {err}"))?
}

#[tauri::command]
pub fn get_startup_state(state: State<'_, SharedHostState>) -> Result<StartupStateDto, String> {
    let guard = state
        .0
        .lock()
        .map_err(|_| "desktop host state lock poisoned".to_string())?;
    Ok(guard.startup_state())
}

#[tauri::command]
pub fn pick_runtime_root_dir() -> Result<String, String> {
    let selected = rfd::FileDialog::new()
        .set_title("Choose BridgingIO Runtime Root")
        .pick_folder()
        .ok_or_else(|| "runtime root picker was cancelled".to_string())?;
    Ok(selected.to_string_lossy().to_string())
}

#[tauri::command]
pub fn validate_runtime_root(
    state: State<'_, SharedHostState>,
    path: String,
) -> Result<RuntimeRootValidationDto, String> {
    let guard = state
        .0
        .lock()
        .map_err(|_| "desktop host state lock poisoned".to_string())?;
    Ok(guard.validate_runtime_root_input(&path))
}

#[tauri::command]
pub fn complete_onboarding_runtime_root(
    state: State<'_, SharedHostState>,
    path: String,
) -> Result<Value, String> {
    with_state(state, |runtime| {
        runtime.complete_onboarding_runtime_root(&path)?;
        Ok(json!({ "ok": true, "route": "workspace" }))
    })
}

#[tauri::command]
pub fn open_main_window(
    app: AppHandle,
    state: State<'_, SharedHostState>,
) -> Result<Value, String> {
    windowing::open_or_focus_main_window(&app)?;
    with_state(state, |runtime| {
        runtime.mark_window_open_or_focus();
        Ok(json!({ "ok": true }))
    })
}

#[tauri::command]
pub fn focus_main_window(
    app: AppHandle,
    state: State<'_, SharedHostState>,
) -> Result<Value, String> {
    open_main_window(app, state)
}

#[tauri::command]
pub fn request_local_user_verification(
    state: State<'_, SharedHostState>,
    action: String,
    reason: Option<String>,
) -> Result<Value, String> {
    with_state(state, |runtime| {
        runtime.record_notification(
            "Trusted local verification",
            &format!(
                "action={} reason={}",
                action,
                reason.unwrap_or_else(|| "none".to_string())
            ),
        );
        Ok(json!({
            "verified": true,
            "status": "verified",
            "mode": "local-development-stub",
            "action": action,
        }))
    })
}

#[tauri::command]
pub fn trusted_local_user_verification(
    state: State<'_, SharedHostState>,
    action: String,
    reason: Option<String>,
) -> Result<Value, String> {
    request_local_user_verification(state, action, reason)
}

#[tauri::command]
pub async fn workspace_bridge_command(
    state: State<'_, SharedHostState>,
    command: String,
    payload: Option<Value>,
) -> Result<Value, String> {
    with_state_async(state, move |runtime| {
        runtime.execute_workspace_command(&command, payload.unwrap_or_else(|| json!({})))
    })
    .await
}

#[tauri::command]
pub async fn desktop_bridge_command(
    state: State<'_, SharedHostState>,
    command: String,
    payload: Option<Value>,
) -> Result<Value, String> {
    workspace_bridge_command(state, command, payload).await
}

#[tauri::command]
pub async fn get_bootstrap_state(
    state: State<'_, SharedHostState>,
    timeline_limit: Option<u64>,
    artifact_limit: Option<u64>,
    transcript_limit: Option<u64>,
) -> Result<Value, String> {
    workspace_bridge_command(
        state,
        "get_bootstrap_state".to_string(),
        Some(json!({
            "timeline_limit": timeline_limit,
            "artifact_limit": artifact_limit,
            "transcript_limit": transcript_limit,
        })),
    )
    .await
}

#[tauri::command]
pub async fn desktop_get_bootstrap_state(
    state: State<'_, SharedHostState>,
    timeline_limit: Option<u64>,
    artifact_limit: Option<u64>,
    transcript_limit: Option<u64>,
) -> Result<Value, String> {
    get_bootstrap_state(state, timeline_limit, artifact_limit, transcript_limit).await
}

#[tauri::command]
pub async fn trusted_get_bootstrap_state(
    state: State<'_, SharedHostState>,
    timeline_limit: Option<u64>,
    artifact_limit: Option<u64>,
    transcript_limit: Option<u64>,
) -> Result<Value, String> {
    get_bootstrap_state(state, timeline_limit, artifact_limit, transcript_limit).await
}

#[tauri::command]
pub async fn get_settings(state: State<'_, SharedHostState>) -> Result<Value, String> {
    workspace_bridge_command(state, "get_settings".to_string(), Some(json!({}))).await
}

#[tauri::command]
pub async fn desktop_get_settings(state: State<'_, SharedHostState>) -> Result<Value, String> {
    get_settings(state).await
}

#[tauri::command]
pub async fn trusted_get_settings(state: State<'_, SharedHostState>) -> Result<Value, String> {
    get_settings(state).await
}

#[tauri::command]
pub async fn update_settings(
    state: State<'_, SharedHostState>,
    core_log_level: Option<String>,
    model_plane_host: Option<String>,
    model_plane_port: Option<u16>,
    artifact_cache_backend: Option<String>,
    artifact_cache_root: Option<String>,
    artifact_cache_max_bytes: Option<u64>,
    artifact_cache_eviction_policy: Option<String>,
    tool_override_command: Option<String>,
    tool_override_path: Option<String>,
) -> Result<Value, String> {
    workspace_bridge_command(
        state,
        "update_settings".to_string(),
        Some(json!({
            "core_log_level": core_log_level,
            "model_plane_host": model_plane_host,
            "model_plane_port": model_plane_port,
            "artifact_cache_backend": artifact_cache_backend,
            "artifact_cache_root": artifact_cache_root,
            "artifact_cache_max_bytes": artifact_cache_max_bytes,
            "artifact_cache_eviction_policy": artifact_cache_eviction_policy,
            "tool_override_command": tool_override_command,
            "tool_override_path": tool_override_path,
        })),
    )
    .await
}

#[tauri::command]
pub async fn desktop_update_settings(
    state: State<'_, SharedHostState>,
    core_log_level: Option<String>,
    model_plane_host: Option<String>,
    model_plane_port: Option<u16>,
    artifact_cache_backend: Option<String>,
    artifact_cache_root: Option<String>,
    artifact_cache_max_bytes: Option<u64>,
    artifact_cache_eviction_policy: Option<String>,
    tool_override_command: Option<String>,
    tool_override_path: Option<String>,
) -> Result<Value, String> {
    update_settings(
        state,
        core_log_level,
        model_plane_host,
        model_plane_port,
        artifact_cache_backend,
        artifact_cache_root,
        artifact_cache_max_bytes,
        artifact_cache_eviction_policy,
        tool_override_command,
        tool_override_path,
    )
    .await
}

#[tauri::command]
pub async fn trusted_update_settings(
    state: State<'_, SharedHostState>,
    core_log_level: Option<String>,
    model_plane_host: Option<String>,
    model_plane_port: Option<u16>,
    artifact_cache_backend: Option<String>,
    artifact_cache_root: Option<String>,
    artifact_cache_max_bytes: Option<u64>,
    artifact_cache_eviction_policy: Option<String>,
    tool_override_command: Option<String>,
    tool_override_path: Option<String>,
) -> Result<Value, String> {
    desktop_update_settings(
        state,
        core_log_level,
        model_plane_host,
        model_plane_port,
        artifact_cache_backend,
        artifact_cache_root,
        artifact_cache_max_bytes,
        artifact_cache_eviction_policy,
        tool_override_command,
        tool_override_path,
    )
    .await
}

#[tauri::command]
pub async fn list_agent_tokens(state: State<'_, SharedHostState>) -> Result<Value, String> {
    workspace_bridge_command(state, "list_agent_tokens".to_string(), Some(json!({}))).await
}

#[tauri::command]
pub async fn desktop_list_agent_tokens(state: State<'_, SharedHostState>) -> Result<Value, String> {
    list_agent_tokens(state).await
}

#[tauri::command]
pub async fn trusted_list_agent_tokens(state: State<'_, SharedHostState>) -> Result<Value, String> {
    list_agent_tokens(state).await
}

#[tauri::command]
pub async fn revoke_agent_token(
    state: State<'_, SharedHostState>,
    token_id: String,
    reason: Option<String>,
) -> Result<Value, String> {
    workspace_bridge_command(
        state,
        "revoke_agent_token".to_string(),
        Some(json!({ "token_id": token_id, "reason": reason })),
    )
    .await
}

#[tauri::command]
pub async fn desktop_revoke_agent_token(
    state: State<'_, SharedHostState>,
    token_id: String,
    reason: Option<String>,
) -> Result<Value, String> {
    revoke_agent_token(state, token_id, reason).await
}

#[tauri::command]
pub async fn trusted_revoke_agent_token(
    state: State<'_, SharedHostState>,
    token_id: String,
    reason: Option<String>,
) -> Result<Value, String> {
    revoke_agent_token(state, token_id, reason).await
}

#[tauri::command]
pub async fn create_agent_token(
    state: State<'_, SharedHostState>,
    label: String,
    expires_in_seconds: Option<u64>,
    scope: Option<Value>,
    attestation_id: Option<String>,
) -> Result<Value, String> {
    workspace_bridge_command(
        state,
        "create_agent_token".to_string(),
        Some(json!({
            "label": label,
            "expires_in_seconds": expires_in_seconds,
            "scope": scope,
            "attestation_id": attestation_id,
        })),
    )
    .await
}

#[tauri::command]
pub async fn desktop_create_agent_token(
    state: State<'_, SharedHostState>,
    label: String,
    expires_in_seconds: Option<u64>,
    scope: Option<Value>,
    attestation_id: Option<String>,
) -> Result<Value, String> {
    create_agent_token(state, label, expires_in_seconds, scope, attestation_id).await
}

#[tauri::command]
pub async fn trusted_create_agent_token(
    state: State<'_, SharedHostState>,
    label: String,
    expires_in_seconds: Option<u64>,
    scope: Option<Value>,
    attestation_id: Option<String>,
) -> Result<Value, String> {
    create_agent_token(state, label, expires_in_seconds, scope, attestation_id).await
}

#[tauri::command]
pub async fn read_artifact(
    state: State<'_, SharedHostState>,
    artifact_id: String,
    offset: Option<u64>,
    limit: Option<u64>,
) -> Result<Value, String> {
    workspace_bridge_command(
        state,
        "read_artifact".to_string(),
        Some(json!({
            "artifact_id": artifact_id,
            "offset": offset,
            "limit": limit,
        })),
    )
    .await
}

#[tauri::command]
pub async fn desktop_read_artifact(
    state: State<'_, SharedHostState>,
    artifact_id: String,
    offset: Option<u64>,
    limit: Option<u64>,
) -> Result<Value, String> {
    read_artifact(state, artifact_id, offset, limit).await
}

#[tauri::command]
pub async fn trusted_read_artifact(
    state: State<'_, SharedHostState>,
    artifact_id: String,
    offset: Option<u64>,
    limit: Option<u64>,
) -> Result<Value, String> {
    read_artifact(state, artifact_id, offset, limit).await
}

#[tauri::command]
pub async fn upsert_profile(
    state: State<'_, SharedHostState>,
    profile: Value,
) -> Result<Value, String> {
    workspace_bridge_command(
        state,
        "upsert_profile".to_string(),
        Some(json!({ "profile": profile })),
    )
    .await
}

#[tauri::command]
pub async fn desktop_upsert_profile(
    state: State<'_, SharedHostState>,
    profile: Value,
) -> Result<Value, String> {
    upsert_profile(state, profile).await
}

#[tauri::command]
pub async fn trusted_upsert_profile(
    state: State<'_, SharedHostState>,
    profile: Value,
) -> Result<Value, String> {
    upsert_profile(state, profile).await
}

#[tauri::command]
pub async fn unlock_vault(
    state: State<'_, SharedHostState>,
    trigger: Option<String>,
) -> Result<Value, String> {
    workspace_bridge_command(
        state,
        "unlock_vault".to_string(),
        Some(json!({ "trigger": trigger })),
    )
    .await
}

#[tauri::command]
pub async fn desktop_unlock_vault(
    state: State<'_, SharedHostState>,
    trigger: Option<String>,
) -> Result<Value, String> {
    unlock_vault(state, trigger).await
}

#[tauri::command]
pub async fn trusted_unlock_vault(
    state: State<'_, SharedHostState>,
    trigger: Option<String>,
) -> Result<Value, String> {
    unlock_vault(state, trigger).await
}
