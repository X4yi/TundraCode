use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub agents: HashMap<String, AgentSettings>,
    pub lsp: LspSettings,
    pub models: HashMap<String, ModelSettings>,
    pub shortcuts: HashMap<String, String>,
    pub autonomous_mode: bool,
    pub budget_per_task_tokens: u32,
    pub last_workspace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSettings {
    pub model: String,
    pub provider: String,
    pub temperature: f32,
    pub max_tokens: u32,
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
                temperature: 0.2,
                max_tokens: 4096,
            },
        );
        agents.insert(
            "build".to_string(),
            AgentSettings {
                model: "big-pickle".to_string(),
                provider: "opencode-free".to_string(),
                temperature: 0.0,
                max_tokens: 8192,
            },
        );
        agents.insert(
            "ask".to_string(),
            AgentSettings {
                model: "big-pickle".to_string(),
                provider: "opencode-free".to_string(),
                temperature: 0.7,
                max_tokens: 4096,
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
            budget_per_task_tokens: 200000,
            last_workspace: None,
        }
    }
}
