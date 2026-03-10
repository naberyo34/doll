mod config;
mod skin;
mod voisona;

use axum::{extract::State, http::StatusCode, routing, Json, Router};
use rand::prelude::IndexedRandom;
use serde::{Deserialize, Serialize};
use skin::SkinInfo;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use voisona::VoisonaClient;

/// Status payload sent by the OpenClaw agent and forwarded to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenClawStatus {
    status: AgentStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    emotion: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
}

/// The lifecycle state reported by the OpenClaw agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum AgentStatus {
    Idle,
    Responding,
    /// Catch-all for unknown values so unrecognised strings don't fail
    /// deserialization. Serializes as `"unknown"`.
    #[serde(other)]
    Unknown,
}

/// Port on which doll listens for status updates from OpenClaw.
const DEFAULT_PORT: u16 = 3000;

/// Interval between repeated thinking TTS phrases.
const THINKING_PHRASE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

/// Shared state passed to every axum handler.
#[derive(Clone)]
struct AppState {
    app: AppHandle,
    tts: Option<Arc<VoisonaClient>>,
    skin_info: Arc<SkinInfo>,
    /// Pre-serialised JSON for `GET /emotions` to avoid repeated cloning.
    emotions_json: Arc<[u8]>,
    /// Handle to the background thinking-phrase loop, aborted when a
    /// non-thinking status arrives.
    thinking_task: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
}

// ---------------------------------------------------------------------------
// HTTP server (receives status from OpenClaw)
// ---------------------------------------------------------------------------

