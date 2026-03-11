use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// VoiceParams — maps to VoiSona Talk `global_parameters`
// ---------------------------------------------------------------------------

/// Voice parameters that map directly to VoiSona Talk API
/// `global_parameters`.
///
/// All fields are optional; omitted values inherit from the skin's base
/// voice settings, and ultimately fall back to VoiSona Talk defaults when
/// sent in a synthesis request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VoiceParams {
    /// Speech rate (default: 1, range: 0.2–5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
    /// Amplitude in decibels (default: 0, range: −8–8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<f64>,
    /// Pitch shift in cents (default: 0, range: −600–600).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pitch: Option<f64>,
    /// Pitch contour variation scale (default: 1, range: 0–2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intonation: Option<f64>,
    /// Age-like voice quality parameter (default: 0, range: −1–1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alp: Option<f64>,
    /// Huskiness control (default: 0, range: −20–20).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub huskiness: Option<f64>,
    /// Style weight coefficients array.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_weights: Option<Vec<f64>>,
}

impl VoiceParams {
    /// Merges `self` (base) with `overlay`, preferring overlay values.
    pub fn merge(&self, overlay: &VoiceParams) -> VoiceParams {
        VoiceParams {
            speed: overlay.speed.or(self.speed),
            volume: overlay.volume.or(self.volume),
            pitch: overlay.pitch.or(self.pitch),
            intonation: overlay.intonation.or(self.intonation),
            alp: overlay.alp.or(self.alp),
            huskiness: overlay.huskiness.or(self.huskiness),
            style_weights: overlay
                .style_weights
                .clone()
                .or_else(|| self.style_weights.clone()),
        }
    }

    /// Returns `true` when every field is `None`.
    pub fn is_empty(&self) -> bool {
        self.speed.is_none()
            && self.volume.is_none()
            && self.pitch.is_none()
            && self.intonation.is_none()
            && self.alp.is_none()
            && self.huskiness.is_none()
            && self.style_weights.is_none()
    }
}

// ---------------------------------------------------------------------------
// skin.toml internal types
// ---------------------------------------------------------------------------

/// Metadata loaded from an optional `skin.toml` inside a skin directory.
#[derive(Debug, Deserialize, Default)]
struct SkinMeta {
    #[serde(default)]
    display_name: Option<String>,
    /// Per-skin voice library override for VoiSona Talk TTS.
    #[serde(default)]
    voice: Option<VoiceOverride>,
    /// Maps emotion name → config (description + optional voice params).
    /// Accepts both a plain string (`happy = "desc"`) and a detailed table
    /// (`[emotions.happy] description = "desc" style_weights = [...]`).
    #[serde(default)]
    emotions: HashMap<String, EmotionValue>,
    /// Phrases spoken via TTS when the agent enters the "thinking" state.
    /// One phrase is chosen at random for each thinking event.
    #[serde(default)]
    thinking_phrases: Vec<String>,
}

/// Flexible deserialization for emotion entries in `skin.toml`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum EmotionValue {
    /// Short form: `happy = "嬉しい・ポジティブな応答"`
    Simple(String),
    /// Full form with optional VoiSona voice parameters.
    Detailed {
        description: String,
        #[serde(flatten)]
        params: VoiceParams,
    },
}

impl EmotionValue {
    fn description(&self) -> &str {
        match self {
            EmotionValue::Simple(s) => s,
            EmotionValue::Detailed { description, .. } => description,
        }
    }

