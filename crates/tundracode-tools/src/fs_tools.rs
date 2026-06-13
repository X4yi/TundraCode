use async_trait::async_trait;
use serde_json::Value;
use std::path::Path;

use crate::{Tool, ToolCategory, ToolContext, ToolError, ToolResult};
use tundracode_permissions::Capability;
use tundracode_security::path_guard::{ensure_within_workspace, is_tundracode_path};

fn resolve_path(context: &ToolContext, path: &str) -> Result<std::path::PathBuf, ToolError> {
    let workspace = Path::new(&context.workspace_path);
    let target = if Path::new(path).is_absolute() {
        Path::new(path).to_path_buf()
    } else {
        workspace.join(path)
    };

    ensure_within_workspace(workspace, &target)
        .map_err(|e| ToolError::ExecutionFailed(e))?;

    Ok(target)
}

pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &'static str {
        "ReadFile"
    }
    fn description(&self) -> &'static str {
        "ReadFile: reads a file from the workspace. Param: p (path)"
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }
    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::FileRead { path_pattern: None }]
    }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({"properties":{"p":{"type":"string"}},"required":["p"]})
    }
    async fn execute(&self, context: &ToolContext, params: Value) -> Result<ToolResult, ToolError> {
        let path = params["p"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParameters("p required".to_string()))?;
        let full_path = resolve_path(context, path)?;

        if is_tundracode_path(&full_path) {
            return Err(ToolError::ExecutionFailed(
                "Cannot read from .tundracode directory".to_string(),
            ));
        }

        match tokio::fs::read_to_string(&full_path).await {
            Ok(content) => {
                let prior = content.clone();
                Ok(ToolResult::ok(content).with_prior(Some(prior), Some(path.to_string())))
            }
            Err(e) => Ok(ToolResult::err(e.to_string())),
        }
    }
}

pub struct WriteFileTool;

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &'static str {
        "WriteFile"
    }
    fn description(&self) -> &'static str {
        "WriteFile: writes content to a file. Params: p (path), c (content)"
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }
    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::FileWrite { path_pattern: None }]
    }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({"properties":{"p":{"type":"string"},"c":{"type":"string"}},"required":["p","c"]})
    }
    async fn execute(&self, context: &ToolContext, params: Value) -> Result<ToolResult, ToolError> {
        let path = params["p"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParameters("p required".to_string()))?;
        let content = params["c"].as_str().unwrap_or("");
        let full_path = resolve_path(context, path)?;

        if is_tundracode_path(&full_path) {
            return Err(ToolError::ExecutionFailed(
                "Cannot write to .tundracode directory".to_string(),
            ));
        }

        let prior = tokio::fs::read_to_string(&full_path).await.ok();

        if context.dry_run {
            return Ok(
                ToolResult::ok(format!("File {} written (dry-run)", path))
                    .with_prior(prior, Some(path.to_string()))
                    .with_resulting_content(Some(content.to_string())),
            );
        }

        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                ToolError::ExecutionFailed(format!("Cannot create directory: {}", e))
            })?;
        }

        match tokio::fs::write(&full_path, content).await {
            Ok(_) => Ok(ToolResult::ok(format!("File {} written", path))
                .with_prior(prior, Some(path.to_string()))
                .with_resulting_content(Some(content.to_string()))),
            Err(e) => Ok(ToolResult::err(e.to_string())),
        }
    }
}

pub struct CreateFileTool;

