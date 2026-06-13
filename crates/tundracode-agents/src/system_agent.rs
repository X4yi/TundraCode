use tundracode_models::{
    CompletionRequest, Conversation, MessageRole, ModelConfig, ProviderRegistry,
};

const SYSTEM_TITLE_PROMPT: &str = include_str!("prompts/title_generator.txt");
const SYSTEM_SUMMARY_PROMPT: &str = include_str!("prompts/session_summarizer.txt");
const SYSTEM_COMPACTION_PROMPT: &str = include_str!("prompts/compaction_summarizer.txt");

/// Generates a short title for a session based on the user's first message.
pub async fn generate_session_title(
    provider_registry: &ProviderRegistry,
    model_config: &ModelConfig,
    user_message: &str,
) -> String {
    let mut conversation = Conversation::new();
    conversation.add_message(MessageRole::User, user_message.to_string());

    let request = CompletionRequest {
        conversation,
        system_prompt: Some(SYSTEM_TITLE_PROMPT.to_string()),
        reasoning_effort: None,
    };

    let provider = match provider_registry.get(&model_config.provider) {
        Some(p) => p,
        None => return truncate_title(user_message),
    };

    match provider.complete(model_config, request, None).await {
        Ok((response, _)) => {
            let title = response.content.trim().to_string();
            if title.is_empty() || title.len() > 50 {
                truncate_title(user_message)
            } else {
                title
            }
        }
        Err(_) => truncate_title(user_message),
    }
}

/// Generates a session summary for persistence into memory.
pub async fn generate_session_summary(
    provider_registry: &ProviderRegistry,
    model_config: &ModelConfig,
    conversation: &Conversation,
    total_tokens: u32,
) -> String {
    let mut conv = conversation.clone();
    conv.add_message(
        MessageRole::User,
        format!(
            "Genera un resumen de esta sesion. Tokens usados: {}",
            total_tokens
        ),
    );

    let request = CompletionRequest {
        conversation: conv,
        system_prompt: Some(SYSTEM_SUMMARY_PROMPT.to_string()),
        reasoning_effort: None,
    };

    let provider = match provider_registry.get(&model_config.provider) {
        Some(p) => p,
        None => return format!("Session: ~{} tokens used", total_tokens),
    };

    match provider.complete(model_config, request, None).await {
        Ok((response, _)) => response.content.trim().to_string(),
        Err(_) => format!("Session: ~{} tokens used", total_tokens),
    }
}

/// Summarizes a portion of conversation for compaction.
pub async fn summarize_for_compaction(
    provider_registry: &ProviderRegistry,
    model_config: &ModelConfig,
    conversation_snippet: &str,
    topic: &str,
) -> String {
    let mut conversation = Conversation::new();
    conversation.add_message(
        MessageRole::User,
        format!("Conversacion a resumir (tema: {}):\n{}", topic, conversation_snippet),
    );

    let request = CompletionRequest {
        conversation,
        system_prompt: Some(SYSTEM_COMPACTION_PROMPT.to_string()),
        reasoning_effort: None,
    };

    let provider = match provider_registry.get(&model_config.provider) {
        Some(p) => p,
        None => return format!("[Compacted: {}]", topic),
    };

    match provider.complete(model_config, request, None).await {
        Ok((response, _)) => response.content.trim().to_string(),
        Err(_) => format!("[Compacted: {}]", topic),
    }
}

fn truncate_title(msg: &str) -> String {
    let clean: String = msg
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect();
    let words: Vec<&str> = clean.split_whitespace().take(5).collect();
    let title = words.join(" ");
    if title.len() > 50 {
        title[..47].to_string() + "..."
    } else {
        title
    }
}
