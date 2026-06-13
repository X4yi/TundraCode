pub mod pool;
pub mod types;

pub use types::{SubagentRequest, SubagentResult};
pub use pool::SubagentPool;

use std::path::Path;
use tundracode_models::{ProviderRegistry, StreamEvent, ToolDefinition};
use tundracode_tools::ToolRegistry;

use crate::agent::{AgentContext, AgentInput, AgentOutput};
use crate::compaction::{CompactionConfig, ContextCompactor};
use crate::context_manager::{ContextEntry, ContextEntryType, ContextManager};
use crate::memory::load_memory;
use crate::profile::AgentProfile;
use crate::r#loop::{AgentLoop, RunConfig, RunOutput};

pub fn subagent_compaction_config() -> CompactionConfig {
    CompactionConfig {
        tool_output_max_age_ms: 120_000,
        reasoning_max_age_ms: 300_000,
        session_summary_threshold: 20_000,
        enable_level1: true,
        enable_level2: true,
        enable_level3: true,
    }
}

pub const SUBAGENT_OVERFLOW_RATIO: f32 = 0.5;

#[allow(dead_code)]
pub struct SubagentOrchestrator {
    max_concurrent: usize,
    budget_used: u32,
}

impl SubagentOrchestrator {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            max_concurrent,
            budget_used: 0,
        }
    }

    pub fn budget_remaining(&self) -> u32 {
        256_000u32.saturating_sub(self.budget_used)
    }

    pub async fn execute_subagent(
        &mut self,
        profile: &AgentProfile,
        request: &SubagentRequest,
        context: &AgentContext,
        mut on_event: Option<&mut (dyn FnMut(StreamEvent) + Send)>,
    ) -> SubagentResult {
        let input = AgentInput {
            user_message: request.task_description.clone(),
            plan_annotations: None,
            memory_excerpt: None,
        };

        if let Some(ref mut ev) = on_event {
            ev(StreamEvent::SubagentStart {
                agent_id: profile.id.clone(),
                task: request.task_description.chars().take(100).collect(),
            });
        }

        let start = std::time::Instant::now();
        let result = match self.run_agent(profile, request, context, input).await {
            Ok(output) => match output {
                AgentOutput::FinalAnswer { content, tokens_used } => {
                    let (summary, full_output, key_findings, files_referenced) =
                        self.summarize_output(&content);
                    self.budget_used += tokens_used;
                    SubagentResult {
                        agent_id: profile.id.clone(),
                        summary,
                        full_output: Some(full_output),
                        tokens_used,
                        success: true,
                        error: None,
                        key_findings,
                        files_referenced,
                    }
                }
                AgentOutput::ProposedChanges {
                    proposals,
                    invocations,
                    tool_log: _,
                    tokens_used,
                } => {
                    self.budget_used += tokens_used;
                    let files: Vec<String> = proposals.iter()
                        .filter_map(|p| Some(p.file_path.clone()))
                        .collect();
                    let findings = vec![
                        format!("{} file changes proposed", proposals.len()),
                        format!("{} tool invocations", invocations.len()),
                    ];
                    SubagentResult {
                        agent_id: profile.id.clone(),
                        summary: format!(
                            "Generated {} proposals with {} tool invocations. Files: {}",
                            proposals.len(),
                            invocations.len(),
                            files.join(", ")
                        ),
                        full_output: None,
                        tokens_used,
                        success: true,
                        error: None,
                        key_findings: findings,
                        files_referenced: files,
                    }
                }
                AgentOutput::Error(e) => SubagentResult {
                    agent_id: profile.id.clone(),
                    summary: format!("Error: {}", e),
                    full_output: None,
                    tokens_used: 0,
                    success: false,
                    error: Some(e),
                    key_findings: vec![],
                    files_referenced: vec![],
                },
            },
            Err(e) => SubagentResult {
                agent_id: profile.id.clone(),
                summary: format!("Execution failed: {}", e),
                full_output: None,
                tokens_used: 0,
                success: false,
                error: Some(e.to_string()),
                key_findings: vec![],
                files_referenced: vec![],
            },
        };

        if let Some(ref mut ev) = on_event {
            ev(StreamEvent::SubagentComplete {
                agent_id: profile.id.clone(),
                duration_ms: start.elapsed().as_millis() as u64,
                success: result.success,
            });
        }

        result
    }

    async fn run_agent(
        &self,
        profile: &AgentProfile,
        request: &SubagentRequest,
        context: &AgentContext,
        input: AgentInput,
    ) -> anyhow::Result<AgentOutput> {
        let provider_registry = ProviderRegistry::new();
        let mut tool_registry = ToolRegistry::new();

        let tool_names: Vec<&str> = profile.allowed_tools.iter().map(|s| s.as_str()).collect();
        #[allow(deprecated)]
        tool_registry.register_subset_legacy(&tool_names);
        tool_registry.register(Box::new(crate::task_tool::TaskTool::new(
            context.clone(),
            crate::profile::AgentProfileRegistry::new(),
        )));

        let tool_context = tundracode_tools::ToolContext {
            workspace_path: context.workspace_path.clone(),
            agent_id: profile.id.clone(),
            dry_run: profile.dry_run,
        };

        let tools: Vec<ToolDefinition> = profile
            .allowed_tools
            .iter()
            .filter_map(|name| {
                tool_registry.get(name).map(|tool| ToolDefinition {
                    name: tool.name().to_string(),
                    description: tool.description().to_string(),
                    parameters: tool.parameters_schema(),
                })
            })
            .collect();

        let context_budget = request.context_budget;
        let context_manager = ContextManager::new(context_budget);
        let compactor = ContextCompactor::new(subagent_compaction_config());
        let memory_store = load_memory(Path::new(&context.workspace_path));

        let mut agent_loop = AgentLoop::new()
            .with_max_iterations(profile.max_iterations)
            .with_budget_tokens(context_budget / 2)
            .with_context_manager(context_manager)
            .with_compactor(compactor)
            .with_memory_store(memory_store)
            .with_subagent_mode(true);

        let run_config = RunConfig {
            provider_registry: &provider_registry,
            tool_registry: &tool_registry,
            tool_context: &tool_context,
            provider_id: &context.model_config.provider,
            model_config: &context.model_config,
            system_prompt: &profile.system_prompt,
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

    fn summarize_output(&self, content: &str) -> (String, String, Vec<String>, Vec<String>) {
        let mut key_findings = Vec::new();
        let mut files_referenced = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
                key_findings.push(trimmed[2..].to_string());
            }
            if trimmed.contains(".rs") || trimmed.contains(".js") || trimmed.contains(".py")
                || trimmed.contains(".ts") || trimmed.contains(".toml")
                || trimmed.contains(".json") || trimmed.contains(".html")
                || trimmed.contains(".css") || trimmed.contains(".md")
            {
                let words: Vec<&str> = trimmed.split_whitespace().collect();
                for word in words {
                    let clean = word.trim_matches(|c: char| c == '`' || c == '(' || c == ')'
                        || c == ',' || c == ':' || c == ';' || c == '"' || c == '\'');
                    if clean.contains('/') && clean.contains('.')
                        && !clean.starts_with("http")
                        && clean.len() < 200
                    {
                        files_referenced.push(clean.to_string());
                    }
                }
            }
        }

        files_referenced.sort();
        files_referenced.dedup();

        let summary_max = 800;
        let summary = if content.len() <= summary_max {
            content.to_string()
        } else {
            let lines: Vec<&str> = content.lines().collect();
            let meaningful: Vec<&str> = lines.iter()
                .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
                .take(8)
                .cloned()
                .collect();
            format!(
                "{}...\n\n[{} chars, {} key findings, {} files referenced]",
                meaningful.join("\n"),
                content.len(),
                key_findings.len(),
                files_referenced.len()
            )
        };

        (summary, content.to_string(), key_findings, files_referenced)
    }

    pub fn inject_subagent_result(
        &self,
        context_manager: &mut ContextManager,
        result: &SubagentResult,
    ) {
        let mut content = result.summary.clone();
        if !result.key_findings.is_empty() {
            content += &format!(
                "\n\nHallazgos: {}",
                result.key_findings.iter().take(10).map(|s| s.as_str()).collect::<Vec<_>>().join("; ")
            );
        }
        if !result.files_referenced.is_empty() {
            content += &format!(
                "\nArchivos: {}",
                result.files_referenced.iter().take(10).map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
            );
        }

        let entry = ContextEntry {
            id: format!("subagent_{}_{}", result.agent_id, now_millis()),
            entry_type: ContextEntryType::SubagentResult,
            content,
            token_estimate: ContextManager::estimate_tokens_for_entry(&result.summary),
            priority: 50,
            created_at: now_millis(),
            last_accessed: now_millis(),
            compacted: false,
            compaction_summary: None,
        };
        context_manager.add_entry(entry);
    }
}