    fn voice_params(&self) -> VoiceParams {
        match self {
            EmotionValue::Simple(_) => VoiceParams::default(),
            EmotionValue::Detailed { params, .. } => params.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Per-skin voice library selection, defined in `skin.toml` under `[voice]`.
///
/// When present, overrides the global `[voisona]` voice settings in
/// `config.toml` for this skin.  The flattened [`VoiceParams`] fields act
/// as the skin-wide base voice parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceOverride {
    /// VoiSona Talk voice library name (e.g. `"nurse-robot-type-t_ja_JP"`).
    pub voice_name: String,
    /// Voice library version. When omitted, resolved automatically via the
    /// VoiSona Talk API.
    #[serde(default)]
    pub voice_version: Option<String>,
    /// Base voice parameters for this skin.
    #[serde(flatten)]
    pub params: VoiceParams,
}

/// A single emotion entry with its name, description, and merged TTS params.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmotionEntry {
    /// Emotion key (matches the PNG filename without extension).
    pub name: String,
    /// Human-readable description of when to use this emotion.
    /// Falls back to the emotion name when not specified in `skin.toml`.
    pub description: String,
    /// Merged voice parameters (skin base + emotion overlay).
    #[serde(skip_serializing)]
    pub voice_params: VoiceParams,
}

/// Information about a discovered skin, sent to the frontend and HTTP clients.
#[derive(Debug, Clone, Serialize)]
pub struct SkinInfo {
    /// Directory name (used as the key in `config.toml`).
    pub name: String,
    /// Human-readable name from `skin.toml`, falls back to `name`.
    pub display_name: String,
    /// Available emotions with descriptions (excluding `idle`).
    pub emotions: Vec<EmotionEntry>,
    /// Per-skin voice library override for TTS.
    #[serde(skip_serializing)]
    pub voice: Option<VoiceOverride>,
    /// Skin-wide base voice parameters (used for thinking and fallback).
    #[serde(skip_serializing)]
    pub base_voice_params: VoiceParams,
    /// Phrases spoken via TTS when the agent enters the "thinking" state.
    #[serde(skip_serializing)]
    pub thinking_phrases: Vec<String>,
}

/// Scans a skin directory and returns its info if valid.
///
/// A directory is valid when it contains at least `idle.png`.
pub fn discover_skin(skins_dir: &Path, name: &str) -> Option<SkinInfo> {
    let dir = skins_dir.join(name);
    if !dir.join("idle.png").is_file() {
        return None;
    }

    let meta = if dir.join("skin.toml").exists() {
        match std::fs::read_to_string(dir.join("skin.toml")) {
            Ok(contents) => match toml::from_str::<SkinMeta>(&contents) {
                Ok(m) => m,
                Err(e) => {
                    log::warn!("Failed to parse {}/skin.toml: {e}", dir.display());
                    SkinMeta::default()
                }
            },
            Err(e) => {
                log::warn!("Failed to read {}/skin.toml: {e}", dir.display());
                SkinMeta::default()
            }
        }
    } else {
        SkinMeta::default()
    };

    let display_name = meta.display_name.unwrap_or_else(|| name.to_string());

    let base_params = meta
        .voice
        .as_ref()
        .map(|v| v.params.clone())
        .unwrap_or_default();

    let mut emotions = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("png") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if stem != "idle" {
                        let (description, voice_params) = if let Some(val) = meta.emotions.get(stem)
                        {
                            let merged = base_params.merge(&val.voice_params());
                            (val.description().to_string(), merged)
                        } else {
                            (stem.to_string(), base_params.clone())
                        };
                        emotions.push(EmotionEntry {
                            name: stem.to_string(),
                            description,
                            voice_params,
                        });
                    }
                }
            }
        }
    }
    emotions.sort_by(|a, b| a.name.cmp(&b.name));

    Some(SkinInfo {
        name: name.to_string(),
        display_name,
        emotions,
        voice: meta.voice,
        base_voice_params: base_params,
        thinking_phrases: meta.thinking_phrases,
    })
}

/// Returns the path to an emotion image for a skin, falling back to `idle.png`
/// when the requested emotion does not exist.
pub fn resolve_image_path(skins_dir: &Path, skin: &str, emotion: &str) -> PathBuf {
    let specific = skins_dir.join(skin).join(format!("{emotion}.png"));
    if specific.is_file() {
        specific
    } else {
        skins_dir.join(skin).join("idle.png")
    }
}

/// Copies bundled skin resources into the user skins directory when the
/// target directory does not already exist.
///
/// Tries multiple candidate paths to find bundled skins — the production
/// resource directory first, then the compile-time source directory as a
/// fallback for `tauri dev`.
pub fn install_bundled_skins(resource_dir: &Path, skins_dir: &Path) {
    let candidates = [
        resource_dir.join("resources").join("skins"),
        resource_dir.join("skins"),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("skins"),
    ];

    let Some(bundled) = candidates.iter().find(|p| p.is_dir()) else {
        log::info!(
            "No bundled skins found (tried: {})",
            candidates
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
        return;
    };

    let Ok(entries) = std::fs::read_dir(bundled) else {
        return;
    };

    for entry in entries.flatten() {
        let src = entry.path();
        if !src.is_dir() {
            continue;
        }
        let Some(name) = src.file_name() else {
            continue;
        };
        let dest = skins_dir.join(name);
        if dest.exists() {
            continue;
        }

        log::info!("Installing bundled skin: {}", name.to_string_lossy());
        if let Err(e) = copy_dir_recursive(&src, &dest) {
            log::warn!("Failed to install skin {}: {e}", name.to_string_lossy());
        }
    }
}

pub(crate) fn copy_dir_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dest_path)?;
        } else {
            std::fs::copy(&src_path, &dest_path)?;
        }
    }
    Ok(())
}
