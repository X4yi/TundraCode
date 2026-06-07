use async_trait::async_trait;
use tundracode_models::{ProviderRegistry, StreamEvent, ToolDefinition};
use tundracode_tools::{generate_unified_diff, ToolRegistry};

use crate::agent::{Agent, AgentContext, AgentInput, AgentOutput, DiffKind, DiffProposal};
use crate::r#loop::{AgentLoop, RunOutput};

pub struct BuildAgent;

#[async_trait]
impl Agent for BuildAgent {
    fn name(&self) -> &'static str {
        "Build"
    }

    fn system_prompt(&self) -> String {
        r#"Eres el agente Build de TundraCode. Implementas un plan leyendo y modificando archivos.

## Herramientas Disponibles
- **ReadFile**: Lee archivos existentes para entender contexto.
- **ApplyPatch**: Aplica cambios quirurgicos en archivos existentes (unified diff).
- **CreateFile**: Crea archivos nuevos.
- **WriteFile**: Sobreescribe un archivo completo (usar solo como ultimo recurso).
- **ListDirectory**: Explora estructura de directorios.
- **RunCommand**: Compila, testea y verifica cambios.
- **GetDiagnostics**: Detecta errores de compilacion.

## Flujo Obligatorio por Task
1. **LEE** el goal del task actual y los archivos relevantes.
2. **PLANIFICA** los cambios exactos antes de ejecutar.
3. **EJECUTA** cambios con ApplyPatch (existentes) o CreateFile (nuevos).
4. **VERIFICA** con RunCommand (compile/test) y GetDiagnostics.
5. Si hay errores, **ANALIZA** y **REPARA** (max 3 intentos).
6. Avanza al siguiente task.

## Reglas Fundamentales
1. **UN TASK A LA VEZ**: No saltes a otro task hasta completar el actual.
2. **SIEMPRE LEE ANTES DE MODIFICAR**: Nunca modifiques un archivo sin leerlo primero.
3. **PROHIBICION ABSOLUTA**: Nunca modifiques archivos en .tundracode/.
4. **CAMBIOS ATOMICOS**: Un task = un cambio pequeno y enfocado.
5. **VERIFICACION**: Despues de cada cambio, compila y verifica.
6. **MAXIMO 3 INTENTOS**: Si un task falla 3 veces, detente y reporta el error.

## Manejo de Errores
- Si un comando falla, analiza el error antes de reintentar.
- Si un cambio rompe algo, revierte antes de continuar.
- Captura stack traces para debugging.

## Calidad del Codigo
- Respeta convenciones existentes del proyecto.
- No introduzcas dependencias nuevas sin justificacion.
- Usa nombres descriptivos, evita magic numbers.
- Funciones pequenas y enfocadas.

## Tool Calling
- Usa herramientas de forma secuencial y deliberada.
- Justifica cada uso de herramienta.
- Verifica resultados antes de continuar.
- Si una herramienta falla, busca alternativa o reporta."#
            .to_string()
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
        let provider_registry = ProviderRegistry::new();
        let mut tool_registry = ToolRegistry::new();
        tool_registry.register_subset(&self.allowed_tools());

        let tool_context = tundracode_tools::ToolContext {
            workspace_path: context.workspace_path.clone(),
            agent_id: "build".to_string(),
            dry_run: !context.autonomous_mode,
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

        let agent_loop = AgentLoop::new()
            .with_max_iterations(30)
            .with_budget_tokens(u32::MAX);
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
            on_event: None,
        };
        let RunOutput {
            content: _,
            invocations,
            tokens_used,
        } = agent_loop.run(run_config).await?;

        let (proposals, tool_log) = self.proposals_from_invocations(&invocations)?;

        Ok(AgentOutput::ProposedChanges {
            proposals,
            invocations,
            tool_log,
            tokens_used,
        })
    }
}

impl BuildAgent {
    pub async fn run_with_streaming(
        &self,
        context: &AgentContext,
        input: AgentInput,
        on_event: Option<Box<dyn FnMut(StreamEvent) + Send>>,
    ) -> anyhow::Result<AgentOutput> {
        let provider_registry = ProviderRegistry::new();
        let mut tool_registry = ToolRegistry::new();
        tool_registry.register_subset(&self.allowed_tools());

        let tool_context = tundracode_tools::ToolContext {
            workspace_path: context.workspace_path.clone(),
            agent_id: "build".to_string(),
            dry_run: !context.autonomous_mode,
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

        let agent_loop = AgentLoop::new()
            .with_max_iterations(30)
            .with_budget_tokens(u32::MAX);
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
        } = agent_loop.run(run_config).await?;

        let (proposals, tool_log) = self.proposals_from_invocations(&invocations)?;

        Ok(AgentOutput::ProposedChanges {
            proposals,
            invocations,
            tool_log,
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

    
    
    
    fn proposals_from_invocations(
        &self,
        invocations: &[crate::agent::ToolInvocation],
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
