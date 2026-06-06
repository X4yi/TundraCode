use crate::{CoreResult, FileId};
use std::collections::HashMap;
use std::path::PathBuf;

pub struct FileManager {
    files: HashMap<FileId, FileEntry>,
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub content: String,
    pub modified: bool,
}

impl Default for FileManager {
    fn default() -> Self {
        Self::new()
    }
}

impl FileManager {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
        }
    }

    pub fn open_file(&mut self, path: PathBuf) -> CoreResult<FileId> {
        let content = std::fs::read_to_string(&path)?;
        let id = FileId(path.to_string_lossy().to_string());
        self.files.insert(
            id.clone(),
            FileEntry {
                path,
                content,
                modified: false,
            },
        );
        Ok(id)
    }

    pub fn get_file(&self, id: &FileId) -> Option<&FileEntry> {
        self.files.get(id)
    }
}
