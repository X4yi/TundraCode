# TundraCode — Especificación del Proyecto v0.3.1

> v0.3.1 añade el flujo completo de gestión de providers sobre v0.3: nuevo
> modal "Connect Provider" con test de conexión, lista de providers
> conectados con disconnect flow, y filtrado del selector de modelo por
> providers activos. Ver "Changelog v0.3 → v0.3.1" al inicio del
> documento para el detalle de cambios.

> v0.3 es el resultado de una auditoría completa del spec v0.2. 26 patches
> incrementales cierran contradicciones internas, añaden contratos al tool
> layer, definen el sandbox de `RunCommand` con tecnología concreta, y
> reorganizan scope/audiencia. Ver "Changelog v0.2 → v0.3" al final del
> documento para el detalle de cambios.

## Changelog v0.3 → v0.3.1

**Gestión de providers (nueva §"Configuración → Gestión de providers")**
- Modelo de datos: `ProviderConnection { id, provider_type, base_url, keyring_account, connected_at, last_validated_at, last_validation_status, model_cache }`. Persistencia: metadata en `providers.toml`, key en libsecret. Una cuenta por provider.
- Botón **"+ Connect Provider"** en §Configuración, subsección "Providers".
- **Modal "Connect Provider"** flotante: selector de provider (agrupado en Cloud APIs / OpenAI-compatible / Local runtime), campo API key con auto-mask a 5s, campo base URL opcional, help text con link a docs, botón "Test connection" obligatorio antes de "Save".
- **Endpoints de test por provider** documentados: Anthropic (POST messages con max_tokens=1), OpenAI-compat (GET /v1/models), Ollama (GET /api/tags), etc.
- **Manejo de errores de test**: 401/403/404/429/network/HTTP-no-HTTPS, cada uno con mensaje accionable.
- **Fallback a `secrets.age` cifrado** si el keyring no está disponible (Flatpak sin Secret portal, CI/headless, sesión SSH).
- **Listado de providers conectados** con icono de estado (verde/amarillo/rojo), timestamp de última validación, menú `⋯` (Test / Reemplazar API key / Disconnect), y preview de los primeros 5 modelos.
- **Disconnect flow** con modal de confirmación: borra key de keyring, invalida model cache, reasigna agentes activos al siguiente provider disponible.
- **Model selector del Agents panel** filtrado estrictamente por providers conectados. Modelos de providers no conectados no aparecen en ningún estado. Empty state con atajo al modal de Connect.
- **Fallback entre providers** configurable (default enabled): si el provider activo falla con 429/5xx, intenta el siguiente antes de rendirse.
- Edge cases documentados: disconnect durante Build activo, keyring no disponible, cambio de API key, Ollama no corriendo, rate limits, modelos deprecated.

**Cambios secundarios**
- §"Modelos de IA → Providers remotos (API)" reescrita para apuntar a la nueva subsección.
- §"Configuración de modelos" ampliada con el filtrado por providers conectados y el cache de modelos (TTL 24h).
- §"Modelos locales" clarifica que Ollama aparece como provider (caso especial sin key) pero el flujo de descarga sigue en subsección separada.
- §"Privacidad y telemetría" añade cross-reference al flujo de Connect/Disconnect (HTTPS obligatorio, auto-mask de la key, key nunca en logs).
- §"Configuración" reorganizada: bullet de "API keys por provider" reemplazado por referencia a la nueva subsección; añadido bullet de "Fallback entre providers".

---

## Changelog v0.2 → v0.3

**Fase 0 — Eliminación del agente Review**
- Eliminado el agente Review en su totalidad (estaba referenciado pero nunca definido).
- Modo autónomo ahora se activa con `autonomous_mode = true` por proyecto (interruptor binario), sin gate automático.

**Fase 1 — Cerrar contradicciones bloqueantes**
- Definido el stack de sandbox para `RunCommand`: bubblewrap + seccomp + Landlock + egress proxy.
- Política de red para `RunCommand`: tres modos (`off`, `egress-allowlist`, `on`) con default seguro.
- Atomicidad de escritura: tmpfile + rename + fsync + preservación de modo/ownership.
- Concurrencia: Build exclusivo por workspace, Ask read-only, lock-based.

**Fase 2 — Contratos del tool layer**
- `WriteFile`/`CreateFile`/`ApplyPatch` colapsados en `Edit` + `Create` + `Delete` + `ApplyDiff` con semánticas claras.
- `ApplyDiff` robusto: applier context-aware, fuzzy match con threshold 0.95.
- `SearchCodebase` con stack concreto: BGE-small + FastEmbed + usearch HNSW + SQLite.
- `SearchInWeb` con provider configurable, rate-limit, cache, redacción de paths.
- `RunCommand` con contrato de output: UTF-8, buffer 10 MB, timeout, cancelable.
- `GetDiagnostics` con política de espera (timeout 30s, cache LRU).
- Nombres de archivo de plan normalizados; anotaciones en formato `> [!NOTE]`.

**Fase 3 — Ajustes de scope y audiencia**
- Java/JDTLS fuera de v1; Python (pyright) entra a v1.
- Snapshots de undo: `git stash` por operación; sin proyecto git → undo solo en RAM.
- Lista explícita de comandos peligrosos que requieren confirmación siempre.
- Modo oscuro respeta el tema del sistema (no hard-locked a dark).
- Providers ampliados (DeepSeek, Gemini, Mistral, Groq vía OpenAI-compatible); local models con adapter (Ollama primero, LM Studio y vLLM después).

**Fase 4 — Calidad, privacidad, completitud**
- Nueva sección "Calidad y testing" con tests de sandbox, atomicidad, `ApplyDiff`, concurrencia.
- Nueva sección "Privacidad y telemetría" con redacción de secretos y bundle de diagnóstico opt-in.
- Nueva sección "Lifecycle de procesos" (LSP servers, runtime local, index daemon).
- Nueva sección "Actualizaciones" (Flatpak vía Flathub, `.tar.gz` self-update).
- Nueva sección "LSP server discovery" con detección multi-estrategia.
- Manejo explícito de encoding, line endings, binarios y archivos grandes.
- Reglas de agentes reescritas con referencia explícita a §Concurrencia.
- Explorador de archivos movido a panel lateral fijo colapsable.

## Visión

IDE agéntico que combina un editor de código tradicional parcial con agentes de IA profundamente integrados al proyecto. No es un IDE completo, no es un chat con IA — es una categoría propia. Funciona sin IA (modo editor clásico), pero alcanza su potencial máximo con los agentes activos.

**Público objetivo:** estudiantes, hobby, trabajo individual, empresas pequeñas/medianas.

---

## Principios de diseño

- **Seguridad primero:** los agentes nunca acceden al filesystem/OS directamente; todo pasa por herramientas auditables.
- **Eficiencia de recursos:** sin memory leaks, sin procesos innecesarios en segundo plano, bajo consumo de RAM/CPU en idle.
- **Consistencia:** comportamiento predecible entre sesiones, modelos y proyectos.
- **Sin sorpresas:** el usuario siempre sabe qué está haciendo el agente, qué archivos tocó, qué cuesta.

---

## Stack técnico

