use serde::{Deserialize, Serialize};

use crate::context_manager::{ContextEntry, ContextEntryType, ContextManager};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionConfig {
    pub tool_output_max_age_ms: u64,
    pub reasoning_max_age_ms: u64,
    pub session_summary_threshold: u32,
    pub enable_level1: bool,
    pub enable_level2: bool,
    pub enable_level3: bool,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            tool_output_max_age_ms: 300_000,
            reasoning_max_age_ms: 600_000,
            session_summary_threshold: 50_000,
            enable_level1: true,
            enable_level2: true,
            enable_level3: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionResult {
    pub entries_compacted: usize,
    pub tokens_saved: u32,
    pub summaries_generated: Vec<String>,
}

pub struct ContextCompactor {
    config: CompactionConfig,
}

impl ContextCompactor {
    pub fn new(config: CompactionConfig) -> Self {
        Self { config }
    }

    pub fn compact(&self, manager: &mut ContextManager) -> CompactionResult {
        let mut result = CompactionResult {
            entries_compacted: 0,
            tokens_saved: 0,
            summaries_generated: Vec::new(),
        };

        if self.config.enable_level1 {
            let level1 = self.compact_tool_outputs(manager);
            result.entries_compacted += level1.0;
            result.tokens_saved += level1.1;
        }

        if self.config.enable_level2 {
            let level2 = self.compact_reasoning_chains(manager);
            result.entries_compacted += level2.0;
            result.tokens_saved += level2.1;
        }

        if self.config.enable_level3 && manager.should_compact() {
            let level3 = self.compact_session(manager);
            result.entries_compacted += level3.0;
            result.tokens_saved += level3.1;
            if let Some(summary) = level3.2 {
                result.summaries_generated.push(summary);
            }
        }

        result
    }

    fn compact_tool_outputs(&self, manager: &mut ContextManager) -> (usize, u32) {
        let mut compacted = 0;
        let mut tokens_saved = 0;
        let now = now_millis();

        let entries_to_compact: Vec<String> = manager
            .entries
            .iter()
            .filter(|e| {
                e.entry_type == ContextEntryType::ToolOutput
                    && !e.compacted
                    && (now.saturating_sub(e.created_at)) > self.config.tool_output_max_age_ms
            })
            .map(|e| e.id.clone())
            .collect();

        for id in entries_to_compact {
            if let Some(entry) = manager.entries.iter().find(|e| e.id == id) {
                let old_tokens = entry.token_estimate;
                let summary = self.summarize_tool_output(&entry.content);
                if manager.compact_entry(&id, summary) {
                    compacted += 1;
                    tokens_saved += old_tokens.saturating_sub(
                        ContextManager::estimate_tokens_for_entry("compacted"),
                    );
                }
            }
        }

        (compacted, tokens_saved)
    }

    fn compact_reasoning_chains(&self, manager: &mut ContextManager) -> (usize, u32) {
        let mut compacted = 0;
        let mut tokens_saved = 0;
        let now = now_millis();

        let entries_to_compact: Vec<String> = manager
            .entries
            .iter()
            .filter(|e| {
                e.entry_type == ContextEntryType::ConversationMessage
                    && !e.compacted
                    && e.content.len() > 500
                    && (now.saturating_sub(e.created_at)) > self.config.reasoning_max_age_ms
            })
            .map(|e| e.id.clone())
            .collect();

        for id in entries_to_compact {
            if let Some(entry) = manager.entries.iter().find(|e| e.id == id) {
                let old_tokens = entry.token_estimate;
                let summary = self.summarize_reasoning(&entry.content);
                if manager.compact_entry(&id, summary.clone()) {
                    compacted += 1;
                    tokens_saved += old_tokens.saturating_sub(
                        ContextManager::estimate_tokens_for_entry(&summary),
                    );
                }
            }
        }

        (compacted, tokens_saved)
    }

    fn compact_session(&self, manager: &mut ContextManager) -> (usize, u32, Option<String>) {
        let mut compacted = 0;
        let mut tokens_saved = 0;

        let compactable: Vec<String> = manager
            .entries
            .iter()
            .filter(|e| {
                !e.compacted
                    && e.entry_type != ContextEntryType::SystemPrompt
                    && e.entry_type != ContextEntryType::UserProfile
                    && e.entry_type != ContextEntryType::ProjectMemory
            })
            .map(|e| e.id.clone())
            .collect();

        let mut session_summary_parts = Vec::new();

        for id in &compactable {
            if let Some(entry) = manager.entries.iter().find(|e| e.id == *id) {
                let old_tokens = entry.token_estimate;
                let summary = self.summarize_entry(entry);
                if manager.compact_entry(id, summary.clone()) {
                    compacted += 1;
                    tokens_saved += old_tokens.saturating_sub(
                        ContextManager::estimate_tokens_for_entry(&summary),
                    );
                    session_summary_parts.push(summary);
                }
            }
        }

        let session_summary = if !session_summary_parts.is_empty() {
            Some(format!(
                "Session summary: {}",
                session_summary_parts.join("; ")
            ))
        } else {
            None
        };

        (compacted, tokens_saved, session_summary)
    }

    fn summarize_tool_output(&self, content: &str) -> String {
        let lines: Vec<&str> = content.lines().collect();
        let line_count = lines.len();

        if line_count <= 3 {
            return content.to_string();
        }

        let preview: Vec<&str> = lines.iter().take(3).copied().collect();
        format!(
            "{}... [{} lines, compacted]",
            preview.join("\n"),
            line_count
        )
    }

    fn summarize_reasoning(&self, content: &str) -> String {
        if content.len() < 200 {
            return content.to_string();
        }

        let sentences: Vec<&str> = content.split('.').collect();
        let key_sentences: Vec<&str> = sentences.iter().take(3).copied().collect();

        format!(
            "Previous reasoning: {}... [compressed]",
            key_sentences.join(".")
        )
    }

    fn summarize_entry(&self, entry: &ContextEntry) -> String {
        match entry.entry_type {
            ContextEntryType::ToolOutput => self.summarize_tool_output(&entry.content),
            ContextEntryType::ConversationMessage => self.summarize_reasoning(&entry.content),
            ContextEntryType::SubagentResult => {
                if entry.content.len() > 200 {
                    format!("{}... [compacted]", &entry.content[..200])
                } else {
                    entry.content.clone()
                }
            }
            _ => entry.content.clone(),
        }
    }
}

fn now_millis() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compactor_default_config() {
        let config = CompactionConfig::default();
        assert!(config.enable_level1);
        assert!(config.enable_level2);
        assert!(config.enable_level3);
    }

    #[test]
    fn test_summarize_tool_output() {
        let compactor = ContextCompactor::new(CompactionConfig::default());
        let short = "short output";
        assert_eq!(compactor.summarize_tool_output(short), short);

        let long = "line1\nline2\nline3\nline4\nline5";
        let summary = compactor.summarize_tool_output(long);
        assert!(summary.contains("compacted"));
        assert!(summary.contains("5 lines"));
    }

    #[test]
    fn test_compact_tool_outputs() {
        let compactor = ContextCompactor::new(CompactionConfig {
            tool_output_max_age_ms: 0,
            ..CompactionConfig::default()
        });

        let mut manager = ContextManager::new(128_000);
        manager.add_entry(ContextEntry {
            id: "tool1".to_string(),
            entry_type: ContextEntryType::ToolOutput,
            content: "line1\nline2\nline3\nline4\nline5\nline6".to_string(),
            token_estimate: 100,
            priority: 1,
            created_at: 0,
            last_accessed: 0,
            compacted: false,
            compaction_summary: None,
        });

        let result = compactor.compact(&mut manager);
        assert!(result.entries_compacted > 0);
        assert!(result.tokens_saved > 0);
    }
}
