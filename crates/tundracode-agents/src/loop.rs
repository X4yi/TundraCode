use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use tundracode_models::{
    CompletionRequest, Conversation, MessageRole, ModelConfig, ProviderRegistry, StreamEvent,
    ToolCallPayload, ToolDefinition,
};
use tundracode_tools::{ToolContext, ToolRegistry};

use crate::agent::ToolInvocation;

pub struct AgentLoop {
    max_iterations: usize,
    cancel_flag: Option<Arc<AtomicBool>>,
    budget_tokens: Option<u32>,
}

pub struct RunConfig<'a> {
    pub provider_registry: &'a ProviderRegistry,
    pub tool_registry: &'a ToolRegistry,
    pub tool_context: &'a ToolContext,
    pub provider_id: &'a str,
    pub model_config: &'a ModelConfig,
    pub system_prompt: &'a str,
    pub user_message: &'a str,
    pub tools: &'a [ToolDefinition],
    pub reasoning_effort: Option<String>,
    pub on_event: Option<Box<dyn FnMut(StreamEvent) + Send>>,
}

pub struct RunOutput {
    pub content: String,
    pub invocations: Vec<ToolInvocation>,
    pub tokens_used: u32,
}

impl AgentLoop {
    pub fn new() -> Self {
        Self {
            max_iterations: 8,
            cancel_flag: None,
            budget_tokens: None,
        }
    }

    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }

    pub fn with_cancel_flag(mut self, flag: Arc<AtomicBool>) -> Self {
        self.cancel_flag = Some(flag);
        self
    }

    pub fn with_budget_tokens(mut self, budget: u32) -> Self {
        self.budget_tokens = Some(budget);
        self
    }

    pub async fn run(&self, mut config: RunConfig<'_>) -> Result<RunOutput> {
        let mut conversation = Conversation::new();
        conversation.add_message(MessageRole::User, config.user_message.to_string());

        let mut invocations: Vec<ToolInvocation> = Vec::new();
        let mut total_tokens: u32 = 0;

        for _iteration in 0..self.max_iterations {
            if let Some(flag) = &self.cancel_flag {
                if flag.load(Ordering::Relaxed) {
                    return Ok(RunOutput {
                        content: "Agent cancelled".to_string(),
                        invocations,
                        tokens_used: total_tokens,
                    });
                }
            }

            if let Some(budget) = self.budget_tokens {
                if total_tokens >= budget {
                    return Ok(RunOutput {
                        content: format!(
                            "Agent reached token budget limit ({}/{})",
                            total_tokens, budget
                        ),
                        invocations,
                        tokens_used: total_tokens,
                    });
                }
            }

            let request = CompletionRequest {
                conversation: conversation.clone(),
                system_prompt: Some(config.system_prompt.to_string()),
                reasoning_effort: config.reasoning_effort.clone(),
            };

            let tools_for_call = if config.tools.is_empty() {
                None
            } else {
                Some(config.tools)
            };

            let provider = config
                .provider_registry
                .get(config.provider_id)
                .ok_or_else(|| anyhow::anyhow!("Provider not found: {}", config.provider_id))?;

            let (response, tool_calls) = if let Some(ref mut on_event) = config.on_event {
                let mut content_buf = String::new();
                let mut tool_calls_buf: Vec<tundracode_models::ToolCall> = Vec::new();

                let result = provider
                    .stream(
                        config.model_config,
                        request,
                        tools_for_call,
                        &mut |event| match event {
                            StreamEvent::Token(t) => {
                                content_buf.push_str(&t);
                                on_event(StreamEvent::Token(t));
                            }
                            StreamEvent::ReasoningToken(t) => {
                                on_event(StreamEvent::ReasoningToken(t));
                            }
                            StreamEvent::ToolCallStart { name, call_id, file_path } => {
                                tool_calls_buf.push(tundracode_models::ToolCall {
                                    id: call_id.clone(),
                                    name: name.clone(),
                                    arguments: serde_json::Value::Object(
                                        serde_json::Map::new(),
                                    ),
                                });
                                on_event(StreamEvent::ToolCallStart { name, call_id, file_path });
                            }
                            StreamEvent::ToolCallDelta {
                                call_id,
                                arguments_delta,
                            } => {
                                if let Some(tc) =
                                    tool_calls_buf.iter_mut().find(|tc| tc.id == call_id)
                                {
                                    if let Some(obj) = tc.arguments.as_object_mut() {
                                        if let Some(raw) = obj.get_mut("_raw") {
                                            if let Some(s) = raw.as_str() {
                                                *raw = serde_json::Value::String(
                                                    s.to_string() + &arguments_delta,
                                                );
                                            }
                                        } else {
                                            obj.insert(
                                                "_raw".to_string(),
                                                serde_json::Value::String(arguments_delta.clone()),
                                            );
                                        }
                                    }
                                }
                                on_event(StreamEvent::ToolCallDelta {
                                    call_id,
                                    arguments_delta,
                                });
                            }
                            StreamEvent::ToolCallEnd { call_id, mut file_path } => {
                                if let Some(tc) =
                                    tool_calls_buf.iter_mut().find(|tc| tc.id == call_id)
                                {
                                    if let Some(obj) = tc.arguments.as_object_mut() {
                                        if let Some(raw) = obj.remove("_raw") {
                                            if let Some(s) = raw.as_str() {
                                                if let Ok(parsed) =
                                                    serde_json::from_str::<serde_json::Value>(s)
                                                {
                                                    if let Some(parsed_obj) = parsed.as_object() {
                                                        // Extract file_path from arguments
                                                        if let Some(path_val) = parsed_obj.get("path") {
                                                            if let Some(path_str) = path_val.as_str() {
                                                                file_path = Some(path_str.to_string());
                                                            }
                                                        }
                                                        tc.arguments = serde_json::Value::Object(
                                                            parsed_obj.clone(),
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                on_event(StreamEvent::ToolCallEnd { call_id, file_path });
                            }
                            StreamEvent::Done(resp) => {
                                total_tokens += resp.tokens_used;
                                on_event(StreamEvent::Done(resp.clone()));
                            }
                            StreamEvent::Error(e) => {
                                on_event(StreamEvent::Error(e.clone()));
                            }
                        },
                    )
                    .await?;

                let final_content = if content_buf.is_empty() {
                    result.0.content
                } else {
                    content_buf
                };

                (
                    tundracode_models::CompletionResponse {
                        content: final_content,
                        tokens_used: result.0.tokens_used,
                        finish_reason: result.0.finish_reason,
                    },
                    if tool_calls_buf.is_empty() {
                        result.1
                    } else {
                        Some(tool_calls_buf)
                    },
                )
            } else {
                provider
                    .complete(config.model_config, request, tools_for_call)
                    .await?
            };

            if config.on_event.is_none() {
                total_tokens += response.tokens_used;
            }

            if let Some(calls) = tool_calls {
                if !calls.is_empty() {
                    let payloads: Vec<ToolCallPayload> =
                        calls.iter().map(|c| c.clone().into()).collect();
                    conversation.add_assistant_with_tool_calls(response.content.clone(), payloads);

                    for call in &calls {
                        let result = config
                            .tool_registry
                            .execute(config.tool_context, &call.name, call.arguments.clone())
                            .await;

                        let (success, output, error, prior, resulting, file_path) = match result {
                            Ok(r) => (
                                r.success,
                                r.output,
                                r.error,
                                r.prior_content,
                                r.resulting_content,
                                r.file_path,
                            ),
                            Err(e) => (
                                false,
                                String::new(),
                                Some(e.to_string()),
                                None,
                                None,
                                None,
                            ),
                        };

                        let after = if success {
                            if let Some(content) = resulting {
                                Some(content)
                            } else {
                                let path = file_path.clone().or_else(|| {
                                    call.arguments
                                        .get("path")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string())
                                });
                                if let Some(p) = path.as_deref() {
                                    let full =
                                        std::path::Path::new(&config.tool_context.workspace_path)
                                            .join(p);
                                    std::fs::read_to_string(&full).ok()
                                } else {
                                    None
                                }
                            }
                        } else {
                            None
                        };

                        invocations.push(ToolInvocation {
                            tool_name: call.name.clone(),
                            call_id: call.id.clone(),
                            arguments: call.arguments.clone(),
                            success,
                            output: output.clone(),
                            file_path,
                            before: prior,
                            after,
                        });

                        let tool_output = if success {
                            output
                        } else {
                            error.unwrap_or_else(|| "Unknown error".to_string())
                        };
                        conversation.add_tool_result(call.id.clone(), tool_output);
                    }
                    continue;
                }
            }

            return Ok(RunOutput {
                content: response.content,
                invocations,
                tokens_used: total_tokens,
            });
        }

        Ok(RunOutput {
            content: "Agent reached maximum iterations without a final answer.".to_string(),
            invocations,
            tokens_used: total_tokens,
        })
    }
}

impl Default for AgentLoop {
    fn default() -> Self {
        Self::new()
    }
}
