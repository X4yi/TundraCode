use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tundracode_models::ModelConfig;

use crate::r#loop::AgentLoop;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContext {
    pub workspace_path: String,
    pub model_config: ModelConfig,
    pub autonomous_mode: bool,
    pub budget_tokens: u32,
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

pub struct AgentRunner {
    loop_engine: AgentLoop,
}

impl AgentRunner {
    pub fn new() -> Self {
        Self {
            loop_engine: AgentLoop::new(),
        }
    }

    pub async fn run_agent(
        &self,
        agent: &dyn Agent,
        context: &AgentContext,
        input: AgentInput,
    ) -> anyhow::Result<AgentOutput> {
        agent.run(context, input).await
    }

    pub fn loop_engine(&self) -> &AgentLoop {
        &self.loop_engine
    }
}

impl Default for AgentRunner {
    fn default() -> Self {
        Self::new()
    }
}
