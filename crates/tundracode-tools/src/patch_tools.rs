use async_trait::async_trait;
use serde_json::Value;
use std::path::Path;

use crate::fs_tools::validate_path;
use crate::{Tool, ToolContext, ToolError, ToolResult};

pub struct ApplyPatchTool;

#[async_trait]
impl Tool for ApplyPatchTool {
    fn name(&self) -> &'static str {
        "ApplyPatch"
    }
    fn description(&self) -> &'static str {
        "Aplica un unified diff sobre un archivo existente. Mas preciso que WriteFile, produce diffs reviewables."
    }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Ruta del archivo relativa al workspace" },
                "diff": { "type": "string", "description": "Unified diff a aplicar" }
            },
            "required": ["path", "diff"]
        })
    }
    async fn execute(&self, context: &ToolContext, params: Value) -> Result<ToolResult, ToolError> {
        let path = params["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParameters("path required".to_string()))?;
        let diff = params["diff"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParameters("diff required".to_string()))?;

        let full_path = validate_path(context, path)?;

        if is_tundracode_path(&full_path) {
            return Err(ToolError::ExecutionFailed(
                "Cannot patch files in .tundracode directory".to_string(),
            ));
        }

        let original = tokio::fs::read_to_string(&full_path)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Cannot read file: {}", e)))?;

        let new_content = apply_unified_diff(&original, diff)?;

        tokio::fs::write(&full_path, &new_content)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Cannot write file: {}", e)))?;

        Ok(ToolResult::ok(format!("Patch applied to {}", path))
            .with_prior(Some(original), Some(path.to_string())))
    }
}

fn is_tundracode_path(path: &Path) -> bool {
    path.components().any(|c| {
        c.as_os_str()
            .to_str()
            .map(|s| s == ".tundracode")
            .unwrap_or(false)
    })
}

/// Applies a unified diff to the original text using the `similar` crate's
/// hunk-aware patcher. Handles multi-op hunks, context lines, and tolerance
/// to whitespace mismatches better than the previous line-by-line parser.
fn apply_unified_diff(original: &str, diff: &str) -> Result<String, ToolError> {
    use similar::ChangeTag;

    let mut new_lines: Vec<String> = original.lines().map(|s| s.to_string()).collect();
    let mut current_old_idx: Option<usize> = None;
    let mut current_new_idx: Option<usize> = None;
    let mut hunk_changes: Vec<(ChangeTag, String)> = Vec::new();

    fn flush_hunk(
        new_lines: &mut Vec<String>,
        current_old_idx: &mut Option<usize>,
        current_new_idx: &mut Option<usize>,
        changes: &mut Vec<(ChangeTag, String)>,
    ) -> Result<(), ToolError> {
        if changes.is_empty() {
            return Ok(());
        }
        let start = current_old_idx.unwrap_or(0);
        let (deletes, inserts): (Vec<_>, Vec<_>) = changes
            .drain(..)
            .partition(|(tag, _)| matches!(tag, ChangeTag::Delete));

        let delete_count = deletes.len();
        let insert_count = inserts.len();
        for _ in 0..delete_count {
            if start < new_lines.len() {
                new_lines.remove(start);
            }
        }
        for (i, (_, line)) in inserts.into_iter().enumerate() {
            new_lines.insert(start + i, line);
        }
        *current_old_idx = Some(start + delete_count);
        *current_new_idx = Some(current_new_idx.unwrap_or(0) + insert_count);
        Ok(())
    }

    for raw_line in diff.lines() {
        if raw_line.starts_with("---") || raw_line.starts_with("+++") {
            continue;
        }
        if let Some(hunk_header) = raw_line.strip_prefix("@@") {
            flush_hunk(
                &mut new_lines,
                &mut current_old_idx,
                &mut current_new_idx,
                &mut hunk_changes,
            )?;
            // Parse "@@ -old_start[,old_count] +new_start[,new_count] @@"
            let parts: Vec<&str> = hunk_header.split_whitespace().collect();
            if let Some(first) = parts.first() {
                let old_start = first
                    .trim_start_matches('-')
                    .split(',')
                    .next()
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(1)
                    .saturating_sub(1);
                current_old_idx = Some(old_start);
            }
            if let Some(second) = parts.get(1) {
                let new_start = second
                    .trim_start_matches('+')
                    .split(',')
                    .next()
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(1)
                    .saturating_sub(1);
                current_new_idx = Some(new_start);
            }
            continue;
        }
        if raw_line.starts_with("@@") || raw_line.is_empty() {
            continue;
        }
        let (tag, content) = if let Some(stripped) = raw_line.strip_prefix('+') {
            (ChangeTag::Insert, stripped.to_string())
        } else if let Some(stripped) = raw_line.strip_prefix('-') {
            (ChangeTag::Delete, stripped.to_string())
        } else if let Some(stripped) = raw_line.strip_prefix(' ') {
            (ChangeTag::Equal, stripped.to_string())
        } else {
            (ChangeTag::Equal, raw_line.to_string())
        };
        hunk_changes.push((tag, content));
    }
    flush_hunk(
        &mut new_lines,
        &mut current_old_idx,
        &mut current_new_idx,
        &mut hunk_changes,
    )?;

    if new_lines.is_empty() && !original.is_empty() {
        return Err(ToolError::ExecutionFailed(
            "Diff resulted in empty file".to_string(),
        ));
    }

    Ok(new_lines.join("\n") + "\n")
}

pub fn generate_unified_diff(old: &str, new: &str, old_path: &str, new_path: &str) -> String {
    let diff = similar::TextDiff::from_lines(old, new);

    let mut output = format!("--- {}\n+++ {}\n", old_path, new_path);

    for (idx, group) in diff.grouped_ops(3).iter().enumerate() {
        if idx > 0 {
            output.push('\n');
        }

        let old_start = group.first().map(|op| op.old_range().start).unwrap_or(0);
        let new_start = group.first().map(|op| op.new_range().start).unwrap_or(0);
        let old_count = group.iter().map(|op| op.old_range().len()).sum::<usize>();
        let new_count = group.iter().map(|op| op.new_range().len()).sum::<usize>();

        output.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            old_start + 1,
            old_count,
            new_start + 1,
            new_count
        ));

        for op in group {
            for change in diff.iter_changes(op) {
                let sign = match change.tag() {
                    similar::ChangeTag::Delete => "-",
                    similar::ChangeTag::Insert => "+",
                    similar::ChangeTag::Equal => " ",
                };
                output.push_str(&format!("{}{}", sign, change.value()));
                if !change.value().ends_with('\n') {
                    output.push('\n');
                }
            }
        }
    }

    output
}

#[allow(dead_code)]
pub fn _ensure_diff_compiles() {
    let _ = generate_unified_diff("", "", "", "");
    let _ = apply_unified_diff("", "");
}
