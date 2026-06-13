use async_trait::async_trait;
use std::path::Path;
use tundracode_models::{ProviderRegistry, StreamEvent, ToolDefinition};
use tundracode_tools::{generate_unified_diff, ToolRegistry};

use crate::agent::{Agent, AgentContext, AgentInput, AgentOutput, BuildMode, DiffKind, DiffProposal};
use crate::compaction::{CompactionConfig, ContextCompactor};
use crate::context_manager::ContextManager;
use crate::memory::load_memory;
use crate::profile::AgentProfileRegistry;
use crate::r#loop::{AgentLoop, RunOutput};

pub struct BuildAgent;

#[async_trait]
impl Agent for BuildAgent {
    fn name(&self) -> &'static str {
        "Build"
    }

    fn system_prompt(&self) -> String {
        include_str!("prompts/build.txt").to_string()
    }

    fn allowed_tools(&self) -> Vec<&'static str> {
        vec![
            "ReadFile",
            "WriteFile",
            "ApplyPatch",
            "CreateFile",
            "DeleteFile",
            "ListDirectory",
            "RunCommand",
            "GetDiagnostics",
        ]
    }

    async fn run(&self, context: &AgentContext, input: AgentInput) -> anyhow::Result<AgentOutput> {
        self.run_internal(context, input, None).await
    }
}

impl BuildAgent {
    pub async fn run_with_streaming(
        &self,
        context: &AgentContext,
        input: AgentInput,
        on_event: Option<Box<dyn FnMut(StreamEvent) + Send>>,
    ) -> anyhow::Result<AgentOutput> {
        self.run_internal(context, input, on_event).await
    }

    async fn run_internal(
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
            agent_id: "build".to_string(),
            dry_run: context.build_mode == BuildMode::ReviewRequired,
        };

        let tools = self.build_tool_definitions(&tool_registry);

        let user_message = if let Some(annotations) = &input.plan_annotations {
            format!(
                "Plan a implementar:\n{}\n\nAnotaciones del usuario:\n{}",
                input.user_message, annotations
            )
        } else {
            input.user_message.clone()
        };

        let context_budget = 128_000u32;
        let context_manager = ContextManager::new(context_budget);
        let compactor = ContextCompactor::new(CompactionConfig::default());
        let memory_store = load_memory(Path::new(&context.workspace_path));

        let mut agent_loop = AgentLoop::new()
            .with_max_iterations(30)
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
            user_message: &user_message,
            tools: &tools,
            reasoning_effort: context.reasoning_effort.clone(),
            on_event,
        };
        let RunOutput {
            content: _,
            invocations,
            tokens_used,
            context_compacted: _,
        } = agent_loop.run(run_config).await?;

        let (proposals, tool_log) = self.proposals_from_invocations(&invocations, None)?;

        Ok(AgentOutput::ProposedChanges {
            proposals,
            invocations,
            tool_log,
            tokens_used,
        })
    }

    pub async fn run_sequential(
        &self,
        context: &AgentContext,
        input: AgentInput,
        on_event: Option<Box<dyn FnMut(StreamEvent) + Send>>,
    ) -> anyhow::Result<AgentOutput> {
        self.run_internal(context, input, on_event).await
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

    fn proposals_from_invocations(
        &self,
        invocations: &[crate::agent::ToolInvocation],
        task_number: Option<usize>,
    ) -> anyhow::Result<(Vec<DiffProposal>, Vec<String>)> {
        let mut proposals: Vec<DiffProposal> = Vec::new();
        let mut tool_log: Vec<String> = Vec::new();

        for (idx, inv) in invocations.iter().enumerate() {
            tool_log.push(self.format_invocation_log(inv));

            if !inv.success {
                continue;
            }

            match inv.tool_name.as_str() {
                "WriteFile" | "ApplyPatch" => {
                    let path = inv
                        .file_path
                        .clone()
                        .or_else(|| Self::path_from_args(&inv.arguments));
                    let Some(path) = path else { continue };

                    let before = inv.before.clone().unwrap_or_default();
                    let after = inv.after.clone().unwrap_or_default();

                    if before == after {
                        continue;
                    }

                    let unified = if before.is_empty() {
                        Self::full_file_diff(&path, &after)
                    } else {
                        generate_unified_diff(
                            &before,
                            &after,
                            &format!("a/{}", path),
                            &format!("b/{}", path),
                        )
                    };

                    proposals.push(DiffProposal {
                        id: format!("proposal_{}", idx + 1),
                        file_path: path,
                        kind: DiffKind::Modify,
                        unified_diff: unified,
                        requires_user_confirmation: true,
                        before,
                        after,
                        tool_call_id: inv.call_id.clone(),
                        task_number,
                    });
                }
                "CreateFile" => {
                    let path = inv
                        .file_path
                        .clone()
                        .or_else(|| Self::path_from_args(&inv.arguments));
                    let Some(path) = path else { continue };

                    let after = inv.after.clone().unwrap_or_default();
                    let before = inv.before.clone().unwrap_or_default();

                    if !before.is_empty() {
                        continue;
                    }

                    proposals.push(DiffProposal {
                        id: format!("proposal_{}", idx + 1),
                        file_path: path.clone(),
                        kind: DiffKind::Create,
                        unified_diff: Self::full_file_diff(&path, &after),
                        requires_user_confirmation: true,
                        before,
                        after,
                        tool_call_id: inv.call_id.clone(),
                        task_number,
                    });
                }
                "DeleteFile" => {
                    let path = inv
                        .file_path
                        .clone()
                        .or_else(|| Self::path_from_args(&inv.arguments));
                    let Some(path) = path else { continue };

                    let before = inv.before.clone().unwrap_or_default();
                    let after = inv.after.clone().unwrap_or_default();

                    if before.is_empty() {
                        continue;
                    }

                    proposals.push(DiffProposal {
                        id: format!("proposal_{}", idx + 1),
                        file_path: path.clone(),
                        kind: DiffKind::Delete,
                        unified_diff: generate_unified_diff(
                            &before,
                            "",
                            &format!("a/{}", path),
                            &format!("b/{}", path),
                        ),
                        requires_user_confirmation: true,
                        before,
                        after,
                        tool_call_id: inv.call_id.clone(),
                        task_number,
                    });
                }
                _ => {}
            }
        }

        Ok((proposals, tool_log))
    }

    fn path_from_args(args: &serde_json::Value) -> Option<String> {
        args.get("path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    fn full_file_diff(path: &str, content: &str) -> String {
        let mut out = String::new();
        out.push_str(&format!("--- /dev/null\n+++ b/{}\n", path));
        out.push_str("@@ -0,0 +1,");
        out.push_str(&content.lines().count().to_string());
        out.push_str(" @@\n");
        for line in content.lines() {
            out.push('+');
            out.push_str(line);
            out.push('\n');
        }
        out
    }

    fn format_invocation_log(&self, inv: &crate::agent::ToolInvocation) -> String {
        let status = if inv.success { "ok" } else { "err" };
        format!(
            "Tool: {} | {} | call_id={} | args={}",
            inv.tool_name,
            status,
            inv.call_id,
            inv.arguments
        )
    }
}
