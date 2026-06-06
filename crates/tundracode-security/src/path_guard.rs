use std::path::{Path, PathBuf};

pub fn ensure_within_workspace(workspace: &Path, target: &Path) -> Result<PathBuf, String> {
    let canonical_workspace = workspace
        .canonicalize()
        .map_err(|e| format!("Cannot resolve workspace: {}", e))?;

    let resolved = if target.is_absolute() {
        target.to_path_buf()
    } else {
        workspace.join(target)
    };

    let canonical = resolved
        .canonicalize()
        .unwrap_or_else(|_| resolved.clone());

    if !canonical.starts_with(&canonical_workspace) {
        return Err(format!(
            "Path '{}' is outside the workspace",
            target.display()
        ));
    }

    Ok(canonical)
}

pub fn is_tundracode_path(path: &Path) -> bool {
    path.components().any(|c| {
        c.as_os_str()
            .to_str()
            .map(|s| s == ".tundracode")
            .unwrap_or(false)
    })
}
