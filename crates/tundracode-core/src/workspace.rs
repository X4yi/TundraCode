use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub root_path: PathBuf,
    pub name: String,
}

impl Workspace {
    pub fn new(root_path: impl AsRef<Path>) -> Self {
        let root_path = root_path.as_ref().to_path_buf();
        let name = root_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed")
            .to_string();

        Self { root_path, name }
    }

    pub fn list_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.root_path) {
            for entry in entries.flatten() {
                files.push(entry.path());
            }
        }
        files
    }
}
