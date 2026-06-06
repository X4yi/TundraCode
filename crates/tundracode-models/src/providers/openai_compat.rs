use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

use crate::provider::{CompletionRequest, CompletionResponse};
use crate::tool_format::{
    format_tools_for_openai, message_to_openai_json, parse_tool_calls_openai, ToolCall,
    ToolDefinition,
};

pub struct OpenAiCompatProvider;

impl OpenAiCompatProvider {
    pub async fn complete(
        client: &Client,
        base_url: &str,
        api_key: Option<&str>,
        model: &str,
        request: &CompletionRequest,
        tools: Option<&[ToolDefinition]>,
        is_keyless: bool,
    ) -> Result<(CompletionResponse, Option<Vec<ToolCall>>)> {
        let mut api_messages: Vec<Value> = Vec::new();

        if let Some(sys) = &request.system_prompt {
            api_messages.push(serde_json::json!({
                "role": "system",
                "content": sys
            }));
        }

        for msg in &request.conversation.messages {
            api_messages.push(message_to_openai_json(msg));
        }

        let mut body = serde_json::json!({
            "model": model,
            "messages": api_messages,
            "temperature": request.temperature,
            "max_tokens": request.max_tokens,
        });

        if let Some(tools) = tools {
            body["tools"] = format_tools_for_openai(tools);
        }

        let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));

        let mut req = client.post(&url).header("Content-Type", "application/json");

        if is_keyless {
            req = req.header("X-TundraCode-Client", "tundracode/0.1.0");
        } else if let Some(key) = api_key {
            req = req.bearer_auth(key);
        }

        let response = req.json(&body).send().await?;

        let status = response.status();
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_default();
            let friendly = if is_keyless {
                match status.as_u16() {
                    401 | 403 => "Modelo free no disponible ahora - intenta otro modelo o configura una API key".to_string(),
                    429 => "Rate limit alcanzado - espera unos segundos e intenta de nuevo".to_string(),
                    404 | 503 => "Modelo temporalmente no disponible".to_string(),
                    _ => format!("API error ({}): {}", status, error_body),
                }
            } else {
                format!("API error ({}): {}", status, error_body)
            };
            return Err(anyhow::anyhow!("{}", friendly));
        }

        let resp: Value = response.json().await?;

        let choice = resp
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
            .ok_or_else(|| anyhow::anyhow!("No choices in response"))?;

        let tokens_used = resp
            .get("usage")
            .and_then(|u| u.get("total_tokens"))
            .and_then(|t| t.as_u64())
            .unwrap_or(0) as u32;

        let finish_reason = choice
            .get("finish_reason")
            .and_then(|r| r.as_str())
            .unwrap_or("unknown")
            .to_string();

        if let Some(tool_calls) = parse_tool_calls_openai(choice) {
            let content = choice
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string();
            return Ok((
                CompletionResponse {
                    content,
                    tokens_used,
                    finish_reason,
                },
                Some(tool_calls),
            ));
        }

        let content = choice
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .ok_or_else(|| anyhow::anyhow!("No content in response"))?
            .to_string();

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
