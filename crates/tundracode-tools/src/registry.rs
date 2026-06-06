use std::collections::HashMap;
use std::path::PathBuf;
use tracing::info;

use crate::{
    CreateFileTool, DeleteFileTool, GetDiagnosticsTool, GetWorkspaceTool, ListDirectoryTool,
    ReadFileTool, RunCommandTool, SearchCodebaseTool, SearchInWebTool, Tool, ToolResult,
    WriteFileTool,
};

pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
    log_path: Option<PathBuf>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            log_path: None,
        }
    }

    pub fn with_audit_log(mut self, log_path: PathBuf) -> Self {
        self.log_path = Some(log_path);
        self
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) {
        let name = tool.name().to_string();
        self.tools.insert(name, tool);
    }

    pub fn register_all_default(&mut self) {
        self.register(Box::new(ReadFileTool));
        self.register(Box::new(WriteFileTool));
        self.register(Box::new(CreateFileTool));
        self.register(Box::new(DeleteFileTool));
        self.register(Box::new(ListDirectoryTool));
        self.register(Box::new(GetWorkspaceTool));
        self.register(Box::new(RunCommandTool));
        self.register(Box::new(SearchCodebaseTool));
        self.register(Box::new(SearchInWebTool));
        self.register(Box::new(GetDiagnosticsTool));
    }

    pub fn register_subset(&mut self, names: &[&str]) {
        for name in names {
            match *name {
                "ReadFile" => self.register(Box::new(ReadFileTool)),
                "WriteFile" => self.register(Box::new(WriteFileTool)),
                "CreateFile" => self.register(Box::new(CreateFileTool)),
                "DeleteFile" => self.register(Box::new(DeleteFileTool)),
                "ListDirectory" => self.register(Box::new(ListDirectoryTool)),
                "GetWorkspace" => self.register(Box::new(GetWorkspaceTool)),
                "RunCommand" => self.register(Box::new(RunCommandTool)),
                "SearchCodebase" => self.register(Box::new(SearchCodebaseTool)),
                "SearchInWeb" => self.register(Box::new(SearchInWebTool)),
                "GetDiagnostics" => self.register(Box::new(GetDiagnosticsTool)),
                _ => {}
            }
        }
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|b| b.as_ref())
    }

    pub fn list_tools(&self) -> Vec<&str> {
        self.tools.keys().map(|k| k.as_str()).collect()
    }

    pub async fn execute(
        &self,
        context: &crate::ToolContext,
        tool_name: &str,
        params: serde_json::Value,
    ) -> Result<ToolResult, crate::ToolError> {
        let tool = self
            .get(tool_name)
            .ok_or_else(|| crate::ToolError::ToolNotFound(tool_name.to_string()))?;

        info!(tool = tool_name, agent = context.agent_id, "executing tool");

        let result = tool.execute(context, params).await;

        if let Some(log_path) = &self.log_path {
            if let Ok(log_entry) = serde_json::to_string(&serde_json::json!({
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "agent": context.agent_id,
                "tool": tool_name,
                "success": result.as_ref().map(|r| r.success).unwrap_or(false),
            })) {
                let _ = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(log_path)
                    .and_then(|mut f| {
                        use std::io::Write;
                        writeln!(f, "{}", log_entry)
                    });
            }
        }

        result
    }
}
