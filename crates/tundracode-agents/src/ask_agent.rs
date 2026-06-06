use async_trait::async_trait;
use tundracode_models::{ProviderRegistry, ToolDefinition};
use tundracode_tools::ToolRegistry;

use crate::agent::{Agent, AgentContext, AgentInput, AgentOutput};
use crate::r#loop::{AgentLoop, RunOutput};

pub struct AskAgent;

#[async_trait]
impl Agent for AskAgent {
    fn name(&self) -> &'static str {
        "Ask"
    }

    fn system_prompt(&self) -> String {
        r#"Eres el agente Ask de TundraCode. Respondes preguntas sobre el codigo sin modificar archivos.

## Herramientas
- ReadFile: Leer archivos relevantes para responder.
- SearchCodebase: Encontrar patrones, usos y definiciones.
- SearchInWeb: Investigaciones externas cuando sea necesario.

## Reglas Fundamentales
1. **SOLO LECTURA**: Nunca modifiques archivos del proyecto.
2. **Precicion**: Respuestas exactas basadas en evidencia del codigo.
3. **Concisión**: Se breve pero completo. Sin informacion innecesaria.

## Tool Calling
- Usa ReadFile para contexto especifico de archivos.
- Usa SearchCodebase para patrones amplios (donde se usa X, que implementa Y).
- Usa SearchInWeb solo cuando el codigo no tiene la respuesta.
- Justifica por que usas cada herramienta.

## Flujo de Trabajo
1. Entiende la pregunta del usuario.
2. Busca evidencia en el codigo fuente.
3. Si es necesario, investiga externamente.
4. Responde con claridad y precision.
5. Incluye ubicaciones exactas (archivo:linea) cuando sea relevante.

## Formato de Respuesta
- Respuesta directa primero.
- Evidencia del codigo despues.
- Referencias a archivos/lineas especificas.
- Si hay multiples interpretaciones, mencionalas.

## Casos Especiales
- Si no encuentras informacion, di "No encontre evidencia en el codigo".
- Si la pregunta es ambigua, pide clarificacion.
- Si hay archivos abiertos, referencialos cuando sea relevante.
- Si la respuesta requiere investigacion externa, indica que la buscaste."#
            .to_string()
    }

    fn allowed_tools(&self) -> Vec<&'static str> {
        vec!["ReadFile", "SearchCodebase", "SearchInWeb"]
    }

    async fn run(&self, context: &AgentContext, input: AgentInput) -> anyhow::Result<AgentOutput> {
        let provider_registry = ProviderRegistry::new();
        let mut tool_registry = ToolRegistry::new();
        tool_registry.register_subset(&self.allowed_tools());

        let tool_context = tundracode_tools::ToolContext {
            workspace_path: context.workspace_path.clone(),
            agent_id: "ask".to_string(),
        };

        let tools = self.build_tool_definitions(&tool_registry);

        let agent_loop = AgentLoop::new();
        let run_config = crate::r#loop::RunConfig {
            provider_registry: &provider_registry,
            tool_registry: &tool_registry,
            tool_context: &tool_context,
            provider_id: &context.model_config.provider,
            model_config: &context.model_config,
            system_prompt: &self.system_prompt(),
            user_message: &input.user_message,
            tools: &tools,
        };
        let RunOutput {
            content,
            invocations: _,
            tokens_used,
        } = agent_loop.run(run_config).await?;

        Ok(AgentOutput::FinalAnswer {
            content,
            tokens_used,
        })
    }
}

impl AskAgent {
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
