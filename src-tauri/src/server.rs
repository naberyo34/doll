use axum::{extract::State, http::StatusCode, routing, Json, Router};
use rand::prelude::IndexedRandom;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use crate::skin::SkinInfo;
use crate::voisona::VoisonaClient;

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
pub(crate) struct AppState {
    app: AppHandle,
    tts: Option<Arc<VoisonaClient>>,
    skin_info: Arc<SkinInfo>,
    /// Pre-serialised JSON for `GET /emotions` to avoid repeated cloning.
    emotions_json: Arc<[u8]>,
    /// Handle to the background thinking-phrase loop, aborted when a
    /// non-thinking status arrives.
    thinking_task: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
}

impl AppState {
    /// Creates a new `AppState` with no active thinking task.
    pub(crate) fn new(
        app: AppHandle,
        tts: Option<Arc<VoisonaClient>>,
        skin_info: Arc<SkinInfo>,
        emotions_json: Arc<[u8]>,
    ) -> Self {
        Self {
            app,
            tts,
            skin_info,
            emotions_json,
            thinking_task: Arc::new(Mutex::new(None)),
        }
    }
}

// ---------------------------------------------------------------------------
// Handlers
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

// ---------------------------------------------------------------------------
// TTS helpers
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Server entry-point
// ---------------------------------------------------------------------------

/// Starts the local HTTP server that OpenClaw pushes status updates to.
pub(crate) async fn http_server(state: AppState, port: u16) {
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
