use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

use tundracode_models::{ProviderRegistry, ToolDefinition};
use tundracode_tools::{ToolCatalog, ToolRegistry};
use tundracode_permissions::{PermissionGuard, PermissionPolicy, PolicyRegistry};

use crate::agent::{AgentContext, AgentInput, AgentOutput};
use crate::compaction::{CompactionConfig, ContextCompactor};
use crate::context_manager::ContextManager;
use crate::events::SubagentEventBus;
use crate::memory::load_memory;
use crate::r#loop::{AgentLoop, RunConfig, RunOutput};

pub struct AgentFactory {
    policy_registry: Arc<RwLock<PolicyRegistry>>,
    tool_catalog: Arc<ToolCatalog>,
    event_bus: Arc<SubagentEventBus>,
}

impl AgentFactory {
    pub fn new(event_bus: Arc<SubagentEventBus>) -> Self {
        Self {
            policy_registry: Arc::new(RwLock::new(PolicyRegistry::new())),
            tool_catalog: Arc::new(Self::build_tool_catalog()),
            event_bus,
        }
    }

    pub fn with_policy_registry(mut self, registry: PolicyRegistry) -> Self {
        self.policy_registry = Arc::new(RwLock::new(registry));
        self
    }

    fn build_tool_catalog() -> ToolCatalog {
        use tundracode_tools::{
            ApplyPatchTool, CreateFileTool, DeleteFileTool, GetDiagnosticsTool, GetWorkspaceTool,
            ListDirectoryTool, PlanCreateFileTool, PlanWriteFileTool, ReadFileTool, RunCommandTool,
            SearchCodebaseTool, SearchInWebTool, WriteFileTool,
        };

        let mut catalog = ToolCatalog::new();
        catalog.register(|| ReadFileTool);
        catalog.register(|| WriteFileTool);
        catalog.register(|| CreateFileTool);
        catalog.register(|| DeleteFileTool);
        catalog.register(|| ListDirectoryTool);
        catalog.register(|| GetWorkspaceTool);
        catalog.register(|| RunCommandTool);
        catalog.register(|| SearchCodebaseTool);
        catalog.register(|| SearchInWebTool);
        catalog.register(|| GetDiagnosticsTool);
        catalog.register(|| ApplyPatchTool);
        catalog.register(|| PlanCreateFileTool);
        catalog.register(|| PlanWriteFileTool);
        catalog
    }

    pub async fn create_agent(
        &self,
        policy_id: &str,
        context: &AgentContext,
    ) -> Result<ManagedAgent, String> {
        let policy = {
            let registry = self.policy_registry.read().await;
            let ids = registry.list_ids().await;
            drop(registry);
            let registry = self.policy_registry.read().await;
            registry.get(policy_id).await.ok_or_else(|| {
                format!("Policy '{}' not found. Available: {:?}", policy_id, ids)
            })?
        };

        let workspace = Path::new(&context.workspace_path);
        let permission_guard = PermissionGuard::new(policy.clone(), workspace.to_path_buf());

        let tool_names: Vec<&str> = policy.allowed_tool_names();
        let mut tool_registry = ToolRegistry::new();
        tool_registry
            .register_subset(&self.tool_catalog, &tool_names)
            .map_err(|e| format!("Failed to register tools: {}", e))?;

        let tools: Vec<ToolDefinition> = tool_names
            .iter()
            .filter_map(|name| {
                tool_registry.get(name).map(|tool| ToolDefinition {
                    name: tool.name().to_string(),
                    description: tool.description().to_string(),
                    parameters: tool.parameters_schema(),
                })
            })
            .collect();

        let context_budget = if policy.budget_tokens > 0 {
            policy.budget_tokens
        } else {
            context.budget_tokens
        };
        let context_manager = ContextManager::new(context_budget);
        let compactor = ContextCompactor::new(CompactionConfig::default());
        let memory_store = load_memory(workspace);

        let agent_loop = AgentLoop::new()
            .with_max_iterations(policy.max_iterations)
            .with_budget_tokens(context_budget / 2)
            .with_context_manager(context_manager)
            .with_compactor(compactor)
            .with_memory_store(memory_store)
            .with_event_bus(Arc::clone(&self.event_bus));

        let tool_context = tundracode_tools::ToolContext {
            workspace_path: context.workspace_path.clone(),
            agent_id: policy.id.clone(),
            dry_run: policy.dry_run,
        };

        Ok(ManagedAgent {
            policy,
            tool_registry,
            permission_guard,
            agent_loop,
            tool_context,
            tools,
        })
    }

