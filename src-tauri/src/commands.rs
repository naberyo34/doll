use std::path::PathBuf;
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem};
use tauri::{AppHandle, Manager};

use crate::config;
use crate::server::{AppState, OpenClawStatus};
use crate::skin::{self, SkinInfo};

/// Remote OpenClaw Gateway connection info.
///
/// When `url` is empty, doll runs in local mode (launch `openclaw` CLI).
/// When set, doll uses the Gateway `/v1/responses` HTTP API.
pub(crate) struct OpenClawRemote {
    pub(crate) url: String,
    pub(crate) token: String,
}

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

/// Sends a user message to the OpenClaw agent.
///
/// In **local mode** (no Gateway URL configured), spawns `openclaw agent`
/// via the CLI.  In **remote mode**, calls the Gateway `/v1/responses`
/// endpoint over HTTP.
///
/// Thinking expression + TTS are triggered directly through [`AppState`]
/// before the agent call.
#[tauri::command]
pub async fn send_message(
    text: String,
    agent_name: tauri::State<'_, Arc<String>>,
    remote: tauri::State<'_, OpenClawRemote>,
    app_state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    if let Err(e) = app_state.process_status(&OpenClawStatus::thinking()).await {
        log::warn!("Failed to emit thinking status: {e}");
    }

    if remote.url.is_empty() {
        send_message_local(&text, &agent_name).await?;
    } else {
        send_message_gateway(&text, &remote.url, &remote.token, &agent_name).await?;
    }

    log::info!("Sent message to OpenClaw: {text}");
    Ok(())
}

/// Sends a message by spawning the local `openclaw` CLI.
async fn send_message_local(text: &str, agent_name: &str) -> Result<(), String> {
    let mut cmd = tokio::process::Command::new("openclaw");
    cmd.args(["agent", "--message", text]);
    if !agent_name.is_empty() {
        cmd.args(["--agent", agent_name]);
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
    Ok(())
}

/// Sends a message to the OpenClaw Gateway via `POST /v1/responses`.
async fn send_message_gateway(
    text: &str,
    base_url: &str,
    token: &str,
    agent: &str,
) -> Result<(), String> {
    let url = format!("{}/v1/responses", base_url.trim_end_matches('/'));

    let mut req = reqwest::Client::new()
        .post(&url)
        .header("content-type", "application/json");

    if !token.is_empty() {
        req = req.bearer_auth(token);
    }
    if !agent.is_empty() {
        req = req.header("x-openclaw-agent-id", agent);
    }

    let body = serde_json::json!({
        "input": text,
    });

    let resp = req
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Gateway request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp
            .text()
            .await
            .unwrap_or_else(|_| "(no body)".to_string());
        return Err(format!("Gateway returned {status}: {body}"));
    }

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
