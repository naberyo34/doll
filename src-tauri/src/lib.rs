mod config;
mod skin;
mod voisona;

use axum::{extract::State, http::StatusCode, routing, Json, Router};
use rand::prelude::IndexedRandom;
use serde::{Deserialize, Serialize};
use skin::SkinInfo;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
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
async fn http_server(state: AppState, port: u16) {
    let router = Router::new()
        .route("/status", routing::post(handle_status))
        .route("/emotions", routing::get(handle_emotions))
        .with_state(state);

    let addr = format!("127.0.0.1:{port}");
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

/// Shows a native context menu with app actions.
#[tauri::command]
async fn show_context_menu(window: tauri::WebviewWindow) -> Result<(), String> {
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

// ---------------------------------------------------------------------------
// OpenClaw integration installer
// ---------------------------------------------------------------------------

/// Copies bundled Skill and Hook files to `~/.openclaw/` and updates
/// `openclaw.json` to enable them.
fn install_openclaw(resource_dir: &Path) -> Result<(), String> {
    install_openclaw_files(resource_dir)?;
    update_openclaw_config()?;
    log::info!("OpenClaw integration installed successfully");
    Ok(())
}

/// Copies `skills/doll/` and `hooks/doll-notify/` into `~/.openclaw/`.
fn install_openclaw_files(resource_dir: &Path) -> Result<(), String> {
    let openclaw_dir = dirs::home_dir()
        .ok_or("Cannot determine home directory")?
        .join(".openclaw");

    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default();

    let skill_candidates = [
        resource_dir.join("skills").join("doll"),
        project_root.join("skills").join("doll"),
    ];
    let hook_candidates = [
        resource_dir.join("hooks").join("doll-notify"),
        project_root.join("hooks").join("doll-notify"),
    ];

    if let Some(src) = skill_candidates.iter().find(|p| p.is_dir()) {
        let dest = openclaw_dir.join("skills").join("doll");
        skin::copy_dir_recursive(src, &dest).map_err(|e| format!("Failed to copy skill: {e}"))?;
        log::info!("Installed skill: {}", dest.display());
    } else {
        return Err("Bundled skill not found".to_string());
    }

    if let Some(src) = hook_candidates.iter().find(|p| p.is_dir()) {
        let dest = openclaw_dir.join("hooks").join("doll-notify");
        skin::copy_dir_recursive(src, &dest).map_err(|e| format!("Failed to copy hook: {e}"))?;
        log::info!("Installed hook: {}", dest.display());
    } else {
        return Err("Bundled hook not found".to_string());
    }

    Ok(())
}

/// Merges doll entries into `~/.openclaw/openclaw.json`, preserving existing
/// settings.
fn update_openclaw_config() -> Result<(), String> {
    let path = dirs::home_dir()
        .ok_or("Cannot determine home directory")?
        .join(".openclaw")
        .join("openclaw.json");

    let mut root: serde_json::Value = if path.exists() {
        let contents = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str(&contents)
            .map_err(|e| format!("Failed to parse openclaw.json: {e}"))?
    } else {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        serde_json::json!({})
    };

    let obj = root
        .as_object_mut()
        .ok_or("openclaw.json is not an object")?;

    // skills.entries.doll.enabled = true
    obj.entry("skills")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or("skills is not an object")?
        .entry("entries")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or("skills.entries is not an object")?
        .entry("doll")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or("skills.entries.doll is not an object")?
        .entry("enabled")
        .or_insert(serde_json::json!(true));

    // hooks.internal.enabled = true, hooks.internal.entries.doll-notify.enabled = true
    let internal = obj
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or("hooks is not an object")?
        .entry("internal")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or("hooks.internal is not an object")?;

    internal.entry("enabled").or_insert(serde_json::json!(true));

    internal
        .entry("entries")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or("hooks.internal.entries is not an object")?
        .entry("doll-notify")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or("hooks.internal.entries.doll-notify is not an object")?
        .entry("enabled")
        .or_insert(serde_json::json!(true));

    let json = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    log::info!("Updated openclaw.json at {}", path.display());

    Ok(())
}

/// Exits the application.
fn quit_app(app: &AppHandle) {
    app.exit(0);
}

/// Opens `~/.config/doll/config.toml` in the user's default text editor.
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
    let port = app_config.port;

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
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            show_context_menu,
            get_skin_info,
            get_skin_image,
        ])
        .on_menu_event(|app, event| match event.id().as_ref() {
            "install_openclaw" => {
                let resource_dir = app
                    .path()
                    .resource_dir()
                    .expect("Cannot determine resource directory");
                match install_openclaw(&resource_dir) {
                    Ok(()) => {
                        app.dialog()
                            .message("OpenClaw 連携をインストールしました。\nGateway を再起動すると反映されます。")
                            .title("doll")
                            .blocking_show();
                    }
                    Err(e) => {
                        log::warn!("Failed to install OpenClaw integration: {e}");
                        app.dialog()
                            .message(format!("インストールに失敗しました:\n{e}"))
                            .title("doll")
                            .blocking_show();
                    }
                }
            }
            "open_config" => {
                if let Err(e) = open_config_file() {
                    log::warn!("Failed to open config: {e}");
                }
            }
            "quit" => quit_app(app),
            _ => {}
        })
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
            tauri::async_runtime::spawn(http_server(state, port));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
