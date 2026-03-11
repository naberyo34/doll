use serde::Deserialize;
use std::path::PathBuf;

/// Default HTTP server port for receiving OpenClaw status updates.
pub const DEFAULT_PORT: u16 = 3000;

/// Top-level configuration loaded from `~/.config/doll/config.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    /// Active skin directory name (default: `"tama"`).
    #[serde(default = "default_skin")]
    pub skin: String,
    /// HTTP server port (default: 3000).
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub openclaw: OpenClawConfig,
    #[serde(default)]
    pub voisona: VoisonaConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            skin: default_skin(),
            port: default_port(),
            openclaw: OpenClawConfig::default(),
            voisona: VoisonaConfig::default(),
        }
    }
}

/// OpenClaw connection settings.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct OpenClawConfig {
    /// Agent name for `openclaw agent --agent <name>` (local) or
    /// `x-openclaw-agent-id` header (remote).
    /// When empty, doll omits the flag / header.
    #[serde(default)]
    pub agent: String,
    /// Remote OpenClaw Gateway URL (e.g. `http://192.168.1.100:18789`).
    /// When empty (default), doll runs in local mode: it spawns `openclaw`
    /// via the CLI and receives status updates on its local HTTP server.
    /// When set, doll operates as a remote client using the Gateway's
    /// `/v1/responses` endpoint.
    #[serde(default)]
    pub url: String,
    /// Bearer token for Gateway authentication.
    /// Required in remote mode. Corresponds to `OPENCLAW_GATEWAY_TOKEN` or
    /// `gateway.auth.password` on the server side.
    #[serde(default)]
    pub token: String,
}

impl OpenClawConfig {
    /// Returns `true` when doll is configured to connect to a remote OpenClaw
    /// server rather than using the local CLI.
    pub fn is_remote(&self) -> bool {
        !self.url.is_empty()
    }
}

fn default_skin() -> String {
    "tama".to_string()
}

fn default_port() -> u16 {
    DEFAULT_PORT
}

/// VoiSona Talk REST API connection settings.
#[derive(Debug, Clone, Deserialize)]
pub struct VoisonaConfig {
    /// Whether TTS via VoiSona Talk is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// VoiSona Talk hostname (default: `"localhost"`).
    #[serde(default = "default_voisona_host")]
    pub host: String,
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
            host: default_voisona_host(),
            port: default_voisona_port(),
            username: String::new(),
            password: String::new(),
            voice_name: String::new(),
            voice_version: String::new(),
        }
    }
}

fn default_voisona_host() -> String {
    "localhost".to_string()
}

fn default_voisona_port() -> u16 {
    32766
}

/// Template written to `config.toml` when the user opens it for the first time.
pub const DEFAULT_TEMPLATE: &str = "\
skin = \"tama\"
port = 3000

[openclaw]
# agent = \"\"
# Remote OpenClaw Gateway URL (e.g. http://192.168.1.100:18789).
# When set, doll uses the Gateway /v1/responses API instead of the local CLI.
# url = \"\"
# Bearer token for Gateway authentication (required in remote mode).
# token = \"\"

[voisona]
enabled = false
host = \"localhost\"
port = 32766
username = \"\"
password = \"\"
# voice_name = \"\"
# voice_version = \"\"
";

/// Returns the base doll config directory (`~/.config/doll/`).
///
/// Respects `$XDG_CONFIG_HOME` when set, otherwise uses `~/.config/doll`
/// regardless of platform so the documented path works on macOS too.
pub fn doll_dir() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(xdg).join("doll"));
    }
    dirs::home_dir().map(|h| h.join(".config").join("doll"))
}

/// Returns the path to the configuration file (`~/.config/doll/config.toml`).
pub fn config_path() -> Option<PathBuf> {
    doll_dir().map(|d| d.join("config.toml"))
}

/// Returns the path to the skins directory (`~/.config/doll/skins/`).
pub fn skins_dir() -> Option<PathBuf> {
    doll_dir().map(|d| d.join("skins"))
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
