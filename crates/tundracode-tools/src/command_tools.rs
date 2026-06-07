use async_trait::async_trait;
use serde_json::Value;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

use crate::{Tool, ToolContext, ToolError, ToolResult};

pub struct RunCommandTool;

#[async_trait]
impl Tool for RunCommandTool {
    fn name(&self) -> &'static str {
        "RunCommand"
    }
    fn description(&self) -> &'static str {
        "Ejecuta un comando de terminal en un sandbox controlado"
    }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Comando a ejecutar" },
                "args": { "type": "array", "items": { "type": "string" } },
                "timeout_seconds": { "type": "number", "default": 60 }
            },
            "required": ["command"]
        })
    }
    async fn execute(&self, context: &ToolContext, params: Value) -> Result<ToolResult, ToolError> {
        let command = params["command"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParameters("command required".to_string()))?;
        let args: Vec<String> = params["args"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let timeout_secs = params["timeout_seconds"].as_u64().unwrap_or(60);

        let sandbox = tundracode_security::CommandSandbox::new(&context.workspace_path);
        sandbox
            .validate_command(command, &args)
            .map_err(ToolError::ExecutionFailed)?;

        let workspace = Path::new(&context.workspace_path);

        let mut cmd = Command::new(command);
        cmd.args(&args)
            .current_dir(workspace)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        cmd.env_remove("LD_PRELOAD")
            .env_remove("LD_LIBRARY_PATH")
            .env_remove("SSH_AUTH_SOCK");

        let output =
            tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), cmd.output())
                .await
                .map_err(|_| {
                    ToolError::ExecutionFailed(format!("Command timed out after {}s", timeout_secs))
                })?
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(ToolResult {
            success: output.status.success(),
            output: stdout,
            error: if stderr.is_empty() {
                None
            } else {
                Some(stderr)
            },
            prior_content: None,
            resulting_content: None,
            file_path: None,
        })
    }
}

pub struct GetDiagnosticsTool;

#[async_trait]
impl Tool for GetDiagnosticsTool {
    fn name(&self) -> &'static str {
        "GetDiagnostics"
    }
    fn description(&self) -> &'static str {
        "Obtiene errores y warnings del archivo. Para .rs usa cargo check, para .ts usa tsc --noEmit."
    }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string" }
            },
            "required": ["file_path"]
        })
    }
    async fn execute(&self, context: &ToolContext, params: Value) -> Result<ToolResult, ToolError> {
        let file_path = params["file_path"].as_str().unwrap_or("");

        let ext = Path::new(file_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let workspace = Path::new(&context.workspace_path);

        let (cmd, args) = match ext {
            "rs" => ("cargo", vec!["check", "--message-format", "short"]),
            "ts" | "tsx" => ("tsc", vec!["--noEmit"]),
            _ => {
                return Ok(ToolResult {
                    success: true,
                    output: format!(
                        "No diagnostics available for file type '.{}'. Supported: .rs, .ts, .tsx",
                        ext
                    ),
                    error: None,
                    prior_content: None,
                    resulting_content: None,
            file_path: None,
                });
            }
        };

        let output = Command::new(cmd)
            .args(&args)
            .current_dir(workspace)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to run {}: {}", cmd, e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let combined = if !stdout.is_empty() && !stderr.is_empty() {
            format!("{}\n{}", stdout, stderr)
        } else if !stdout.is_empty() {
            stdout
        } else {
            stderr
        };

        if combined.trim().is_empty() {
            Ok(ToolResult {
                success: true,
                output: "No diagnostics found".to_string(),
                error: None,
                prior_content: None,
                resulting_content: None,
            file_path: None,
            })
        } else {
            Ok(ToolResult {
                success: output.status.success(),
                output: combined.trim().to_string(),
                error: if output.status.success() {
                    None
                } else {
                    Some("Diagnostics returned errors".to_string())
                },
                prior_content: None,
                resulting_content: None,
            file_path: None,
            })
        }
    }
}
