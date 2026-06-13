use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentRequest {
    pub agent_profile_id: String,
    pub task_description: String,
    pub context_budget: u32,
    pub parent_context_ids: Vec<String>,
    pub parent_model_context: u32,
}

impl SubagentRequest {
    pub fn new(agent_profile_id: String, task_description: String, parent_model_context: u32) -> Self {
        let context_budget = Self::calculate_budget(parent_model_context);
        Self {
            agent_profile_id,
            task_description,
            context_budget,
            parent_context_ids: Vec::new(),
            parent_model_context,
        }
    }

    pub fn calculate_budget(parent_context: u32) -> u32 {
        if parent_context == 0 {
            return 64_000;
        }
        (parent_context as f32 * 0.4) as u32
    }

    pub fn with_context_budget(mut self, budget: u32) -> Self {
        self.context_budget = budget;
        self
    }

    pub fn with_parent_context_ids(mut self, ids: Vec<String>) -> Self {
        self.parent_context_ids = ids;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentResult {
    pub agent_id: String,
    pub summary: String,
    pub full_output: Option<String>,
    pub tokens_used: u32,
    pub success: bool,
    pub error: Option<String>,
    pub key_findings: Vec<String>,
    pub files_referenced: Vec<String>,
}
