use async_trait::async_trait;
use serde_json::Value;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

use crate::{Tool, ToolCategory, ToolContext, ToolError, ToolResult};
use tundracode_permissions::Capability;
use tundracode_security::sandbox::CommandSandbox;

pub struct RunCommandTool;

#[async_trait]
impl Tool for RunCommandTool {
    fn name(&self) -> &'static str {
        "RunCommand"
    }
    fn description(&self) -> &'static str {
        "RunCommand: executes a command in sandbox. Params: c (command), a (args[]), t (timeout_secs, def 60)"
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::Command
    }
    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::CommandExecute { allowed: vec![] }]
    }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({"properties":{"c":{"type":"string"},"a":{"type":"array","items":{"type":"string"}},"t":{"type":"number"}},"required":["c"]})
    }
    async fn execute(&self, context: &ToolContext, params: Value) -> Result<ToolResult, ToolError> {
        let command = params["c"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParameters("c required".to_string()))?;
        let args: Vec<String> = params["a"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let timeout_secs = params["t"].as_u64().unwrap_or(60);

        let sandbox = CommandSandbox::new(&context.workspace_path);
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

        let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();

        let exit_code = output.status.code().map(|c| c.to_string()).unwrap_or_else(|| "unknown".to_string());

        let output_combined = if output.status.success() {
            stdout_str
        } else {
            let err = if stderr_str.is_empty() { stdout_str.clone() } else { stderr_str };
            format!("Exit code: {}\n{}", exit_code, err)
        };

        Ok(ToolResult {
            success: output.status.success(),
            output: output_combined,
            error: if output.status.success() {
                None
            } else {
                Some(format!("Command exited with code {}", exit_code))
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
        "GetDiagnostics: returns errors/warnings. .rs->cargo check, .ts->tsc. Param: f (file_path)"
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::Diagnostics
    }
    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::GetDiagnostics]
    }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({"properties":{"f":{"type":"string"}},"required":["f"]})
    }
    async fn execute(&self, context: &ToolContext, params: Value) -> Result<ToolResult, ToolError> {
        let file_path = params["f"].as_str().unwrap_or("");

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
