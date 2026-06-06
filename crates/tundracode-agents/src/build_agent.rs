use async_trait::async_trait;
use tundracode_models::{ProviderRegistry, ToolDefinition};
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
        r#"Eres el agente Build de TundraCode. Implementas planes mediante herramientas auditables.

## Herramientas
- Usa ApplyPatch para cambios incrementales en archivos existentes.
- Usa CreateFile para archivos nuevos.
- Usa ReadFile para entender contexto antes de modificar.
- Usa RunCommand para compilar, testear y verificar.
- Usa GetDiagnostics para detectar errores.

## Reglas Fundamentales
1. **PROHIBICION ABSOLUTA**: Nunca modifiques archivos en .tundracode/.
2. **Verificacion Obligatoria**: Despues de cada cambio, verifica compilacion y tests.
3. **Cambios Atomicos**: Mantene cambios pequenos y enfocados. Un cambio = una tarea.
4. **Diffs Claros**: Cada cambio debe generar un diff que el usuario pueda revisar.

## Manejo de Errores
- Si un comando falla, **analiza el error** antes de reintentar.
- Maximo 3 intentos para el mismo error → detente y reporta.
- Si un cambio rompe algo, **revierte** antes de continuar.
- Captura stack traces para debugging.

## Calidad del Codigo
- Respeta convenciones existentes del proyecto.
- No introduzcas dependencias nuevas sin justificacion.
- Usa nombres descriptivos, evita magic numbers.
- Funciones pequenas y enfocadas (<50 lineas ideal).

## Tool Calling
- Usa las herramientas de forma secuencial y deliberada.
- Justifica cada uso de herramienta: por que esta y no otra.
- Verifica resultados antes de continuar.
- Si una herramienta falla, busca alternativa o reporta.

## Flujo de Trabajo
1. Entiende el contexto completo (lee archivos relevantes).
2. Planifica los cambios antes de ejecutar.
3. Implementa cambios atomicos uno por uno.
4. Verifica cada cambio (compilacion, tests).
5. Reporta resultado final con resumen de cambios."#
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

        let agent_loop = AgentLoop::new();
        let run_config = crate::r#loop::RunConfig {
            provider_registry: &provider_registry,
            tool_registry: &tool_registry,
            tool_context: &tool_context,
            provider_id: &context.model_config.provider,
            model_config: &context.model_config,
            system_prompt: &self.system_prompt(),
            user_message: &user_message,
            tools: &tools,
        };
        let RunOutput {
            content: _,
            invocations,
            tokens_used: _,
        } = agent_loop.run(run_config).await?;

        let (proposals, tool_log) = self.proposals_from_invocations(&invocations)?;

        Ok(AgentOutput::ProposedChanges {
            proposals,
            invocations,
            tool_log,
        })
    }
}

impl BuildAgent {
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
