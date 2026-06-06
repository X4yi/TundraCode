use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

use crate::provider::{CompletionRequest, CompletionResponse};
use crate::tool_format::{
    format_tools_for_anthropic, message_to_anthropic_json, parse_tool_calls_anthropic, ToolCall,
    ToolDefinition,
};

pub struct AnthropicProvider;

impl AnthropicProvider {
    pub async fn complete(
        client: &Client,
        base_url: &str,
        api_key: Option<&str>,
        model: &str,
        request: &CompletionRequest,
        tools: Option<&[ToolDefinition]>,
    ) -> Result<(CompletionResponse, Option<Vec<ToolCall>>)> {
        let mut api_messages: Vec<Value> = Vec::new();

        for msg in &request.conversation.messages {
            if let Some(json) = message_to_anthropic_json(msg) {
                api_messages.push(json);
            }
        }

        let mut body = serde_json::json!({
            "model": model,
            "max_tokens": request.max_tokens,
            "messages": api_messages,
        });

        if let Some(sys) = &request.system_prompt {
            body["system"] = serde_json::json!(sys);
        }

        if let Some(tools) = tools {
            body["tools"] = format_tools_for_anthropic(tools);
        }

        let url = format!("{}/v1/messages", base_url.trim_end_matches('/'));

        let mut req = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("anthropic-version", "2023-06-01");

        if let Some(key) = api_key {
            req = req.header("x-api-key", key);
        }

        let response = req.json(&body).send().await?;

        let status = response.status();
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("API error ({}): {}", status, error_body));
        }

        let resp: Value = response.json().await?;

        let tokens_used = resp
            .get("usage")
            .and_then(|u| u.get("total_tokens"))
            .and_then(|t| t.as_u64())
            .unwrap_or(0) as u32;

        let finish_reason = resp
            .get("stop_reason")
            .and_then(|r| r.as_str())
            .unwrap_or("unknown")
            .to_string();

        let content = resp.get("content");

        if let Some(tool_calls) = content.and_then(parse_tool_calls_anthropic) {
            let text_content = extract_text_from_anthropic_content(content);
            return Ok((
                CompletionResponse {
                    content: text_content,
                    tokens_used,
                    finish_reason,
                },
                Some(tool_calls),
            ));
        }

        let content_str = extract_text_from_anthropic_content(content);
        if content_str.is_empty() {
            return Err(anyhow::anyhow!("No content in response"));
        }

        Ok((
            CompletionResponse {
                content: content_str,
                tokens_used,
                finish_reason,
            },
            None,
        ))
    }
}

fn extract_text_from_anthropic_content(content: Option<&Value>) -> String {
    let content_array = match content.and_then(|c| c.as_array()) {
        Some(arr) => arr,
        None => return content.and_then(|c| c.as_str()).unwrap_or("").to_string(),
    };

    let mut texts = Vec::new();
    for block in content_array {
        if block.get("type").and_then(|t| t.as_str()) == Some("text") {
            if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                texts.push(text);
            }
        }
    }
    texts.join("\n")
}