#[async_trait]
impl Tool for CreateFileTool {
    fn name(&self) -> &'static str {
        "CreateFile"
    }
    fn description(&self) -> &'static str {
        "CreateFile: creates a new file. Fails if exists. Params: p (path), c (content optional)"
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }
    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::FileWrite { path_pattern: None }]
    }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({"properties":{"p":{"type":"string"},"c":{"type":"string"}},"required":["p"]})
    }
    async fn execute(&self, context: &ToolContext, params: Value) -> Result<ToolResult, ToolError> {
        let path = params["p"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParameters("p required".to_string()))?;
        let content = params["c"].as_str().unwrap_or("");
        let full_path = resolve_path(context, path)?;

        if is_tundracode_path(&full_path) {
            return Err(ToolError::ExecutionFailed(
                "Cannot create files in .tundracode directory".to_string(),
            ));
        }

        if full_path.exists() {
            return Err(ToolError::ExecutionFailed(format!(
                "File already exists: {}",
                path
            )));
        }

        if context.dry_run {
            return Ok(
                ToolResult::ok(format!("File {} created (dry-run)", path))
                    .with_prior(None, Some(path.to_string()))
                    .with_resulting_content(Some(content.to_string())),
            );
        }

        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                ToolError::ExecutionFailed(format!("Cannot create directory: {}", e))
            })?;
        }

        match tokio::fs::write(&full_path, content).await {
            Ok(_) => Ok(ToolResult::ok(format!("File {} created", path))
                .with_prior(None, Some(path.to_string()))
                .with_resulting_content(Some(content.to_string()))),
            Err(e) => Ok(ToolResult::err(e.to_string())),
        }
    }
}

pub struct DeleteFileTool;

#[async_trait]
impl Tool for DeleteFileTool {
    fn name(&self) -> &'static str {
        "DeleteFile"
    }
    fn description(&self) -> &'static str {
        "DeleteFile: deletes a file. Param: p (path)"
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }
    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::FileDelete { path_pattern: None }]
    }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({"properties":{"p":{"type":"string"}},"required":["p"]})
    }
    async fn execute(&self, context: &ToolContext, params: Value) -> Result<ToolResult, ToolError> {
        let path = params["p"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParameters("p required".to_string()))?;
        let full_path = resolve_path(context, path)?;

        if is_tundracode_path(&full_path) {
            return Err(ToolError::ExecutionFailed(
                "Cannot delete files in .tundracode directory".to_string(),
            ));
        }

        if !full_path.exists() {
            return Err(ToolError::ExecutionFailed(format!(
                "File not found: {}",
                path
            )));
        }

        let prior = tokio::fs::read_to_string(&full_path).await.ok();

        if context.dry_run {
            return Ok(
                ToolResult::ok(format!("File {} deleted (dry-run)", path))
                    .with_prior(prior, Some(path.to_string()))
                    .with_resulting_content(Some(String::new())),
            );
        }

        match tokio::fs::remove_file(&full_path).await {
            Ok(_) => Ok(ToolResult::ok(format!("File {} deleted", path))
                .with_prior(prior, Some(path.to_string()))
                .with_resulting_content(Some(String::new()))),
            Err(e) => Ok(ToolResult::err(e.to_string())),
        }
    }
}

pub struct ListDirectoryTool;

#[async_trait]
impl Tool for ListDirectoryTool {
    fn name(&self) -> &'static str {
        "ListDirectory"
    }
    fn description(&self) -> &'static str {
        "ListDirectory: lists directory contents. Param: p (path, default '.')"
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }
    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::ListDirectory { path_pattern: None }]
    }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({"properties":{"p":{"type":"string"}},"required":[]})
    }
    async fn execute(&self, context: &ToolContext, params: Value) -> Result<ToolResult, ToolError> {
        let path = params["p"].as_str().unwrap_or(".");
        let full_path = resolve_path(context, path)?;

        if is_tundracode_path(&full_path) {
            return Err(ToolError::ExecutionFailed(
                "Cannot list .tundracode directory".to_string(),
            ));
        }

        match tokio::fs::read_dir(&full_path).await {
            Ok(mut entries) => {
                let mut items = Vec::new();
                while let Ok(Some(entry)) = entries.next_entry().await {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let is_dir = entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
                    items.push(format!(
                        "{}{}",
                        if is_dir { "[DIR] " } else { "[FILE] " },
                        name
                    ));
                }
                Ok(ToolResult::ok(items.join("\n")))
            }
            Err(e) => Ok(ToolResult::err(e.to_string())),
        }
    }
}

pub struct GetWorkspaceTool;

