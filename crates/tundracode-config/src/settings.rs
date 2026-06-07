use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub agents: HashMap<String, AgentSettings>,
    pub lsp: LspSettings,
    pub models: HashMap<String, ModelSettings>,
    pub shortcuts: HashMap<String, String>,
    pub autonomous_mode: bool,
    pub last_workspace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSettings {
    pub model: String,
    pub provider: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspSettings {
    pub enabled_languages: Vec<String>,
    pub server_paths: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSettings {
    pub provider: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        let mut agents = HashMap::new();
        agents.insert(
            "plan".to_string(),
            AgentSettings {
                model: "big-pickle".to_string(),
                provider: "opencode-free".to_string(),
            },
        );
        agents.insert(
            "build".to_string(),
            AgentSettings {
                model: "big-pickle".to_string(),
                provider: "opencode-free".to_string(),
            },
        );
        agents.insert(
            "ask".to_string(),
            AgentSettings {
                model: "big-pickle".to_string(),
                provider: "opencode-free".to_string(),
            },
        );

        Self {
            agents,
            lsp: LspSettings {
                enabled_languages: vec![
                    "rust".to_string(),
                    "typescript".to_string(),
                    "java".to_string(),
                ],
                server_paths: HashMap::new(),
            },
            models: HashMap::new(),
            shortcuts: HashMap::new(),
            autonomous_mode: false,
            last_workspace: None,
        }
    }
}