| Capa | Tecnología |
|------|-----------|
| Backend / core | Rust |
| Frontend / shell | Tauri |
| Parsing / análisis | tree-sitter |
| LSP client | Rust nativo (integrado) |
| Auxiliares | Lenguajes/frameworks puntuales según feature |

---

## Distribución

- **Linux únicamente en v1.**
- Formatos: **Flatpak** (primary, para distribución en Flathub) + **`.tar.gz`** (binario autónomo, extraer y ejecutar sin instaladores).
- El binario `.tar.gz` no debe tener dependencias de sistema no estándar: todo lo necesario va bundleado.
- Windows / macOS son roadmap futuro (Tauri lo permite), no objetivo de v1.

---

## UI / UX

### Estilo visual

- **Modo oscuro por defecto, respeta el tema del sistema.** El usuario puede forzar dark/light/auto en Settings. El spec asume dark para los assets visuales bundled (iconos, paleta de sintaxis por defecto), pero la UI adapta fondo, textos y contraste al modo activo. Los assets bundled tienen variante light y dark cuando aplica.
- Sin emojis en ninguna parte de la UI.
- Iconografía exclusivamente **Lucide** (outline, minimalista). Un único theme en toda la aplicación, sin mezclar con otros sets.
- Tipografía monoespaciada para código, sans-serif para UI (sistema o bundleada).
- Moderna, limpia, sin decoración innecesaria.

### Layout principal

```
┌─────────────────────────────────────────────────────────────┐
│  [Explore ▾]  [tabs de archivos abiertos ...]   [acciones]  │  ← Barra superior
├─────────────────────────────────┬───────────────────────────┤
│                                 │                           │
│   Área de edición               │   Panel "Agents"          │
│                                 │                           │
│   · Editor de código (tabs)     │   · Plan + anotaciones    │
│   · Diff inline al recibir      │   · Build log             │
│     cambios del agente          │   · Timeline de cambios   │
│   · Mini-mapa de contexto       │   · Ask / Chat            │
│                                 │   · Settings rápidos      │
│                                 │                           │
├─────────────────────────────────┴───────────────────────────┤
│  LSP: rust-analyzer ●  |  main  |  Tokens: 4.2k  |  Usage: 10% …  │  ← Barra inferior
└─────────────────────────────────────────────────────────────┘
```

#### Explorador de archivos (panel lateral)

Panel lateral izquierdo, colapsable. Botón toggle en la barra superior (atajo: `Ctrl+B` por convención).

- El árbol respeta el estado de expansión/scroll entre sesiones por workspace.
- Carpeta raíz es el workspace abierto. `.tundracode/` se muestra con icono distinto y los planes son expandibles inline (preview de los primeros 200 chars del cuerpo).
- Botón "abrir en nueva tab" con clic central o menú contextual.
- Indicador de archivos modificados/staged (ver §"Integración Git").
- Búsqueda fuzzy en el árbol (`Ctrl+P` extendido o `Ctrl+Shift+E` para foco en el árbol).
- Click en archivo: abre tab (o la activa si ya existe). Click en carpeta: expande/colapsa. Doble click en archivo: lo abre en split view (si está disponible — ver v1.1).
- El panel acepta drag-and-drop de archivos desde fuera del IDE (cuando se permiten — sujeto a permisos de Flatpak portal).

**Por qué panel lateral y no dropdown:** el árbol persistente reduce el coste cognitivo de navegar código no familiar. El argumento "foco en código" se preserva con el toggle `Ctrl+B`; perder el árbol permanente cuesta más de lo que ahorra.

#### Tabs de archivos

- Múltiples tabs abiertas simultáneamente (estilo VSCode).
- Tabs en la barra superior, junto al botón Explore.
- Indicador visual en la tab si el archivo tiene cambios sin guardar.
- Indicador visual si el archivo está siendo modificado por un agente en ese momento.
- Cierre individual con `×`, cierre de todas salvo la activa disponible, "reopen closed tab" con `Ctrl+Shift+T`.
- Sin split view en v1 (roadmap v1.1).

#### Panel Agents

- Colapsable completamente → modo editor clásico sin ninguna fricción de IA.
- Atajo de teclado para abrir/cerrar.
- Proporciones por defecto: 62% editor / 38% agents.
- El usuario puede redimensionar el split arrastrando.

### Área de edición

- Editor de código con syntax highlighting vía tree-sitter.
- Números de línea, indentación guiada, scroll suave.
- **Diff inline antes de aplicar:** cuando Build propone cambios, el editor muestra el unified diff (líneas verdes/rojas) en la tab afectada. El usuario acepta o rechaza por archivo o por hunk antes de que se escriba a disco.
- **Mini-mapa de contexto:** badge en la barra inferior que muestra qué archivos están en el contexto del agente activo, cuántos tokens consumen, y si alguno fue excluido por límite.

### Integración Git

- Stage de archivos individuales o en bloque desde el IDE.
- Commit con mensaje desde el IDE.
- Indicador de rama activa en la barra inferior.
- Indicador de archivos modificados/staged en las tabs y en el explorador dropdown.
- Sin historial de commits, merge ni rebase en v1 — esas operaciones se delegan a terminal.

### Panel derecho — Agents

Subpaneles o tabs dentro del panel derecho:

1. **Plan** — renderizador Markdown del plan generado. Permite añadir anotaciones del usuario en formato `> [!NOTE] ...` (bloques admonition de GitHub-flavored Markdown) sobre cada ítem del plan, como un PR review. El parser del Build extrae todas las anotaciones y las entrega al LLM como restricciones hard por sección (proximidad de heading).
2. **Build log** — output en tiempo real de las acciones del agente (herramientas llamadas, archivos tocados, errores).
3. **Output** — output de `RunCommand` en tiempo real (stdout + stderr). Separado del Build log para no mezclar acciones del agente con output de comandos. Conserva el historial de la sesión con timestamps.
4. **Timeline** — historial cronológico de todas las operaciones de agentes: qué archivos tocaron, cuándo, con qué plan asociado. Cada entrada es clickeable (muestra el diff). Permite undo granular por hunk y por archivo. El mecanismo de undo se detalla en §"Snapshots y undo".
5. **Ask / Chat** — interfaz de chat con contexto del proyecto (agente Ask).
6. **Settings rápidos** — dropdown de modelo activo, presupuesto por tarea, modo autónomo/asistido.

---

## Agentes

### 1. Plan
- Lee el workspace (estructura de archivos, archivos relevantes).
- Lee `.tundracode/memory.md` (contexto persistente del proyecto) si existe.
- Recibe el input del usuario.
- Genera un plan de implementación estructurado en Markdown.
- El plan se guarda automáticamente en `.tundracode/plans/{slug}_{ISO8601-UTC-sin-colons}.md` (formato detallado en §"Planes versionados").
- Nunca toca archivos del proyecto directamente.

### 2. Build
- Recibe el plan aprobado, incluyendo las anotaciones del usuario en bloques `> [!NOTE]`.
- Lee las anotaciones como restricciones hard; antes de ejecutar cada paso, el system prompt incluye las anotaciones que aplican a esa sección por proximidad de heading.
- Implementa el plan mediante herramientas (ver §"Herramientas (Tool Calling)").
- Opera en modo propuesta: los cambios a archivos se presentan como diff antes de aplicarse (skipeable en modo autónomo — ver §"Reglas de agentes").
- Puede ser interrumpido manualmente en cualquier punto desde la UI.

