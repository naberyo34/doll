use serde::Deserialize;
use std::path::PathBuf;

/// Top-level configuration loaded from `~/.config/doll/config.toml`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub voisona: VoisonaConfig,
}

/// VoiSona Talk REST API connection settings.
#[derive(Debug, Clone, Deserialize)]
pub struct VoisonaConfig {
    /// Whether TTS via VoiSona Talk is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// VoiSona Talk REST API port (default: 32766).
    #[serde(default = "default_voisona_port")]
    pub port: u16,
    /// Username for HTTP Basic auth (the email used for VoiSona registration).
    #[serde(default)]
    pub username: String,
    /// API password configured in VoiSona Talk settings.
    #[serde(default)]
    pub password: String,
    /// Explicit voice name to use. When empty, the first available library is
    /// selected automatically.
    #[serde(default)]
    pub voice_name: String,
    /// Explicit voice version. Only used when `voice_name` is set.
    #[serde(default)]
    pub voice_version: String,
}

impl Default for VoisonaConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: default_voisona_port(),
            username: String::new(),
            password: String::new(),
            voice_name: String::new(),
            voice_version: String::new(),
        }
    }
}

fn default_voisona_port() -> u16 {
    32766
}

/// Template written to `config.toml` when the user opens it for the first time.
pub const DEFAULT_TEMPLATE: &str = "\
[voisona]
enabled = false
port = 32766
username = \"\"
password = \"\"
# voice_name = \"\"
# voice_version = \"\"
";

/// Returns the path to the configuration file (`~/.config/doll/config.toml`).
///
/// Uses `$XDG_CONFIG_HOME/doll/config.toml` when set, otherwise
/// `~/.config/doll/config.toml` regardless of platform so the documented path
/// works on macOS too (where `dirs::config_dir` would return
/// `~/Library/Application Support`).
pub fn config_path() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(xdg).join("doll").join("config.toml"));
    }
    dirs::home_dir().map(|h| h.join(".config").join("doll").join("config.toml"))
}

/// Loads the application configuration from disk.
///
/// Returns `AppConfig::default()` when the file does not exist or cannot be
/// parsed, so doll always starts even without a config file.
pub fn load_config() -> AppConfig {
    let Some(path) = config_path() else {
        log::info!("Could not determine config directory; using defaults");
        return AppConfig::default();
    };

    match std::fs::read_to_string(&path) {
        Ok(contents) => match toml::from_str::<AppConfig>(&contents) {
            Ok(cfg) => {
                log::info!("Loaded config from {}", path.display());
                cfg
            }
            Err(e) => {
                log::warn!("Failed to parse {}: {e}; using defaults", path.display());
                AppConfig::default()
            }
        },
        Err(_) => {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if std::fs::write(&path, DEFAULT_TEMPLATE).is_ok() {
                log::info!("Created default config at {}", path.display());
            } else {
                log::warn!("Failed to create config at {}", path.display());
            }
            AppConfig::default()
        }
    }
}
