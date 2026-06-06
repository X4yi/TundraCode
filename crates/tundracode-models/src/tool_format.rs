use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::conversation::{Message, MessageRole, ToolCallPayload};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultContent {
    pub call_id: String,
    pub output: String,
}

pub fn format_tools_for_openai(tools: &[ToolDefinition]) -> Value {
    Value::Array(
        tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters
                    }
                })
            })
            .collect(),
    )
}

pub fn format_tools_for_anthropic(tools: &[ToolDefinition]) -> Value {
    Value::Array(
        tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.parameters
                })
            })
            .collect(),
    )
}

pub fn format_tools_for_google(tools: &[ToolDefinition]) -> Value {
    Value::Array(
        tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters
                })
            })
            .collect(),
    )
}

pub fn parse_tool_calls_openai(choice: &Value) -> Option<Vec<ToolCall>> {
    let tool_calls = choice.get("message")?.get("tool_calls")?.as_array()?;
    let mut result = Vec::new();
    for tc in tool_calls {
        let id = tc.get("id")?.as_str()?.to_string();
        let function = tc.get("function")?;
        let name = function.get("name")?.as_str()?.to_string();
        let args_str = function.get("arguments")?.as_str()?;
        let args: Value =
            serde_json::from_str(args_str).unwrap_or(Value::Object(Default::default()));
        result.push(ToolCall {
            id,
            name,
            arguments: args,
        });
    }
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

pub fn parse_tool_calls_anthropic(content: &Value) -> Option<Vec<ToolCall>> {
    let content_array = content.as_array()?;
    let mut result = Vec::new();
    for block in content_array {
        if block.get("type")?.as_str()? == "tool_use" {
            let id = block.get("id")?.as_str()?.to_string();
            let name = block.get("name")?.as_str()?.to_string();
            let input = block
                .get("input")
                .cloned()
                .unwrap_or(Value::Object(Default::default()));
            result.push(ToolCall {
                id,
                name,
                arguments: input,
            });
        }
    }
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

pub fn parse_tool_calls_google(candidate: &Value) -> Option<Vec<ToolCall>> {
    let parts = candidate.get("content")?.get("parts")?.as_array()?;
    let mut result = Vec::new();
    for part in parts {
        if let Some(fc) = part.get("functionCall") {
            let name = fc.get("name")?.as_str()?.to_string();
            let args = fc
                .get("args")
                .cloned()
                .unwrap_or(Value::Object(Default::default()));
            let id = format!("google_{}", name);
            result.push(ToolCall {
                id,
                name,
                arguments: args,
            });
        }
    }
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

pub fn message_to_openai_json(msg: &Message) -> Value {
    match msg.role {
        MessageRole::System | MessageRole::User => {
            serde_json::json!({"role": role_to_string(&msg.role), "content": msg.content})
        }
        MessageRole::Assistant => {
            let mut obj = serde_json::json!({"role": "assistant", "content": msg.content});
            if let Some(calls) = &msg.tool_calls {
                if !calls.is_empty() {
                    let tcs: Vec<Value> = calls
                        .iter()
                        .map(|c| {
                            serde_json::json!({
                                "id": c.id,
                                "type": "function",
                                "function": {
                                    "name": c.name,
                                    "arguments": serde_json::to_string(&c.arguments).unwrap_or_else(|_| "{}".to_string())
                                }
                            })
                        })
                        .collect();
                    obj["tool_calls"] = Value::Array(tcs);
                }
            }
            obj
        }
        MessageRole::Tool => {
            serde_json::json!({
                "role": "tool",
                "tool_call_id": msg.tool_call_id.clone().unwrap_or_default(),
                "content": msg.content
            })
        }
    }
}

pub fn message_to_anthropic_json(msg: &Message) -> Option<Value> {
    match msg.role {
        MessageRole::System => None,
        MessageRole::User => Some(serde_json::json!({
            "role": "user",
            "content": msg.content
        })),
        MessageRole::Assistant => {
            let mut blocks: Vec<Value> = Vec::new();
            if !msg.content.is_empty() {
                blocks.push(serde_json::json!({"type": "text", "text": msg.content}));
            }
            if let Some(calls) = &msg.tool_calls {
                for c in calls {
                    blocks.push(serde_json::json!({
                        "type": "tool_use",
                        "id": c.id,
                        "name": c.name,
                        "input": c.arguments
                    }));
                }
            }
            if blocks.is_empty() {
                blocks.push(serde_json::json!({"type": "text", "text": ""}));
            }
            Some(serde_json::json!({"role": "assistant", "content": blocks}))
        }
        MessageRole::Tool => Some(serde_json::json!({
            "role": "user",
            "content": [{
                "type": "tool_result",
                "tool_use_id": msg.tool_call_id.clone().unwrap_or_default(),
                "content": msg.content
            }]
        })),
    }
}

pub fn message_to_google_json(msg: &Message) -> Option<Value> {
    match msg.role {
        MessageRole::System => None,
        MessageRole::User => Some(serde_json::json!({
            "role": "user",
            "parts": [{"text": msg.content}]
        })),
        MessageRole::Assistant => {
            let mut parts: Vec<Value> = Vec::new();
            if !msg.content.is_empty() {
                parts.push(serde_json::json!({"text": msg.content}));
            }
            if let Some(calls) = &msg.tool_calls {
                for c in calls {
                    parts.push(serde_json::json!({
                        "functionCall": {"name": c.name, "args": c.arguments}
                    }));
                }
            }
            if parts.is_empty() {
                parts.push(serde_json::json!({"text": ""}));
            }
            Some(serde_json::json!({"role": "model", "parts": parts}))
        }
        MessageRole::Tool => Some(serde_json::json!({
            "role": "user",
            "parts": [{
                "functionResponse": {
                    "name": "tool",
                    "response": {
                        "content": {
                            "result": msg.content,
                            "call_id": msg.tool_call_id.clone().unwrap_or_default()
                        }
                    }
                }
            }]
        })),
    }
}

fn role_to_string(role: &MessageRole) -> &'static str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    }
}

impl From<ToolCall> for ToolCallPayload {
    fn from(c: ToolCall) -> Self {
        Self {
            id: c.id,
            name: c.name,
            arguments: c.arguments,
        }
    }
}

