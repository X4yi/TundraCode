use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCategory {
    Free,
    Opencode,
    ThirdParty,
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub provider: String,
    pub model: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub temperature: f32,
    pub max_tokens: u32,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            api_key: None,
            base_url: None,
            temperature: 0.7,
            max_tokens: 4096,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub base_url: String,
    pub api_key_required: bool,
    pub is_free: bool,
    pub is_subscription: bool,
    pub is_payg: bool,
    pub is_keyless: bool,
    pub category: ProviderCategory,
    pub models_endpoint: Option<String>,
    pub default_models: Vec<String>,
    pub description: String,
}

pub fn get_all_providers() -> Vec<ProviderInfo> {
    vec![
        ProviderInfo {
            id: "opencode-free".to_string(),
            name: "OpenCode Free".to_string(),
            icon: "gift".to_string(),
            base_url: "https://opencode.ai/zen/v1".to_string(),
            api_key_required: false,
            is_free: true,
            is_subscription: false,
            is_payg: false,
            is_keyless: true,
            category: ProviderCategory::Free,
            models_endpoint: Some("https://opencode.ai/zen/v1/models".to_string()),
            default_models: vec![],
            description: "Free models, no API key required".to_string(),
        },
        ProviderInfo {
            id: "opencode-zen".to_string(),
            name: "OpenCode Zen".to_string(),
            icon: "zap".to_string(),
            base_url: "https://opencode.ai/zen/v1".to_string(),
            api_key_required: true,
            is_free: false,
            is_subscription: false,
            is_payg: true,
            is_keyless: false,
            category: ProviderCategory::Opencode,
            models_endpoint: Some("https://opencode.ai/zen/v1/models".to_string()),
            default_models: vec![],
            description: "Pay-as-you-go, 50+ curated models".to_string(),
        },
        ProviderInfo {
            id: "opencode-go".to_string(),
            name: "OpenCode Go".to_string(),
            icon: "arrow-right".to_string(),
            base_url: "https://opencode.ai/zen/go/v1".to_string(),
            api_key_required: true,
            is_free: false,
            is_subscription: true,
            is_payg: false,
            is_keyless: false,
            category: ProviderCategory::Opencode,
            models_endpoint: Some("https://opencode.ai/zen/go/v1/models".to_string()),
            default_models: vec![],
            description: "$10/month subscription, open-source models".to_string(),
        },
        ProviderInfo {
            id: "openai".to_string(),
            name: "OpenAI".to_string(),
            icon: "message-square".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key_required: true,
            is_free: false,
            is_subscription: false,
            is_payg: false,
            is_keyless: false,
            category: ProviderCategory::ThirdParty,
            models_endpoint: Some("https://api.openai.com/v1/models".to_string()),
            default_models: vec![],
            description: "GPT-4o, o3, and more".to_string(),
        },
        ProviderInfo {
            id: "anthropic".to_string(),
            name: "Anthropic".to_string(),
            icon: "brain".to_string(),
            base_url: "https://api.anthropic.com".to_string(),
            api_key_required: true,
            is_free: false,
            is_subscription: false,
            is_payg: false,
            is_keyless: false,
            category: ProviderCategory::ThirdParty,
            models_endpoint: Some("https://api.anthropic.com/v1/models".to_string()),
            default_models: vec![],
            description: "Claude Sonnet, Opus, Haiku".to_string(),
        },
        ProviderInfo {
            id: "google".to_string(),
            name: "Google".to_string(),
            icon: "sparkle".to_string(),
            base_url: "https://generativelanguage.googleapis.com".to_string(),
            api_key_required: true,
            is_free: false,
            is_subscription: false,
            is_payg: false,
            is_keyless: false,
            category: ProviderCategory::ThirdParty,
            models_endpoint: Some("https://generativelanguage.googleapis.com/v1beta/models".to_string()),
            default_models: vec![],
            description: "Gemini 3.x models".to_string(),
        },
        ProviderInfo {
            id: "alibaba".to_string(),
            name: "Alibaba".to_string(),
            icon: "cloud".to_string(),
            base_url: "https://dashscope.aliyuncs.com".to_string(),
            api_key_required: true,
            is_free: false,
            is_subscription: false,
            is_payg: false,
            is_keyless: false,
            category: ProviderCategory::ThirdParty,
            models_endpoint: Some("https://dashscope.aliyuncs.com/compatible-mode/v1/models".to_string()),
            default_models: vec![],
            description: "Qwen models via DashScope".to_string(),
        },
        ProviderInfo {
            id: "kimi".to_string(),
            name: "Kimi".to_string(),
            icon: "moon".to_string(),
            base_url: "https://api.moonshot.cn".to_string(),
            api_key_required: true,
            is_free: false,
            is_subscription: false,
            is_payg: false,
            is_keyless: false,
            category: ProviderCategory::ThirdParty,
            models_endpoint: Some("https://api.moonshot.cn/v1/models".to_string()),
            default_models: vec![],
            description: "Kimi K2 models".to_string(),
        },
        ProviderInfo {
            id: "ollama".to_string(),
            name: "Ollama".to_string(),
            icon: "cpu".to_string(),
            base_url: "http://localhost:11434".to_string(),
            api_key_required: false,
            is_free: true,
            is_subscription: false,
            is_payg: false,
            is_keyless: true,
            category: ProviderCategory::Local,
            models_endpoint: None,
            default_models: vec![
                "codellama".to_string(),
                "deepseek-coder".to_string(),
                "llama3".to_string(),
                "qwen2.5-coder".to_string(),
            ],
            description: "Local models via Ollama runtime".to_string(),
        },
    ]
}

pub fn get_provider_by_id(id: &str) -> Option<ProviderInfo> {
    get_all_providers().into_iter().find(|p| p.id == id)
}

#[async_trait::async_trait]
pub trait ModelProvider: Send + Sync {
    fn name(&self) -> &'static str;
    async fn complete(
        &self,
        config: &ModelConfig,
        request: CompletionRequest,
        tools: Option<&[crate::tool_format::ToolDefinition]>,
    ) -> anyhow::Result<(
        CompletionResponse,
        Option<Vec<crate::tool_format::ToolCall>>,
    )>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub conversation: crate::conversation::Conversation,
    pub system_prompt: Option<String>,
    pub temperature: f32,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub content: String,
    pub tokens_used: u32,
    pub finish_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderModel {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}