fn now_millis() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subagent_result_summary() {
        let orchestrator = SubagentOrchestrator::new(3);
        let result = SubagentResult {
            agent_id: "explorer".to_string(),
            summary: "Found 5 relevant files".to_string(),
            full_output: None,
            tokens_used: 100,
            success: true,
            error: None,
            key_findings: vec!["Found 5 files".to_string()],
            files_referenced: vec!["src/main.rs".to_string()],
        };

        let mut manager = ContextManager::new(128_000);
        orchestrator.inject_subagent_result(&mut manager, &result);
        assert_eq!(manager.entries.len(), 1);
    }

    #[test]
    fn test_summarize_output() {
        let orchestrator = SubagentOrchestrator::new(3);
        let short = "short output";
        let (summary, full, findings, files) = orchestrator.summarize_output(short);
        assert_eq!(summary, short);
        assert_eq!(full, short);
        assert!(findings.is_empty());
        assert!(files.is_empty());

        let long = "- Found main function in src/main.rs\n- Identified auth module in src/auth.rs\n".to_string() + &"a ".repeat(1000);
        let (summary2, _, findings2, files2) = orchestrator.summarize_output(&long);
        assert!(summary2.len() < long.len());
        assert_eq!(findings2.len(), 2);
        assert!(files2.len() >= 2);
    }

    #[test]
    fn test_budget_tracking() {
        let mut orchestrator = SubagentOrchestrator::new(4);
        assert_eq!(orchestrator.budget_remaining(), 256_000);
        orchestrator.budget_used = 100_000;
        assert_eq!(orchestrator.budget_remaining(), 256_000 - 100_000);
    }

    #[test]
    fn test_dynamic_budget_calculation() {
        assert_eq!(SubagentRequest::calculate_budget(128_000), 51_200);
        assert_eq!(SubagentRequest::calculate_budget(200_000), 80_000);
        assert_eq!(SubagentRequest::calculate_budget(0), 64_000);
    }
}
