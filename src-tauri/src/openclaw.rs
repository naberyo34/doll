use std::path::{Path, PathBuf};

use crate::skin;

/// Copies bundled Skill and Hook files to `~/.openclaw/` and updates
/// `openclaw.json` to enable them.
pub(crate) fn install(resource_dir: &Path) -> Result<(), String> {
    install_files(resource_dir)?;
    update_config()?;
    log::info!("OpenClaw integration installed successfully");
    Ok(())
}

/// Copies `skills/doll/` and `hooks/doll-notify/` into `~/.openclaw/`.
fn install_files(resource_dir: &Path) -> Result<(), String> {
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
fn update_config() -> Result<(), String> {
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
