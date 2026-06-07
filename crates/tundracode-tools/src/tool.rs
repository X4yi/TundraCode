use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolContext {
    pub workspace_path: String,
    pub agent_id: String,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub prior_content: Option<String>,
    pub resulting_content: Option<String>,
    pub file_path: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Tool execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Invalid parameters: {0}")]
    InvalidParameters(String),
    #[error("Tool not found: {0}")]
    ToolNotFound(String),
}

impl ToolResult {
    pub fn ok(output: impl Into<String>) -> Self {
        Self {
            success: true,
            output: output.into(),
            error: None,
            prior_content: None,
            resulting_content: None,
            file_path: None,
        }
    }

    pub fn err(error: impl Into<String>) -> Self {
        Self {
            success: false,
            output: String::new(),
            error: Some(error.into()),
            prior_content: None,
            resulting_content: None,
            file_path: None,
        }
    }

    pub fn with_prior(mut self, prior: Option<String>, path: Option<String>) -> Self {
        self.prior_content = prior;
        self.file_path = path;
        self
    }

    pub fn with_resulting_content(mut self, content: Option<String>) -> Self {
        self.resulting_content = content;
        self
    }
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn parameters_schema(&self) -> Value;
    async fn execute(&self, context: &ToolContext, params: Value) -> Result<ToolResult, ToolError>;
}
