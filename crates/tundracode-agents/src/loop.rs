use anyhow::Result;
use tundracode_models::{
    CompletionRequest, Conversation, MessageRole, ModelConfig, ProviderRegistry, ToolCallPayload,
    ToolDefinition,
};
use tundracode_tools::{ToolContext, ToolRegistry};

use crate::agent::ToolInvocation;

pub struct AgentLoop {
    max_iterations: usize,
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
}

pub struct RunOutput {
    pub content: String,
    pub invocations: Vec<ToolInvocation>,
    pub tokens_used: u32,
}

impl AgentLoop {
    pub fn new() -> Self {
        Self { max_iterations: 8 }
    }

    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }

    pub async fn run(&self, config: RunConfig<'_>) -> Result<RunOutput> {
        let mut conversation = Conversation::new();
        conversation.add_message(MessageRole::User, config.user_message.to_string());

        let mut invocations: Vec<ToolInvocation> = Vec::new();
        let mut total_tokens: u32 = 0;

        for _iteration in 0..self.max_iterations {
            let request = CompletionRequest {
                conversation: conversation.clone(),
                system_prompt: Some(config.system_prompt.to_string()),
                temperature: config.model_config.temperature,
                max_tokens: config.model_config.max_tokens,
            };

            let tools_for_call = if config.tools.is_empty() {
                None
            } else {
                Some(config.tools)
            };

            let (response, tool_calls) = config
                .provider_registry
                .get(config.provider_id)
                .ok_or_else(|| anyhow::anyhow!("Provider not found: {}", config.provider_id))?
                .complete(config.model_config, request, tools_for_call)
                .await?;

            total_tokens += response.tokens_used;

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

                        let (success, output, error, prior, file_path) = match result {
                            Ok(r) => (r.success, r.output, r.error, r.prior_content, r.file_path),
                            Err(e) => (false, String::new(), Some(e.to_string()), None, None),
                        };

                        let after = if success {
                            let path = file_path.clone().or_else(|| {
                                call.arguments
                                    .get("path")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string())
                            });
                            if let Some(p) = path.as_deref() {
                                let full = std::path::Path::new(&config.tool_context.workspace_path)
                                    .join(p);
                                std::fs::read_to_string(&full).ok()
                            } else {
                                None
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
