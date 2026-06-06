use anyhow::Result;
use reqwest::Client;
use std::collections::HashMap;

use crate::provider::{CompletionRequest, CompletionResponse, ModelConfig, ModelProvider};
use crate::tool_format::{ToolCall, ToolDefinition};

pub mod anthropic;
pub mod google;
pub mod ollama;
pub mod openai_compat;

pub struct ProviderRegistry {
    providers: HashMap<String, Box<dyn ModelProvider>>,
    client: Client,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("Failed to create HTTP client");

        let mut registry = Self {
            providers: HashMap::new(),
            client,
        };

        registry.register_openai_compat();
        registry.register_anthropic();
        registry.register_google();
        registry.register_ollama();

        registry
    }

    fn register_openai_compat(&mut self) {
        let provider_ids = [
            "openai",
            "opencode-free",
            "opencode-zen",
            "opencode-go",
            "alibaba",
            "kimi",
        ];
        for id in provider_ids {
            self.providers.insert(
                id.to_string(),
                Box::new(OpenAiCompatProviderWrapper { id: id.to_string() }),
            );
        }
    }

    fn register_anthropic(&mut self) {
        self.providers
            .insert("anthropic".to_string(), Box::new(AnthropicProviderWrapper));
    }

    fn register_google(&mut self) {
        self.providers
            .insert("google".to_string(), Box::new(GoogleProviderWrapper));
    }

    fn register_ollama(&mut self) {
        self.providers
            .insert("ollama".to_string(), Box::new(OllamaProviderWrapper));
    }

    pub fn get(&self, provider_id: &str) -> Option<&dyn ModelProvider> {
        self.providers.get(provider_id).map(|p| p.as_ref())
    }

    pub fn has(&self, provider_id: &str) -> bool {
        self.providers.contains_key(provider_id)
    }

    pub fn list_ids(&self) -> Vec<&str> {
        self.providers.keys().map(|k| k.as_str()).collect()
    }

    pub fn client(&self) -> &Client {
        &self.client
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

struct OpenAiCompatProviderWrapper {
    id: String,
}

#[async_trait::async_trait]
impl ModelProvider for OpenAiCompatProviderWrapper {
    fn name(&self) -> &'static str {
        "openai-compat"
    }

    async fn complete(
        &self,
        config: &ModelConfig,
        request: CompletionRequest,
        tools: Option<&[ToolDefinition]>,
    ) -> Result<(CompletionResponse, Option<Vec<ToolCall>>)> {
        let api_key = config.api_key.as_deref();
        let default_url = self.default_base_url();
        let base_url = config.base_url.as_deref().unwrap_or(&default_url);
        let is_keyless = self.is_keyless();

        openai_compat::OpenAiCompatProvider::complete(
            &Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()?,
            base_url,
            api_key,
            &config.model,
            &request,
            tools,
            is_keyless,
        )
        .await
    }
}

impl OpenAiCompatProviderWrapper {
    fn default_base_url(&self) -> String {
        match self.id.as_str() {
            "openai" => "https://api.openai.com/v1".to_string(),
            "opencode-free" => "https://opencode.ai/zen/v1".to_string(),
            "opencode-zen" => "https://opencode.ai/zen/v1".to_string(),
            "opencode-go" => "https://opencode.ai/zen/go/v1".to_string(),
            "alibaba" => "https://dashscope.aliyuncs.com".to_string(),
            "kimi" => "https://api.moonshot.cn".to_string(),
            _ => "https://api.openai.com/v1".to_string(),
        }
    }

    fn is_keyless(&self) -> bool {
        matches!(self.id.as_str(), "opencode-free")
    }
}

struct AnthropicProviderWrapper;

#[async_trait::async_trait]
impl ModelProvider for AnthropicProviderWrapper {
    fn name(&self) -> &'static str {
        "anthropic"
    }

    async fn complete(
        &self,
        config: &ModelConfig,
        request: CompletionRequest,
        tools: Option<&[ToolDefinition]>,
    ) -> Result<(CompletionResponse, Option<Vec<ToolCall>>)> {
        let api_key = config.api_key.as_deref();
        let base_url = config
            .base_url
            .as_deref()
            .unwrap_or("https://api.anthropic.com");

        anthropic::AnthropicProvider::complete(
            &Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()?,
            base_url,
            api_key,
            &config.model,
            &request,
            tools,
        )
        .await
    }
}

struct GoogleProviderWrapper;

#[async_trait::async_trait]
impl ModelProvider for GoogleProviderWrapper {
    fn name(&self) -> &'static str {
        "google"
    }

    async fn complete(
        &self,
        config: &ModelConfig,
        request: CompletionRequest,
        tools: Option<&[ToolDefinition]>,
    ) -> Result<(CompletionResponse, Option<Vec<ToolCall>>)> {
        let api_key = config.api_key.as_deref();
        let base_url = config
            .base_url
            .as_deref()
            .unwrap_or("https://generativelanguage.googleapis.com");

        google::GoogleProvider::complete(
            &Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()?,
            base_url,
            api_key,
            &config.model,
            &request,
            tools,
        )
        .await
    }
}

pub use anthropic::AnthropicProvider;
pub use google::GoogleProvider;
pub use ollama::OllamaProvider;
pub use openai_compat::OpenAiCompatProvider;

struct OllamaProviderWrapper;

#[async_trait::async_trait]
impl ModelProvider for OllamaProviderWrapper {
    fn name(&self) -> &'static str {
        "ollama"
    }

    async fn complete(
        &self,
        config: &ModelConfig,
        request: CompletionRequest,
        tools: Option<&[ToolDefinition]>,
    ) -> Result<(CompletionResponse, Option<Vec<ToolCall>>)> {
        let base_url = config
            .base_url
            .as_deref()
            .unwrap_or("http://localhost:11434");

        ollama::OllamaProvider::complete(
            &Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()?,
            base_url,
            &config.model,
            &request,
            tools,
        )
        .await
    }
}