    pub async fn run_agent(
        &self,
        policy_id: &str,
        context: &AgentContext,
        input: AgentInput,
        system_prompt: &str,
    ) -> anyhow::Result<AgentOutput> {
        let managed = self.create_agent(policy_id, context).await
            .map_err(|e| anyhow::anyhow!(e))?;

        managed.run(input, system_prompt, context, None).await
    }

    pub async fn run_agent_with_streaming(
        &self,
        policy_id: &str,
        context: &AgentContext,
        input: AgentInput,
        system_prompt: &str,
        on_event: Option<Box<dyn FnMut(tundracode_models::StreamEvent) + Send>>,
    ) -> anyhow::Result<AgentOutput> {
        let managed = self.create_agent(policy_id, context).await
            .map_err(|e| anyhow::anyhow!(e))?;

        managed.run(input, system_prompt, context, on_event).await
    }

    pub fn event_bus(&self) -> &SubagentEventBus {
        &self.event_bus
    }

    pub async fn register_policy(&self, policy: PermissionPolicy) {
        let mut registry = self.policy_registry.write().await;
        registry.register(policy).await;
    }

    pub async fn list_policies(&self) -> Vec<String> {
        let registry = self.policy_registry.read().await;
        registry.list_ids().await
    }
}

pub struct ManagedAgent {
    pub policy: PermissionPolicy,
    pub tool_registry: ToolRegistry,
    pub permission_guard: PermissionGuard,
    pub agent_loop: AgentLoop,
    pub tool_context: tundracode_tools::ToolContext,
    pub tools: Vec<ToolDefinition>,
}

impl ManagedAgent {
    pub async fn run(
        mut self,
        input: AgentInput,
        system_prompt: &str,
        context: &AgentContext,
        on_event: Option<Box<dyn FnMut(tundracode_models::StreamEvent) + Send>>,
    ) -> anyhow::Result<AgentOutput> {
        let provider_registry = ProviderRegistry::new();

        let user_message = if let Some(annotations) = &input.plan_annotations {
            format!(
                "Plan to implement:\n{}\n\nUser annotations:\n{}",
                input.user_message, annotations
            )
        } else if let Some(memory) = &input.memory_excerpt {
            format!(
                "Project context (memory.md):\n{}\n\nUser task:\n{}",
                memory, input.user_message
            )
        } else {
            input.user_message.clone()
        };

        let run_config = RunConfig {
            provider_registry: &provider_registry,
            tool_registry: &self.tool_registry,
            tool_context: &self.tool_context,
            provider_id: &context.model_config.provider,
            model_config: &context.model_config,
            system_prompt,
            user_message: &user_message,
            tools: &self.tools,
            reasoning_effort: context.reasoning_effort.clone(),
            on_event,
        };

        let RunOutput {
            content,
            invocations,
            tokens_used,
            context_compacted: _,
        } = self.agent_loop.run(run_config).await?;

        let is_build_policy = self.policy.id == "build";
        if is_build_policy {
            let proposals = self.build_proposals_from_invocations(&invocations);
            let tool_log: Vec<String> = invocations.iter().map(|inv| {
                let status = if inv.success { "ok" } else { "err" };
                format!(
                    "Tool: {} | {} | call_id={} | args={}",
                    inv.tool_name, status, inv.call_id, inv.arguments
                )
            }).collect();

            Ok(AgentOutput::ProposedChanges {
                proposals,
                invocations,
                tool_log,
                tokens_used,
            })
        } else {
            Ok(AgentOutput::FinalAnswer {
                content,
                tokens_used,
            })
        }
    }

    fn build_proposals_from_invocations(
        &self,
        invocations: &[crate::agent::ToolInvocation],
    ) -> Vec<crate::agent::DiffProposal> {
        use tundracode_tools::generate_unified_diff;
        use crate::agent::{DiffKind, DiffProposal};

        let mut proposals = Vec::new();

        for (idx, inv) in invocations.iter().enumerate() {
            if !inv.success {
                continue;
            }

            match inv.tool_name.as_str() {
                "WriteFile" | "ApplyPatch" => {
                    let path = inv.file_path.clone()
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
                            &before, &after,
                            &format!("a/{}", path), &format!("b/{}", path),
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
                        task_number: None,
                    });
                }
                "CreateFile" => {
                    let path = inv.file_path.clone()
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
                        task_number: None,
                    });
                }
                "DeleteFile" => {
                    let path = inv.file_path.clone()
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
                            &before, "",
                            &format!("a/{}", path), &format!("b/{}", path),
                        ),
                        requires_user_confirmation: true,
                        before,
                        after,
                        tool_call_id: inv.call_id.clone(),
                        task_number: None,
                    });
                }
                _ => {}
            }
        }

        proposals
    }

    fn path_from_args(args: &serde_json::Value) -> Option<String> {
        args.get("path")
            .or_else(|| args.get("p"))
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
}
