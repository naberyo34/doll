use crate::config::VoisonaConfig;
use crate::skin::VoiceOverride;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;

/// Timeout for individual HTTP requests to VoiSona Talk.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum time to wait for a synthesis request to complete.
const SYNTHESIS_TIMEOUT: Duration = Duration::from_secs(30);

/// Interval between synthesis status polls.
const POLL_INTERVAL: Duration = Duration::from_millis(200);

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur during VoiSona Talk TTS operations.
#[derive(Debug)]
pub enum VoisonaError {
    /// Network or connection failure (potentially retryable).
    Network(String),
    /// Authentication or authorization failure.
    Auth(String),
    /// The requested voice library was not found.
    VoiceNotFound(String),
    /// Synthesis request was rejected or failed server-side.
    Synthesis(String),
    /// Synthesis did not complete within the timeout.
    Timeout,
}

impl fmt::Display for VoisonaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VoisonaError::Network(msg) => write!(f, "network error: {msg}"),
            VoisonaError::Auth(msg) => write!(f, "auth error: {msg}"),
            VoisonaError::VoiceNotFound(msg) => write!(f, "voice not found: {msg}"),
            VoisonaError::Synthesis(msg) => write!(f, "synthesis error: {msg}"),
            VoisonaError::Timeout => write!(f, "synthesis timed out"),
        }
    }
}

// ---------------------------------------------------------------------------
// API types
// ---------------------------------------------------------------------------

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
    #[serde(skip_serializing_if = "Option::is_none")]
    global_parameters: Option<GlobalParameters>,
}

