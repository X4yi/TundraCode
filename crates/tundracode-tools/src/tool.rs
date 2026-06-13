use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use tundracode_permissions::Capability;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolCategory {
    FileSystem,
    Command,
    Search,
    Patch,
    Diagnostics,
    Subagent,
    Planning,
}

impl ToolCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FileSystem => "filesystem",
            Self::Command => "command",
            Self::Search => "search",
            Self::Patch => "patch",
            Self::Diagnostics => "diagnostics",
            Self::Subagent => "subagent",
            Self::Planning => "planning",
        }
    }
}

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
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
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
    fn category(&self) -> ToolCategory;
    fn required_capabilities(&self) -> Vec<Capability>;
    fn parameters_schema(&self) -> Value;

    fn validate_params(&self, params: &Value) -> Result<(), ToolError> {
        let _ = params;
        Ok(())
    }

    async fn execute(&self, context: &ToolContext, params: Value) -> Result<ToolResult, ToolError>;
}

pub type ToolFactory = Box<dyn Fn() -> Box<dyn Tool> + Send + Sync>;

pub struct ToolCatalog {
    factories: HashMap<String, ToolFactory>,
}

impl ToolCatalog {
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    pub fn register<T: Tool + 'static>(&mut self, factory: impl Fn() -> T + Send + Sync + 'static) {
        let name = factory().name().to_string();
        self.factories.insert(
            name,
            Box::new(move || Box::new(factory())),
        );
    }

    pub fn create(&self, name: &str) -> Option<Box<dyn Tool>> {
        self.factories.get(name).map(|f| f())
    }

    pub fn all_names(&self) -> Vec<&str> {
        self.factories.keys().map(|k| k.as_str()).collect()
    }

    pub fn exists(&self, name: &str) -> bool {
        self.factories.contains_key(name)
    }
}
