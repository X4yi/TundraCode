use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ProviderCredentials {
    api_key: Option<String>,
    base_url: Option<String>,
}

pub fn get_api_key(provider_id: &str) -> Result<Option<String>> {
    if let Ok(key) = get_from_keyring(provider_id) {
        return Ok(Some(key));
    }
    get_from_fallback(provider_id)
        .map(|c| c.api_key)
        .or(Ok(None))
}

pub fn get_base_url(provider_id: &str) -> Result<Option<String>> {
    get_from_fallback(provider_id)
        .map(|c| c.base_url)
        .or(Ok(None))
}

pub fn set_api_key(provider_id: &str, api_key: &str) -> Result<()> {
    set_in_keyring(provider_id, api_key).or_else(|_| {
        let mut creds = get_from_fallback(provider_id).unwrap_or_default();
        creds.api_key = Some(api_key.to_string());
        save_fallback(provider_id, &creds)
    })
}

pub fn set_base_url(provider_id: &str, base_url: &str) -> Result<()> {
    let mut creds = get_from_fallback(provider_id).unwrap_or_default();
    creds.base_url = Some(base_url.to_string());
    save_fallback(provider_id, &creds)
}

pub fn delete_api_key(provider_id: &str) -> Result<()> {
    delete_from_keyring(provider_id).ok();
    let mut creds = get_from_fallback(provider_id).unwrap_or_default();
    creds.api_key = None;
    save_fallback(provider_id, &creds)
}

fn get_config_path() -> Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("No config directory found"))?
        .join("tundracode");
    Ok(config_dir.join("providers.json"))
}

fn get_from_keyring(provider_id: &str) -> Result<String> {
    let entry = keyring::Entry::new("tundracode", &format!("{}_api_key", provider_id))?;
    entry
        .get_password()
        .map_err(|e| anyhow::anyhow!("Keyring error: {}", e))
}

fn set_in_keyring(provider_id: &str, api_key: &str) -> Result<()> {
    let entry = keyring::Entry::new("tundracode", &format!("{}_api_key", provider_id))?;
    entry
        .set_password(api_key)
        .map_err(|e| anyhow::anyhow!("Keyring error: {}", e))
}

fn delete_from_keyring(provider_id: &str) -> Result<()> {
    let entry = keyring::Entry::new("tundracode", &format!("{}_api_key", provider_id))?;
    entry
        .delete_password()
        .map_err(|e| anyhow::anyhow!("Keyring error: {}", e))
}

fn get_from_fallback(provider_id: &str) -> Result<ProviderCredentials> {
    let path = get_config_path()?;
    if !path.exists() {
        return Err(anyhow::anyhow!("Config file not found"));
    }
    let content = std::fs::read_to_string(&path)?;
    let configs: HashMap<String, ProviderCredentials> = serde_json::from_str(&content)?;
    configs
        .get(provider_id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Provider not found in config"))
}

fn save_fallback(provider_id: &str, creds: &ProviderCredentials) -> Result<()> {
    let path = get_config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut configs: HashMap<String, ProviderCredentials> = if path.exists() {
        let content = std::fs::read_to_string(&path)?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        HashMap::new()
    };
    configs.insert(provider_id.to_string(), creds.clone());
    let content = serde_json::to_string_pretty(&configs)?;
    std::fs::write(&path, content)?;
    Ok(())
}
