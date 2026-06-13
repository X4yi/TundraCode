use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackProviderConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

fn providers_config_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from(".")))
        .join("tundracode")
        .join("providers.json")
}

pub fn set_api_key_in_keyring(provider_id: &str, api_key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new("tundracode", &format!("{}_api_key", provider_id))
        .map_err(|e| format!("Keyring error: {}", e))?;
    entry
        .set_password(api_key)
        .map_err(|e| format!("Failed to set password: {}", e))
}

pub fn get_api_key_from_keyring(provider_id: &str) -> Result<String, String> {
    let entry = keyring::Entry::new("tundracode", &format!("{}_api_key", provider_id))
        .map_err(|e| format!("Keyring error: {}", e))?;
    entry
        .get_password()
        .map_err(|e| format!("Failed to get password: {}", e))
}

pub fn delete_provider_api_key(provider_id: &str) -> Result<(), String> {
    let entry = keyring::Entry::new("tundracode", &format!("{}_api_key", provider_id))
        .map_err(|e| format!("Keyring error: {}", e))?;
    entry
        .delete_password()
        .map_err(|e| format!("Failed to delete password: {}", e))
}

pub async fn save_provider_fallback(
    provider_id: &str,
    api_key: &str,
    base_url: Option<&str>,
) -> Result<(), String> {
    let config_dir = dirs::config_dir()
        .ok_or("Cannot find config directory".to_string())?
        .join("tundracode");
    tokio::fs::create_dir_all(&config_dir)
        .await
        .map_err(|e| format!("Failed to create config dir: {}", e))?;

    let config_path = config_dir.join("providers.json");
    let mut configs: HashMap<String, FallbackProviderConfig> =
        if config_path.exists() {
            let content = tokio::fs::read_to_string(&config_path)
                .await
                .unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            HashMap::new()
        };

    let entry = configs
        .entry(provider_id.to_string())
        .or_insert_with(|| FallbackProviderConfig {
            api_key: None,
            base_url: None,
        });
    entry.api_key = Some(api_key.to_string());
    if let Some(url) = base_url {
        entry.base_url = Some(url.to_string());
    }

    let content = serde_json::to_string_pretty(&configs)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    tokio::fs::write(&config_path, content)
        .await
        .map_err(|e| format!("Failed to write config: {}", e))
}

pub async fn read_provider_fallback(provider_id: &str) -> Result<FallbackProviderConfig, String> {
    let config_path = providers_config_path();

    if !config_path.exists() {
        return Err("Config file not found".to_string());
    }

    let content = tokio::fs::read_to_string(&config_path)
        .await
        .map_err(|e| format!("Failed to read config: {}", e))?;
    let configs: HashMap<String, FallbackProviderConfig> =
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse config: {}", e))?;

    configs
        .get(provider_id)
        .cloned()
        .ok_or_else(|| "Provider not found in config".to_string())
}

pub async fn delete_provider_fallback(provider_id: &str) -> Result<(), String> {
    let config_path = providers_config_path();

    if !config_path.exists() {
        return Ok(());
    }

    let content = tokio::fs::read_to_string(&config_path)
        .await
        .map_err(|e| format!("Failed to read config: {}", e))?;
    let mut configs: HashMap<String, FallbackProviderConfig> =
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse config: {}", e))?;

    configs.remove(provider_id);

    let new_content = serde_json::to_string_pretty(&configs)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    tokio::fs::write(&config_path, new_content)
        .await
        .map_err(|e| format!("Failed to write config: {}", e))
}

pub async fn save_provider_base_url(provider_id: &str, base_url: &str) -> Result<(), String> {
    let config_dir = dirs::config_dir()
        .ok_or("Cannot find config directory".to_string())?
        .join("tundracode");
    tokio::fs::create_dir_all(&config_dir)
        .await
        .map_err(|e| format!("Failed to create config dir: {}", e))?;

    let config_path = config_dir.join("providers.json");
    let mut configs: HashMap<String, FallbackProviderConfig> =
        if config_path.exists() {
            let content = tokio::fs::read_to_string(&config_path)
                .await
                .unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            HashMap::new()
        };

    let entry = configs
        .entry(provider_id.to_string())
        .or_insert_with(|| FallbackProviderConfig {
            api_key: None,
            base_url: None,
        });
    entry.base_url = Some(base_url.to_string());

    let content = serde_json::to_string_pretty(&configs)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    tokio::fs::write(&config_path, content)
        .await
        .map_err(|e| format!("Failed to write config: {}", e))
}

#[allow(dead_code)]
pub fn resolve_provider_config(provider_id: &str) -> Result<(Option<String>, String), String> {
    let provider = tundracode_models::get_provider_by_id(provider_id)
        .ok_or(format!("Provider not found: {}", provider_id))?;

    let api_key = tundracode_models::credentials::get_api_key(provider_id)
        .ok()
        .flatten();

    let base_url = tundracode_models::credentials::get_base_url(provider_id)
        .ok()
        .flatten()
        .unwrap_or_else(|| provider.base_url.clone());

    Ok((api_key, base_url))
}
