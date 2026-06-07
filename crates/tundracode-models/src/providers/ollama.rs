use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

use crate::conversation::MessageRole;
use crate::provider::{CompletionRequest, CompletionResponse, StreamEvent};
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

        let mut body = serde_json::json!({
            "model": model,
            "messages": messages,
            "stream": false,
        });
        body["think"] = serde_json::json!(reasoning_enabled(request));

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

    pub async fn stream(
        client: &Client,
        base_url: &str,
        model: &str,
        request: &CompletionRequest,
        _tools: Option<&[ToolDefinition]>,
        on_event: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<(CompletionResponse, Option<Vec<ToolCall>>)> {
        use futures::StreamExt;

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

        let mut body = serde_json::json!({
            "model": model,
            "messages": messages,
            "stream": true,
        });
        body["think"] = serde_json::json!(reasoning_enabled(request));

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

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut content = String::new();
        let mut tokens_used: u32 = 0;
        let mut finish_reason = "stop".to_string();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.map_err(|e| anyhow::anyhow!("Stream error: {}", e))?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(line_end) = buffer.find('\n') {
                let line = buffer[..line_end].trim().to_string();
                buffer = buffer[line_end + 1..].to_string();

                if line.is_empty() {
                    continue;
                }

                if let Ok(json) = serde_json::from_str::<Value>(&line) {
                    if let Some(msg) = json.get("message") {
                        if let Some(text) = msg.get("content").and_then(|c| c.as_str()) {
                            if !text.is_empty() {
                                content.push_str(text);
                                on_event(StreamEvent::Token(text.to_string()));
                            }
                        }
                    }

                    if json.get("done").and_then(|d| d.as_bool()).unwrap_or(false) {
                        tokens_used = json.get("eval_count").and_then(|c| c.as_u64()).unwrap_or(0) as u32;
                        finish_reason = json.get("done_reason").and_then(|r| r.as_str()).unwrap_or("stop").to_string();
                    }
                }
            }
        }

        let response_obj = CompletionResponse {
            content,
            tokens_used,
            finish_reason,
        };

        on_event(StreamEvent::Done(response_obj.clone()));
        Ok((response_obj, None))
    }
}

fn reasoning_enabled(request: &CompletionRequest) -> bool {
    matches!(
        request.reasoning_effort.as_deref(),
        Some("low" | "medium" | "high")
    )
}
