use crate::config::VoisonaConfig;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Timeout for individual HTTP requests to VoiSona Talk.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum time to wait for a synthesis request to complete.
const SYNTHESIS_TIMEOUT: Duration = Duration::from_secs(30);

/// Interval between synthesis status polls.
const POLL_INTERVAL: Duration = Duration::from_millis(200);

/// A voice library entry returned by `GET /api/talk/v1/voices`.
#[derive(Debug, Deserialize)]
struct VoiceLibrary {
    voice_name: String,
    voice_version: String,
    languages: Vec<String>,
}

/// Response wrapper for the voices endpoint.
#[derive(Debug, Deserialize)]
struct VoicesResponse {
    items: Vec<VoiceLibrary>,
}

/// Payload for `POST /api/talk/v1/speech-syntheses`.
#[derive(Debug, Serialize)]
struct SynthesisRequest {
    text: String,
    language: String,
    voice_name: String,
    voice_version: String,
    force_enqueue: bool,
}

/// Response from a synthesis POST.
#[derive(Debug, Deserialize)]
struct SynthesisResponse {
    uuid: String,
}

/// Status returned when polling a synthesis request.
#[derive(Debug, Deserialize)]
struct SynthesisStatus {
    state: String,
}

/// Synthesises `text` via VoiSona Talk and plays it through the default audio
/// device. This function is designed to be called from a fire-and-forget
/// `spawn`; errors are logged rather than propagated.
pub async fn synthesize(text: &str, config: &VoisonaConfig) {
    if let Err(e) = synthesize_inner(text, config).await {
        log::warn!("VoiSona TTS failed: {e}");
    }
}

async fn synthesize_inner(text: &str, config: &VoisonaConfig) -> Result<(), String> {
    let client = Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let base = format!("http://localhost:{}/api/talk/v1/", config.port);

    let (voice_name, voice_version, language) = resolve_voice(&client, &base, config).await?;

    let uuid = request_synthesis(
        &client,
        &base,
        config,
        text,
        &voice_name,
        &voice_version,
        &language,
    )
    .await?;

    poll_until_done(&client, &base, config, &uuid).await?;

    Ok(())
}

/// Determines which voice library to use. If the config specifies one, uses
/// that; otherwise queries VoiSona Talk and picks the first available library.
async fn resolve_voice(
    client: &Client,
    base: &str,
    config: &VoisonaConfig,
) -> Result<(String, String, String), String> {
    if !config.voice_name.is_empty() && !config.voice_version.is_empty() {
        return Ok((
            config.voice_name.clone(),
            config.voice_version.clone(),
            "ja_JP".to_string(),
        ));
    }

    let resp = client
        .get(format!("{base}voices"))
        .basic_auth(&config.username, Some(&config.password))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch voices: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!(
            "VoiSona voices endpoint returned {}",
            resp.status()
        ));
    }

    let voices: VoicesResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse voices response: {e}"))?;

    let lib = voices
        .items
        .first()
        .ok_or_else(|| "No voice libraries available in VoiSona Talk".to_string())?;

    let lang = lib
        .languages
        .first()
        .cloned()
        .unwrap_or("ja_JP".to_string());
    Ok((lib.voice_name.clone(), lib.voice_version.clone(), lang))
}

/// Sends a synthesis request and returns the UUID.
async fn request_synthesis(
    client: &Client,
    base: &str,
    config: &VoisonaConfig,
    text: &str,
    voice_name: &str,
    voice_version: &str,
    language: &str,
) -> Result<String, String> {
    let payload = SynthesisRequest {
        text: text.to_string(),
        language: language.to_string(),
        voice_name: voice_name.to_string(),
        voice_version: voice_version.to_string(),
        force_enqueue: true,
    };

    let resp = client
        .post(format!("{base}speech-syntheses"))
        .basic_auth(&config.username, Some(&config.password))
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Synthesis request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Synthesis endpoint returned {}", resp.status()));
    }

    let body: SynthesisResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse synthesis response: {e}"))?;

    log::info!("VoiSona synthesis queued: {}", body.uuid);
    Ok(body.uuid)
}

/// Polls the synthesis status until it reaches `"succeeded"` or times out.
async fn poll_until_done(
    client: &Client,
    base: &str,
    config: &VoisonaConfig,
    uuid: &str,
) -> Result<(), String> {
    let start = tokio::time::Instant::now();

    loop {
        if start.elapsed() > SYNTHESIS_TIMEOUT {
            let _ = delete_request(client, base, config, uuid).await;
            return Err("Synthesis timed out".to_string());
        }

        tokio::time::sleep(POLL_INTERVAL).await;

        let resp = client
            .get(format!("{base}speech-syntheses/{uuid}"))
            .basic_auth(&config.username, Some(&config.password))
            .send()
            .await
            .map_err(|e| format!("Status poll failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("Status poll returned {}", resp.status()));
        }

        let status: SynthesisStatus = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse status: {e}"))?;

        match status.state.as_str() {
            "succeeded" => {
                log::info!("VoiSona synthesis completed: {uuid}");
                return Ok(());
            }
            "failed" => {
                return Err(format!("Synthesis failed for {uuid}"));
            }
            _ => {}
        }
    }
}

/// Sends a DELETE request to clean up a synthesis entry.
async fn delete_request(
    client: &Client,
    base: &str,
    config: &VoisonaConfig,
    uuid: &str,
) -> Result<(), String> {
    client
        .delete(format!("{base}speech-syntheses/{uuid}"))
        .basic_auth(&config.username, Some(&config.password))
        .send()
        .await
        .map_err(|e| format!("Delete request failed: {e}"))?;
    Ok(())
}
