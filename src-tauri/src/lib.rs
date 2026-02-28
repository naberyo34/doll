use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::net::TcpListener;

/// Status payload sent by the OpenClaw agent and forwarded to the frontend.
///
/// - `status`: `"idle"` or `"responding"`
/// - `emotion`: present when `status == "responding"`
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenClawStatus {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    emotion: Option<String>,
}

/// Port on which doll listens for status updates from OpenClaw.
const DEFAULT_PORT: u16 = 3000;

// ---------------------------------------------------------------------------
// HTTP server (receives status from OpenClaw)
// ---------------------------------------------------------------------------

/// Handles `POST /status` — parses the JSON body and emits a Tauri event
/// so the React frontend can update the mascot expression.
async fn handle_status(
    State(app): State<AppHandle>,
    Json(payload): Json<OpenClawStatus>,
) -> StatusCode {
    log::info!("Received status: {:?}", payload);
    match app.emit("openclaw-status", &payload) {
        Ok(()) => StatusCode::OK,
        Err(e) => {
            log::warn!("Failed to emit status event: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

/// Starts the local HTTP server that OpenClaw pushes status updates to.
///
/// The server binds to `127.0.0.1:DEFAULT_PORT` and exposes a single
/// endpoint: `POST /status`.
async fn http_server(app: AppHandle) {
    let router = Router::new()
        .route("/status", post(handle_status))
        .with_state(app);

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

/// Debug command: manually push a status event from the frontend (for testing
/// without a running OpenClaw instance).
#[tauri::command]
fn set_mock_status(app: AppHandle, status: String, emotion: Option<String>) -> Result<(), String> {
    let payload = OpenClawStatus { status, emotion };
    app.emit("openclaw-status", &payload)
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// App entry-point
// ---------------------------------------------------------------------------

/// Builds and runs the Tauri application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .invoke_handler(tauri::generate_handler![set_mock_status])
        .setup(|app| {
            // Spawn the HTTP server as a background async task.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(http_server(handle));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