#[async_trait]
impl Tool for GetWorkspaceTool {
    fn name(&self) -> &'static str {
        "GetWorkspace"
    }
    fn description(&self) -> &'static str {
        "Returns the general project structure, excluding .tundracode, target, node_modules, .git"
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }
    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::ListDirectory { path_pattern: None }]
    }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }
    async fn execute(
        &self,
        context: &ToolContext,
        _params: Value,
    ) -> Result<ToolResult, ToolError> {
        let workspace = Path::new(&context.workspace_path);
        let excluded = [".tundracode", "target", "node_modules", ".git"];

        fn walk_dir(dir: &Path, prefix: &str, excluded: &[&str]) -> Vec<String> {
            let mut result = Vec::new();
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if excluded.contains(&name.as_str()) {
                        continue;
                    }
                    let path = entry.path();
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    result.push(format!("{}{}", prefix, name));
                    if is_dir {
                        result.extend(walk_dir(&path, &format!("{}  ", prefix), excluded));
                    }
                }
            }
            result
        }

        let tree = walk_dir(workspace, "", &excluded);
        Ok(ToolResult::ok(tree.join("\n")))
    }
}

fn validate_plan_path(context: &ToolContext, path: &str) -> Result<std::path::PathBuf, ToolError> {
    if !path.starts_with(".tundracode/plans/") && !path.starts_with(".tundracode/plans") {
        return Err(ToolError::ExecutionFailed(format!(
            "Path '{}' must be inside .tundracode/plans/",
            path
        )));
    }
    let workspace = std::path::Path::new(&context.workspace_path);
    let full_path = workspace.join(path);
    Ok(full_path)
}

pub struct PlanCreateFileTool;

#[async_trait]
impl Tool for PlanCreateFileTool {
    fn name(&self) -> &'static str {
        "PlanCreateFile"
    }
    fn description(&self) -> &'static str {
        "PlanCreateFile: creates a plan file. Only writes to .tundracode/plans/. Fails if exists. Params: p (path), c (content)"
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::Planning
    }
    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::FileWrite { path_pattern: None }]
    }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({"properties":{"p":{"type":"string"},"c":{"type":"string"}},"required":["p","c"]})
    }
    async fn execute(&self, context: &ToolContext, params: Value) -> Result<ToolResult, ToolError> {
        let path = params["p"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParameters("p required".to_string()))?;
        let content = params["c"].as_str().unwrap_or("");
        let full_path = validate_plan_path(context, path)?;

        if full_path.exists() {
            return Err(ToolError::ExecutionFailed(format!(
                "Plan file already exists: {}",
                path
            )));
        }

        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                ToolError::ExecutionFailed(format!("Cannot create directory: {}", e))
            })?;
        }

        match tokio::fs::write(&full_path, content).await {
            Ok(_) => Ok(ToolResult::ok(format!("Plan {} created", path))
                .with_prior(None, Some(path.to_string()))
                .with_resulting_content(Some(content.to_string()))),
            Err(e) => Ok(ToolResult::err(e.to_string())),
        }
    }
}

pub struct PlanWriteFileTool;

#[async_trait]
impl Tool for PlanWriteFileTool {
    fn name(&self) -> &'static str {
        "PlanWriteFile"
    }
    fn description(&self) -> &'static str {
        "PlanWriteFile: writes to a plan file. Only writes to .tundracode/plans/. Params: p (path), c (content)"
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::Planning
    }
    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::FileWrite { path_pattern: None }]
    }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({"properties":{"p":{"type":"string"},"c":{"type":"string"}},"required":["p","c"]})
    }
    async fn execute(&self, context: &ToolContext, params: Value) -> Result<ToolResult, ToolError> {
        let path = params["p"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParameters("p required".to_string()))?;
        let content = params["c"].as_str().unwrap_or("");
        let full_path = validate_plan_path(context, path)?;

        let prior = tokio::fs::read_to_string(&full_path).await.ok();

        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                ToolError::ExecutionFailed(format!("Cannot create directory: {}", e))
            })?;
        }

        match tokio::fs::write(&full_path, content).await {
            Ok(_) => Ok(ToolResult::ok(format!("Plan {} written", path))
                .with_prior(prior, Some(path.to_string()))
                .with_resulting_content(Some(content.to_string()))),
            Err(e) => Ok(ToolResult::err(e.to_string())),
        }
    }
}
