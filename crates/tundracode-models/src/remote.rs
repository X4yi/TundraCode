use async_trait::async_trait;

use crate::provider::{CompletionRequest, CompletionResponse, ModelConfig, ModelProvider};
use crate::tool_format::{ToolCall, ToolDefinition};

pub struct OpenAiProvider;

#[async_trait]
impl ModelProvider for OpenAiProvider {
    fn name(&self) -> &'static str {
        "openai"
    }

    async fn complete(
        &self,
        _config: &ModelConfig,
        request: CompletionRequest,
        _tools: Option<&[ToolDefinition]>,
    ) -> anyhow::Result<(CompletionResponse, Option<Vec<ToolCall>>)> {
        Ok((
            CompletionResponse {
                content: format!(
                    "Respuesta placeholder para {} mensajes",
                    request.conversation.messages.len()
                ),
                tokens_used: 100,
                finish_reason: "stop".to_string(),
            },
            None,
        ))
    }
}
