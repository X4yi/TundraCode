use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMemory {
    pub messages: Vec<MemoryMessage>,
    pub summary: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMessage {
    pub role: String,
    pub content: String,
    pub timestamp: String,
    pub token_estimate: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMemory {
    pub state_summary: String,
    pub completed_features: Vec<String>,
    pub pending_features: Vec<String>,
    pub known_issues: Vec<String>,
    pub architecture_notes: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMemory {
    pub task_id: String,
    pub title: String,
    pub result_summary: String,
    pub files_modified: Vec<String>,
    pub tokens_used: u32,
    pub completed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMemory {
    pub active_files: Vec<String>,
    pub recent_decisions: Vec<String>,
    pub injected_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStore {
    pub session: SessionMemory,
    pub project: ProjectMemory,
    pub tasks: Vec<TaskMemory>,
    pub context: ContextMemory,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            session: SessionMemory {
                messages: Vec::new(),
                summary: None,
                created_at: chrono::Utc::now().to_rfc3339(),
                updated_at: chrono::Utc::now().to_rfc3339(),
            },
            project: ProjectMemory {
                state_summary: String::new(),
                completed_features: Vec::new(),
                pending_features: Vec::new(),
                known_issues: Vec::new(),
                architecture_notes: String::new(),
                updated_at: chrono::Utc::now().to_rfc3339(),
            },
            tasks: Vec::new(),
            context: ContextMemory {
                active_files: Vec::new(),
                recent_decisions: Vec::new(),
                injected_at: chrono::Utc::now().to_rfc3339(),
            },
        }
    }

    pub fn add_message(&mut self, role: &str, content: &str) {
        let token_estimate = content.len() as u32 / 4;
        self.session.messages.push(MemoryMessage {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            token_estimate,
        });
        self.session.updated_at = chrono::Utc::now().to_rfc3339();
    }

    pub fn add_task_result(&mut self, task: TaskMemory) {
        self.tasks.push(task);
    }

    pub fn update_project_state(&mut self, summary: String) {
        self.project.state_summary = summary;
        self.project.updated_at = chrono::Utc::now().to_rfc3339();
    }

    pub fn add_completed_feature(&mut self, feature: String) {
        self.project.completed_features.push(feature);
        self.project.updated_at = chrono::Utc::now().to_rfc3339();
    }

    pub fn add_pending_feature(&mut self, feature: String) {
        self.project.pending_features.push(feature);
        self.project.updated_at = chrono::Utc::now().to_rfc3339();
    }

    pub fn add_known_issue(&mut self, issue: String) {
        self.project.known_issues.push(issue);
        self.project.updated_at = chrono::Utc::now().to_rfc3339();
    }

    pub fn set_active_files(&mut self, files: Vec<String>) {
        self.context.active_files = files;
        self.context.injected_at = chrono::Utc::now().to_rfc3339();
    }

    pub fn add_decision(&mut self, decision: String) {
        self.context.recent_decisions.push(decision);
        if self.context.recent_decisions.len() > 10 {
            self.context.recent_decisions.remove(0);
        }
    }

    pub fn session_summary(&self) -> String {
        if let Some(ref summary) = self.session.summary {
            return summary.clone();
        }

        let msg_count = self.session.messages.len();
        let total_tokens: u32 = self
            .session
            .messages
            .iter()
            .map(|m| m.token_estimate)
            .sum();

        format!(
            "Session: {} messages, ~{} tokens",
            msg_count, total_tokens
        )
    }

    pub fn project_summary(&self) -> String {
        let mut parts = Vec::new();

        if !self.project.state_summary.is_empty() {
            parts.push(self.project.state_summary.clone());
        }

        if !self.project.completed_features.is_empty() {
            parts.push(format!(
                "Completed: {}",
                self.project.completed_features.join(", ")
            ));
        }

        if !self.project.pending_features.is_empty() {
            parts.push(format!(
                "Pending: {}",
                self.project.pending_features.join(", ")
            ));
        }

        if !self.project.known_issues.is_empty() {
            parts.push(format!(
                "Issues: {}",
                self.project.known_issues.join(", ")
            ));
        }

        if parts.is_empty() {
            "No project state recorded".to_string()
        } else {
            parts.join("\n")
        }
    }

    pub fn task_summaries(&self) -> String {
        if self.tasks.is_empty() {
            return "No tasks completed yet".to_string();
        }

        self.tasks
            .iter()
            .map(|t| format!("[x] {} - {}", t.title, t.result_summary))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn context_summary(&self) -> String {
        let mut parts = Vec::new();

        if !self.context.active_files.is_empty() {
            parts.push(format!(
                "Active files: {}",
                self.context.active_files.join(", ")
            ));
        }

        if !self.context.recent_decisions.is_empty() {
            let recent: Vec<&str> = self.context.recent_decisions.iter().rev().take(3).map(|s| s.as_str()).collect();
            parts.push(format!("Recent decisions: {}", recent.join("; ")));
        }

        if parts.is_empty() {
            "No active context".to_string()
        } else {
            parts.join("\n")
        }
    }

    pub fn full_context_injection(&self) -> String {
        let mut parts = Vec::new();

        let project_summary = self.project_summary();
        if !project_summary.is_empty() && project_summary != "No project state recorded" {
            parts.push(format!("Project State:\n{}", project_summary));
        }

        let task_summaries = self.task_summaries();
        if task_summaries != "No tasks completed yet" {
            parts.push(format!("Completed Tasks:\n{}", task_summaries));
        }

        let context_summary = self.context_summary();
        if context_summary != "No active context" {
            parts.push(format!("Active Context:\n{}", context_summary));
        }

        if parts.is_empty() {
            String::new()
        } else {
            parts.join("\n\n")
        }
    }

    pub fn save_to_file(&self, path: &Path) -> Result<(), String> {
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize memory: {}", e))?;
        std::fs::write(path, content).map_err(|e| format!("Failed to write memory: {}", e))
    }

    pub fn load_from_file(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read memory: {}", e))?;
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse memory: {}", e))
    }

    pub fn load_from_markdown(path: &Path) -> Self {
        let content = std::fs::read_to_string(path).unwrap_or_default();
        let mut store = Self::new();
        store.project.state_summary = content;
        store
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

pub fn memory_dir(workspace: &Path) -> PathBuf {
    workspace.join(".tundracode")
}

pub fn memory_path(workspace: &Path) -> PathBuf {
    memory_dir(workspace).join("memory.json")
}

pub fn legacy_memory_path(workspace: &Path) -> PathBuf {
    memory_dir(workspace).join("memory.md")
}

pub fn load_memory(workspace: &Path) -> MemoryStore {
    let json_path = memory_path(workspace);
    if json_path.exists() {
        MemoryStore::load_from_file(&json_path).unwrap_or_else(|_| {
            let md_path = legacy_memory_path(workspace);
            if md_path.exists() {
                MemoryStore::load_from_markdown(&md_path)
            } else {
                MemoryStore::new()
            }
        })
    } else {
        let md_path = legacy_memory_path(workspace);
        if md_path.exists() {
            MemoryStore::load_from_markdown(&md_path)
        } else {
            MemoryStore::new()
        }
    }
}

pub fn save_memory(workspace: &Path, store: &MemoryStore) -> Result<(), String> {
    let dir = memory_dir(workspace);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create memory dir: {}", e))?;
    store.save_to_file(&memory_path(workspace))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir() -> PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0) as u64;
        let dir = std::env::temp_dir().join(format!("tundra_mem_test_{}", nanos));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn test_memory_store_new() {
        let store = MemoryStore::new();
        assert!(store.session.messages.is_empty());
        assert!(store.tasks.is_empty());
    }

    #[test]
    fn test_add_message() {
        let mut store = MemoryStore::new();
        store.add_message("user", "Hello");
        assert_eq!(store.session.messages.len(), 1);
        assert_eq!(store.session.messages[0].role, "user");
    }

    #[test]
    fn test_project_summary() {
        let mut store = MemoryStore::new();
        store.add_completed_feature("MCP support".to_string());
        store.add_pending_feature("Telemetry".to_string());

        let summary = store.project_summary();
        assert!(summary.contains("MCP support"));
        assert!(summary.contains("Telemetry"));
    }

    #[test]
    fn test_task_summaries() {
        let mut store = MemoryStore::new();
        store.add_task_result(TaskMemory {
            task_id: "t1".to_string(),
            title: "Task 1".to_string(),
            result_summary: "Done".to_string(),
            files_modified: vec!["src/main.rs".to_string()],
            tokens_used: 100,
            completed_at: chrono::Utc::now().to_rfc3339(),
        });

        let summaries = store.task_summaries();
        assert!(summaries.contains("[x] Task 1"));
    }

    #[test]
    fn test_save_and_load() {
        let dir = temp_dir();
        let mut store = MemoryStore::new();
        store.add_message("user", "test message");
        store.add_completed_feature("feature1".to_string());

        save_memory(&dir, &store).unwrap();
        let loaded = load_memory(&dir);
        assert_eq!(loaded.session.messages.len(), 1);
        assert_eq!(loaded.project.completed_features.len(), 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_full_context_injection() {
        let mut store = MemoryStore::new();
        store.update_project_state("Project is active".to_string());
        store.add_completed_feature("feature1".to_string());

        let injection = store.full_context_injection();
        assert!(injection.contains("Project is active"));
        assert!(injection.contains("feature1"));
    }
}
