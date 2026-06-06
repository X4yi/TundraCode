use async_trait::async_trait;
use std::path::Path;
use tundracode_models::{ProviderRegistry, ToolDefinition};
use tundracode_tools::ToolRegistry;

use crate::agent::{Agent, AgentContext, AgentInput, AgentOutput};
use crate::r#loop::{AgentLoop, RunOutput};

pub struct PlanAgent;

#[async_trait]
impl Agent for PlanAgent {
    fn name(&self) -> &'static str {
        "Plan"
    }

    fn system_prompt(&self) -> String {
        r#"Eres el agente Plan de TundraCode. Investigas, analizas y generas planes de implementacion tecnicos fundamentados.

## Herramientas
- ReadFile / ListDirectory: Para entender la estructura del proyecto.
- SearchCodebase: Para encontrar patrones y codigo existente.
- SearchInWeb: Para investigar APIs, frameworks y mejores practicas.

## Workflow Obligatorio

### 1. Auditoria del Proyecto
- Lee estructura del workspace y archivos clave.
- Identifica stack tecnico, dependencias y convenciones.
- Revisa .tundracode/memory.md si existe.
- **NO estimimes tokens** - investiga el codigo real.

### 2. Investigacion Externa
- Usa SearchInWeb para documentacion oficial de APIs/frameworks.
- Busca mejores practicas para la tarea especifica.
- Verifica compatibilidad de versiones.
- Incluye SIEMPRE al menos 1 busqueda web.

### 3. Analisis Comparativo
- Enumera 2-3 alternativas para cada decision tecnica.
- Analiza pros/contras de cada una.
- Recomienda basandote en evidencia, no suposiciones.

### 4. Generacion del Plan
Estructura obligatoria:

## Stack
Justificacion de elecciones tecnicas. Lenguaje, frameworks, librerias, y por que.

## Alternativas
| Opcion | Pros | Contras | Veredicto |

## Pasos
Lista numerada. Cada paso incluye:
- Que se hace (especifico)
- Archivos a crear/modificar (rutas exactas)
- Herramientas a usar
- Criterio de aceptacion

## Riesgos
Riesgos identificados y mitigaciones.

## Complejidad
Nivel: baja/media/alta. Archivos a modificar: N.

## Reglas
- **NUNCA** toques archivos del proyecto.
- **NO estimes tokens** - investiga codigo real.
- Usa evidencia del codigo existente para cada decision.
- Si el workspace tiene stack definido, respetalo.
- Plan conciso pero completo.
- Incluye verificacion al final.

## Tool Calling
- Usa herramientas de forma secuencial para investigar.
- Primero entiende el codigo, luego busca documentacion externa.
- Verifica que la informacion este actualizada.
- Cita fuentes cuando sea posible."#
            .to_string()
    }

    fn allowed_tools(&self) -> Vec<&'static str> {
        vec![
            "ReadFile",
            "ListDirectory",
            "GetWorkspace",
            "SearchCodebase",
            "SearchInWeb",
        ]
    }

    async fn run(&self, context: &AgentContext, input: AgentInput) -> anyhow::Result<AgentOutput> {
        let provider_registry = ProviderRegistry::new();
        let mut tool_registry = ToolRegistry::new();
        tool_registry.register_subset(&self.allowed_tools());

        let tool_context = tundracode_tools::ToolContext {
            workspace_path: context.workspace_path.clone(),
            agent_id: "plan".to_string(),
        };

        let tools = self.build_tool_definitions(&tool_registry);

        let memory_excerpt = input
            .memory_excerpt
            .or_else(|| read_memory_md(&context.workspace_path));

        let user_message = if let Some(memory) = &memory_excerpt {
            format!(
                "Contexto del proyecto (memory.md):\n{}\n\nTarea del usuario:\n{}",
                memory, input.user_message
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
            content,
            invocations: _,
            tokens_used,
        } = agent_loop.run(run_config).await?;

        let plan_slug = slugify(&input.user_message);
        let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        let plan_path = format!(
            "{}/.tundracode/plans/{}_{}.md",
            context.workspace_path, plan_slug, timestamp
        );

        if let Some(parent) = Path::new(&plan_path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let provider_name = &context.model_config.provider;
        let model_name = &context.model_config.model;

        let frontmatter = format!(
            "---\ngenerated_at: {}\nprovider: {}/{}\nestimated_build_tokens: {}\n---\n\n",
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
            provider_name,
            model_name,
            tokens_used * 3,
        );

        let plan_content = format!("{}{}", frontmatter, content);
        let _ = std::fs::write(&plan_path, &plan_content);

        Ok(AgentOutput::FinalAnswer {
            content,
            tokens_used,
        })
    }
}

impl PlanAgent {
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

fn read_memory_md(workspace: &str) -> Option<String> {
    let path = Path::new(workspace).join(".tundracode/memory.md");
    std::fs::read_to_string(&path).ok()
}

fn slugify(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ')
        .collect::<String>()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_")
        .chars()
        .take(50)
        .collect()
}
