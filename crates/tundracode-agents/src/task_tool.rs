use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tundracode_models::StreamEvent;
use tundracode_tools::{Tool, ToolCategory, ToolContext, ToolError, ToolResult};
use tundracode_permissions::Capability;

use crate::agent::AgentContext;
use crate::events::SubagentEventBus;
use crate::profile::AgentProfileRegistry;
use crate::subagent::{SubagentOrchestrator, SubagentRequest, SubagentPool};

pub struct TaskTool {
    context: AgentContext,
    profile_registry: AgentProfileRegistry,
    on_event: Option<std::sync::Arc<std::sync::Mutex<dyn FnMut(StreamEvent) + Send>>>,
    event_bus: Option<Arc<SubagentEventBus>>,
    pool: Option<Arc<SubagentPool>>,
}

impl TaskTool {
    pub fn new(context: AgentContext, profile_registry: AgentProfileRegistry) -> Self {
        Self {
            context,
            profile_registry,
            on_event: None,
            event_bus: None,
            pool: None,
        }
    }

    pub fn with_on_event(mut self, on_event: Option<std::sync::Arc<std::sync::Mutex<dyn FnMut(StreamEvent) + Send>>>) -> Self {
        self.on_event = on_event;
        self
    }

    pub fn with_event_bus(mut self, event_bus: Arc<SubagentEventBus>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    pub fn with_pool(mut self, pool: Arc<SubagentPool>) -> Self {
        self.pool = Some(pool);
        self
    }
}

#[async_trait]
impl Tool for TaskTool {
    fn name(&self) -> &'static str {
        "Task"
    }

    fn description(&self) -> &'static str {
        "Delegates a task to a specialized subagent. \
         Available subagents: explorer (investigate codebase), searcher (web research), \
         debugger (analyze bugs and root causes). \
         The subagent will execute the task with its own tools and return results. \
         Also supports batch mode: send an array of tasks to execute in parallel."
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Subagent
    }

    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::SubagentSpawn { allowed_profiles: vec![] }]
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "agent": {
                    "type": "string",
                    "description": "Subagent name: explorer, searcher, debugger"
                },
                "task": {
                    "type": "string",
                    "description": "Detailed task description"
                },
                "tasks": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "agent": { "type": "string", "description": "Subagent name" },
                            "task": { "type": "string", "description": "Task description" }
                        },
                        "required": ["agent", "task"]
                    },
                    "description": "Batch of tasks to execute in parallel. If provided, ignores individual agent/task."
                }
            },
            "required": []
        })
    }

    async fn execute(&self, _context: &ToolContext, params: Value) -> Result<ToolResult, ToolError> {
        if let Some(tasks) = params.get("tasks").and_then(|v| v.as_array()) {
            if !tasks.is_empty() {
                return self.execute_batch(tasks).await;
            }
        }

        let agent = params
            .get("agent")
            .and_then(|v| v.as_str())
            .unwrap_or("explorer")
            .to_string();
        let task = params
            .get("task")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let profile = self.profile_registry.get(&agent);
        let Some(profile) = profile else {
            return Ok(ToolResult::ok(format!(
                "Error: Agent '{}' not found. Available agents: explorer, searcher, debugger",
                agent
            )));
        };

        let mut orchestrator = SubagentOrchestrator::new(4);

        let parent_context = self.context.budget_tokens;
        let request = SubagentRequest::new(
            agent.clone(),
            task.clone(),
            parent_context,
        );

        let result = orchestrator
            .execute_subagent(profile, &request, &self.context, None)
            .await;

        self.format_result(&agent, &result)
    }
}