/// Voice style parameters sent alongside a synthesis request.
#[derive(Debug, Serialize)]
struct GlobalParameters {
    style_weights: Vec<f64>,
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

/// Resolved voice parameters ready to be sent in a synthesis request.
struct ResolvedVoice {
    name: String,
    version: String,
    language: String,
    style_weights: Option<Vec<f64>>,
}

// ---------------------------------------------------------------------------
// VoisonaClient
// ---------------------------------------------------------------------------

/// Shared VoiSona Talk client that reuses the HTTP connection pool and caches
/// the resolved voice library across requests.
pub struct VoisonaClient {
    client: Client,
    config: VoisonaConfig,
    base: String,
    /// Caches the resolved (voice_name, voice_version, language) tuple so the
    /// voices API is only queried once.
    cached_voice: tokio::sync::OnceCell<(String, String, String)>,
}

impl VoisonaClient {
    /// Creates a new client for the given configuration.
    pub fn new(config: VoisonaConfig) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| format!("HTTP client error: {e}"))?;
        let base = format!("http://localhost:{}/api/talk/v1/", config.port);
        Ok(Self {
            client,
            config,
            base,
            cached_voice: tokio::sync::OnceCell::new(),
        })
    }

    /// Synthesises `text` and plays it through the default audio device.
    /// Errors are logged rather than propagated.
    ///
    /// `voice_override` takes precedence over the global voice settings.
    /// `style_weights`, when provided, controls the voice's emotional tone.
    pub async fn synthesize(
        &self,
        text: &str,
        voice_override: Option<&VoiceOverride>,
        style_weights: Option<&[f64]>,
    ) {
        if let Err(e) = self
            .synthesize_inner(text, voice_override, style_weights)
            .await
        {
            log::warn!("VoiSona TTS failed: {e}");
        }
    }

    async fn synthesize_inner(
        &self,
        text: &str,
        voice_override: Option<&VoiceOverride>,
        style_weights: Option<&[f64]>,
    ) -> Result<(), VoisonaError> {
        let (name, version, language) = self
            .cached_voice
            .get_or_try_init(|| self.resolve_voice(voice_override))
            .await?;

        let voice = ResolvedVoice {
            name: name.clone(),
            version: version.clone(),
            language: language.clone(),
            style_weights: style_weights.map(|w| w.to_vec()),
        };

        let uuid = self.request_synthesis(text, &voice).await?;
        self.poll_until_done(&uuid).await?;

        Ok(())
    }

    /// Determines which voice library to use.
    ///
    /// Priority: `voice_override` (skin) > `config` voice_name/version > API auto.
    async fn resolve_voice(
        &self,
        voice_override: Option<&VoiceOverride>,
    ) -> Result<(String, String, String), VoisonaError> {
        let (want_name, want_version) = if let Some(ov) = voice_override {
            (Some(ov.voice_name.as_str()), ov.voice_version.as_deref())
        } else if !self.config.voice_name.is_empty() {
            let ver = if self.config.voice_version.is_empty() {
                None
            } else {
                Some(self.config.voice_version.as_str())
            };
            (Some(self.config.voice_name.as_str()), ver)
        } else {
            (None, None)
        };

        if let (Some(name), Some(version)) = (want_name, want_version) {
            return Ok((name.to_string(), version.to_string(), "ja_JP".to_string()));
        }

        let voices = self.fetch_voices().await?;

        if let Some(name) = want_name {
            let lib = voices
                .items
                .iter()
                .find(|v| v.voice_name == name)
                .ok_or_else(|| VoisonaError::VoiceNotFound(name.to_string()))?;
            let lang = lib
                .languages
                .first()
                .cloned()
                .unwrap_or_else(|| "ja_JP".to_string());
            return Ok((lib.voice_name.clone(), lib.voice_version.clone(), lang));
        }

        let lib = voices.items.first().ok_or_else(|| {
            VoisonaError::VoiceNotFound("no voice libraries available".to_string())
        })?;
        let lang = lib
            .languages
            .first()
            .cloned()
            .unwrap_or_else(|| "ja_JP".to_string());
        Ok((lib.voice_name.clone(), lib.voice_version.clone(), lang))
    }

    /// Fetches the list of available voice libraries from VoiSona Talk.
    async fn fetch_voices(&self) -> Result<VoicesResponse, VoisonaError> {
        let resp = self
            .client
            .get(format!("{}voices", self.base))
            .basic_auth(&self.config.username, Some(&self.config.password))
            .send()
            .await
            .map_err(|e| VoisonaError::Network(e.to_string()))?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(VoisonaError::Auth("invalid credentials".to_string()));
        }
        if !resp.status().is_success() {
            return Err(VoisonaError::Network(format!(
                "voices endpoint returned {}",
                resp.status()
            )));
        }

        resp.json()
            .await
            .map_err(|e| VoisonaError::Network(format!("failed to parse voices: {e}")))
    }

    /// Sends a synthesis request and returns the UUID.
    async fn request_synthesis(
        &self,
        text: &str,
        voice: &ResolvedVoice,
    ) -> Result<String, VoisonaError> {
        let payload = SynthesisRequest {
            text: text.to_string(),
            language: voice.language.clone(),
            voice_name: voice.name.clone(),
            voice_version: voice.version.clone(),
            force_enqueue: true,
            global_parameters: voice.style_weights.as_ref().map(|w| GlobalParameters {
                style_weights: w.clone(),
            }),
        };

        let resp = self
            .client
            .post(format!("{}speech-syntheses", self.base))
            .basic_auth(&self.config.username, Some(&self.config.password))
            .json(&payload)
            .send()
            .await
            .map_err(|e| VoisonaError::Network(e.to_string()))?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(VoisonaError::Auth("invalid credentials".to_string()));
        }
        if !resp.status().is_success() {
            return Err(VoisonaError::Synthesis(format!(
                "endpoint returned {}",
                resp.status()
            )));
        }

        let body: SynthesisResponse = resp
            .json()
            .await
            .map_err(|e| VoisonaError::Network(format!("failed to parse response: {e}")))?;

        log::info!("VoiSona synthesis queued: {}", body.uuid);
        Ok(body.uuid)
    }

    /// Polls the synthesis status until it reaches `"succeeded"` or times out.
    async fn poll_until_done(&self, uuid: &str) -> Result<(), VoisonaError> {
        let start = tokio::time::Instant::now();

        loop {
            if start.elapsed() > SYNTHESIS_TIMEOUT {
                let _ = self.delete_request(uuid).await;
                return Err(VoisonaError::Timeout);
            }

            tokio::time::sleep(POLL_INTERVAL).await;

            let resp = self
                .client
                .get(format!("{}speech-syntheses/{uuid}", self.base))
                .basic_auth(&self.config.username, Some(&self.config.password))
                .send()
                .await
                .map_err(|e| VoisonaError::Network(e.to_string()))?;

            if !resp.status().is_success() {
                return Err(VoisonaError::Synthesis(format!(
                    "status poll returned {}",
                    resp.status()
                )));
            }

            let status: SynthesisStatus = resp
                .json()
                .await
                .map_err(|e| VoisonaError::Network(format!("failed to parse status: {e}")))?;

            match status.state.as_str() {
                "succeeded" => {
                    log::info!("VoiSona synthesis completed: {uuid}");
                    return Ok(());
                }
                "failed" => {
                    return Err(VoisonaError::Synthesis(format!(
                        "synthesis failed for {uuid}"
                    )));
                }
                _ => {}
            }
        }
    }

    /// Sends a DELETE request to clean up a synthesis entry.
    async fn delete_request(&self, uuid: &str) -> Result<(), VoisonaError> {
        self.client
            .delete(format!("{}speech-syntheses/{uuid}", self.base))
            .basic_auth(&self.config.username, Some(&self.config.password))
            .send()
            .await
            .map_err(|e| VoisonaError::Network(e.to_string()))?;
        Ok(())
    }
}
