use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::capability::Capability;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
pub struct PermissionPolicy {
    pub id: String,
    pub name: String,
    pub description: String,
    pub capabilities: Vec<Capability>,
    pub max_iterations: usize,
    pub budget_tokens: u32,
    pub dry_run: bool,
    pub reasoning_effort: Option<String>,
    pub model_override: Option<String>,
    pub execution_strategy: ExecutionStrategy,
}

impl PermissionPolicy {
    pub fn has_capability(&self, name: &str) -> bool {
        self.capabilities.iter().any(|c| c.name() == name)
    }

    pub fn get_capability(&self, name: &str) -> Option<&Capability> {
        self.capabilities.iter().find(|c| c.name() == name)
    }

    pub fn allows_tool(&self, tool_name: &str) -> bool {
        let cap_name = tool_to_capability(tool_name);
        self.has_capability(&cap_name)
    }

    pub fn allowed_tool_names(&self) -> Vec<&str> {
        self.capabilities.iter().map(|c| c.name()).collect()
    }
}

fn tool_to_capability(tool_name: &str) -> String {
    match tool_name {
        "ReadFile" => "file_read".to_string(),
        "WriteFile" | "CreateFile" | "PlanWriteFile" | "PlanCreateFile" => "file_write".to_string(),
        "DeleteFile" => "file_delete".to_string(),
        "ListDirectory" | "GetWorkspace" => "list_directory".to_string(),
        "RunCommand" => "command_execute".to_string(),
        "SearchCodebase" => "search_codebase".to_string(),
        "SearchInWeb" => "search_web".to_string(),
        "GetDiagnostics" => "get_diagnostics".to_string(),
        "ApplyPatch" => "apply_patch".to_string(),
        "Task" => "subagent_spawn".to_string(),
        other => other.to_lowercase(),
    }
}

impl PermissionPolicy {
    pub fn explorer() -> Self {
        Self {
            id: "explorer".to_string(),
            name: "Explorer".to_string(),
            description: "Investigates the project codebase structure and content".to_string(),
            capabilities: vec![
                Capability::FileRead { path_pattern: None },
                Capability::ListDirectory { path_pattern: None },
                Capability::SearchCodebase,
                Capability::GetDiagnostics,
            ],
            max_iterations: 15,
            budget_tokens: 0,
            dry_run: true,
            reasoning_effort: Some("medium".to_string()),
            model_override: None,
            execution_strategy: ExecutionStrategy::Sequential,
        }
    }

    pub fn searcher() -> Self {
        Self {
            id: "searcher".to_string(),
            name: "Searcher".to_string(),
            description: "Researches information on the web for documentation and solutions".to_string(),
            capabilities: vec![
                Capability::SearchWeb,
                Capability::SearchCodebase,
            ],
            max_iterations: 10,
            budget_tokens: 0,
            dry_run: true,
            reasoning_effort: Some("low".to_string()),
            model_override: None,
            execution_strategy: ExecutionStrategy::Sequential,
        }
    }

    pub fn debugger() -> Self {
        Self {
            id: "debugger".to_string(),
            name: "Debugger".to_string(),
            description: "Analyzes bugs, stack traces, and finds root causes".to_string(),
            capabilities: vec![
                Capability::FileRead { path_pattern: None },
                Capability::ListDirectory { path_pattern: None },
                Capability::SearchCodebase,
                Capability::CommandExecute { allowed: vec![] },
                Capability::GetDiagnostics,
            ],
            max_iterations: 12,
            budget_tokens: 0,
            dry_run: true,
            reasoning_effort: Some("high".to_string()),
            model_override: None,
            execution_strategy: ExecutionStrategy::Sequential,
        }
    }

    pub fn plan() -> Self {
        Self {
            id: "plan".to_string(),
            name: "Planner".to_string(),
            description: "Creates detailed implementation plans without modifying files".to_string(),
            capabilities: vec![
                Capability::FileRead { path_pattern: None },
                Capability::ListDirectory { path_pattern: None },
                Capability::SearchCodebase,
                Capability::SearchWeb,
            ],
            max_iterations: 10,
            budget_tokens: 0,
            dry_run: true,
            reasoning_effort: Some("high".to_string()),
            model_override: None,
            execution_strategy: ExecutionStrategy::Sequential,
        }
    }

    pub fn build() -> Self {
        Self {
            id: "build".to_string(),
            name: "Builder".to_string(),
            description: "Implements changes, writes files, runs commands".to_string(),
            capabilities: vec![
                Capability::FileRead { path_pattern: None },
                Capability::FileWrite { path_pattern: None },
                Capability::FileDelete { path_pattern: None },
                Capability::ApplyPatch { path_pattern: None },
                Capability::ListDirectory { path_pattern: None },
                Capability::CommandExecute { allowed: vec![] },
                Capability::GetDiagnostics,
                Capability::SubagentSpawn {
                    allowed_profiles: vec!["explorer".to_string(), "searcher".to_string(), "debugger".to_string()],
                },
            ],
            max_iterations: 30,
            budget_tokens: 0,
            dry_run: false,
            reasoning_effort: Some("medium".to_string()),
            model_override: None,
            execution_strategy: ExecutionStrategy::Sequential,
        }
    }

    pub fn ask() -> Self {
        Self {
            id: "ask".to_string(),
            name: "Assistant".to_string(),
            description: "Read-only Q&A about the codebase".to_string(),
            capabilities: vec![
                Capability::FileRead { path_pattern: None },
                Capability::SearchCodebase,
                Capability::SearchWeb,
            ],
            max_iterations: 8,
            budget_tokens: 0,
            dry_run: true,
            reasoning_effort: None,
            model_override: None,
            execution_strategy: ExecutionStrategy::Sequential,
        }
    }

    pub fn reviewer() -> Self {
        Self {
            id: "reviewer".to_string(),
            name: "Reviewer".to_string(),
            description: "Reviews code for quality, bugs, and best practices".to_string(),
            capabilities: vec![
                Capability::FileRead { path_pattern: None },
                Capability::SearchCodebase,
                Capability::GetDiagnostics,
            ],
            max_iterations: 10,
            budget_tokens: 0,
            dry_run: true,
            reasoning_effort: Some("high".to_string()),
            model_override: None,
            execution_strategy: ExecutionStrategy::Sequential,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PolicyRegistry {
    policies: Arc<RwLock<HashMap<String, PermissionPolicy>>>,
}

impl PolicyRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            policies: Arc::new(RwLock::new(HashMap::new())),
        };
        registry.register_defaults();
        registry
    }

    fn register_defaults(&mut self) {
        let defaults = vec![
            PermissionPolicy::explorer(),
            PermissionPolicy::searcher(),
            PermissionPolicy::debugger(),
            PermissionPolicy::plan(),
            PermissionPolicy::build(),
            PermissionPolicy::ask(),
            PermissionPolicy::reviewer(),
        ];
        for policy in defaults {
            let id = policy.id.clone();
            let policies = self.policies.clone();
            tokio::spawn(async move {
                policies.write().await.insert(id, policy);
            });
        }
    }

    pub async fn get(&self, id: &str) -> Option<PermissionPolicy> {
        self.policies.read().await.get(id).cloned()
    }

    pub async fn register(&mut self, policy: PermissionPolicy) {
        self.policies.write().await.insert(policy.id.clone(), policy);
    }

    pub async fn list_ids(&self) -> Vec<String> {
        self.policies.read().await.keys().cloned().collect()
    }

    pub async fn exists(&self, id: &str) -> bool {
        self.policies.read().await.contains_key(id)
    }
}