impl TaskTool {
    async fn execute_batch(&self, tasks: &[Value]) -> Result<ToolResult, ToolError> {
        if let Some(ref pool) = self.pool {
            return self.execute_batch_with_pool(pool, tasks).await;
        }

        let mut handles = Vec::new();

        for task_params in tasks {
            let agent = task_params.get("agent")
                .and_then(|v| v.as_str())
                .unwrap_or("explorer")
                .to_string();
            let task = task_params.get("task")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let profile = self.profile_registry.get(&agent).cloned();
            let context = self.context.clone();
            let parent_context = context.budget_tokens;
            let request = SubagentRequest::new(
                agent.clone(),
                task,
                parent_context,
            );

            handles.push(tokio::spawn(async move {
                if let Some(profile) = profile {
                    let mut orch = SubagentOrchestrator::new(4);
                    orch.execute_subagent(&profile, &request, &context, None).await
                } else {
                    crate::subagent::SubagentResult {
                        agent_id: agent.clone(),
                        summary: format!("Unknown agent: {}", agent),
                        full_output: None,
                        tokens_used: 0,
                        success: false,
                        error: Some("Agent not found".to_string()),
                        key_findings: vec![],
                        files_referenced: vec![],
                    }
                }
            }));
        }

        let mut results = Vec::new();
        for handle in handles {
            if let Ok(result) = handle.await {
                results.push(result);
            }
        }

        let output = results.iter().map(|r| {
            if r.success {
                let mut out = format!("[{}]\n{}", r.agent_id, r.summary);
                if !r.key_findings.is_empty() {
                    out += &format!("\n\nHallazgos:\n{}", r.key_findings.iter()
                        .map(|f| format!("  - {}", f))
                        .collect::<Vec<_>>()
                        .join("\n"));
                }
                if !r.files_referenced.is_empty() {
                    out += &format!("\nArchivos: {}", r.files_referenced.join(", "));
                }
                out
            } else {
                format!("[{} - Fallo]\n{}", r.agent_id, r.error.as_deref().unwrap_or("Unknown error"))
            }
        }).collect::<Vec<_>>().join("\n\n");

        Ok(ToolResult::ok(output))
    }

    async fn execute_batch_with_pool(
        &self,
        pool: &SubagentPool,
        tasks: &[Value],
    ) -> Result<ToolResult, ToolError> {
        let parent_context = self.context.budget_tokens;
        let requests: Vec<SubagentRequest> = tasks.iter().map(|task_params| {
            let agent = task_params.get("agent")
                .and_then(|v| v.as_str())
                .unwrap_or("explorer")
                .to_string();
            let task = task_params.get("task")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            SubagentRequest::new(agent, task, parent_context)
        }).collect();

        let profile_registry = self.profile_registry.clone();
        let context = self.context.clone();

        let executor = move |request: SubagentRequest| {
            let profile_registry = profile_registry.clone();
            let context = context.clone();

            Box::pin(async move {
                let profile = profile_registry.get(&request.agent_profile_id);
                let Some(profile) = profile else {
                    return crate::subagent::SubagentResult {
                        agent_id: request.agent_profile_id,
                        summary: "Agent profile not found".to_string(),
                        full_output: None,
                        tokens_used: 0,
                        success: false,
                        error: Some("Profile not found".to_string()),
                        key_findings: vec![],
                        files_referenced: vec![],
                    };
                };

                let mut orch = SubagentOrchestrator::new(4);
                orch.execute_subagent(&profile, &request, &context, None).await
            }) as futures::future::BoxFuture<'static, crate::subagent::SubagentResult>
        };

        let results = pool.execute_batch(requests, executor).await;

        let output = results.iter().map(|r| {
            if r.success {
                let mut out = format!("[{}]\n{}", r.agent_id, r.summary);
                if !r.key_findings.is_empty() {
                    out += &format!("\n\nHallazgos:\n{}", r.key_findings.iter()
                        .map(|f| format!("  - {}", f))
                        .collect::<Vec<_>>()
                        .join("\n"));
                }
                if !r.files_referenced.is_empty() {
                    out += &format!("\nArchivos: {}", r.files_referenced.join(", "));
                }
                out
            } else {
                format!("[{} - Fallo]\n{}", r.agent_id, r.error.as_deref().unwrap_or("Unknown error"))
            }
        }).collect::<Vec<_>>().join("\n\n");

        Ok(ToolResult::ok(output))
    }

    fn format_result(&self, agent: &str, result: &crate::subagent::SubagentResult) -> Result<ToolResult, ToolError> {
        if result.success {
            let mut output = format!("[Subagente: {}]\n{}", agent, result.summary);
            if !result.key_findings.is_empty() {
                output += &format!(
                    "\n\nHallazgos clave:\n{}",
                    result.key_findings.iter()
                        .map(|f| format!("- {}", f))
                        .collect::<Vec<_>>()
                        .join("\n")
                );
            }
            if !result.files_referenced.is_empty() {
                output += &format!(
                    "\n\nArchivos referenciados: {}",
                    result.files_referenced.join(", ")
                );
            }
            output += &format!("\n\n(Tokens usados: {})", result.tokens_used);
            Ok(ToolResult::ok(output))
        } else {
            Ok(ToolResult::ok(format!(
                "[Subagente: {} - Fallo]\n{}",
                agent,
                result.error.as_deref().unwrap_or("Error desconocido")
            )))
        }
    }
}
