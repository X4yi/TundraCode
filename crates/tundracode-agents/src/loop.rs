use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use tundracode_models::{
    CompletionRequest, Conversation, MessageRole, ModelConfig, ProviderRegistry, StreamEvent,
    ToolCallPayload, ToolDefinition,
};
use tundracode_tools::{ToolContext, ToolRegistry};

use crate::agent::ToolInvocation;
use crate::context_manager::{ContextEntry, ContextEntryType, ContextManager};
use crate::compaction::ContextCompactor;
use crate::events::{SubagentEvent, SubagentEventBus};
use crate::memory::MemoryStore;

const TOOL_OUTPUT_MAX_CHARS: usize = 1_200;
const SUBAGENT_TOOL_OUTPUT_MAX_CHARS: usize = 800;
const OUTPUT_TRUNCATION_SUFFIX: &str = "... [truncated]";
const CONTEXT_OVERFLOW_RATIO: f32 = 0.7;
const SUBAGENT_OVERFLOW_RATIO: f32 = 0.5;
#[allow(dead_code)]
const DEEP_COMPACT_TOKEN_BUDGET: u32 = 4_000;

pub struct AgentLoop {
    max_iterations: usize,
    cancel_flag: Option<Arc<AtomicBool>>,
    budget_tokens: Option<u32>,
    context_manager: Option<ContextManager>,
    compactor: Option<ContextCompactor>,
    memory_store: Option<MemoryStore>,
    deep_compact_in_progress: bool,
    event_bus: Option<Arc<SubagentEventBus>>,
    subagent_mode: bool,
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
    pub reasoning_effort: Option<String>,
    pub on_event: Option<Box<dyn FnMut(StreamEvent) + Send>>,
}

pub struct RunOutput {
    pub content: String,
    pub invocations: Vec<ToolInvocation>,
    pub tokens_used: u32,
    pub context_compacted: bool,
}

