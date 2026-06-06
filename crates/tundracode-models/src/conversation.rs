use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallPayload>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallPayload {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

impl Default for Conversation {
    fn default() -> Self {
        Self::new()
    }
}

impl Conversation {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
        }
    }

    pub fn add_message(&mut self, role: MessageRole, content: String) {
        self.messages.push(Message {
            role,
            content,
            tool_calls: None,
            tool_call_id: None,
        });
    }

    pub fn add_assistant_with_tool_calls(
        &mut self,
        content: String,
        calls: Vec<ToolCallPayload>,
    ) {
        self.messages.push(Message {
            role: MessageRole::Assistant,
            content,
            tool_calls: if calls.is_empty() { None } else { Some(calls) },
            tool_call_id: None,
        });
    }

    pub fn add_tool_result(&mut self, call_id: String, output: String) {
        self.messages.push(Message {
            role: MessageRole::Tool,
            content: output,
            tool_calls: None,
            tool_call_id: Some(call_id),
        });
    }
}
