use async_trait::async_trait;
use std::path::Path;
use tundracode_models::{ProviderRegistry, StreamEvent, ToolDefinition};
use tundracode_tools::ToolRegistry;

use crate::agent::{Agent, AgentContext, AgentInput, AgentOutput};
use crate::compaction::{CompactionConfig, ContextCompactor};
use crate::context_manager::ContextManager;
use crate::memory::load_memory;
use crate::profile::AgentProfileRegistry;
use crate::r#loop::{AgentLoop, RunOutput};

pub struct AskAgent;

#[async_trait]
impl Agent for AskAgent {
    fn name(&self) -> &'static str {
        "Ask"
    }

    fn system_prompt(&self) -> String {
        include_str!("prompts/ask.txt").to_string()
    }

    fn allowed_tools(&self) -> Vec<&'static str> {
        vec!["ReadFile", "SearchCodebase", "SearchInWeb"]
    }

    async fn run(&self, context: &AgentContext, input: AgentInput) -> anyhow::Result<AgentOutput> {
        let provider_registry = ProviderRegistry::new();
        let mut tool_registry = ToolRegistry::new();
        #[allow(deprecated)]
        tool_registry.register_subset_legacy(&self.allowed_tools());
        tool_registry.register(Box::new(crate::task_tool::TaskTool::new(
            context.clone(),
            AgentProfileRegistry::new(),
        )));

        let tool_context = tundracode_tools::ToolContext {
            workspace_path: context.workspace_path.clone(),
            agent_id: "ask".to_string(),
            dry_run: true,
        };

        let tools = self.build_tool_definitions(&tool_registry);

        let context_budget = 128_000u32;
        let context_manager = ContextManager::new(context_budget);
        let compactor = ContextCompactor::new(CompactionConfig::default());
        let memory_store = load_memory(Path::new(&context.workspace_path));

        let mut agent_loop = AgentLoop::new()
            .with_max_iterations(20)
            .with_budget_tokens(context_budget / 2)
            .with_context_manager(context_manager)
            .with_compactor(compactor)
            .with_memory_store(memory_store);
        let run_config = crate::r#loop::RunConfig {
            provider_registry: &provider_registry,
            tool_registry: &tool_registry,
            tool_context: &tool_context,
            provider_id: &context.model_config.provider,
            model_config: &context.model_config,
            system_prompt: &self.system_prompt(),
            user_message: &input.user_message,
            tools: &tools,
            reasoning_effort: context.reasoning_effort.clone(),
            on_event: None,
        };
        let RunOutput {
            content,
            invocations: _,
            tokens_used,
            context_compacted: _,
        } = agent_loop.run(run_config).await?;

        Ok(AgentOutput::FinalAnswer {
            content,
            tokens_used,
        })
    }
}

impl AskAgent {
    pub async fn run_with_streaming(
        &self,
        context: &AgentContext,
        input: AgentInput,
        on_event: Option<Box<dyn FnMut(StreamEvent) + Send>>,
    ) -> anyhow::Result<AgentOutput> {
        let provider_registry = ProviderRegistry::new();
        let mut tool_registry = ToolRegistry::new();
        #[allow(deprecated)]
        tool_registry.register_subset_legacy(&self.allowed_tools());
        tool_registry.register(Box::new(crate::task_tool::TaskTool::new(
            context.clone(),
            AgentProfileRegistry::new(),
        )));

        let tool_context = tundracode_tools::ToolContext {
            workspace_path: context.workspace_path.clone(),
            agent_id: "ask".to_string(),
            dry_run: true,
        };

        let tools = self.build_tool_definitions(&tool_registry);

        let context_budget = 128_000u32;
        let context_manager = ContextManager::new(context_budget);
        let compactor = ContextCompactor::new(CompactionConfig::default());
        let memory_store = load_memory(Path::new(&context.workspace_path));

        let mut agent_loop = AgentLoop::new()
            .with_max_iterations(20)
            .with_budget_tokens(context_budget / 2)
            .with_context_manager(context_manager)
            .with_compactor(compactor)
            .with_memory_store(memory_store);
        let run_config = crate::r#loop::RunConfig {
            provider_registry: &provider_registry,
            tool_registry: &tool_registry,
            tool_context: &tool_context,
            provider_id: &context.model_config.provider,
            model_config: &context.model_config,
            system_prompt: &self.system_prompt(),
            user_message: &input.user_message,
            tools: &tools,
            reasoning_effort: context.reasoning_effort.clone(),
            on_event,
        };
        let RunOutput {
            content,
            invocations: _,
            tokens_used,
            context_compacted: _,
        } = agent_loop.run(run_config).await?;

        Ok(AgentOutput::FinalAnswer {
            content,
            tokens_used,
        })
    }

    fn build_tool_definitions(&self, registry: &ToolRegistry) -> Vec<ToolDefinition> {
        self.allowed_tools()
            .iter()
            .filter_map(|name| {
                registry.get(name).map(|tool| ToolDefinition {
                    name: tool.name().to_string(),
                    description: tool.description().to_string(),
                    parameters: tool.parameters_schema(),
                })
            })
            .collect()
    }
}