### 3. Ask
- Lee el workspace con contexto relevante.
- Puede buscar en la web si es necesario (`SearchInWeb`).
- Puede hacer búsqueda semántica en el codebase (`SearchCodebase`).
- Responde preguntas del usuario en el chat.
- No modifica archivos.

### Reglas de agentes

- Ningún agente interactúa directamente con el filesystem/OS. Todo vía herramientas (ver §"Herramientas (Tool Calling)").
- **Workflow:** `Plan → (anotaciones del usuario) → Build`. El paso de anotaciones es opcional pero recomendado. El workflow se serializa por construcción (ver §"Concurrencia").
- **Modo asistido:** el usuario aprueba cada hunk o archivo de la propuesta del Build antes de aplicar.
- **Modo autónomo:** el usuario activa `autonomous_mode = true` por proyecto (interruptor binario). Con esta flag activa, el Build se auto-aprueba y se aplica sin diff inline previo. El Timeline registra toda la operación; el usuario puede revisar y revertir hunk por hunk después.
- En modo autónomo, las tools destructivas siguen requiriendo confirmación explícita: `Delete` siempre; `RunCommand` con patrones peligrosos (ver §"Comandos que siempre requieren confirmación") siempre; cualquier comando con modo de red `on` siempre.
- **Concurrencia:** ver §"Concurrencia". Un Build activo bloquea el workspace; Ask puede correr en paralelo con permisos de solo lectura.

---

## Concurrencia

- **Un solo Build activo por workspace.** Se serializa mediante `flock` exclusivo en `.tundracode/.lock`. Si un segundo Build se intenta iniciar mientras el primero está en curso, queda en cola o se rechaza con un mensaje claro al usuario.
- **Ask puede correr en paralelo** con un Build activo, pero con permisos de solo lectura sobre el filesystem: las tools de escritura (`Edit`, `Create`, `Delete`, `ApplyDiff`) se deshabilitan a nivel tool layer mientras un Build está activo. `RunCommand` y `SearchInWeb` siguen disponibles para Ask.
- **Plan no puede correr** mientras un Build está activo. El IDE muestra "Build en progreso, espera a que termine" y el botón Plan se deshabilita.
- **El usuario puede cancelar** un Build en cualquier momento desde el panel Build log. La cancelación se propaga al `RunCommand` activo vía SIGTERM al process group.
- La estructura anterior sienta las bases para una versión futura con paralelismo real (multi-scope Build, file-claim registry por path, conflict-detection en fan-in); no se implementa en v1.

---

## Planes versionados

- Cada plan generado se persiste en `.tundracode/plans/{slug}_{ISO8601-UTC-sin-colons}.md`.
- `slug`: lowercase, alfanumérico + guiones, max 60 chars, derivado del título con sanitización (caracteres no-FS como `:`, `/`, `\0` se eliminan; espacios se colapsan a `-`). Si colisiona en el mismo segundo, sufijo `-2`, `-3`, etc.
- Formato del archivo: timestamp, input original del usuario, estado del workspace en ese momento, plan generado.
- **Anotaciones del usuario** (opcionales): bloques `> [!NOTE] ...` en el Markdown del plan, como PR review comments. El parser extrae todas las anotaciones y las entrega al Build como restricciones hard por sección (proximidad de heading).
- El directorio `.tundracode/` es controlado por el IDE, no por los agentes directamente. Los agentes no pueden escribir, modificar ni eliminar archivos dentro de `.tundracode/`.
- Los planes son auditables, comparables entre sesiones, y sirven como log de decisiones.

---

## Memoria de proyecto

- Archivo `.tundracode/memory.md` con contexto persistente: convenciones del proyecto, decisiones de arquitectura, dependencias clave, notas del usuario.
- El agente Plan lo lee al inicio de cada sesión.
- El usuario puede editarlo manualmente o el IDE puede ofrecer actualización sugerida después de un Build exitoso.
- **Tamaño objetivo:** <50 KB. El IDE ofrece un comando "compactar memoria" si supera 100 KB.
- **Resolución de conflictos:** si el usuario edita `memory.md` a la vez que el Build sugiere una actualización, prevalece la edición del usuario; la sugerencia se descarta con una nota en el log.

---

## Snapshots y undo

TundraCode depende de git para snapshots de undo. Por cada operación de escritura (`Edit`, `Create`, `Delete`, `ApplyDiff`):

1. Si el proyecto es repo git, el IDE ejecuta `git stash push -u -m "tundracode pre-op-<id>"` antes de aplicar el cambio. El snapshot incluye solo los archivos del workspace afectados por la operación.
2. La entrada del Timeline guarda el `stash` ref + el plan + los diffs.
3. El "undo" de una operación hace `git stash pop` del ref correspondiente, con resolución de conflictos si el usuario editó el archivo manualmente después. Si hay conflicto, el IDE muestra el merge conflict estándar y permite resolución visual.
4. Snapshots se eliminan al `git stash drop` explícito o al cerrar el workspace.

**Si el proyecto NO es repo git**, el "undo" cubre solo operaciones dentro de la misma sesión: el IDE mantiene un buffer de "previous content" en RAM para los últimos N cambios (default 50, configurable). El IDE recomienda al usuario iniciar git (`git init` + commit inicial) si quiere undo persistente.

**Limitaciones explícitas:**

- El "undo por hunk" sí está soportado (cada hunk aplicado genera un snapshot atómico).
- El "undo de `RunCommand`" **NO** está soportado en v1. Las acciones de `RunCommand` no son reversibles (afectan `node_modules/`, `target/`, bases de datos, sistema de archivos fuera del workspace, etc.). Se loggean en el Timeline para auditoría, pero no son "undoable".
- El IDE muestra un warning persistente en la barra inferior cuando un proyecto no es repo git, recordando la limitación de undo.

---

## LSP nativo

- Cliente LSP implementado en Rust, integrado directamente en el backend de TundraCode.
- Sin dependencia de extensiones externas ni ecosistema de plugins de terceros.
- Funcionalidades: autocompletado, go-to-definition, hover info, diagnósticos en tiempo real.
- Los diagnósticos son consumibles por el agente Build y Ask vía herramienta `GetDiagnostics`.
- Los language servers se detectan automáticamente si están instalados en el sistema; si no, el IDE ofrece instrucciones de instalación.

### Lenguajes prioritarios en v1

| Lenguaje | Language Server |
|----------|----------------|
| Rust | rust-analyzer |
| JavaScript / TypeScript | typescript-language-server (ts-server) |
| Python | pyright (basado en Rust, sin dependencias de Python runtime) |

**Python:**

- Detección de proyecto: `pyproject.toml`, `setup.py`, `requirements.txt`, `uv.lock`, `poetry.lock`, `Pipfile`. El IDE respeta el entorno virtual activo (venv, poetry, uv) si está presente, y lo comunica a pyright vía `pythonPath`.
- Formatter/linter: configurable por proyecto (`ruff`, `black`, `autopep8`). Default: `ruff check` y `ruff format` si están disponibles; si no, fallback a `pyright` solo.
- Soporte para type checking estricto (mypy-compatible) y modo básico.

