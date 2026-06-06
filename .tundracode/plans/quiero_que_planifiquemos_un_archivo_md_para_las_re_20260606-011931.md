---
generated_at: 2026-06-06T01:19:31Z
provider: opencode-free/mimo-v2.5-free
estimated_build_tokens: 269058
---

---

## Plan de implementación: Archivo de reglas y contexto para IA agéntica

### 📋 Contexto

El usuario quiere crear un archivo `.md` con reglas y contexto del proyecto destinado a agentes de IA como TundraCode Plan/Build/Ask, y otros agentes agénticos (Cursor, Copilot, Claude, etc). El archivo debe contener **reglas adaptadas al proyecto** (no al código), incluyendo principios, convenciones, stack, arquitectura, y directrices generales.

---

## 🏗️ Stack

El archivo será **Markdown puro** (`.md`), compatible con los estándares actuales de archivos de contexto para IA:
- **Formato**: `.github/copilot-instructions.md` (estándar Copilot + universal)
- **Estructura**: Secciones jerárquicas con headers claros
- **Extensión adicional**: Se puede symlinkear o copiar a `.cursorrules`, `AGENTS.md`, `CLAUDE.md` si se necesita compatibilidad multi-herramienta

---

## 📊 Alternativas

| Opción | Pros | Contras | Veredicto |
|--------|------|---------|-----------|
| **`.github/copilot-instructions.md`** | Estándar de GitHub Copilot, soportado por Cursor y otras herramientas, ubicación canónica en repos GitHub | Solo una herramienta lo lee nativamente | ✅ Recomendado (estándar de facto) |
| **`AGENTS.md` en raíz** | Formato agnóstico, legible por múltiples agentes, claro en propósito | Menos conocido que el estándar de GitHub | ✅ Complementario |
| **`.cursorrules`** | Popular, buen ecosistema | Específico de Cursor, formato propio | ❌ Demasiado limitado |
| **`CLAUDE.md`** | Nativo de Claude Code | Solo Claude, formato nuevo | ❌ No universal |

**Recomendación**: Crear **un archivo principal** (ej: `CONTRIBUTING.md` o un archivo dedicado) que sea legible por múltiples agentes. El contenido es lo importante, no el nombre exacto.

---

## 🔧 Pasos de implementación

### Paso 1: Definir la estructura del archivo

Crear el archivo con estas secciones principales:

```markdown
# TundraCode — Reglas y Contexto para IA Agéntica

## 1. Qué es este proyecto
- Descripción en 2-3 líneas
- Audiencia objetivo
- Estado actual

## 2. Stack técnico
- Lenguaje principal: Rust
- Framework: Tauri v2
- Frontend: HTML/CSS/JS vanilla
- Lenguajes de parsing: tree-sitter

## 3. Principios fundamentales (inviolables)
- Seguridad primero: agentes NUNCA acceden directamente al filesystem
- Todo pasa por herramientas auditables (Tool Calling)
- Sin sorpresas: el usuario siempre ve qué hace el agente
- Transparencia total sobre costos y archivos tocados
- Linux únicamente en v1

## 4. Convenciones de código
### Rust
- `cargo fmt` y `cargo clippy` obligatorios
- Error handling: `anyhow` en capas superiores, `thiserror` en crates internos
- Async con `tokio` y `async-trait`
- Tests unitarios para funcionalidades críticas

### Frontend
- Sin emojis en la UI (nunca)
- Dark mode único por defecto
- Iconografía: solo Lucide (outline, minimalista)
- Vanilla JS + CSS (sin frameworks pesados)
- Nombres en español donde sea apropiado para el dominio

## 5. Estructura del proyecto
- Workspace Rust con múltiples crates
- Organización por dominio (core, tools, agents, models, lsp, git, config, security)
- Frontend en `/src`, backend en `/src-tauri`

## 6. Agentes de IA — Cómo interactúan
- **Plan**: Lee workspace + memory.md, genera plan, NUNCA toca archivos
- **Build**: Implementa plan mediante herramientas, con diff previo
- **Ask**: Chat read-only con contexto del proyecto
- Workflow: Plan → Anotaciones → Build

## 7. Herramientas (Tool Calling)
- Solo usar las herramientas definidas (ReadFile, Edit, Create, etc.)
- Nunca usar herramientas no definidas
- Respetar atomicidad de escrituras
- `Delete` siempre requiere confirmación

## 8. Concurrencia
- Un solo Build activo por workspace
- Ask puede correr en paralelo (solo lectura)
- No interrumpir un Build activo

## 9. Seguridad
- API keys en keyring del SO, nunca en archivos planos
- RunCommand en sandbox (bwrap + seccomp + Landlock)
- Agentes no acceden fuera del workspace
- `.tundracode/` es intocable por agentes

## 10. Qué NO hacer
- No modificar `.tundracode/` directamente
- No usar `WriteFile` (usar `Edit` o `Create`)
- No saltar el paso de aprobación en modo asistido
- No ignorar los diffs propuestos
- No acceder a rutas fuera del workspace
```

### Paso 2: Contenido específico del proyecto

Incluir:
- **Decisiones arquitectónicas clave**: por qué Tauri, por qué tree-sitter, por qué LSP nativo
- **Dependencias críticas**: lista de crates internos y su propósito
- **Patrones de error handling**: `anyhow` arriba, `thiserror` abajo
- **Testing**: cobertura objetivo >80% en core

### Paso 3: Reglas para agentes de IA

Incluir sección explícita de "reglas para agentes":
- Leer `.tundracode/memory.md` antes de actuar
- Nunca modificar archivos del proyecto sin aprobación
- Respetar el workflow Plan → Build
- Usar solo las herramientas definidas
- Reportar cada acción en el log

### Paso 4: Ejemplos concretos

Incluir 2-3 ejemplos de "buen comportamiento" vs "mal comportamiento" para que el agente entienda los límites.

---

## ⚠️ Riesgos

| Riesgo | Mitigación |
|--------|------------|
| Archivo demasiado largo (>5KB) | Mantener <3KB, usar bullets concisos, evitar código |
| Reglas contradictorias con el spec | Revisar contra `tundracode_spec_v2.md` antes de publicar |
| Incompatibilidad multi-herramienta | Usar formato estándar Markdown, sin sintaxis proprietary |
| Reglas obvias que el modelo ya sabe | Incluir solo lo específico del proyecto, no reglas genéricas de programación |

---

## 💰 Estimación

- **Tokens necesarios**: ~800-1200 tokens para el archivo completo
- **Costo relativo**: **Bajo** (es un solo archivo estático, sin dependencias)
- **Tiempo de implementación**: ~15 minutos

---

## 📁 Archivos a crear/modificar

1. **Crear**: `AGENTS.md` (o `.github/copilot-instructions.md`) en la raíz del workspace
2. **Opcional**: Symlink desde `.cursorrules`, `CLAUDE.md` si se necesita compatibilidad

---

## 🎯 Criterio de aceptación

El archivo debe:
1. ✅ Ser legible por humanos Y por IA
2. ✅ Contener solo reglas específicas de TundraCode (no genéricas)
3. ✅ Ser conciso (<3KB)
4. ✅ Incluir ejemplos concretos de buen/mal comportamiento
5. ✅ Ser compatible con los principales agentes de IA (Copilot, Cursor, Claude Code)

---

**¿Quieres que proceda a generar el contenido completo del archivo basado en este plan?**