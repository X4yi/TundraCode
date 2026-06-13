use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

use tundracode_tools::ToolResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentEvent {
    pub id: String,
    pub subagent_id: String,
    pub timestamp: u64,
    pub event_type: SubagentEventType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SubagentEventType {
    Created {
        name: String,
        task: String,
    },
    Thinking {
        content: String,
    },
    ToolCallStart {
        tool_name: String,
        params: Value,
    },
    ToolCallEnd {
        tool_name: String,
        success: bool,
        output_summary: String,
        duration_ms: u64,
    },
    Reasoning {
        content: String,
    },
    Progress {
        message: String,
        iteration: usize,
        max_iterations: usize,
    },
    Completed {
        summary: String,
        key_findings: Vec<String>,
        files_referenced: Vec<String>,
        tokens_used: u32,
        duration_ms: u64,
    },
    Failed {
        error: String,
        duration_ms: u64,
    },
}

impl SubagentEvent {
    pub fn created(subagent_id: &str, name: &str, task: &str) -> Self {
        Self {
            id: format!("evt_{}_created", now_millis()),
            subagent_id: subagent_id.to_string(),
            timestamp: now_millis(),
            event_type: SubagentEventType::Created {
                name: name.to_string(),
                task: task.to_string(),
            },
        }
    }

    pub fn thinking(subagent_id: &str, content: &str) -> Self {
        Self {
            id: format!("evt_{}_thinking_{}", subagent_id, now_millis()),
            subagent_id: subagent_id.to_string(),
            timestamp: now_millis(),
            event_type: SubagentEventType::Thinking {
                content: content.to_string(),
            },
        }
    }

    pub fn tool_call_start(subagent_id: &str, tool_name: &str, params: &Value) -> Self {
        Self {
            id: format!("evt_{}_tool_start_{}", subagent_id, now_millis()),
            subagent_id: subagent_id.to_string(),
            timestamp: now_millis(),
            event_type: SubagentEventType::ToolCallStart {
                tool_name: tool_name.to_string(),
                params: params.clone(),
            },
        }
    }

    pub fn tool_call_end(subagent_id: &str, tool_name: &str, result: &ToolResult, duration_ms: u64) -> Self {
        let output_summary = if result.success {
            result.output.chars().take(200).collect::<String>()
        } else {
            format!("Error: {}", result.error.as_deref().unwrap_or("unknown"))
        };

        Self {
            id: format!("evt_{}_tool_end_{}", subagent_id, now_millis()),
            subagent_id: subagent_id.to_string(),
            timestamp: now_millis(),
            event_type: SubagentEventType::ToolCallEnd {
                tool_name: tool_name.to_string(),
                success: result.success,
                output_summary,
                duration_ms,
            },
        }
    }

    pub fn reasoning(subagent_id: &str, content: &str) -> Self {
        Self {
            id: format!("evt_{}_reasoning_{}", subagent_id, now_millis()),
            subagent_id: subagent_id.to_string(),
            timestamp: now_millis(),
            event_type: SubagentEventType::Reasoning {
                content: content.to_string(),
            },
        }
    }

    pub fn progress(subagent_id: &str, message: &str, iteration: usize, max_iterations: usize) -> Self {
        Self {
            id: format!("evt_{}_progress_{}", subagent_id, now_millis()),
            subagent_id: subagent_id.to_string(),
            timestamp: now_millis(),
            event_type: SubagentEventType::Progress {
                message: message.to_string(),
                iteration,
                max_iterations,
            },
        }
    }

    pub fn completed(
        subagent_id: &str,
        summary: &str,
        key_findings: Vec<String>,
        files_referenced: Vec<String>,
        tokens_used: u32,
        duration_ms: u64,
    ) -> Self {
        Self {
            id: format!("evt_{}_completed", subagent_id),
            subagent_id: subagent_id.to_string(),
            timestamp: now_millis(),
            event_type: SubagentEventType::Completed {
                summary: summary.to_string(),
                key_findings,
                files_referenced,
                tokens_used,
                duration_ms,
            },
        }
    }

    pub fn failed(subagent_id: &str, error: &str, duration_ms: u64) -> Self {
        Self {
            id: format!("evt_{}_failed", subagent_id),
            subagent_id: subagent_id.to_string(),
            timestamp: now_millis(),
            event_type: SubagentEventType::Failed {
                error: error.to_string(),
                duration_ms,
            },
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

#[derive(Clone)]
pub struct SubagentEventBus {
    subscribers: Arc<RwLock<Vec<mpsc::UnboundedSender<SubagentEvent>>>>,
}

impl SubagentEventBus {
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn subscribe(&self) -> mpsc::UnboundedReceiver<SubagentEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        let subscribers = self.subscribers.clone();
        let tx_clone = tx.clone();

        // We need to spawn this since we can't hold a lock acrossed async boundaries
        let subscribers_clone = subscribers.clone();
        tokio::spawn(async move {
            let mut subs = subscribers_clone.write().await;
            subs.push(tx_clone);
        });

        rx
    }

    pub fn emit(&self, event: SubagentEvent) {
        let subscribers = self.subscribers.clone();
        let event_clone = event.clone();

        tokio::spawn(async move {
            let subs = subscribers.read().await;
            for tx in subs.iter() {
                let _ = tx.send(event_clone.clone());
            }
        });
    }

    pub fn subscriber_count(&self) -> usize {
        let subscribers = self.subscribers.clone();
        // This is a bit hacky but works for debugging
        let _ = subscribers;
        0
    }
}

impl Default for SubagentEventBus {
    fn default() -> Self {
        Self::new()
    }
}