Java, Go, C/C++ y otros lenguajes son roadmap (v1.1+): la arquitectura del cliente LSP debe ser agnóstica al lenguaje desde el principio.

---

## Modelos de IA

### Providers remotos (API)

La lista de providers disponibles es fija en v1 (ver tabla completa en §"Gestión de providers") y no se añaden nuevos sin update del binario. Cada provider tiene:

- **API key en keyring** (libsecret en Linux, vía Secret portal en Flatpak; fallback a `secrets.age` si keyring no disponible).
- **Base URL** configurable; default por provider. HTTPS obligatorio para cloud providers; HTTP solo permitido para `localhost` / `127.0.0.1` / `::1` (Ollama local).
- **Endpoint de test** ligero, definido en §"Gestión de providers".
- **Flujo de conexión**: modal "Connect Provider" con test obligatorio antes de guardar (ver §"Gestión de providers").

Proveedores soportados en v1:

- **Cloud APIs**: Anthropic, OpenAI, Gemini, Mistral, DeepSeek, Groq, Kimi
- **OpenAI-compatible**: OpenRouter, Fireworks, OpenCode Zen, Azure OpenAI, Custom
- **Local runtime**: Ollama (único sin API key; usa base URL local)

La condición genérica para añadir un provider: el endpoint implementa la API de OpenAI (chat completions + tool calling) o la API de Anthropic (messages + tool use). El adapter es genérico por familia.

### Modelos locales
- **UI especializada** para descargar, gestionar e iniciar modelos locales sin necesidad de ejecutar el runtime manualmente desde terminal.
- **Adapter de runtime:** TundraCode habla con un adapter abstracto, no con Ollama directamente. El primer adapter soportado es **Ollama** (API: `/api/chat`, `/api/ps`, `/api/pull`). LM Studio (OpenAI-compatible) y vLLM quedan como adapters secundarios en v1.1.
- Listado de modelos disponibles con tamaño, capacidades (tool calling, vision, context length) y recomendación de uso (cuáles sirven para código, cuáles para chat, cuáles soportan tool calling).
- Estado del runtime visible en la barra inferior (activo/inactivo, VRAM usada si disponible, vía el adapter).
- Configurable por agente: ej. Ask usa modelo local (privacidad), Plan/Build usan modelo cloud (capacidad).
- **Ollama como provider**: Ollama aparece en el listado de providers de §"Gestión de providers" como caso especial (sin API key, con base URL configurable, default `http://localhost:11434`). El flujo de descarga de modelos se mantiene como subsección de §Configuración bajo "Local Models" — separado de "Providers" para evitar mezclar concerns. La UI de Connect/Disconnect para Ollama es la misma que para los cloud providers (test connection = `GET /api/tags`).

### Configuración de modelos
- **Dropdown de modelo activo** visible en el panel derecho (sub-panel "Settings rápidos").
- El dropdown está **filtrado por providers conectados** (ver §"Gestión de providers"). Solo aparecen modelos de providers con `ProviderConnection` activa. Si no hay providers conectados, el dropdown muestra el placeholder "No providers connected" y un atajo al modal de Connect.
- **Configuración por agente** (no global forzado). Cada agente (Plan, Build, Ask) tiene su propia selección de modelo.
- **Parámetros por modelo**: temperatura, max tokens. Se configuran en un sub-modal al hacer click en el ícono ⚙ junto al dropdown de modelo, o en el sub-panel de Settings del agente.
- **Cache de modelos**: TTL 24h, refreshed en background al abrir el dropdown. Botón "Refresh models" en el header del dropdown para forzar.

---

## Herramientas (Tool Calling)

Los agentes se comunican con el sistema exclusivamente a través de estas herramientas. Cada llamada es loggeable, auditable y visible en el panel derecho.

| Herramienta | Cuándo usarla | Atomic | Diff previo |
|-------------|---------------|--------|-------------|
| `ReadFile` | leer contenido | n/a | n/a |
| `Edit` (search-replace fuzzy) | cambios pequeños/medianos sobre archivo existente | sí | sí |
| `Create` | archivo nuevo (falla EEXIST si existe) | sí | sí |
| `Delete` | eliminar archivo (con confirmación siempre) | sí (vía `.tundracode/.trash/`) | sí |
| `ApplyDiff` (unified diff context-aware) | cambios grandes / multi-hunk / renames | sí | sí |
| `RunCommand` | ejecutar proceso sandboxed | n/a | n/a |
| `SearchCodebase` | búsqueda semántica local | n/a | n/a |
| `SearchInWeb` | búsqueda web (Ask/Plan) | n/a | n/a |
| `GetDiagnostics` | diagnósticos LSP | n/a | n/a |
| `ListDirectory` | listar un directorio | n/a | n/a |
| `GetWorkspace` | estructura general del proyecto | n/a | n/a |

`WriteFile` se elimina como primitiva. Toda escritura pasa por `Edit` (existente) o `Create` (nuevo).

### Atomicidad y permisos de archivos

Toda escritura (`Create`, `Edit`, `ApplyDiff`) sigue la secuencia:

1. Crear tmpfile en el mismo directorio del target (no en `/tmp`).
2. Escribir contenido, `fsync(tmp)`.
3. `fchmod(tmp, original_mode)` o `0644`/`0755` si el archivo no existía.
4. `rename(tmp, target)` (atómico en POSIX).
5. `fsync(parent_dir)`.
6. Si uid/gid del target difieren del proceso: fallback a in-place write con `O_NOFOLLOW` + guards fstat (preserva ownership, pierde atomicidad). Esto se loggea explícitamente.

Limpieza del tmpfile en cada path de error. En caso de crash, el tmpfile queda como orfan y se elimina al siguiente arranque.

### Aplicación robusta de diffs (`ApplyDiff`)

`ApplyDiff` recibe un unified diff. El applier:

