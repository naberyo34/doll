mod config;
mod voisona;

use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use config::VoisonaConfig;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
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

    // Emit event to frontend (send only status + emotion, not the full text).
    if let Err(e) = state.app.emit("openclaw-status", &payload) {
        log::warn!("Failed to emit status event: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    // Fire-and-forget TTS when text is present and VoiSona is configured.
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

/// Starts the local HTTP server that OpenClaw pushes status updates to.
///
/// The server binds to `127.0.0.1:DEFAULT_PORT` and exposes a single
/// endpoint: `POST /status`.
async fn http_server(app: AppHandle, voisona: Arc<VoisonaConfig>) {
    let state = AppState { app, voisona };
    let router = Router::new()
        .route("/status", post(handle_status))
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

/// Opens `~/.config/doll/config.toml` in the user's default text editor.
/// Creates the file with a commented-out template when it does not yet exist.
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

    std::process::Command::new("open")
        .arg("-t")
        .arg(&path)
        .spawn()
        .map_err(|e| format!("Failed to open editor: {e}"))?;

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

    if voisona_cfg.enabled {
        log::info!("VoiSona TTS enabled (port {})", voisona_cfg.port);
    } else {
        log::info!("VoiSona TTS disabled");
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .invoke_handler(tauri::generate_handler![open_config_file])
        .setup(move |app| {
            let handle = app.handle().clone();
            let cfg = Arc::clone(&voisona_cfg);
            tauri::async_runtime::spawn(http_server(handle, cfg));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
