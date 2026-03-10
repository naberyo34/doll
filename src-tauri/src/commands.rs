use std::path::PathBuf;
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem};
use tauri::{AppHandle, Manager};

use crate::config;
use crate::skin::{self, SkinInfo};

/// The port the local HTTP server is listening on.
///
/// Wrapped in a newtype so it can be registered as Tauri managed state
/// without conflicting with other `u16` values.
pub(crate) struct ServerPort(pub(crate) u16);

/// Returns cached information about the currently active skin.
#[tauri::command]
pub fn get_skin_info(skin_info: tauri::State<'_, Arc<SkinInfo>>) -> SkinInfo {
    skin_info.as_ref().clone()
}

/// Returns the raw PNG bytes for an emotion of the active skin.
#[tauri::command]
pub fn get_skin_image(
    emotion: String,
    skin_info: tauri::State<'_, Arc<SkinInfo>>,
    skins_dir: tauri::State<'_, Arc<PathBuf>>,
) -> Result<tauri::ipc::Response, String> {
    let path = skin::resolve_image_path(&skins_dir, &skin_info.name, &emotion);
    let bytes =
        std::fs::read(&path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// Shows a native context menu with app actions.
#[tauri::command]
pub async fn show_context_menu(window: tauri::WebviewWindow) -> Result<(), String> {
    let app = window.app_handle();
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(
                app,
                "install_openclaw",
                "OpenClaw 連携をインストール",
                true,
                None::<&str>,
            )
            .map_err(|e| e.to_string())?,
            &MenuItem::with_id(app, "open_config", "設定ファイルを開く", true, None::<&str>)
                .map_err(|e| e.to_string())?,
            &MenuItem::with_id(app, "quit", "終了する", true, None::<&str>)
                .map_err(|e| e.to_string())?,
        ],
    )
    .map_err(|e| e.to_string())?;
    window.popup_menu(&menu).map_err(|e| e.to_string())
}

/// Sends a user message to the OpenClaw agent by spawning `openclaw agent`.
/// Triggers thinking expression + TTS by posting to our own HTTP endpoint
/// before the call, then lets the agent's Skill response (via HTTP
/// `POST /status`) update the emotion.
#[tauri::command]
pub async fn send_message(
    text: String,
    agent_name: tauri::State<'_, Arc<String>>,
    server_port: tauri::State<'_, ServerPort>,
) -> Result<(), String> {
    let url = format!("http://127.0.0.1:{}/status", server_port.0);
    let _ = reqwest::Client::new()
        .post(&url)
        .json(&serde_json::json!({"status": "responding", "emotion": "thinking"}))
        .send()
        .await;

    let mut cmd = tokio::process::Command::new("openclaw");
    cmd.args(["agent", "--message", &text]);
    if !agent_name.is_empty() {
        cmd.args(["--agent", &agent_name]);
    }

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to run openclaw: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::warn!("openclaw agent failed: {stderr}");
        return Err(format!("openclaw agent failed: {stderr}"));
    }

    log::info!("Sent message to OpenClaw: {text}");
    Ok(())
}

/// Exits the application.
pub(crate) fn quit_app(app: &AppHandle) {
    app.exit(0);
}

/// Opens `~/.config/doll/config.toml` in the user's default text editor.
pub(crate) fn open_config_file() -> Result<(), String> {
    let path = config::config_path().ok_or("Cannot determine config directory")?;

    if !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, config::DEFAULT_TEMPLATE).map_err(|e| e.to_string())?;
        log::info!("Created default config at {}", path.display());
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-t")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open editor: {e}"))?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open editor: {e}"))?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/c", "start", ""])
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open editor: {e}"))?;
    }

    Ok(())
}