- No confía en los line numbers de los hunk headers (`@@ -X,Y +X,Y @@`).
- Busca el bloque de contexto con ventana deslizante ±20 líneas alrededor de la posición declarada.
- Si no hay match exacto, fuzzy match con Levenshtein + score de confianza.
- Aplica solo si confianza ≥ 0.95; si no, devuelve error estructurado con: confianza, candidatos alternativos, y un mensaje accionable para el LLM ("el archivo cambió desde el read, vuelve a leer y regenera el diff").
- Normaliza líneas vacías dentro de hunks (trátalas como contexto).
- Detecta y rechaza hunks parciales (response truncado por streaming).
- Maneja `fenced diff` (`` ```diff `` wrappers) extrayendo el contenido.

### Modos de red de `RunCommand`

| Modo | Default | Uso típico | Warning al usuario |
|------|---------|------------|-------------------|
| `off` | sí | tests, lints, formatters, builds ya resueltos | sin red |
| `egress-allowlist` | opt-in | `cargo build` (crates.io), `npm install` (registry.npmjs.org) | "permitirá conexiones a N hosts" |
| `on` | opt-in | clonar un repo externo, `curl` ad-hoc | "el comando tiene acceso a internet sin restricción" |

`egress-allowlist` se configura por proyecto en `.tundracode/sandbox.toml`: lista explícita de hosts. El modo `on` requiere confirmación explícita del usuario por comando, incluso en modo autónomo. La resolución de DNS del egress proxy bloquea dominios en deny-list (`pypi.org`, `crates.io`, `registry.npmjs.org`, etc. según config) y permite solo los hosts en allow-list.

### Output de `RunCommand`

- stdout y stderr se reportan en streams separados en el panel Output.
- Encoding: UTF-8 asumido; si el comando emite otra codificación, el IDE intenta auto-detectar y advierte al usuario.
- Buffer: máximo 10 MB por stream; truncation con warning si se excede.
- Timeout hard: configurable por comando (default 10 min).
- Timeout soft: ningún stream recibe bytes en 60s → advertencia; el usuario puede abortar.
- Cancelable desde UI: SIGTERM + grace 5s + SIGKILL al process group (ver §"Sandbox de comandos").

### Stack de búsqueda semántica (`SearchCodebase`)

- **Embeddings:** BGE-small-en-v1.5 vía FastEmbed (ONNX CPU, ~130 MB). Cacheado en `~/.cache/tundracode/embeddings/`. Sin red en runtime.
- **Chunking:** tree-sitter queries extrayendo funciones/métodos/clases. Fallback a line-window (1000 chars, 200 overlap) para lenguajes sin grammar disponible.
- **Vector store:** HNSW in-process vía `usearch`. Sin daemon.
- **Index persistente:** SQLite en `.tundracode/index/`.
- **Invalidación:** incremental con `blake3(text + trivia)`. Re-index debounced 3s después de cambios en disco.
- **Skips:** archivos >1 MB, binarios, todo lo cubierto por `.gitignore` + `.tundracodeignore`.

### Búsqueda web (`SearchInWeb`)

- Provider configurable: Brave Search API / Tavily / DuckDuckGo.
- Rate limit: 60 req/min por sesión.
- Cache de resultados: 24h, key por query normalizada.
- Redacción automática: los paths absolutos del workspace se reemplazan por `<workspace>` antes de enviar queries.
- Sin red para el comando sandbox (el provider se llama desde el host, fuera del sandbox).

### Diagnósticos LSP (`GetDiagnostics`)

- El IDE espera a que el LSP server del lenguaje correspondiente haya terminado de indexar el archivo, con timeout 30s.
- Si timeout: devuelve snapshot vacío + warning estructurado.
- Cache LRU por path con TTL 5s.
- Si el lenguaje no tiene LSP server activo: devuelve error claro "no LSP server para \<lenguaje\>".

### Manejo de archivos

- **Encoding:** UTF-8 asumido. Si la heurística detecta >5% de bytes no-válidos en UTF-8, se advierte al usuario y se trata como binario.
- **Line endings:** CRLF y LF se preservan en `Edit`. Se detecta per-file; mixed endings dentro de un archivo se preservan tal cual.
- **BOM:** se preserva si el archivo original lo tiene.
- **Archivos binarios:** `ReadFile`, `Edit`, `ApplyDiff` rechazan binarios (>5% bytes no-imprimibles en una muestra de 4 KB). El IDE sugiere abrir en modo "binary viewer" (read-only, hex dump).
- **Tamaño máximo:** `ReadFile` rechaza >5 MB y sugiere modo "read-only con resaltado parcial". `Edit` y `Create` sin límite duro pero con progress bar.

---

## Presupuesto por tarea

- Antes de ejecutar un Plan, el IDE estima el costo aproximado en tokens (y en dinero si el provider tiene pricing conocido).
- El usuario puede definir un límite por tarea (en tokens o USD).
- Si el agente supera el límite estimado, pausa y pide confirmación antes de continuar.
- Contador de uso acumulado visible en el panel derecho (por sesión y por proyecto).

---

## Configuración

Configuraciones disponibles (sin redundancias ni configs innecesarias):

- **Providers conectados** (ver §"Gestión de providers"): conectar, desconectar, validar API key. El selector de modelo del Agents panel filtra por providers con conexión activa.
- **Modelo activo por agente.** Configurable por agente (no global forzado).
- **Modo autónomo vs asistido** (por proyecto, vía `autonomous_mode = true` en `.tundracode/config.toml`).
- **Runtime de modelos locales** (path al ejecutable si no está en PATH; e.g., binario de Ollama).
- **Lenguajes habilitados para LSP** y qué language server usar.
- **Teclas de acceso rápido** personalizables (mínimo: abrir/cerrar panel derecho, activar agente Ask, iniciar Plan, etc).
- **Fallback entre providers**: si el provider activo falla con 429 o 5xx, intentar el siguiente antes de rendirse. Default: enabled.

Evitar: configs de UI puramente estéticas, configs que puedan ser hardcodeadas sin romper nada, configs duplicadas entre agentes.

### Gestión de providers

Cada provider conectado se representa como:

```rust
struct ProviderConnection {
    id: Uuid,                              // estable, generado al conectar
    provider_type: ProviderType,           // enum: Anthropic, OpenAI, OpenRouter,
                                           // Kimi, DeepSeek, Gemini, Mistral, Groq,
                                           // AzureOpenAI, Fireworks, OpenCodeZen, Ollama, Custom
    base_url: Option<String>,              // default por provider; Ollama = http://localhost:11434
    keyring_account: Option<String>,       // solo para key-requiring; null = Ollama
    connected_at: DateTime<Utc>,
    last_validated_at: Option<DateTime<Utc>>,
    last_validation_status: Option<Status>, // ok | error(<code>, <message>)
    model_cache: Option<Vec<ModelInfo>>,   // cacheado tras test, TTL 24h
}
```

**Persistencia:**
- Metadata (sin la key) en `~/.config/tundracode/providers.toml` (en Flatpak: `~/.var/app/$APPID/config/tundracode/providers.toml`).
- API key en **libsecret** (Secret Service) bajo `service = "tundracode"`, `account = "{provider_type}"`. No se permiten dos conexiones al mismo provider_type (decisión: una cuenta por provider).

#### Proveedores y sus campos requeridos

| Provider | API key | Base URL | Notas |
|----------|---------|----------|-------|
| Anthropic | sí | no (default `https://api.anthropic.com`) | requiere tool use |
| OpenAI | sí | no (default `https://api.openai.com/v1`) | requiere tool calling |
| OpenRouter | sí | no (default `https://openrouter.ai/api/v1`) | OpenAI-compatible |
| Kimi | sí | no (default `https://api.moonshot.ai/v1`) | OpenAI-compatible |
| DeepSeek | sí | no (default `https://api.deepseek.com/v1`) | OpenAI-compatible |
| Gemini | sí | sí (default `https://generativelanguage.googleapis.com/v1beta`) | OpenAI-compatible adapter |
| Mistral | sí | no (default `https://api.mistral.ai/v1`) | OpenAI-compatible |
| Groq | sí | no (default `https://api.groq.com/openai/v1`) | OpenAI-compatible |
| Azure OpenAI | sí | **sí** (obligatorio: el endpoint del recurso) | OpenAI-compatible |
| Fireworks | sí | no (default `https://api.fireworks.ai/inference/v1`) | OpenAI-compatible |
| OpenCode Zen | sí | no (default `https://api.opencode.ai/v1`) | OpenAI-compatible, free tier |
| **Ollama** | **no** | **sí** (default `http://localhost:11434`) | único sin key; HTTP solo para localhost |
| **Custom** | sí | **sí** (obligatorio) | OpenAI-compatible genérico |

#### UI — Listado de providers conectados

En §Configuración, subsección "Providers":

```
┌─ Configuración ─────────────────────────────────────────┐
│ ... (existentes) ...                                     │
│                                                         │
│ Providers                                            ⊕  │
│ ──────────────────────────────────────────────────────  │
│  Anthropic                  ● Conectado · 5m ago   ⋯   │
│    claude-opus-4, claude-sonnet-4, ...                  │
│  OpenAI                     ● Conectado · 2h ago   ⋯   │
│    gpt-4o, o3, o1, ...                                  │
│  Ollama                    ◌ No requiere key      ⋯   │
│    llama3.2, qwen2.5-coder, ...                         │
│                                                         │
│  [+ Connect Provider]                                   │
└─────────────────────────────────────────────────────────┘
```

Cada fila de provider conectado:
- **Icono estado**: ● verde (conectado + última validación OK), ● amarillo (conectado pero validación >24h o falló la última), ● rojo (desconectado, en error).
- **Timestamp** de última validación exitosa.
- **Menú `⋯`**: "Test connection", "Reemplazar API key", "Disconnect".
- **Lista de modelos** detectados (primeros 5 + "y N más...").
- **Sin keyreq (Ollama)**: muestra estado del runtime (activo/inactivo, modelos instalados).

Botón **`+ Connect Provider`** al final de la lista.

#### UI — Modal "Connect Provider"

Activado por `+ Connect Provider`. Modal flotante (overlay sobre el panel de Settings, no ventana Tauri separada — más simple y consistente con el resto).

```
┌─ Connect Provider ──────────────────────────── [×] ┐
│                                                    │
│  Provider                                         │
│  [ Anthropic                          ▾]          │
│                                                    │
│  API key                                          │
│  [ ●●●●●●●●●●●●●●●●●●●●●● ]   [👁 Show]          │
│  Get your key at: console.anthropic.com/settings   │
│                                                    │
│  Base URL (opcional)                              │
│  [ https://api.anthropic.com         ]            │
│                                                    │
│  ─────────────────────────────────────────────    │
│                                                    │
│  Last test:  —                                    │
│                                                    │
│  [Test connection]  [Save]  [Cancel]              │
└────────────────────────────────────────────────────┘
```

Comportamiento:
- **Selector de provider**: dropdown agrupado:
  - "Cloud APIs" — Anthropic, OpenAI, Gemini, Mistral, DeepSeek, Groq, Kimi
  - "OpenAI-compatible" — OpenRouter, Fireworks, OpenCode Zen, Azure OpenAI, Custom
  - "Local runtime" — Ollama
- Al cambiar el selector: los campos se reconfiguran. El campo "API key" se oculta/disabled para Ollama. El campo "Base URL" se rellena con el default; aparece obligatorio para Azure, Custom y Ollama. El help text apunta a las docs del provider.
- **API key**: input tipo `password` con botón "Show" (auto-mask a los 5s tras revelar). Validación de formato en blur (no regex estricta, solo longitud > 20 chars para evitar typos obvios).
- **Base URL**: default visible pero editable. Validación: HTTPS obligatorio para cloud providers; HTTP solo permitido para `localhost` / `127.0.0.1` / `::1` (Ollama local).
- **"Test connection"**:
  - Disabled hasta que API key (si requerida) tenga contenido Y base URL sea válida.
  - Llama al endpoint más barato del provider (ver tabla abajo).
  - Muestra spinner durante el test.
  - Resultado: badge verde "✓ OK · found N models" o rojo con el error textual del provider.
  - El botón **Save** se habilita solo si el test pasó.
- **Save**:
  - Persiste metadata en `providers.toml`.
  - Escribe la key en keyring vía `keyring` crate (servicio `tundracode`, account = provider_type).
  - Refresca el model cache (segundo fetch paralelo al test, también cheap).
  - Cierra el modal.
  - La fila del provider aparece en la lista con estado "conectado · hace 0s".
- **Cancel / ×**: descarta sin guardar.
- **Si el keyring falla** (no D-Bus, sin secret service): modal de error con dos opciones:
  - "Usar archivo cifrado local" (fallback con passphrase del usuario, almacenado en `~/.config/tundracode/secrets.age` cifrado con `age`).
  - "Cancelar" y bloquear la conexión.

#### Endpoints de "Test connection" por provider

| Provider | Endpoint | Costo |
|----------|----------|-------|
| Anthropic | `POST /v1/messages` con `max_tokens=1, messages=[{role:user, content:"ping"}]` | un micro-request |
| OpenAI-compat | `GET /v1/models` | cero, solo lista |
| Gemini (OpenAI-compat) | `GET /v1beta/models` | cero |
| Ollama | `GET /api/tags` | cero, lista modelos instalados |
| Azure OpenAI | `GET {base_url}/openai/deployments?api-version=...` | cero |

Códigos de error manejados:
- `401 Unauthorized` → "API key inválida o revocada"
- `403 Forbidden` → "Acceso denegado. Verifica permisos o región"
- `404 Not Found` → "Base URL incorrecta o recurso no existe"
- `429 Too Many Requests` → "Rate limit. Reintenta en N segundos"
- Network error / timeout 10s → "No se pudo conectar. Verifica base URL y red"
- HTTP no soportado (http:// en cloud) → bloqueado con error "HTTPS requerido"

#### Disconnect flow

1. Click en `⋯ → Disconnect` (o `Disconnect` directo en menú de fila).
2. **Modal de confirmación**:
   ```
   ┌─ Disconnect Anthropic? ───────────────────── [×] ┐
   │                                                   │
   │  This will:                                       │
   │  · Remove the API key from the system keyring     │
   │  · Remove "Anthropic" from the model selector     │
   │  · Revert the active model in [N] agents to the   │
   │    next available provider, or "No provider"      │
   │                                                   │
   │  You'll need to re-enter the API key to use it    │
   │  again.                                           │
   │                                                   │
   │            [Cancel]  [Disconnect]                 │
   └───────────────────────────────────────────────────┘
   ```
3. On confirm:
   - Quita la entry del `providers.toml`.
   - Borra la key del keyring (`keyring.delete_credential()`).
   - Invalida el model cache.
   - Recorre los agentes activos (`Plan`, `Build`, `Ask`):
     - Si su `provider_type == disconnected`, reasigna al siguiente provider conectado (por orden de la lista, primero en la lista = preferido).
     - Si no quedan providers: estado "No provider" (dropdown disabled, banner en el panel).
   - La fila desaparece de la lista con animación fade-out.
   - Toast de confirmación: "Anthropic disconnected. [N] agents reverted to [next provider]".

#### Model selector en el Agents panel

Dropdown en "Settings rápidos" del Agents panel. Comportamiento:

```
┌─ Model ─────────────────────── [▾] ┐
│                                   │
│ Anthropic                  ✓      │  ← provider header
│   claude-opus-4                   │
│   claude-sonnet-4           ★     │  ← ★ = current selection
│   claude-haiku-4                  │
│ OpenAI                            │
│   gpt-4o                          │
│   o3                              │
│ Ollama (local)                    │
│   llama3.2:70b                    │
│   qwen2.5-coder:32b               │
│                                   │
│ ─────────────────────────         │
│   + Connect a provider...         │  ← solo si no hay conectados
└───────────────────────────────────┘
```

- **Header del provider** se muestra en bold/grey.
- **Modelos** listados bajo su provider.
- **Modelo activo** marcado con ★.
- **Filtrado estricto**: SOLO aparecen modelos de providers con `ProviderConnection` activa. Ningún modelo "bloqueado" se muestra ni en estado disabled — directamente no aparece. Si todos están desconectados, el dropdown muestra el placeholder "No providers connected" y al clickear abre el modal de Connect (atajo al §Configuración).
- **Sub-grupo por agente**: el selector es por agente (Plan/Build/Ask cada uno tiene su selección). El dropdown abierto muestra los modelos agrupados por provider; la selección actual de ese agente lleva el ★.
- **Provider actual** se indica en el header del subgrupo (e.g., "Anthropic · claude-sonnet-4") en el sub-panel del agente.

#### Empty state

Si no hay providers conectados:
- Dropdown: "No providers connected — click to add".
- Banner persistente en el Agents panel: "Configura al menos un provider para usar los agentes. [+ Connect provider]".
- Botón "Iniciar Plan" en Plan y "Preguntar" en Ask quedan disabled con tooltip "Configura un provider primero".
- Settings rápidos muestra el estado vacío.

#### Edge cases

1. **Keyring no disponible** (Flatpak sin Secret portal, CI/headless, sesión SSH): fallback a `secrets.age` cifrado con `age` (passphrase del usuario). El modal de Connect lo explica. El usuario elige: bloquear conexión o usar fallback.
2. **Cambiar API key de un provider ya conectado**: menú `⋯ → Reemplazar API key` abre el modal pre-llenado (key masked con placeholder `•••• (12 chars shown)`). Requiere nuevo test.
3. **Disconnect durante un Build activo**: la desconexión se permite; el Build en curso termina con la key actual (ya cargada en memoria). Las nuevas operaciones del Build fallarán si intenta refrescar; se loggea el error. El usuario puede reconectar sin perder el Build.
4. **Provider con lista de modelos dinámica** (e.g., Azure con deployments que cambian): cache TTL 24h + botón "Refresh models" en la fila.
5. **Múltiples providers caídos simultáneamente**: el agente Plan/Build, al fallar, intenta el siguiente provider en la lista antes de rendirse. Configurable: `provider_fallback = enabled | disabled` en §Configuración.
6. **Provider con rate limits** (e.g., Groq free tier): el error 429 se traduce en "rate limit, fallback a {next provider}?". El usuario decide.
7. **Provider marcado como "deprecated"** (e.g., un día Anthropic anuncia deprecation de un modelo): el modelo sigue en el selector pero con badge "deprecated · will be removed on {date}". El usuario puede pre-emptively cambiar.
8. **Ollama no está corriendo al hacer Test connection**: error claro "Ollama no responde en {base_url}. Inicia el runtime desde Settings → Local Models o desde terminal con `ollama serve`."

---

## Seguridad

- Las API keys se almacenan en el keyring del sistema operativo (libsecret en Linux), no en archivos planos.
- `RunCommand` ejecuta en un entorno sandbox con acceso limitado: solo el directorio del workspace, sin acceso a red salvo configuración explícita. El stack concreto y la política de red se definen en §"Sandbox de comandos" y §"Modos de red de RunCommand".
- `Delete` siempre requiere confirmación explícita del usuario, incluso en modo autónomo.
- Los agentes no tienen acceso a rutas fuera del workspace abierto. El tool layer canonicaliza cada path y rechaza accesos fuera del workspace antes del syscall.
- El directorio `.tundracode/` está excluido de las operaciones de Build (los agentes no pueden modificarlo directamente).

### Sandbox de comandos

`RunCommand` se ejecuta siempre dentro de un proceso sandbox en Linux. La cadena de aislamiento es, en orden:

1. **bubblewrap (bwrap)** — unprivileged user namespace + mount namespace.
   - Bind-mount del workspace como RW.
   - `/tmp` tmpfs con scope por comando.
   - `/dev/null` y `/dev/urandom` solo.
   - Bind-mounts denegados (no se exponen, no son `EACCES`): `~/.ssh`, `~/.aws`, `~/.config/gh`, `~/.gnupg`, `~/.kube`, `~/.docker`.
2. **seccomp-bpf** — deny-list explícita: `mount`, `unshare`, `ptrace`, `kexec_load`, `init_module`, `finit_module`, `bpf`, `userfaultfd`.
3. **Landlock** — enforcement redundante sobre las mismas reglas de FS como defense-in-depth. En kernels <5.13 se omite (bwrap-only).
4. **egress proxy** — para HTTP/HTTPS, un proxy local en `127.0.0.1` filtra por allowlist de hosts; el resto del tráfico TCP/UDP se deniega en namespace.

**Cancelación:** el IDE envía SIGTERM al PID, espera 5s, luego SIGKILL. Se mata el process group completo (configurado vía `prctl(PR_SET_PDEATHSIG)` en el child).

**Dependencias externas requeridas:** `bubblewrap` y `bwrap` deben estar en PATH. Si no, el IDE lo indica al abrir el primer workspace.

### Comandos que siempre requieren confirmación

Independiente del modo autónomo, los siguientes patrones requieren confirmación explícita del usuario por comando (match por regex en el comando completo, no por el binario solo):

- `rm -rf` (cualquier argumento con `-rf` o `-fr`)
- `mkfs.*`, `dd if=`, `:(){:|:&};:`
- `chmod -R 777`, `chown -R`
- `curl ... | sh`, `wget ... | sh`, `eval $(curl ...)`
- `git reset --hard`, `git clean -fd`, `git push --force`
- `sudo`, `su`, `doas`

La lista es extensible por proyecto en `.tundracode/sandbox.toml`.

---

## Calidad y testing

- Tests unitarios por crate Rust (objetivo: >80% coverage en core).
- Integration tests del workflow Plan→Build con fixtures de proyectos (Rust mínimo, Python mínimo, JS mínimo, multi-package).
- **Tests de sandbox:** intentos de escape documentados deben fallar (intentar leer `~/.ssh`, escribir fuera del workspace, montar filesystems, `unshare`, `ptrace`). El test verifica que el sandbox retorna `EACCES`/`EPERM`/`ENOENT` según corresponda.
- **Tests de atomicidad:** `kill -9` mid-write, leer archivo después, verificar integridad (mismo hash que input completo). Test de preservación de modo (chmod 600 → write → sigue 600). Test de preservación de uid/gid en Docker/root scenario.
- **Tests de `ApplyDiff`:** corpus de fixtures de fallos reales documentados (fence wrappers, line drift, hunks parciales, trailing whitespace, missing lead-out context). Cada fixture es un diff malformado y un test verifica que el applier devuelve el error estructurado correcto.
- **Tests de concurrencia:** dos operaciones de escritura simultáneas, verificar que el lock serializa. Un Build + Ask simultáneo, verificar que Ask no puede escribir.
- **Tests de tools peligrosos:** verificar que `rm -rf /`, `git reset --hard`, etc. requieren confirmación.
- **Tests LSP:** indexación, timeouts, restart tras crash, race entre `didChange` y `didOpen`.
- CI ejecuta los tests en Ubuntu LTS, Fedora latest, Arch latest. Coverage report se publica en cada PR.

---

## Privacidad y telemetría

- TundraCode es open source; el binario es verificable.
- **Sin telemetría implícita.** Cero conexiones salientes por defecto.
- Las API keys nunca salen del keyring del usuario (en Flatpak, vía Secret portal; en `.tar.gz`, vía libsecret/zbus a Secret Service — ver §"Seguridad").
- **Redacción de secretos en logs:** el detector enmascara strings que matchean `/api[_-]?key|token|secret|password|private[_-]?key|bearer\s+[a-z0-9]/i` antes de escribir al Timeline o al Build log. La regex es extensible en `.tundracode/redaction.toml`.
- **Diagnóstico opt-in:** el usuario puede generar un bundle de diagnóstico (logs + stack + contexto, secretos redactados) para reportar bugs. Nunca se envía automáticamente. El bundle se exporta a un archivo local; el usuario decide si y cómo enviarlo.
- **Lista de endpoints externos contactables:** configurable en `.tundracode/endpoints.toml`, vacía por defecto. Cada endpoint nuevo (provider LLM, search provider, update server) requiere consentimiento explícito del usuario al activarlo. El IDE muestra un warning si una operación intenta conectar a un endpoint no listado.
- **Secret scanning al abrir workspace:** el IDE indexa (sin enviar a ningún lado) patrones de secretos en archivos del workspace y muestra un warning si encuentra credenciales hardcodeadas. El warning es solo informativo, no bloqueante.
- **Flujo de conexión de providers** (ver §"Gestión de providers"): la API key nunca se transmite fuera del keyring local. El "Test connection" del modal hace una llamada directa al endpoint del provider; la key viaja solo a ese destino. La base URL del provider es visible al usuario en el modal antes de testear, para que verifique el destino. HTTPS es obligatorio para todos los cloud providers; HTTP solo se permite para `localhost` / `127.0.0.1` / `::1` (Ollama local). El campo de API key es tipo `password` con botón "Show" que auto-maskea a los 5 segundos. La key nunca aparece en logs, en el Timeline, ni en el bundle de diagnóstico.

---

## Lifecycle de procesos

- **LSP servers:** start on first file of that language open. Stop on workspace close con grace 10s. Restart on new file con lazy start. Restart con backoff exponencial en crash (max 3 intentos, base 1s). Monitoring de RSS con kill si >2 GB configurable. Si el LSP server no responde a `initialize` en 10s, se descarta y se reintenta.
- **Local model runtime (Ollama y otros adapters):** start/stop desde la UI opcionalmente. Si se inicia, se monitorea VRAM vía el adapter. Matar el runtime desde TundraCode es destructivo (afecta otros clientes del mismo usuario); el IDE advierte.
- **Daemon de index semántico:** siempre activo si el workspace tiene un index. Reindex debounced 3s. Apagado limpio al cerrar el workspace (commit final del index a SQLite).
- **Tauri webview:** config de sandbox del webview habilitado por defecto (CSP estricta, sin `nodeIntegration`, `contextIsolation: true`, `webSecurity: true`).
- **Procesos sandbox (`RunCommand`):** ver §"Sandbox de comandos". Cada comando es un proceso efímero; no hay daemon de comandos.
- **Bucle principal (Tauri backend):** tokio multi-thread. Una tarea por agente activo, una por LSP server, una por sandbox command. Cancelación cooperativa con `CancellationToken` propagado a todas las tareas hijas.

---

## Actualizaciones

- **Flatpak:** actualizaciones vía Flathub (no self-update). El `.flatpak` se compila desde el mismo código que el `.tar.gz` con el mismo pipeline de CI.
- **`.tar.gz`:** self-update con delta opcional (bsdiff). Channels: `stable` y `beta`. La actualización respeta la configuración del usuario (`~/.config/tundracode/config.toml`); no se borra ni sobrescribe.
- **Migración de config entre versiones:** automática para cambios aditivos; manual con asistente para cambios breaking (changelog en `.tundracode/MIGRATION.md` y, en su defecto, en el release notes publicado).
- **Versionado:** semver estricto. Major bump solo si hay breaking change en el formato de `.tundracode/`.

---

## LSP server discovery

Módulo único centraliza detección e instalación de language servers:

1. **Búsqueda en `PATH`** para binarios comunes (`rust-analyzer`, `typescript-language-server`, `pyright-langserver`).
2. **Búsqueda en ubicaciones estándar** por lenguaje:
   - Rust: `~/.cargo/bin/rust-analyzer`
   - JS/TS: `npm root -g` + binarios
   - Python: `~/.local/bin/pyright-langserver`, `which pyright`
3. **Búsqueda en binarios comunes de usuario** (`~/.local/bin`, `~/bin`).
4. Si no se encuentra, el IDE muestra el comando exacto para instalar:
   - Rust: `rustup component add rust-analyzer`
   - JS/TS: `npm install -g typescript typescript-language-server`
   - Python: `pipx install pyright` o `uv tool install pyright`
5. **Versión requerida:** mínima compatible con LSP 3.17. Si el binario es más viejo, el IDE advierte y degrada features específicas según capabilities negotiation.
6. **Override manual** en Settings → "Lenguajes y servidores": path absoluto al binario, argumentos extra, variables de entorno.

El usuario puede override manual en cualquier momento. El IDE cachea la decisión para no volver a preguntar en el mismo workspace.

---

## Diferenciación clave

TundraCode no es un wrapper de VSCode con IA. Es un IDE construido desde cero con los agentes como ciudadanos de primera clase, priorizando:

1. Transparencia total: el usuario siempre ve qué hace el agente, qué archivos toca, qué cuesta.
2. Control granular: aprobación por hunk/archivo, anotaciones en el plan, undo por hunk vía `git stash` (o buffer RAM si no hay git).
3. Eficiencia real: LSP nativo Rust, herramientas precisas (`Edit` con search-replace fuzzy, `ApplyDiff` context-aware), búsqueda semántica local (BGE-small + usearch, sin red en runtime).
4. Sin lock-in: cualquier provider con API OpenAI-compatible o Anthropic-compatible, modelos locales con adapter (Ollama primero), sin cuenta obligatoria, sin telemetría.
5. Distribución limpia: Flatpak (Flathub) o `.tar.gz` autosuficiente. Sandbox de comandos con bwrap + seccomp + Landlock + egress proxy. Open source, verificable.
