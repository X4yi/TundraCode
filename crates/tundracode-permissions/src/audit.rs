use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::guard::ToolAction;
use crate::guard::PermissionResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: std::time::SystemTime,
    pub agent_id: String,
    pub action: String,
    pub result: AuditResult,
    pub duration_ms: u64,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditResult {
    Allowed,
    Denied { reason: String },
    Simulated { reason: String },
}

pub struct AuditLog {
    entries: Vec<AuditEntry>,
    max_entries: usize,
}

impl AuditLog {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::with_capacity(max_entries),
            max_entries,
        }
    }

    pub fn record(&mut self, agent_id: &str, action: &ToolAction, result: &PermissionResult, start: Instant) {
        let action_str = match action {
            ToolAction::ReadFile { path } => format!("ReadFile({:?})", path),
            ToolAction::WriteFile { path } => format!("WriteFile({:?})", path),
            ToolAction::DeleteFile { path } => format!("DeleteFile({:?})", path),
            ToolAction::ListDirectory { path } => format!("ListDirectory({:?})", path),
            ToolAction::ApplyPatch { path } => format!("ApplyPatch({:?})", path),
            ToolAction::RunCommand { command, args } => format!("RunCommand({} {})", command, args.join(" ")),
            ToolAction::SearchCodebase => "SearchCodebase".to_string(),
            ToolAction::SearchWeb { host } => format!("SearchWeb({:?})", host),
            ToolAction::GetDiagnostics => "GetDiagnostics".to_string(),
            ToolAction::SpawnSubagent { profile_id } => format!("SpawnSubagent({})", profile_id),
        };

        let audit_result = match result {
            PermissionResult::Allowed => AuditResult::Allowed,
            PermissionResult::Denied(e) => AuditResult::Denied { reason: e.clone() },
        };

        let entry = AuditEntry {
            timestamp: std::time::SystemTime::now(),
            agent_id: agent_id.to_string(),
            action: action_str,
            result: audit_result,
            duration_ms: start.elapsed().as_millis() as u64,
            details: None,
        };

        self.entries.push(entry);

        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
    }

    pub fn record_simulated(&mut self, agent_id: &str, action: &ToolAction, start: Instant) {
        let action_str = match action {
            ToolAction::ReadFile { path } => format!("ReadFile({:?})", path),
            ToolAction::WriteFile { path } => format!("WriteFile({:?})", path),
            ToolAction::DeleteFile { path } => format!("DeleteFile({:?})", path),
            ToolAction::ListDirectory { path } => format!("ListDirectory({:?})", path),
            ToolAction::ApplyPatch { path } => format!("ApplyPatch({:?})", path),
            ToolAction::RunCommand { command, args } => format!("RunCommand({} {})", command, args.join(" ")),
            ToolAction::SearchCodebase => "SearchCodebase".to_string(),
            ToolAction::SearchWeb { host } => format!("SearchWeb({:?})", host),
            ToolAction::GetDiagnostics => "GetDiagnostics".to_string(),
            ToolAction::SpawnSubagent { profile_id } => format!("SpawnSubagent({})", profile_id),
        };

        let entry = AuditEntry {
            timestamp: std::time::SystemTime::now(),
            agent_id: agent_id.to_string(),
            action: action_str,
            result: AuditResult::Simulated { reason: "dry_run mode".to_string() },
            duration_ms: start.elapsed().as_millis() as u64,
            details: None,
        };

        self.entries.push(entry);

        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
    }

    pub fn entries(&self) -> &[AuditEntry] {
        &self.entries
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn denied_count(&self) -> usize {
        self.entries.iter().filter(|e| matches!(e.result, AuditResult::Denied { .. })).count()
    }

    pub fn allowed_count(&self) -> usize {
        self.entries.iter().filter(|e| matches!(e.result, AuditResult::Allowed)).count()
    }
}
