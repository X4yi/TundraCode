use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

use crate::provider::{CompletionRequest, CompletionResponse, StreamEvent};
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
        });

        if let Some(effort) = normalized_reasoning_effort(request) {
            body["reasoning_effort"] = serde_json::json!(effort);
        }

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

    pub async fn stream(
        client: &Client,
        base_url: &str,
        api_key: Option<&str>,
        model: &str,
        request: &CompletionRequest,
        tools: Option<&[ToolDefinition]>,
        is_keyless: bool,
        on_event: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<(CompletionResponse, Option<Vec<ToolCall>>)> {
        use futures::StreamExt;

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
            "stream": true,
        });

        if let Some(effort) = normalized_reasoning_effort(request) {
            body["reasoning_effort"] = serde_json::json!(effort);
        }

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
                    401 | 403 => "Modelo free no disponible ahora".to_string(),
                    429 => "Rate limit alcanzado".to_string(),
                    404 | 503 => "Modelo temporalmente no disponible".to_string(),
                    _ => format!("API error ({}): {}", status, error_body),
                }
            } else {
                format!("API error ({}): {}", status, error_body)
            };
            return Err(anyhow::anyhow!("{}", friendly));
        }

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut content = String::new();
        let mut tool_calls_acc: std::collections::HashMap<u32, (String, String, String)> = std::collections::HashMap::new();
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
                    let data = data.trim();
                    if data == "[DONE]" {
                        continue;
                    }

                    if let Ok(json) = serde_json::from_str::<Value>(data) {
                        if let Some(usage) = json.get("usage") {
                            tokens_used = usage
                                .get("total_tokens")
                                .and_then(|t| t.as_u64())
                                .unwrap_or(0) as u32;
                        }

                        if let Some(choices) = json.get("choices").and_then(|c| c.as_array()) {
                            if let Some(choice) = choices.first() {
                                if let Some(reason) = choice.get("finish_reason").and_then(|r| r.as_str()) {
                                    finish_reason = reason.to_string();
                                }

                                if let Some(delta) = choice.get("delta") {
                                    if let Some(text) = delta.get("content").and_then(|c| c.as_str()) {
                                        if !text.is_empty() {
                                            content.push_str(text);
                                            on_event(StreamEvent::Token(text.to_string()));
                                        }
                                    }

                                    if let Some(reasoning) = delta.get("reasoning_content").and_then(|r| r.as_str()) {
                                        if !reasoning.is_empty() {
                                            on_event(StreamEvent::ReasoningToken(reasoning.to_string()));
                                        }
                                    }

                                    if let Some(thinking) = delta.get("thinking").and_then(|t| t.as_str()) {
                                        if !thinking.is_empty() {
                                            on_event(StreamEvent::ReasoningToken(thinking.to_string()));
                                        }
                                    }

                    if let Some(tc_array) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                        for tc in tc_array {
                            let idx = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as u32;
                            let entry = tool_calls_acc.entry(idx).or_insert_with(|| {
                                let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                let name = tc.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()).unwrap_or("").to_string();
                                if !name.is_empty() {
                                    on_event(StreamEvent::ToolCallStart { name: name.clone(), call_id: id.clone(), file_path: None, arguments: None });
                                }
                                (id, name, String::new())
                            });

                            if let Some(args_delta) = tc.get("function").and_then(|f| f.get("arguments")).and_then(|a| a.as_str()) {
                                entry.2.push_str(args_delta);
                                on_event(StreamEvent::ToolCallDelta { call_id: entry.0.clone(), arguments_delta: args_delta.to_string() });
                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let response_obj = CompletionResponse {
            content: content.clone(),
            tokens_used,
            finish_reason,
        };

        let parsed_tool_calls = if !tool_calls_acc.is_empty() {
            let mut calls: Vec<ToolCall> = Vec::new();
            let mut sorted: Vec<_> = tool_calls_acc.into_iter().collect();
            sorted.sort_by_key(|(k, _)| *k);
            for (_idx, (id, name, args_json)) in sorted {
                let arguments: Value = serde_json::from_str(&args_json).unwrap_or(Value::Object(serde_json::Map::new()));
                on_event(StreamEvent::ToolCallEnd { call_id: id.clone(), file_path: None });
                calls.push(ToolCall { id, name, arguments });
            }
            if calls.is_empty() { None } else { Some(calls) }
        } else {
            None
        };

        on_event(StreamEvent::Done(response_obj.clone()));
        Ok((response_obj, parsed_tool_calls))
    }
}

fn normalized_reasoning_effort(request: &CompletionRequest) -> Option<&str> {
    request
        .reasoning_effort
        .as_deref()
        .filter(|effort| !effort.is_empty() && *effort != "none")
}