impl AgentLoop {
    pub fn new() -> Self {
        Self {
            max_iterations: 8,
            cancel_flag: None,
            budget_tokens: None,
            context_manager: None,
            compactor: None,
            memory_store: None,
            deep_compact_in_progress: false,
            event_bus: None,
            subagent_mode: false,
        }
    }

    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }

    pub fn with_cancel_flag(mut self, flag: Arc<AtomicBool>) -> Self {
        self.cancel_flag = Some(flag);
        self
    }

    pub fn with_budget_tokens(mut self, budget: u32) -> Self {
        self.budget_tokens = Some(budget);
        self
    }

    pub fn with_context_manager(mut self, manager: ContextManager) -> Self {
        self.context_manager = Some(manager);
        self
    }

    pub fn with_compactor(mut self, compactor: ContextCompactor) -> Self {
        self.compactor = Some(compactor);
        self
    }

    pub fn with_memory_store(mut self, store: MemoryStore) -> Self {
        self.memory_store = Some(store);
        self
    }

    pub fn with_event_bus(mut self, event_bus: Arc<SubagentEventBus>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    pub fn with_subagent_mode(mut self, enabled: bool) -> Self {
        self.subagent_mode = enabled;
        self
    }

    pub async fn run(&mut self, mut config: RunConfig<'_>) -> Result<RunOutput> {
        let mut conversation = Conversation::new();
        conversation.add_message(MessageRole::User, config.user_message.to_string());

        let mut invocations: Vec<ToolInvocation> = Vec::new();
        let mut total_tokens: u32 = 0;
        let mut context_compacted = false;

        // Build base system prompt with memory and context
        let mut system_prompt_parts = vec![config.system_prompt.to_string()];

        if let Some(ref memory_store) = self.memory_store {
            let memory = memory_store.full_context_injection();
            if !memory.is_empty() {
                system_prompt_parts.push(format!(
                    "\n## Memoria del Proyecto\n{}",
                    memory
                ));
            }
        }

        for _iteration in 0..self.max_iterations {
            if let Some(flag) = &self.cancel_flag {
                if flag.load(Ordering::Relaxed) {
                    return Ok(RunOutput {
                        content: "Agent cancelled".to_string(),
                        invocations,
                        tokens_used: total_tokens,
                        context_compacted,
                    });
                }
            }

            if let Some(budget) = self.budget_tokens {
                if total_tokens >= budget {
                    if self.deep_compact_in_progress {
                        return Ok(RunOutput {
                            content: format!(
                                "Agent reached token budget limit after compaction ({}/{})",
                                total_tokens, budget
                            ),
                            invocations,
                            tokens_used: total_tokens,
                            context_compacted: true,
                        });
                    }
                    self.deep_compact_in_progress = true;
                    if let Some(ref mut on_event) = config.on_event {
                        on_event(StreamEvent::ContextCompacted {
                            message: "Contexto agotado, compactando...".to_string(),
                        });
                    }
                    self.deep_compact(
                        &mut conversation,
                        &mut total_tokens,
                        config.provider_registry,
                        config.provider_id,
                        config.model_config,
                        config.user_message,
                    ).await?;
                    if let Some(ref mut on_event) = config.on_event {
                        on_event(StreamEvent::ReasoningToken(
                            format!("\n[Contexto compactado: {} tokens restantes]\n", total_tokens),
                        ));
                    }
                    continue;
                }
            }

            // Truncar tool outputs en la conversacion si son muy grandes
            // Esto evita que la conversacion crezca sin control
            let tool_output_max = if self.subagent_mode {
                SUBAGENT_TOOL_OUTPUT_MAX_CHARS
            } else {
                TOOL_OUTPUT_MAX_CHARS
            };
            maybe_truncate_tool_outputs(&mut conversation, tool_output_max);

            // Compact context if needed before each iteration
            if let Some(ref mut cm) = self.context_manager {
                if let Some(ref compactor) = self.compactor {
                    let ctx_estimate = estimate_conversation_tokens(&conversation);
                    let budget = cm.budget.max_tokens;
                    let overflow_ratio = if self.subagent_mode {
                        SUBAGENT_OVERFLOW_RATIO
                    } else {
                        CONTEXT_OVERFLOW_RATIO
                    };
                    if ctx_estimate > (budget as f32 * overflow_ratio) as u32 {
                        let result = compactor.compact(cm);
                        if result.entries_compacted > 0 {
                            context_compacted = true;
                        }
                    } else if cm.should_compact() {
                        let result = compactor.compact(cm);
                        if result.entries_compacted > 0 {
                            context_compacted = true;
                        }
                    }
                }
            }

            // Build system prompt with context
            let mut enriched_prompt = system_prompt_parts.join("\n\n");
            if let Some(ref cm) = self.context_manager {
                let ctx_str = cm.build_context_string();
                if !ctx_str.is_empty() {
                    enriched_prompt = format!("{}\n\n## Contexto de Sesion\n{}", enriched_prompt, ctx_str);
                }
            }

            let request = CompletionRequest {
                conversation: conversation.clone(),
                system_prompt: Some(enriched_prompt),
                reasoning_effort: config.reasoning_effort.clone(),
            };

            let tools_for_call = if config.tools.is_empty() {
                None
            } else {
                Some(config.tools)
            };

            let provider = config
                .provider_registry
                .get(config.provider_id)
                .ok_or_else(|| anyhow::anyhow!("Provider not found: {}", config.provider_id))?;

            let (response, tool_calls) = if let Some(ref mut on_event) = config.on_event {
                let mut content_buf = String::new();
                let mut tool_calls_buf: Vec<tundracode_models::ToolCall> = Vec::new();

                let result = provider
                    .stream(
                        config.model_config,
                        request,
                        tools_for_call,
                        &mut |event| match event {
                            StreamEvent::Token(t) => {
                                content_buf.push_str(&t);
                                on_event(StreamEvent::Token(t));
                            }
                            StreamEvent::ReasoningToken(t) => {
                                on_event(StreamEvent::ReasoningToken(t));
                            }
                            StreamEvent::ToolCallStart { name, call_id, file_path, arguments } => {
                                tool_calls_buf.push(tundracode_models::ToolCall {
                                    id: call_id.clone(),
                                    name: name.clone(),
                                    arguments: serde_json::Value::Object(
                                        serde_json::Map::new(),
                                    ),
                                });
                                on_event(StreamEvent::ToolCallStart { name, call_id, file_path, arguments });
                            }
                            StreamEvent::ToolCallDelta {
                                call_id,
                                arguments_delta,
                            } => {
                                if let Some(tc) =
                                    tool_calls_buf.iter_mut().find(|tc| tc.id == call_id)
                                {
                                    if let Some(obj) = tc.arguments.as_object_mut() {
                                        if let Some(raw) = obj.get_mut("_raw") {
                                            if let Some(s) = raw.as_str() {
                                                *raw = serde_json::Value::String(
                                                    s.to_string() + &arguments_delta,
                                                );
                                            }
                                        } else {
                                            obj.insert(
                                                "_raw".to_string(),
                                                serde_json::Value::String(arguments_delta.clone()),
                                            );
                                        }
                                    }
                                }
                                on_event(StreamEvent::ToolCallDelta {
                                    call_id,
                                    arguments_delta,
                                });
                            }
                            StreamEvent::ToolCallEnd { call_id, mut file_path } => {
                                if let Some(tc) =
                                    tool_calls_buf.iter_mut().find(|tc| tc.id == call_id)
                                {
                                    if let Some(obj) = tc.arguments.as_object_mut() {
                                        if let Some(raw) = obj.remove("_raw") {
                                            if let Some(s) = raw.as_str() {
                                                if let Ok(parsed) =
                                                    serde_json::from_str::<serde_json::Value>(s)
                                                {
                                                    if let Some(parsed_obj) = parsed.as_object() {
                                                        // Extract file_path from arguments
                                                        if let Some(path_val) = parsed_obj.get("path") {
                                                            if let Some(path_str) = path_val.as_str() {
                                                                file_path = Some(path_str.to_string());
                                                            }
                                                        }
                                                        tc.arguments = serde_json::Value::Object(
                                                            parsed_obj.clone(),
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                on_event(StreamEvent::ToolCallEnd { call_id, file_path });
                            }
                            StreamEvent::Done(resp) => {
                                total_tokens += resp.tokens_used;
                                on_event(StreamEvent::Done(resp.clone()));
                            }
                            StreamEvent::Error(e) => {
                                on_event(StreamEvent::Error(e.clone()));
                            }
                            StreamEvent::ContextCompacted { message } => {
                                on_event(StreamEvent::ContextCompacted { message });
                            }
                            StreamEvent::SubagentStart { agent_id, task } => {
                                on_event(StreamEvent::SubagentStart { agent_id, task });
                            }
                            StreamEvent::SubagentComplete { agent_id, duration_ms, success } => {
                                on_event(StreamEvent::SubagentComplete { agent_id, duration_ms, success });
                            }
                        },
                    )
                    .await?;

                let final_content = if content_buf.is_empty() {
                    result.0.content
                } else {
                    content_buf
                };

                (
                    tundracode_models::CompletionResponse {
                        content: final_content,
                        tokens_used: result.0.tokens_used,
                        finish_reason: result.0.finish_reason,
                    },
                    if tool_calls_buf.is_empty() {
                        result.1
                    } else {
                        Some(tool_calls_buf)
                    },
                )
            } else {
                provider
                    .complete(config.model_config, request, tools_for_call)
                    .await?
            };

            if config.on_event.is_none() {
                total_tokens += response.tokens_used;
            }

            if let Some(calls) = tool_calls {
                if !calls.is_empty() {
                    let payloads: Vec<ToolCallPayload> =
                        calls.iter().map(|c| c.clone().into()).collect();
                    conversation.add_assistant_with_tool_calls(response.content.clone(), payloads);

                    for call in &calls {
                        if let Some(ref eb) = self.event_bus {
                            eb.emit(SubagentEvent::tool_call_start(
                                &config.tool_context.agent_id,
                                &call.name,
                                &call.arguments,
                            ));
                        }

                        let exec_start = std::time::Instant::now();
                        let result = config
                            .tool_registry
                            .execute(config.tool_context, &call.name, call.arguments.clone())
                            .await;
                        let duration_ms = exec_start.elapsed().as_millis() as u64;

                        let (success, output, error, prior, resulting, file_path) = match result {
                            Ok(r) => (
                                r.success,
                                r.output.clone(),
                                r.error,
                                r.prior_content,
                                r.resulting_content,
                                r.file_path,
                            ),
                            Err(e) => (
                                false,
                                String::new(),
                                Some(e.to_string()),
                                None,
                                None,
                                None,
                            ),
                        };

                        if let Some(ref eb) = self.event_bus {
                            let tool_result = tundracode_tools::ToolResult {
                                success,
                                output: output.clone(),
                                error: error.clone(),
                                prior_content: prior.clone(),
                                resulting_content: resulting.clone(),
                                file_path: file_path.clone(),
                            };
                            eb.emit(SubagentEvent::tool_call_end(
                                &config.tool_context.agent_id,
                                &call.name,
                                &tool_result,
                                duration_ms,
                            ));
                        }

                        let after = if success {
                            if let Some(content) = resulting {
                                Some(content)
                            } else {
                                let path = file_path.clone().or_else(|| {
                                    call.arguments
                                        .get("p")
                                        .or_else(|| call.arguments.get("path"))
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string())
                                });
                                if let Some(p) = path.as_deref() {
                                    let full =
                                        std::path::Path::new(&config.tool_context.workspace_path)
                                            .join(p);
                                    std::fs::read_to_string(&full).ok()
                                } else {
                                    None
                                }
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
                            truncate_string(&output, TOOL_OUTPUT_MAX_CHARS)
                        } else {
                            truncate_string(
                                &error.unwrap_or_else(|| "Unknown error".to_string()),
                                TOOL_OUTPUT_MAX_CHARS,
                            )
                        };
                        conversation.add_tool_result(call.id.clone(), tool_output.clone());

                        // Track tool result in context manager
                        if let Some(ref mut cm) = self.context_manager {
                            let entry_content = if tool_output.is_empty() {
                                format!("{}: done", call.name)
                            } else if tool_output.len() > 400 {
                                format!("{}: {}...", call.name, &tool_output[..400])
                            } else {
                                format!("{}: {}", call.name, tool_output)
                            };
                            cm.add_entry(ContextEntry {
                                id: format!("tool_{}", call.id),
                                entry_type: ContextEntryType::ToolOutput,
                                content: entry_content,
                                token_estimate: (tool_output.len() / 4).max(1) as u32,
                                priority: 5,
                                created_at: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_millis() as u64,
                                last_accessed: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_millis() as u64,
                                compacted: false,
                                compaction_summary: None,
                            });
                        }
                    }
                    continue;
                }
            }

            return Ok(RunOutput {
                content: response.content,
                invocations,
                tokens_used: total_tokens,
                context_compacted,
            });
        }

        Ok(RunOutput {
            content: "Agent reached maximum iterations without a final answer.".to_string(),
            invocations,
            tokens_used: total_tokens,
            context_compacted,
        })
    }

    async fn deep_compact(
        &self,
        conversation: &mut Conversation,
        total_tokens: &mut u32,
        provider_registry: &ProviderRegistry,
        provider_id: &str,
        model_config: &ModelConfig,
        user_message: &str,
    ) -> Result<()> {
        let summary_prompt = format!(
            "Resume el progreso de esta sesion de trabajo. Incluye:\n\
             1. Que se ha hecho hasta ahora (acciones completadas)\n\
             2. Que queda por hacer (objetivos pendientes)\n\
             3. Archivos modificados o creados (rutas exactas)\n\
             4. Decisiones clave tomadas\n\
             5. Errores encontrados y como se resolvieron\n\
             Sé conciso pero incluye todos los detalles importantes. \
             Tu respuesta sera usada como contexto para continuar el trabajo."
        );

        let mut summary_conv = Conversation::new();
        for msg in &conversation.messages {
            summary_conv.add_message(msg.role.clone(), msg.content.clone());
        }
        summary_conv.add_message(MessageRole::User, summary_prompt);

        let summary_request = CompletionRequest {
            conversation: summary_conv,
            system_prompt: Some(
                "Eres un asistente que resume el progreso de trabajo. \
                 Produce un resumen conciso en texto plano."
                    .to_string(),
            ),
            reasoning_effort: None,
        };

        let provider = provider_registry
            .get(provider_id)
            .ok_or_else(|| anyhow::anyhow!("Provider not found: {}", provider_id))?;

        let (summary_response, _) = provider
            .complete(model_config, summary_request, None)
            .await?;

        let summary_tokens = summary_response.tokens_used;

        let mut new_conversation = Conversation::new();
        new_conversation.add_message(
            MessageRole::User,
            user_message.to_string(),
        );
        new_conversation.add_message(
            MessageRole::User,
            format!(
                "[Contexto compactado - resumen de la sesion anterior]\n\n{}\n\n\
                 Continua el trabajo desde aqui. Los archivos mencionados arriba \
                 ya han sido modificados. No repitas acciones ya completadas.",
                summary_response.content
            ),
        );

        *conversation = new_conversation;
        *total_tokens = summary_tokens + estimate_conversation_tokens(conversation);

        Ok(())
    }
}

fn truncate_string(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let header_lines: Vec<&str> = s.lines().take(3).collect();
    let header_len: usize = header_lines.iter().map(|l| l.len() + 1).sum();
    if header_len < max / 2 {
        let mut result = header_lines.join("\n");
        result.push('\n');
        result.push_str(OUTPUT_TRUNCATION_SUFFIX);
        result.push_str(&format!(" ({} total bytes)", s.len()));
        result
    } else {
        let mut truncated = s[..max].to_string();
        truncated.push_str(OUTPUT_TRUNCATION_SUFFIX);
        truncated
    }
}

fn estimate_conversation_tokens(conv: &Conversation) -> u32 {
    let total_chars: usize = conv
        .messages
        .iter()
        .map(|m| m.content.len())
        .sum();
    (total_chars / 4) as u32
}

fn maybe_truncate_tool_outputs(conv: &mut Conversation, max_chars: usize) {
    use tundracode_models::MessageRole;
    for msg in &mut conv.messages {
        if msg.role == MessageRole::Tool && msg.content.len() > max_chars {
            msg.content = truncate_string(&msg.content, max_chars);
        }
    }
}

impl Default for AgentLoop {
    fn default() -> Self {
        Self::new()
    }
}
