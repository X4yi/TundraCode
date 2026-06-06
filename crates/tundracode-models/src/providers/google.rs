use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

use crate::provider::{CompletionRequest, CompletionResponse};
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
            "generationConfig": {
                "temperature": request.temperature,
                "maxOutputTokens": request.max_tokens,
            }
        });

        if let Some(sys) = &request.system_prompt {
            body["systemInstruction"] = serde_json::json!({
                "parts": [{"text": sys}]
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
