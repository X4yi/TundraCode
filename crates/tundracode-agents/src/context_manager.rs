use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextBudget {
    pub max_tokens: u32,
    pub reserved_for_system: u32,
    pub reserved_for_response: u32,
    pub available_for_context: u32,
}

impl ContextBudget {
    pub fn new(max_context_tokens: u32) -> Self {
        let reserved_for_system = 4096;
        let reserved_for_response = 8192;
        let available_for_context = max_context_tokens
            .saturating_sub(reserved_for_system)
            .saturating_sub(reserved_for_response);

        Self {
            max_tokens: max_context_tokens,
            reserved_for_system,
            reserved_for_response,
            available_for_context,
        }
    }

    pub fn usage_ratio(&self, current_tokens: u32) -> f32 {
        if self.available_for_context == 0 {
            return 1.0;
        }
        current_tokens as f32 / self.available_for_context as f32
    }

    pub fn should_compact(&self, current_tokens: u32) -> bool {
        self.usage_ratio(current_tokens) > 0.7
    }

    pub fn remaining(&self, current_tokens: u32) -> u32 {
        self.available_for_context.saturating_sub(current_tokens)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEntry {
    pub id: String,
    pub entry_type: ContextEntryType,
    pub content: String,
    pub token_estimate: u32,
    pub priority: u32,
    pub created_at: u64,
    pub last_accessed: u64,
    pub compacted: bool,
    pub compaction_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContextEntryType {
    SystemPrompt,
    UserProfile,
    ProjectMemory,
    TaskSummary,
    ToolOutput,
    ConversationMessage,
    SubagentResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextManager {
    pub entries: Vec<ContextEntry>,
    pub budget: ContextBudget,
    pub total_estimated_tokens: u32,
}

impl ContextManager {
    pub fn new(max_context_tokens: u32) -> Self {
        Self {
            entries: Vec::new(),
            budget: ContextBudget::new(max_context_tokens),
            total_estimated_tokens: 0,
        }
    }

    pub fn add_entry(&mut self, entry: ContextEntry) {
        self.total_estimated_tokens += entry.token_estimate;
        self.entries.push(entry);
    }

    pub fn remove_entry(&mut self, id: &str) -> Option<ContextEntry> {
        if let Some(pos) = self.entries.iter().position(|e| e.id == id) {
            let entry = self.entries.remove(pos);
            self.total_estimated_tokens = self.total_estimated_tokens.saturating_sub(entry.token_estimate);
            Some(entry)
        } else {
            None
        }
    }

    pub fn compact_entry(&mut self, id: &str, summary: String) -> bool {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            let old_tokens = entry.token_estimate;
            entry.compacted = true;
            entry.compaction_summary = Some(summary.clone());
            let new_tokens = estimate_tokens(&summary);
            entry.content = summary;
            entry.token_estimate = new_tokens;
            self.total_estimated_tokens = self.total_estimated_tokens
                .saturating_sub(old_tokens)
                .saturating_add(new_tokens);
            true
        } else {
            false
        }
    }

    pub fn should_compact(&self) -> bool {
        self.budget.should_compact(self.total_estimated_tokens)
    }

    pub fn compactable_entries(&self) -> Vec<&ContextEntry> {
        self.entries
            .iter()
            .filter(|e| {
                !e.compacted
                    && e.entry_type != ContextEntryType::SystemPrompt
                    && e.entry_type != ContextEntryType::UserProfile
            })
            .collect()
    }

    pub fn oldest_compactable(&self) -> Option<&ContextEntry> {
        self.entries
            .iter()
            .filter(|e| {
                !e.compacted
                    && e.entry_type != ContextEntryType::SystemPrompt
                    && e.entry_type != ContextEntryType::UserProfile
            })
            .min_by_key(|e| e.last_accessed)
    }

    pub fn build_context_string(&self) -> String {
        let mut parts = Vec::new();

        let mut sorted_entries: Vec<&ContextEntry> = self.entries.iter().collect();
        sorted_entries.sort_by(|a, b| {
            a.entry_type
                .cmp(&b.entry_type)
                .then(b.priority.cmp(&a.priority))
        });

        for entry in &sorted_entries {
            if entry.compacted {
                if let Some(ref summary) = entry.compaction_summary {
                    parts.push(format!("[Compacted: {}]", summary));
                }
            } else {
                parts.push(entry.content.clone());
            }
        }

        parts.join("\n\n")
    }

    pub fn estimate_tokens_for_entry(content: &str) -> u32 {
        estimate_tokens(content)
    }
}

pub fn estimate_tokens(text: &str) -> u32 {
    let char_count = text.len() as u32;
    char_count / 4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_budget() {
        let budget = ContextBudget::new(128_000);
        assert_eq!(budget.max_tokens, 128_000);
        assert!(budget.available_for_context > 0);
        assert!(budget.available_for_context < 128_000);
    }

    #[test]
    fn test_should_compact() {
        let budget = ContextBudget::new(128_000);
        assert!(!budget.should_compact(budget.available_for_context / 2));
        assert!(budget.should_compact(budget.available_for_context - 100));
    }

    #[test]
    fn test_context_manager() {
        let mut manager = ContextManager::new(128_000);
        manager.add_entry(ContextEntry {
            id: "test".to_string(),
            entry_type: ContextEntryType::ToolOutput,
            content: "test content".to_string(),
            token_estimate: 3,
            priority: 1,
            created_at: 0,
            last_accessed: 0,
            compacted: false,
            compaction_summary: None,
        });
        assert_eq!(manager.entries.len(), 1);
        assert_eq!(manager.total_estimated_tokens, 3);
    }

    #[test]
    fn test_compact_entry() {
        let mut manager = ContextManager::new(128_000);
        manager.add_entry(ContextEntry {
            id: "test".to_string(),
            entry_type: ContextEntryType::ToolOutput,
            content: "a".repeat(1000),
            token_estimate: 250,
            priority: 1,
            created_at: 0,
            last_accessed: 0,
            compacted: false,
            compaction_summary: None,
        });

        assert!(manager.compact_entry("test", "summary".to_string()));
        let entry = manager.entries.iter().find(|e| e.id == "test").unwrap();
        assert!(entry.compacted);
        assert!(entry.token_estimate < 250);
    }

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens("hello"), 1);
        assert_eq!(estimate_tokens("12345678"), 2);
    }
}
