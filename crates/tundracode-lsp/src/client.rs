use anyhow::Result;
use tokio::process::Command;
use tracing::{info, warn};

pub struct LspClient {
    server_path: String,
    language_id: String,
    is_running: bool,
}

impl Default for LspClient {
    fn default() -> Self {
        Self::new("rust-analyzer", "rust")
    }
}

impl LspClient {
    pub fn new(server_path: impl Into<String>, language_id: impl Into<String>) -> Self {
        Self {
            server_path: server_path.into(),
            language_id: language_id.into(),
            is_running: false,
        }
    }

    pub async fn start(&mut self) -> Result<()> {
        info!(
            "Starting LSP server: {} for language: {}",
            self.server_path, self.language_id
        );

        
        
        let output = Command::new(&self.server_path)
            .arg("--version")
            .output()
            .await;

        match output {
            Ok(out) => {
                let version = String::from_utf8_lossy(&out.stdout);
                info!(
                    "LSP server {} available: {}",
                    self.server_path,
                    version.trim()
                );
                self.is_running = true;
                Ok(())
            }
            Err(e) => {
                warn!("LSP server {} not available: {}", self.server_path, e);
                Err(anyhow::anyhow!("LSP server not found: {}", e))
            }
        }
    }

    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping LSP server: {}", self.language_id);
        self.is_running = false;
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.is_running
    }

    pub fn language_id(&self) -> &str {
        &self.language_id
    }
}


#[derive(Debug, Clone)]
pub struct LanguageServer {
    pub name: String,
    pub command: String,
    pub language_id: String,
    pub extensions: Vec<String>,
}

impl LanguageServer {
    pub fn rust() -> Self {
        Self {
            name: "rust-analyzer".to_string(),
            command: "rust-analyzer".to_string(),
            language_id: "rust".to_string(),
            extensions: vec!["rs".to_string()],
        }
    }

    pub fn java() -> Self {
        Self {
            name: "Eclipse JDT LS".to_string(),
            command: "jdtls".to_string(),
            language_id: "java".to_string(),
            extensions: vec!["java".to_string()],
        }
    }

    pub fn python() -> Self {
        Self {
            name: "Pyright".to_string(),
            command: "pyright-langserver".to_string(),
            language_id: "python".to_string(),
            extensions: vec!["py".to_string(), "pyi".to_string()],
        }
    }

    pub fn all() -> Vec<Self> {
        vec![Self::rust(), Self::java(), Self::python()]
    }

    
    pub async fn is_available(&self) -> bool {
        Command::new(&self.command)
            .arg("--version")
            .output()
            .await
            .is_ok()
    }

    
    pub fn from_extension(ext: &str) -> Option<Self> {
        Self::all()
            .into_iter()
            .find(|server| server.extensions.iter().any(|e| e == ext))
    }
}