/// Handles `POST /status` — parses the JSON body, emits a Tauri event so the
/// React frontend can update the mascot expression, and optionally triggers
/// VoiSona Talk TTS when `text` is present.
///
/// When `emotion` is `"thinking"` and no explicit `text` is given, a
/// background loop is started that speaks a random thinking phrase every
/// [`THINKING_PHRASE_INTERVAL`]. The loop is cancelled as soon as any
/// non-thinking status arrives.
async fn handle_status(
    State(state): State<AppState>,
    Json(payload): Json<OpenClawStatus>,
) -> StatusCode {
    log::info!("Received status: {:?}", payload);

    if let Err(e) = state.app.emit("openclaw-status", &payload) {
        log::warn!("Failed to emit status event: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    let is_thinking = payload.emotion.as_deref() == Some("thinking")
        && payload.text.as_deref().is_none_or(|t| t.is_empty());

    // Cancel any running thinking loop when a non-thinking status arrives.
    if !is_thinking {
        let mut guard = state.thinking_task.lock().await;
        if let Some(handle) = guard.take() {
            handle.abort();
        }
    }

    if is_thinking {
        // Only start a new loop if one isn't already running.
        let mut guard = state.thinking_task.lock().await;
        if guard.is_none() {
            let s = state.clone();
            let handle = tauri::async_runtime::spawn(thinking_phrase_loop(s));
            *guard = Some(handle);
        }
    } else {
        spawn_tts_for_payload(&state, &payload);
    }

    StatusCode::OK
}

/// Speaks a random thinking phrase immediately, then repeats every
/// [`THINKING_PHRASE_INTERVAL`] until the task is aborted.
async fn thinking_phrase_loop(state: AppState) {
    loop {
        speak_random_thinking_phrase(&state);
        tokio::time::sleep(THINKING_PHRASE_INTERVAL).await;
    }
}

/// Picks a random thinking phrase and spawns a TTS task for it.
fn speak_random_thinking_phrase(state: &AppState) {
    let Some(ref tts) = state.tts else { return };
    let Some(text) = state
        .skin_info
        .thinking_phrases
        .choose(&mut rand::rng())
        .cloned()
    else {
        return;
    };
    let tts = Arc::clone(tts);
    let voice = state.skin_info.voice.clone();
    let style_weights = state
        .skin_info
        .emotions
        .iter()
        .find(|e| e.name == "thinking")
        .and_then(|e| e.style_weights.clone());
    tauri::async_runtime::spawn(async move {
        tts.synthesize(&text, voice.as_ref(), style_weights.as_deref())
            .await;
    });
}

/// Spawns a one-shot TTS task for a normal (non-thinking) status payload.
fn spawn_tts_for_payload(state: &AppState, payload: &OpenClawStatus) {
    let text = match payload.text.as_deref() {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => return,
    };
    let Some(ref tts) = state.tts else { return };
    let tts = Arc::clone(tts);
    let voice = state.skin_info.voice.clone();
    let style_weights = payload.emotion.as_ref().and_then(|emo| {
        state
            .skin_info
            .emotions
            .iter()
            .find(|e| e.name == *emo)
            .and_then(|e| e.style_weights.clone())
    });
    tauri::async_runtime::spawn(async move {
        tts.synthesize(&text, voice.as_ref(), style_weights.as_deref())
            .await;
    });
}

/// Handles `GET /emotions` — returns the pre-serialised JSON list of available
/// emotions for the currently active skin.
async fn handle_emotions(
    State(state): State<AppState>,
) -> (StatusCode, [(&'static str, &'static str); 1], Vec<u8>) {
    (
        StatusCode::OK,
        [("content-type", "application/json")],
        state.emotions_json.to_vec(),
    )
}

/// Starts the local HTTP server that OpenClaw pushes status updates to.
async fn http_server(state: AppState) {
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
    let bytes =
        std::fs::read(&path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// Exits the application.
#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
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

// ---------------------------------------------------------------------------
// App entry-point
// ---------------------------------------------------------------------------

/// Builds and runs the Tauri application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_config = config::load_config();
    let skin_name = app_config.skin;

    let tts = if app_config.voisona.enabled {
        log::info!("VoiSona TTS enabled (port {})", app_config.voisona.port);
        match VoisonaClient::new(app_config.voisona) {
            Ok(client) => Some(Arc::new(client)),
            Err(e) => {
                log::warn!("Failed to initialise VoiSona client: {e}");
                None
            }
        }
    } else {
        log::info!("VoiSona TTS disabled");
        None
    };

    let skins_dir = config::skins_dir().expect("Cannot determine skins directory");
    let _ = std::fs::create_dir_all(&skins_dir);
    let skins_dir = Arc::new(skins_dir);

    log::info!("Active skin: {}", skin_name);

    let setup_skins_dir = Arc::clone(&skins_dir);
    let setup_skin_name = skin_name.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            open_config_file,
            quit_app,
            get_skin_info,
            get_skin_image,
        ])
        .setup(move |app| {
            let resource_dir = app
                .path()
                .resource_dir()
                .expect("Cannot determine resource directory");
            skin::install_bundled_skins(&resource_dir, &setup_skins_dir);

            let skin_info =
                skin::discover_skin(&setup_skins_dir, &setup_skin_name).unwrap_or_else(|| {
                    log::warn!("Skin '{}' not found; using empty fallback", setup_skin_name);
                    SkinInfo {
                        name: setup_skin_name.clone(),
                        display_name: setup_skin_name.clone(),
                        emotions: Vec::new(),
                        voice: None,
                        thinking_phrases: Vec::new(),
                    }
                });
            log::info!(
                "Skin '{}' loaded with {} emotions",
                skin_info.display_name,
                skin_info.emotions.len()
            );

            let agent_emotions: Vec<_> = skin_info
                .emotions
                .iter()
                .filter(|e| e.name != "thinking")
                .collect();
            let emotions_json: Arc<[u8]> = serde_json::to_vec(&agent_emotions)
                .unwrap_or_default()
                .into();

            let skin_info = Arc::new(skin_info);

            app.manage(Arc::clone(&skin_info));
            app.manage(Arc::clone(&setup_skins_dir));

            let state = AppState {
                app: app.handle().clone(),
                tts,
                skin_info,
                emotions_json,
                thinking_task: Arc::new(Mutex::new(None)),
            };
            tauri::async_runtime::spawn(http_server(state));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
