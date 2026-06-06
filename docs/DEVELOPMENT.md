# Guía de desarrollo de TundraCode

## Arquitectura

TundraCode sigue una arquitectura modular con un workspace Rust que contiene:

1. **src-tauri**: Aplicación principal que orquesta todo via Tauri
2. **Crates internos**: Lógica de negocio organizada por dominio
3. **Frontend**: UI vanilla (HTML/CSS/JS) que se comunica con Rust via comandos Tauri

## Flujo de comunicación

```
Frontend (JS)  <--invoke-->  Tauri (Rust)  <--llamadas-->  Crates internos
    |                            |                              |
    v                            v                              v
  Eventos UI              Comandos Tauri              Lógica de negocio
```

## Convenciones de código

### Rust

- `cargo fmt` y `cargo clippy` son obligatorios antes de cada commit
- Cada crate debe tener tests unitarios para funcionalidades críticas
- Documentación pública con `///` para APIs expuestas
- Error handling con `anyhow` en capas superiores, `thiserror` en crates internos
- Async con `tokio` y `async-trait`

### Frontend

- Sin emojis en la UI (especificación)
- Dark mode único, sin toggle
- Iconografía SVG inline (estilo Lucide)
- Sin frameworks pesados: vanilla JS + CSS
- Nombres de funciones y variables en español donde sea apropiado para el dominio

## Sistema de herramientas (Tool Calling)

Las herramientas son la única forma en que los agentes interactúan con el sistema. Cada herramienta implementa el trait `Tool`:

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn parameters_schema(&self) -> Value;
    async fn execute(&self, context: &ToolContext, params: Value) -> Result<ToolResult, ToolError>;
}
```

### Herramientas implementadas

| Herramienta | Descripción |
|-------------|-------------|
| `ReadFile` | Lee contenido de archivo |
| `WriteFile` | Escribe archivo completo |
| `ApplyPatch` | Aplica unified diff |
| `CreateFile` | Crea archivo nuevo |
| `DeleteFile` | Elimina archivo (con confirmación) |
| `ListDirectory` | Lista directorio |
| `GetWorkspace` | Estructura del proyecto |
| `RunCommand` | Ejecuta comando sandboxed |
| `SearchCodebase` | Búsqueda semántica |
| `SearchInWeb` | Búsqueda web |
| `GetDiagnostics` | Diagnósticos LSP |

## Agentes

### Plan
- Lee workspace y `.tundracode/memory.md`
- Genera plan Markdown estructurado
- Guarda en `.tundracode/plans/`
- No modifica archivos del proyecto

### Build
- Recibe plan aprobado + anotaciones
- Implementa mediante herramientas
- Modo propuesta: diffs antes de aplicar
- Interrumpible manualmente

### Ask
- Chat con contexto del proyecto
- Acceso a `SearchCodebase` y `SearchInWeb`
- No modifica archivos

## Comandos Tauri

Los comandos Tauri conectan el frontend con el backend:

| Comando | Descripción |
|---------|-------------|
| `open_workspace` | Abre un directorio como workspace |
| `get_workspace` | Obtiene ruta del workspace actual |
| `list_directory` | Lista archivos de un directorio |
| `read_file` | Lee contenido de un archivo |
| `write_file` | Escribe contenido en archivo |
| `get_git_status` | Estado de Git del workspace |
| `git_stage` | Stage de archivo |
| `git_commit` | Commit con mensaje |
| `get_lsp_status` | Estado del LSP |
| `run_agent_ask` | Ejecuta agente Ask |
| `generate_plan` | Genera plan con agente Plan |

## Testing

### Unitarios

```bash
# Todos los crates
cargo test --all

# Crate específico
cargo test -p tundracode-core
```

### Integración

Los tests de integración verifican:
- Flujo completo Plan → Build
- Operaciones de herramientas con filesystem real (en tmpdir)
- Cliente LSP con language server de prueba

### UI

El frontend se prueba manualmente ejecutando la app:

```bash
cargo tauri dev
```

## Debugging

### Backend (Rust)

```bash
# Con logs de tracing
RUST_LOG=debug cargo tauri dev
```

### Frontend

Abrir DevTools con F12 (en modo desarrollo).

## Roadmap inmediato

1. Integración tree-sitter para syntax highlighting
2. Implementación real del cliente LSP
3. Conexión con providers de IA (OpenAI, Anthropic)
4. Sistema de diffs inline
5. Búsqueda semántica local con embeddings
6. UI para gestión de modelos locales (Ollama)

## Notas de seguridad

- Las API keys nunca se almacenan en archivos planos (keyring del SO)
- `RunCommand` ejecuta en sandbox limitado al workspace
- `DeleteFile` siempre requiere confirmación del usuario
- Los agentes no pueden acceder fuera del workspace abierto
- `.tundracode/` está excluido de operaciones de Build
