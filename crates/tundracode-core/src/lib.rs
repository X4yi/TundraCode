pub mod editor;
pub mod file_manager;
pub mod workspace;

use serde::{Deserialize, Serialize};


#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct FileId(pub String);


#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct WorkspacePath(pub String);


#[derive(thiserror::Error, Debug)]
pub enum CoreError {
    #[error("Archivo no encontrado: {0}")]
    FileNotFound(String),
    #[error("Directorio no encontrado: {0}")]
    DirectoryNotFound(String),
    #[error("Permiso denegado: {0}")]
    PermissionDenied(String),
    #[error("Operación inválida: {0}")]
    InvalidOperation(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub type CoreResult<T> = Result<T, CoreError>;
