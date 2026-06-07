use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

use crate::provider::{CompletionRequest, CompletionResponse, StreamEvent};
use crate::tool_format::{
    format_tools_for_google, message_to_google_json, parse_tool_calls_google, ToolCall,
    ToolDefinition,
};

pub struct GoogleProvider;

impl GoogleProvider {
    pub async fn complete(
        client: &Client,
        base_url: &str,
        api_key: Option<&str>,
        model: &str,
        request: &CompletionRequest,
        tools: Option<&[ToolDefinition]>,
    ) -> Result<(CompletionResponse, Option<Vec<ToolCall>>)> {
        let mut contents: Vec<Value> = Vec::new();

        for msg in &request.conversation.messages {
            if let Some(json) = message_to_google_json(msg) {
                contents.push(json);
            }
        }

        let mut body = serde_json::json!({
            "contents": contents,
        });

        if let Some(sys) = &request.system_prompt {
            body["systemInstruction"] = serde_json::json!({
                "parts": [{"text": sys}]
            });
        }

        if let Some(budget) = reasoning_budget_tokens(request) {
            body["generationConfig"] = serde_json::json!({
                "thinkingConfig": {
                    "thinkingBudget": budget,
                }
            });
        }

        if let Some(tools) = tools {
            body["tools"] = serde_json::json!({
                "function_declarations": format_tools_for_google(tools)
            });
        }

        let url = if let Some(key) = api_key {
            format!(
                "{}/v1/models/{}:generateContent?key={}",
                base_url.trim_end_matches('/'),
                model,
                key
            )
        } else {
            format!(
                "{}/v1/models/{}:generateContent",
                base_url.trim_end_matches('/'),
                model
            )
        };

        let response = client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("API error ({}): {}", status, error_body));
        }

        let resp: Value = response.json().await?;

        let candidate = resp
            .get("candidates")
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
            .ok_or_else(|| anyhow::anyhow!("No candidates in response"))?;

        let tokens_used = resp
            .get("usageMetadata")
            .and_then(|u| u.get("totalTokenCount"))
            .and_then(|t| t.as_u64())
            .unwrap_or(0) as u32;

        let finish_reason = candidate
            .get("finishReason")
            .and_then(|r| r.as_str())
            .unwrap_or("unknown")
            .to_string();

        if let Some(tool_calls) = parse_tool_calls_google(candidate) {
            let content = extract_text_from_google_candidate(candidate);
            return Ok((
                CompletionResponse {
                    content,
                    tokens_used,
                    finish_reason,
                },
                Some(tool_calls),
            ));
        }

        let content = extract_text_from_google_candidate(candidate);
        if content.is_empty() {
            return Err(anyhow::anyhow!("No content in response"));
        }

        Ok((
            CompletionResponse {
                content,
                tokens_used,
                finish_reason,
            },
            None,
        ))
    }

    pub async fn stream(
        client: &Client,
        base_url: &str,
        api_key: Option<&str>,
        model: &str,
        request: &CompletionRequest,
        tools: Option<&[ToolDefinition]>,
        on_event: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<(CompletionResponse, Option<Vec<ToolCall>>)> {
        use futures::StreamExt;

        let mut contents: Vec<Value> = Vec::new();

        for msg in &request.conversation.messages {
            if let Some(json) = message_to_google_json(msg) {
                contents.push(json);
            }
        }

        let mut body = serde_json::json!({
            "contents": contents,
            "generationConfig": {
                "stream": true,
            }
        });

        if let Some(sys) = &request.system_prompt {
            body["systemInstruction"] = serde_json::json!({
                "parts": [{"text": sys}]
            });
        }

        if let Some(budget) = reasoning_budget_tokens(request) {
            body["generationConfig"]["thinkingConfig"] = serde_json::json!({
                "thinkingBudget": budget,
            });
        }

        if let Some(tools) = tools {
            body["tools"] = serde_json::json!({
                "function_declarations": format_tools_for_google(tools)
            });
        }

        let url = if let Some(key) = api_key {
            format!(
                "{}/v1/models/{}:streamGenerateContent?alt=sse&key={}",
                base_url.trim_end_matches('/'),
                model,
                key
            )
        } else {
            format!(
                "{}/v1/models/{}:streamGenerateContent?alt=sse",
                base_url.trim_end_matches('/'),
                model
            )
        };

        let response = client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("API error ({}): {}", status, error_body));
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

                if line.is_empty() || line.starts_with(':') {
                    continue;
                }

                if let Some(data) = line.strip_prefix("data: ") {
                    if let Ok(json) = serde_json::from_str::<Value>(data) {
                        if let Some(candidates) = json.get("candidates").and_then(|c| c.as_array()) {
                            if let Some(candidate) = candidates.first() {
                                if let Some(reason) = candidate.get("finishReason").and_then(|r| r.as_str()) {
                                    finish_reason = reason.to_string();
                                }

                                if let Some(content_obj) = candidate.get("content") {
                                    if let Some(parts) = content_obj.get("parts").and_then(|p| p.as_array()) {
                                        for part in parts {
                                            if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                                content.push_str(text);
                                                on_event(StreamEvent::Token(text.to_string()));
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        if let Some(usage) = json.get("usageMetadata") {
                            tokens_used = usage.get("totalTokenCount").and_then(|t| t.as_u64()).unwrap_or(0) as u32;
                        }
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

fn extract_text_from_google_candidate(candidate: &Value) -> String {
    let parts = match candidate
        .get("content")
        .and_then(|c| c.get("parts"))
        .and_then(|p| p.as_array())
    {
        Some(arr) => arr,
        None => return String::new(),
    };

    let mut texts = Vec::new();
    for part in parts {
        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
            texts.push(text);
        }
    }
    texts.join("\n")
}

fn reasoning_budget_tokens(request: &CompletionRequest) -> Option<u32> {
    match request.reasoning_effort.as_deref() {
        Some("low") => Some(1024),
        Some("medium") => Some(4096),
        Some("high") => Some(8192),
        _ => None,
    }
}
