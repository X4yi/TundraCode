use crate::Settings;
use anyhow::Result;
use std::path::PathBuf;

pub struct ConfigStorage {
    config_dir: PathBuf,
}

impl ConfigStorage {
    pub fn new() -> Result<Self> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("No config directory found"))?
            .join("tundracode");

        std::fs::create_dir_all(&config_dir)?;

        Ok(Self { config_dir })
    }

    pub fn load(&self) -> Result<Settings> {
        let path = self.config_dir.join("settings.toml");
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            let settings: Settings = toml::from_str(&content)?;
            Ok(settings)
        } else {
            Ok(Settings::default())
        }
    }

    pub fn save(&self, settings: &Settings) -> Result<()> {
        let path = self.config_dir.join("settings.toml");
        let content = toml::to_string_pretty(settings)?;
        std::fs::write(&path, content)?;
        Ok(())
    }
}
