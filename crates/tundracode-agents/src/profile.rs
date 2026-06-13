use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    pub id: String,
    pub name: String,
    pub system_prompt: String,
    pub allowed_tools: Vec<String>,
    pub max_iterations: usize,
    pub budget_tokens: u32,
    pub dry_run: bool,
    pub reasoning_effort: Option<String>,
    pub model_override: Option<ModelPreference>,
    pub execution_strategy: ExecutionStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPreference {
    pub provider: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionStrategy {
    Sequential,
    Parallel,
    BestEffort,
}

impl Default for ExecutionStrategy {
    fn default() -> Self {
        Self::Sequential
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfileRegistry {
    profiles: HashMap<String, AgentProfile>,
}

impl AgentProfileRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            profiles: HashMap::new(),
        };
        registry.register_defaults();
        registry
    }

    pub fn register(&mut self, profile: AgentProfile) {
        self.profiles.insert(profile.id.clone(), profile);
    }

    pub fn get(&self, id: &str) -> Option<&AgentProfile> {
        self.profiles.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut AgentProfile> {
        self.profiles.get_mut(id)
    }

    pub fn list_ids(&self) -> Vec<&str> {
        self.profiles.keys().map(|k| k.as_str()).collect()
    }

    pub fn remove(&mut self, id: &str) -> Option<AgentProfile> {
        self.profiles.remove(id)
    }

    fn register_defaults(&mut self) {
        self.register(AgentProfile {
            id: "plan".to_string(),
            name: "Plan Agent".to_string(),
            system_prompt: include_str!("prompts/plan.txt").to_string(),
            allowed_tools: vec![
                "ReadFile".to_string(),
                "ListDirectory".to_string(),
                "GetWorkspace".to_string(),
                "SearchCodebase".to_string(),
                "SearchInWeb".to_string(),
            ],
            max_iterations: 50,
            budget_tokens: u32::MAX,
            dry_run: true,
            reasoning_effort: None,
            model_override: None,
            execution_strategy: ExecutionStrategy::Sequential,
        });

        self.register(AgentProfile {
            id: "build".to_string(),
            name: "Build Agent".to_string(),
            system_prompt: include_str!("prompts/build.txt").to_string(),
            allowed_tools: vec![
                "ReadFile".to_string(),
                "WriteFile".to_string(),
                "ApplyPatch".to_string(),
                "CreateFile".to_string(),
                "DeleteFile".to_string(),
                "ListDirectory".to_string(),
                "RunCommand".to_string(),
                "GetDiagnostics".to_string(),
            ],
            max_iterations: 30,
            budget_tokens: u32::MAX,
            dry_run: false,
            reasoning_effort: None,
            model_override: None,
            execution_strategy: ExecutionStrategy::Sequential,
        });

        self.register(AgentProfile {
            id: "ask".to_string(),
            name: "Ask Agent".to_string(),
            system_prompt: include_str!("prompts/ask.txt").to_string(),
            allowed_tools: vec![
                "ReadFile".to_string(),
                "SearchCodebase".to_string(),
                "SearchInWeb".to_string(),
            ],
            max_iterations: 20,
            budget_tokens: u32::MAX,
            dry_run: true,
            reasoning_effort: None,
            model_override: None,
            execution_strategy: ExecutionStrategy::Sequential,
        });

        self.register(AgentProfile {
            id: "explorer".to_string(),
            name: "Explorer Agent".to_string(),
            system_prompt: include_str!("prompts/explorer.txt").to_string(),
            allowed_tools: vec![
                "ReadFile".to_string(),
                "ListDirectory".to_string(),
                "SearchCodebase".to_string(),
                "GetWorkspace".to_string(),
            ],
            max_iterations: 15,
            budget_tokens: 50_000,
            dry_run: true,
            reasoning_effort: None,
            model_override: None,
            execution_strategy: ExecutionStrategy::Sequential,
        });

        self.register(AgentProfile {
            id: "reviewer".to_string(),
            name: "Review Agent".to_string(),
            system_prompt: include_str!("prompts/reviewer.txt").to_string(),
            allowed_tools: vec![
                "ReadFile".to_string(),
                "SearchCodebase".to_string(),
                "GetDiagnostics".to_string(),
            ],
            max_iterations: 10,
            budget_tokens: 30_000,
            dry_run: true,
            reasoning_effort: None,
            model_override: None,
            execution_strategy: ExecutionStrategy::Sequential,
        });

        self.register(AgentProfile {
            id: "scout".to_string(),
            name: "Scout Agent".to_string(),
            system_prompt: include_str!("prompts/scout.txt").to_string(),
            allowed_tools: vec![
                "ReadFile".to_string(),
                "ListDirectory".to_string(),
                "SearchCodebase".to_string(),
                "SearchInWeb".to_string(),
                "GetWorkspace".to_string(),
            ],
            max_iterations: 15,
            budget_tokens: 30_000,
            dry_run: true,
            reasoning_effort: None,
            model_override: None,
            execution_strategy: ExecutionStrategy::Sequential,
        });
    }
}

impl Default for AgentProfileRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_profiles() {
        let registry = AgentProfileRegistry::new();
        assert!(registry.get("plan").is_some());
        assert!(registry.get("build").is_some());
        assert!(registry.get("ask").is_some());
        assert!(registry.get("explorer").is_some());
        assert!(registry.get("reviewer").is_some());
        assert!(registry.get("scout").is_some());
    }

    #[test]
    fn test_custom_profile() {
        let mut registry = AgentProfileRegistry::new();
        registry.register(AgentProfile {
            id: "custom".to_string(),
            name: "Custom Agent".to_string(),
            system_prompt: "Custom prompt".to_string(),
            allowed_tools: vec!["ReadFile".to_string()],
            max_iterations: 5,
            budget_tokens: 10_000,
            dry_run: true,
            reasoning_effort: None,
            model_override: None,
            execution_strategy: ExecutionStrategy::Sequential,
        });

        let profile = registry.get("custom").unwrap();
        assert_eq!(profile.name, "Custom Agent");
        assert_eq!(profile.max_iterations, 5);
    }

    #[test]
    fn test_remove_profile() {
        let mut registry = AgentProfileRegistry::new();
        assert!(registry.remove("plan").is_some());
        assert!(registry.get("plan").is_none());
    }
}
