mod config;
mod skin;
mod voisona;

use axum::{extract::State, http::StatusCode, routing, Json, Router};
use config::VoisonaConfig;
use serde::{Deserialize, Serialize};
use skin::SkinInfo;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::net::TcpListener;

/// Status payload sent by the OpenClaw agent and forwarded to the frontend.
///
/// - `status`: `"idle"` or `"responding"`
/// - `emotion`: present when `status == "responding"`
/// - `text`: optional reply text to be spoken via VoiSona Talk TTS
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenClawStatus {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    emotion: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
}

/// Port on which doll listens for status updates from OpenClaw.
const DEFAULT_PORT: u16 = 3000;

/// Shared state passed to every axum handler.
#[derive(Clone)]
struct AppState {
    app: AppHandle,
    voisona: Arc<VoisonaConfig>,
    skin_info: Arc<SkinInfo>,
}

// ---------------------------------------------------------------------------
// HTTP server (receives status from OpenClaw)
// ---------------------------------------------------------------------------

/// Handles `POST /status` — parses the JSON body, emits a Tauri event so the
/// React frontend can update the mascot expression, and optionally triggers
/// VoiSona Talk TTS when `text` is present.
async fn handle_status(
    State(state): State<AppState>,
    Json(payload): Json<OpenClawStatus>,
) -> StatusCode {
    log::info!("Received status: {:?}", payload);

    if let Err(e) = state.app.emit("openclaw-status", &payload) {
        log::warn!("Failed to emit status event: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    if let Some(ref text) = payload.text {
        if state.voisona.enabled && !text.is_empty() {
            let text = text.clone();
            let cfg = Arc::clone(&state.voisona);
            tauri::async_runtime::spawn(async move {
                voisona::synthesize(&text, &cfg).await;
            });
        }
    }

    StatusCode::OK
}

/// Handles `GET /emotions` — returns the cached list of available emotions
/// (with descriptions) for the currently active skin.
async fn handle_emotions(State(state): State<AppState>) -> Json<Vec<skin::EmotionEntry>> {
    Json(state.skin_info.emotions.clone())
}

/// Starts the local HTTP server that OpenClaw pushes status updates to.
async fn http_server(app: AppHandle, voisona: Arc<VoisonaConfig>, skin_info: Arc<SkinInfo>) {
    let state = AppState {
        app,
        voisona,
        skin_info,
    };
    let router = Router::new()
        .route("/status", routing::post(handle_status))
        .route("/emotions", routing::get(handle_emotions))
        .with_state(state);

    let addr = format!("127.0.0.1:{DEFAULT_PORT}");
    log::info!("doll HTTP server listening on {addr}");

    match TcpListener::bind(&addr).await {
        Ok(listener) => {
            if let Err(e) = axum::serve(listener, router).await {
                log::warn!("HTTP server error: {e}");
            }
        }
        Err(e) => {
            log::warn!("Failed to bind HTTP server on {addr}: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Returns cached information about the currently active skin.
#[tauri::command]
fn get_skin_info(skin_info: tauri::State<'_, Arc<SkinInfo>>) -> SkinInfo {
    skin_info.as_ref().clone()
}

/// Returns the raw PNG bytes for an emotion of the active skin.
#[tauri::command]
fn get_skin_image(
    emotion: String,
    skin_info: tauri::State<'_, Arc<SkinInfo>>,
    skins_dir: tauri::State<'_, Arc<PathBuf>>,
) -> Result<tauri::ipc::Response, String> {
    let path = skin::resolve_image_path(&skins_dir, &skin_info.name, &emotion);
    let bytes = std::fs::read(&path)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// Opens `~/.config/doll/config.toml` in the user's default text editor.
#[tauri::command]
fn open_config_file() -> Result<(), String> {
    let path = config::config_path().ok_or("Cannot determine config directory")?;

    if !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, config::DEFAULT_TEMPLATE).map_err(|e| e.to_string())?;
        log::info!("Created default config at {}", path.display());
    }

    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("open")
        .arg("-t")
        .arg(&path)
        .spawn();
    #[cfg(target_os = "linux")]
    let status = std::process::Command::new("xdg-open").arg(&path).spawn();
    #[cfg(target_os = "windows")]
    let status = std::process::Command::new("cmd")
        .args(["/c", "start", ""])
        .arg(&path)
        .spawn();

    status.map_err(|e| format!("Failed to open editor: {e}"))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// App entry-point
// ---------------------------------------------------------------------------

/// Builds and runs the Tauri application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_config = config::load_config();
    let voisona_cfg = Arc::new(app_config.voisona);
    let skin_name = app_config.skin;

    let skins_dir = config::skins_dir().expect("Cannot determine skins directory");
    let _ = std::fs::create_dir_all(&skins_dir);
    let skins_dir = Arc::new(skins_dir);

    if voisona_cfg.enabled {
        log::info!("VoiSona TTS enabled (port {})", voisona_cfg.port);
    } else {
        log::info!("VoiSona TTS disabled");
    }
    log::info!("Active skin: {}", skin_name);

    let setup_skins_dir = Arc::clone(&skins_dir);
    let setup_skin_name = skin_name.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            open_config_file,
            get_skin_info,
            get_skin_image,
        ])
        .setup(move |app| {
            let resource_dir = app
                .path()
                .resource_dir()
                .expect("Cannot determine resource directory");
            skin::install_bundled_skins(&resource_dir, &setup_skins_dir);

            let skin_info = skin::discover_skin(&setup_skins_dir, &setup_skin_name)
                .unwrap_or_else(|| {
                    log::warn!(
                        "Skin '{}' not found; using empty fallback",
                        setup_skin_name
                    );
                    SkinInfo {
                        name: setup_skin_name.clone(),
                        display_name: setup_skin_name.clone(),
                        emotions: Vec::new(),
                    }
                });
            log::info!(
                "Skin '{}' loaded with {} emotions",
                skin_info.display_name,
                skin_info.emotions.len()
            );
            let skin_info = Arc::new(skin_info);

            app.manage(Arc::clone(&skin_info));
            app.manage(Arc::clone(&setup_skins_dir));

            let handle = app.handle().clone();
            let cfg = Arc::clone(&voisona_cfg);
            let si = Arc::clone(&skin_info);
            tauri::async_runtime::spawn(http_server(handle, cfg, si));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
