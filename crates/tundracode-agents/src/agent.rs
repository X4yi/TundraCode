use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tundracode_models::ModelConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildMode {
    Autonomous,
    ReviewRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContext {
    pub workspace_path: String,
    pub model_config: ModelConfig,
    pub build_mode: BuildMode,
    pub budget_tokens: u32,
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInput {
    pub user_message: String,
    pub plan_annotations: Option<String>,
    pub memory_excerpt: Option<String>,
}

#[derive(Debug, Clone)]
pub enum AgentOutput {
    FinalAnswer {
        content: String,
        tokens_used: u32,
    },
    ProposedChanges {
        proposals: Vec<DiffProposal>,
        invocations: Vec<ToolInvocation>,
        tool_log: Vec<String>,
        tokens_used: u32,
    },
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffProposal {
    pub id: String,
    pub file_path: String,
    pub kind: DiffKind,
    pub unified_diff: String,
    pub requires_user_confirmation: bool,
    pub before: String,
    pub after: String,
    pub tool_call_id: String,
    pub task_number: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiffKind {
    Create,
    Modify,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInvocation {
    pub tool_name: String,
    pub call_id: String,
    pub arguments: Value,
    pub success: bool,
    pub output: String,
    pub file_path: Option<String>,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[async_trait]
pub trait Agent: Send + Sync {
    fn name(&self) -> &'static str;
    fn system_prompt(&self) -> String;
    fn allowed_tools(&self) -> Vec<&'static str>;
    async fn run(&self, context: &AgentContext, input: AgentInput) -> anyhow::Result<AgentOutput>;
}
