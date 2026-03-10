use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Metadata loaded from an optional `skin.toml` inside a skin directory.
#[derive(Debug, Deserialize, Default)]
struct SkinMeta {
    #[serde(default)]
    display_name: Option<String>,
    /// Per-skin voice library override for VoiSona Talk TTS.
    #[serde(default)]
    voice: Option<VoiceOverride>,
    /// Maps emotion name → config (description + optional style weights).
    /// Accepts both a plain string (`happy = "desc"`) and a detailed table
    /// (`[emotions.happy] description = "desc" style_weights = [...]`).
    #[serde(default)]
    emotions: HashMap<String, EmotionValue>,
}

/// Flexible deserialization for emotion entries in `skin.toml`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum EmotionValue {
    /// Short form: `happy = "嬉しい・ポジティブな応答"`
    Simple(String),
    /// Full form with optional VoiSona style weights.
    Detailed {
        description: String,
        #[serde(default)]
        style_weights: Option<Vec<f64>>,
    },
}

impl EmotionValue {
    fn description(&self) -> &str {
        match self {
            EmotionValue::Simple(s) => s,
            EmotionValue::Detailed { description, .. } => description,
        }
    }

    fn style_weights(&self) -> Option<&Vec<f64>> {
        match self {
            EmotionValue::Simple(_) => None,
            EmotionValue::Detailed { style_weights, .. } => style_weights.as_ref(),
        }
    }
}

/// Per-skin voice library selection, defined in `skin.toml` under `[voice]`.
///
/// When present, overrides the global `[voisona]` voice settings in
/// `config.toml` for this skin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceOverride {
    /// VoiSona Talk voice library name (e.g. `"nurse-robot-type-t_ja_JP"`).
    pub voice_name: String,
    /// Voice library version. When omitted, resolved automatically via the
    /// VoiSona Talk API.
    #[serde(default)]
    pub voice_version: Option<String>,
}

/// A single emotion entry with its name, description, and optional TTS style.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmotionEntry {
    /// Emotion key (matches the PNG filename without extension).
    pub name: String,
    /// Human-readable description of when to use this emotion.
    /// Falls back to the emotion name when not specified in `skin.toml`.
    pub description: String,
    /// VoiSona Talk style weights for this emotion. Sent as
    /// `global_parameters.style_weights` in the synthesis request.
    #[serde(skip_serializing)]
    pub style_weights: Option<Vec<f64>>,
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

    let display_name = meta
        .display_name
        .unwrap_or_else(|| name.to_string());

    let mut emotions = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("png") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if stem != "idle" {
                        let (description, style_weights) =
                            if let Some(val) = meta.emotions.get(stem) {
                                (
                                    val.description().to_string(),
                                    val.style_weights().cloned(),
                                )
                            } else {
                                (stem.to_string(), None)
                            };
                        emotions.push(EmotionEntry {
                            name: stem.to_string(),
                            description,
                            style_weights,
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

fn copy_dir_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
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
