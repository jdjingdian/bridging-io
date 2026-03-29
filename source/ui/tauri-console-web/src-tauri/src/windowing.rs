use tauri::{AppHandle, Manager};

pub fn open_or_focus_main_window(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window is not registered".to_string())?;
    window
        .show()
        .map_err(|err| format!("show main window failed: {err}"))?;
    window
        .set_focus()
        .map_err(|err| format!("focus main window failed: {err}"))?;
    Ok(())
}
