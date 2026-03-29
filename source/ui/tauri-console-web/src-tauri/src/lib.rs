mod app_state;
mod bridge_transport;
mod commands;
mod sidecar;
mod windowing;

use std::sync::{Arc, Mutex};

use app_state::AppRuntimeState;
use tauri::Manager;

pub struct SharedHostState(pub Arc<Mutex<AppRuntimeState>>);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("bridgingio-tauri-console"));
            let prefs_dir = app_data_dir.join("prefs");
            std::fs::create_dir_all(&prefs_dir)?;

            let state = AppRuntimeState::new(
                prefs_dir.join("runtime-root.cfg"),
                sidecar::resolve_core_sidecar(),
            )
            .map_err(std::io::Error::other)?;
            app.manage(SharedHostState(Arc::new(Mutex::new(state))));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_startup_state,
            commands::pick_runtime_root_dir,
            commands::validate_runtime_root,
            commands::complete_onboarding_runtime_root,
            commands::open_main_window,
            commands::focus_main_window,
            commands::request_local_user_verification,
            commands::trusted_local_user_verification,
            commands::workspace_bridge_command,
            commands::desktop_bridge_command,
            commands::get_bootstrap_state,
            commands::desktop_get_bootstrap_state,
            commands::trusted_get_bootstrap_state,
            commands::get_settings,
            commands::desktop_get_settings,
            commands::trusted_get_settings,
            commands::update_settings,
            commands::desktop_update_settings,
            commands::trusted_update_settings,
            commands::list_agent_tokens,
            commands::desktop_list_agent_tokens,
            commands::trusted_list_agent_tokens,
            commands::revoke_agent_token,
            commands::desktop_revoke_agent_token,
            commands::trusted_revoke_agent_token,
            commands::create_agent_token,
            commands::desktop_create_agent_token,
            commands::trusted_create_agent_token,
            commands::read_artifact,
            commands::desktop_read_artifact,
            commands::trusted_read_artifact,
            commands::upsert_profile,
            commands::desktop_upsert_profile,
            commands::trusted_upsert_profile,
            commands::unlock_vault,
            commands::desktop_unlock_vault,
            commands::trusted_unlock_vault
        ])
        .build(tauri::generate_context!())
        .expect("failed to build BridgingIO desktop shell host");

    let exit_code = app.run_return(|app_handle, event| {
        if matches!(
            event,
            tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
        ) {
            if let Some(shared) = app_handle.try_state::<SharedHostState>() {
                if let Ok(mut state) = shared.0.lock() {
                    state.shutdown_for_app_exit();
                }
            }
        }
    });
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
}
