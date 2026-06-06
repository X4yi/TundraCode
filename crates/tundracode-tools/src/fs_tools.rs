use async_trait::async_trait;
use serde_json::Value;
use std::path::Path;

use crate::{Tool, ToolContext, ToolError, ToolResult};

pub fn validate_path(context: &ToolContext, path: &str) -> Result<std::path::PathBuf, ToolError> {
    let workspace = Path::new(&context.workspace_path);
    let full_path = workspace.join(path);

    let canonical_workspace = workspace
        .canonicalize()
        .map_err(|e| ToolError::ExecutionFailed(format!("Cannot resolve workspace: {}", e)))?;

    let canonical = full_path
        .canonicalize()
        .unwrap_or_else(|_| full_path.clone());

    if !canonical.starts_with(&canonical_workspace) {
        return Err(ToolError::ExecutionFailed(format!(
            "Path '{}' is outside the workspace",
            path
        )));
    }

    Ok(full_path)
}

fn is_tundracode_path(path: &std::path::Path) -> bool {
    path.components().any(|c| {
        c.as_os_str()
            .to_str()
            .map(|s| s == ".tundracode")
            .unwrap_or(false)
    })
}

pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &'static str {
        "ReadFile"
    }
    fn description(&self) -> &'static str {
        "Lee el contenido de un archivo del workspace"
    }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Ruta del archivo relativa al workspace" }
            },
            "required": ["path"]
        })
    }
    async fn execute(&self, context: &ToolContext, params: Value) -> Result<ToolResult, ToolError> {
        let path = params["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParameters("path required".to_string()))?;
        let full_path = validate_path(context, path)?;

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
        "Escribe contenido completo en un archivo"
    }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" }
            },
            "required": ["path", "content"]
        })
    }
    async fn execute(&self, context: &ToolContext, params: Value) -> Result<ToolResult, ToolError> {
        let path = params["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParameters("path required".to_string()))?;
        let content = params["content"].as_str().unwrap_or("");
        let full_path = validate_path(context, path)?;

        if is_tundracode_path(&full_path) {
            return Err(ToolError::ExecutionFailed(
                "Cannot write to .tundracode directory".to_string(),
            ));
        }

        let prior = tokio::fs::read_to_string(&full_path).await.ok();

        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                ToolError::ExecutionFailed(format!("Cannot create directory: {}", e))
            })?;
        }

        match tokio::fs::write(&full_path, content).await {
            Ok(_) => Ok(ToolResult::ok(format!("Archivo {} escrito", path))
                .with_prior(prior, Some(path.to_string()))),
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
        "Crea un archivo nuevo. Falla si el archivo ya existe."
    }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string", "default": "" }
            },
            "required": ["path"]
        })
    }
    async fn execute(&self, context: &ToolContext, params: Value) -> Result<ToolResult, ToolError> {
        let path = params["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParameters("path required".to_string()))?;
        let content = params["content"].as_str().unwrap_or("");
        let full_path = validate_path(context, path)?;

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

        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                ToolError::ExecutionFailed(format!("Cannot create directory: {}", e))
            })?;
        }

        match tokio::fs::write(&full_path, content).await {
            Ok(_) => Ok(ToolResult::ok(format!("Archivo {} creado", path))
                .with_prior(None, Some(path.to_string()))),
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
        "Elimina un archivo. Requiere confirmacion del usuario."
    }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" }
            },
            "required": ["path"]
        })
    }
    async fn execute(&self, context: &ToolContext, params: Value) -> Result<ToolResult, ToolError> {
        let path = params["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParameters("path required".to_string()))?;
        let full_path = validate_path(context, path)?;

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

        match tokio::fs::remove_file(&full_path).await {
            Ok(_) => Ok(ToolResult::ok(format!("Archivo {} eliminado", path))
                .with_prior(prior, Some(path.to_string()))),
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
        "Lista el contenido de un directorio"
    }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" }
            },
            "required": ["path"]
        })
    }
    async fn execute(&self, context: &ToolContext, params: Value) -> Result<ToolResult, ToolError> {
        let path = params["path"].as_str().unwrap_or(".");
        let full_path = validate_path(context, path)?;

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
        "Devuelve la estructura general del proyecto, excluyendo .tundracode, target, node_modules, .git"
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
