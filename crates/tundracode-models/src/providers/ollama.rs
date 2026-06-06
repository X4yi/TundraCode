use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

use crate::conversation::MessageRole;
use crate::provider::{CompletionRequest, CompletionResponse};
use crate::tool_format::{ToolCall, ToolDefinition};

pub struct OllamaProvider;

impl OllamaProvider {
    pub async fn complete(
        client: &Client,
        base_url: &str,
        model: &str,
        request: &CompletionRequest,
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<(CompletionResponse, Option<Vec<ToolCall>>)> {
        let mut messages: Vec<Value> = Vec::new();

        if let Some(sys) = &request.system_prompt {
            messages.push(serde_json::json!({
                "role": "system",
                "content": sys
            }));
        }

        for msg in &request.conversation.messages {
            let role = match msg.role {
                MessageRole::System => "system",
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::Tool => "tool",
            };
            let mut entry = serde_json::json!({
                "role": role,
                "content": msg.content,
            });
            if matches!(msg.role, MessageRole::Tool) {
                if let Some(call_id) = &msg.tool_call_id {
                    entry["tool_call_id"] = serde_json::Value::String(call_id.clone());
                }
            }
            messages.push(entry);
        }

        let body = serde_json::json!({
            "model": model,
            "messages": messages,
            "stream": false,
            "options": {
                "temperature": request.temperature,
                "num_predict": request.max_tokens,
            }
        });

        let url = format!("{}/api/chat", base_url.trim_end_matches('/'));

        let response = client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Ollama error ({}): {}", status, error_body));
        }

        let resp: Value = response.json().await?;

        let content = resp
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();

        let tokens_used = resp
            .get("eval_count")
            .and_then(|c| c.as_u64())
            .unwrap_or(0) as u32;

        let done_reason = resp
            .get("done_reason")
            .and_then(|r| r.as_str())
            .unwrap_or("stop")
            .to_string();

        Ok((
            CompletionResponse {
                content,
                tokens_used,
                finish_reason: done_reason,
            },
            None,
        ))
    }
}
