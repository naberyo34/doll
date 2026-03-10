mod commands;
mod config;
mod openclaw;
mod server;
mod skin;
mod voisona;

use commands::ServerPort;
use skin::SkinInfo;
use std::sync::Arc;
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;
use voisona::VoisonaClient;

/// Builds and runs the Tauri application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_config = config::load_config();
    let skin_name = app_config.skin;
    let port = app_config.port;
    let openclaw_agent = Arc::new(app_config.openclaw_agent);

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
            commands::show_context_menu,
            commands::get_skin_info,
            commands::get_skin_image,
            commands::send_message,
        ])
        .on_menu_event(|app, event| match event.id().as_ref() {
            "install_openclaw" => {
                let resource_dir = app
                    .path()
                    .resource_dir()
                    .expect("Cannot determine resource directory");
                match openclaw::install(&resource_dir) {
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
                if let Err(e) = commands::open_config_file() {
                    log::warn!("Failed to open config: {e}");
                }
            }
            "quit" => commands::quit_app(app),
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
            app.manage(Arc::clone(&openclaw_agent));
            app.manage(ServerPort(port));

            let state = server::AppState::new(app.handle().clone(), tts, skin_info, emotions_json);
            tauri::async_runtime::spawn(server::http_server(state, port));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
