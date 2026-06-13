use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

use crate::provider::{CompletionRequest, CompletionResponse, StreamEvent};
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
            "messages": api_messages,
        });

        if let Some(sys) = &request.system_prompt {
            body["system"] = serde_json::json!(sys);
        }

        if let Some(budget) = reasoning_budget_tokens(request) {
            body["thinking"] = serde_json::json!({
                "type": "enabled",
                "budget_tokens": budget,
            });
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

        let mut api_messages: Vec<Value> = Vec::new();

        for msg in &request.conversation.messages {
            if let Some(json) = message_to_anthropic_json(msg) {
                api_messages.push(json);
            }
        }

        let mut body = serde_json::json!({
            "model": model,
            "messages": api_messages,
            "stream": true,
        });

        if let Some(sys) = &request.system_prompt {
            body["system"] = serde_json::json!(sys);
        }

        if let Some(budget) = reasoning_budget_tokens(request) {
            body["thinking"] = serde_json::json!({
                "type": "enabled",
                "budget_tokens": budget,
            });
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

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut content = String::new();
        let mut tokens_used: u32 = 0;
        let mut finish_reason = "stop".to_string();
        let mut current_tool_id = String::new();
        let mut current_tool_name = String::new();
        let mut current_tool_args = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.map_err(|e| anyhow::anyhow!("Stream error: {}", e))?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(line_end) = buffer.find('\n') {
                let line = buffer[..line_end].trim().to_string();
                buffer = buffer[line_end + 1..].to_string();

                if line.is_empty() {
                    continue;
                }

                if let Some(data) = line.strip_prefix("data: ") {
                    if let Ok(json) = serde_json::from_str::<Value>(data) {
                        let event_type = json.get("type").and_then(|t| t.as_str()).unwrap_or("");

                        match event_type {
                            "content_block_start" => {
                                if let Some(block) = json.get("content_block") {
                                    let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                                    if block_type == "tool_use" {
                                        current_tool_id = block.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                        current_tool_name = block.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                        current_tool_args.clear();
                                        on_event(StreamEvent::ToolCallStart { name: current_tool_name.clone(), call_id: current_tool_id.clone(), file_path: None, arguments: None });
                                    }
                                }
                            }
                            "content_block_delta" => {
                                if let Some(delta) = json.get("delta") {
                                    let delta_type = delta.get("type").and_then(|t| t.as_str()).unwrap_or("");
                                    match delta_type {
                                        "text_delta" => {
                                            if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                                                content.push_str(text);
                                                on_event(StreamEvent::Token(text.to_string()));
                                            }
                                        }
                                        "thinking_delta" => {
                                            if let Some(thinking) = delta.get("thinking").and_then(|t| t.as_str()) {
                                                if !thinking.is_empty() {
                                                    on_event(StreamEvent::ReasoningToken(thinking.to_string()));
                                                }
                                            }
                                        }
                                        "input_json_delta" => {
                                            if let Some(partial) = delta.get("partial_json").and_then(|p| p.as_str()) {
                                                current_tool_args.push_str(partial);
                                                on_event(StreamEvent::ToolCallDelta { call_id: current_tool_id.clone(), arguments_delta: partial.to_string() });
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            "content_block_stop" => {
                                if !current_tool_id.is_empty() {
                                    let arguments: Value = serde_json::from_str(&current_tool_args).unwrap_or(Value::Object(serde_json::Map::new()));
                                    on_event(StreamEvent::ToolCallEnd { call_id: current_tool_id.clone(), file_path: None });
                                    tool_calls.push(ToolCall {
                                        id: current_tool_id.clone(),
                                        name: current_tool_name.clone(),
                                        arguments,
                                    });
                                    current_tool_id.clear();
                                    current_tool_name.clear();
                                    current_tool_args.clear();
                                }
                            }
                            "message_delta" => {
                                if let Some(delta) = json.get("delta") {
                                    if let Some(reason) = delta.get("stop_reason").and_then(|r| r.as_str()) {
                                        finish_reason = reason.to_string();
                                    }
                                }
                                if let Some(usage) = json.get("usage") {
                                    tokens_used = usage.get("output_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as u32;
                                }
                            }
                            "message_start" => {
                                if let Some(usage) = json.get("message").and_then(|m| m.get("usage")) {
                                    tokens_used = usage.get("input_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as u32;
                                }
                            }
                            _ => {}
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

        let parsed_tool_calls = if tool_calls.is_empty() { None } else { Some(tool_calls) };
        on_event(StreamEvent::Done(response_obj.clone()));
        Ok((response_obj, parsed_tool_calls))
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

fn reasoning_budget_tokens(request: &CompletionRequest) -> Option<u32> {
    match request.reasoning_effort.as_deref() {
        Some("low") => Some(1024),
        Some("medium") => Some(4096),
        Some("high") => Some(8192),
        _ => None,
    }
}
