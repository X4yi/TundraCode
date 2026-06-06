use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_util::sync::CancellationToken;
use tundracode_agents::{
    r#loop::{AgentLoop, RunConfig},
    Agent, AgentContext, AgentInput, AgentOutput, AskAgent, BuildAgent, PlanAgent, ToolInvocation,
};
use tundracode_models::{
    get_all_providers, get_provider_by_id, ModelConfig, ProviderInfo, ProviderModel,
    ProviderRegistry, ToolDefinition,
};
use tundracode_tools::{ToolContext, ToolRegistry};

#[derive(Default)]
struct AppState {
    workspace_path: Option<std::path::PathBuf>,
}

struct AgentOrchestrator {
    cancel_token: RwLock<Option<CancellationToken>>,
    running: RwLock<bool>,
}

impl AgentOrchestrator {
    fn new() -> Self {
        Self {
            cancel_token: RwLock::new(None),
            running: RwLock::new(false),
        }
    }

    async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    async fn cancel(&self) {
        if let Some(token) = self.cancel_token.read().await.as_ref() {
            token.cancel();
        }
        *self.running.write().await = false;
    }
}

type SharedState = Arc<Mutex<AppState>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub is_file: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContent {
    pub path: String,
    pub content: String,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitStatus {
    pub branch: String,
    pub is_dirty: bool,
    pub modified_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspStatus {
    pub active: bool,
    pub server_name: Option<String>,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspServerInfo {
    pub name: String,
    pub language_id: String,
    pub available: bool,
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub decorations: bool,
    pub is_wayland: bool,
}



#[cfg(target_os = "linux")]
fn is_wayland_session() -> bool {
    std::env::var("WAYLAND_DISPLAY")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
        || std::env::var("XDG_SESSION_TYPE")
            .map(|v| v.eq_ignore_ascii_case("wayland"))
            .unwrap_or(false)
}



#[tauri::command]
async fn open_workspace(path: String, state: State<'_, SharedState>) -> Result<String, String> {
    let mut guard = state.lock().await;
    let path_buf = std::path::PathBuf::from(&path);

    if !path_buf.exists() || !path_buf.is_dir() {
        return Err(format!("La ruta no existe o no es un directorio: {}", path));
    }

    guard.workspace_path = Some(path_buf.clone());
    Ok(path)
}

#[tauri::command]
async fn get_workspace(state: State<'_, SharedState>) -> Result<Option<String>, String> {
    let guard = state.lock().await;
    Ok(guard
        .workspace_path
        .as_ref()
        .map(|p| p.to_string_lossy().to_string()))
}

#[tauri::command]
async fn list_directory(
    path: String,
    state: State<'_, SharedState>,
) -> Result<Vec<FileEntry>, String> {
    let guard = state.lock().await;
    let workspace = guard
        .workspace_path
        .as_ref()
        .ok_or("No hay workspace abierto")?;

    let target_path = if path.is_empty() || path == "." {
        workspace.clone()
    } else {
        workspace.join(&path)
    };

    let mut entries = Vec::new();

    match tokio::fs::read_dir(&target_path).await {
        Ok(mut dir) => {
            while let Ok(Some(entry)) = dir.next_entry().await {
                let name = entry.file_name().to_string_lossy().to_string();
                let full_path = entry.path();
                let relative_path = full_path
                    .strip_prefix(workspace)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| full_path.to_string_lossy().to_string());

                let metadata = entry.metadata().await.ok();
                let is_directory = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                let is_file = metadata.as_ref().map(|m| m.is_file()).unwrap_or(false);

                entries.push(FileEntry {
                    name,
                    path: relative_path,
                    is_directory,
                    is_file,
                });
            }

            entries.sort_by(|a, b| match (a.is_directory, b.is_directory) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            });

            Ok(entries)
        }
        Err(e) => Err(format!("Error al leer directorio: {}", e)),
    }
}

#[tauri::command]
async fn read_file(path: String, state: State<'_, SharedState>) -> Result<FileContent, String> {
    let guard = state.lock().await;
    let workspace = guard
        .workspace_path
        .as_ref()
        .ok_or("No hay workspace abierto")?;

    let file_path = workspace.join(&path);

    if !file_path.exists() {
        return Err(format!("Archivo no encontrado: {}", path));
    }

    match tokio::fs::read_to_string(&file_path).await {
        Ok(content) => {
            let language = detect_language(&file_path);
            Ok(FileContent {
                path,
                content,
                language,
            })
        }
        Err(e) => Err(format!("Error al leer archivo: {}", e)),
    }
}

#[tauri::command]
async fn write_file(
    path: String,
    content: String,
    state: State<'_, SharedState>,
) -> Result<(), String> {
    let guard = state.lock().await;
    let workspace = guard
        .workspace_path
        .as_ref()
        .ok_or("No hay workspace abierto")?;

    let file_path = workspace.join(&path);

    if !file_path.starts_with(workspace) {
        return Err("No se permite escribir fuera del workspace".to_string());
    }

    match tokio::fs::write(&file_path, content).await {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Error al escribir archivo: {}", e)),
    }
}

#[tauri::command]
async fn get_git_status(state: State<'_, SharedState>) -> Result<GitStatus, String> {
    let guard = state.lock().await;
    let workspace = guard
        .workspace_path
        .as_ref()
        .ok_or("No hay workspace abierto")?;

    match tundracode_git::GitRepository::open(workspace) {
        Ok(repo) => {
            let branch = repo.branch().unwrap_or_else(|_| "unknown".to_string());
            let is_dirty = repo.is_dirty().unwrap_or(false);

            let modified_files = match tundracode_git::operations::get_status(&repo.repo) {
                Ok(statuses) => statuses
                    .into_iter()
                    .filter(|s| s.is_modified || s.is_new)
                    .map(|s| s.path)
                    .collect(),
                Err(_) => Vec::new(),
            };

            Ok(GitStatus {
                branch,
                is_dirty,
                modified_files,
            })
        }
        Err(_) => Ok(GitStatus {
            branch: "-".to_string(),
            is_dirty: false,
            modified_files: Vec::new(),
        }),
    }
}

#[tauri::command]
async fn git_stage(path: String, state: State<'_, SharedState>) -> Result<(), String> {
    let guard = state.lock().await;
    let workspace = guard
        .workspace_path
        .as_ref()
        .ok_or("No hay workspace abierto")?;

    let repo = tundracode_git::GitRepository::open(workspace).map_err(|e| e.to_string())?;

    tundracode_git::operations::stage(&repo.repo, &path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn git_commit(message: String, state: State<'_, SharedState>) -> Result<(), String> {
    let guard = state.lock().await;
    let workspace = guard
        .workspace_path
        .as_ref()
        .ok_or("No hay workspace abierto")?;

    let repo = tundracode_git::GitRepository::open(workspace).map_err(|e| e.to_string())?;

    tundracode_git::operations::commit(&repo.repo, &message).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_lsp_status() -> Result<LspStatus, String> {
    Ok(LspStatus {
        active: false,
        server_name: None,
        language: None,
    })
}

#[tauri::command]
async fn detect_lsp_servers() -> Result<Vec<LspServerInfo>, String> {
    use tundracode_lsp::LanguageServer;

    let servers = LanguageServer::all();
    let mut results = Vec::new();

    for server in servers {
        let available = server.is_available().await;
        results.push(LspServerInfo {
            name: server.name,
            language_id: server.language_id,
            available,
            extensions: server.extensions,
        });
    }

    Ok(results)
}

#[tauri::command]
async fn get_window_info() -> Result<WindowInfo, String> {
    #[cfg(target_os = "linux")]
    {
        let is_wayland = is_wayland_session();
        Ok(WindowInfo {
            decorations: !is_wayland,
            is_wayland,
        })
    }

    #[cfg(not(target_os = "linux"))]
    {
        Ok(WindowInfo {
            decorations: true,
            is_wayland: false,
        })
    }
}

#[tauri::command]
async fn run_agent_ask(
    message: String,
    provider_id: String,
    model_id: String,
    state: State<'_, SharedState>,
    orchestrator: State<'_, Arc<AgentOrchestrator>>,
) -> Result<String, String> {
    let workspace = {
        let guard = state.lock().await;
        guard
            .workspace_path
            .as_ref()
            .ok_or("No hay workspace abierto")?
            .clone()
    };

    let cancel = CancellationToken::new();
    *orchestrator.cancel_token.write().await = Some(cancel.clone());
    *orchestrator.running.write().await = true;

    let provider =
        get_provider_by_id(&provider_id).ok_or(format!("Provider not found: {}", provider_id))?;

    let api_key = tundracode_models::credentials::get_api_key(&provider_id)
        .ok()
        .flatten();

    let base_url = tundracode_models::credentials::get_base_url(&provider_id)
        .ok()
        .flatten()
        .unwrap_or_else(|| provider.base_url.clone());

    let model_config = tundracode_models::ModelConfig {
        provider: provider_id,
        model: model_id,
        api_key,
        base_url: Some(base_url),
        temperature: 0.7,
        max_tokens: 4096,
    };

    let context = AgentContext {
        workspace_path: workspace.to_string_lossy().to_string(),
        model_config,
        autonomous_mode: false,
        budget_tokens: 200000,
    };

    let input = AgentInput {
        user_message: message,
        plan_annotations: None,
        memory_excerpt: None,
    };

    let agent = AskAgent;
    let result = tokio::select! {
        output = agent.run(&context, input) => output,
        _ = cancel.cancelled() => Ok(AgentOutput::Error("Agent cancelled".to_string())),
    };

    *orchestrator.running.write().await = false;

    match result {
        Ok(AgentOutput::FinalAnswer { content, .. }) => Ok(content),
        Ok(AgentOutput::Error(e)) => Err(e),
        Ok(AgentOutput::ProposedChanges { .. }) => {
            Ok("Build output not expected from Ask".to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
async fn generate_plan(
    description: String,
    provider_id: String,
    model_id: String,
    state: State<'_, SharedState>,
    orchestrator: State<'_, Arc<AgentOrchestrator>>,
) -> Result<String, String> {
    let workspace = {
        let guard = state.lock().await;
        guard
            .workspace_path
            .as_ref()
            .ok_or("No hay workspace abierto")?
            .clone()
    };

    let cancel = CancellationToken::new();
    *orchestrator.cancel_token.write().await = Some(cancel.clone());
    *orchestrator.running.write().await = true;

    let provider =
        get_provider_by_id(&provider_id).ok_or(format!("Provider not found: {}", provider_id))?;

    let api_key = tundracode_models::credentials::get_api_key(&provider_id)
        .ok()
        .flatten();

    let base_url = tundracode_models::credentials::get_base_url(&provider_id)
        .ok()
        .flatten()
        .unwrap_or_else(|| provider.base_url.clone());

    let model_config = tundracode_models::ModelConfig {
        provider: provider_id,
        model: model_id,
        api_key,
        base_url: Some(base_url),
        temperature: 0.2,
        max_tokens: 8192,
    };

    let context = AgentContext {
        workspace_path: workspace.to_string_lossy().to_string(),
        model_config,
        autonomous_mode: false,
        budget_tokens: 200000,
    };

    let input = AgentInput {
        user_message: description,
        plan_annotations: None,
        memory_excerpt: None,
    };

    let agent = PlanAgent;
    let result = tokio::select! {
        output = agent.run(&context, input) => output,
        _ = cancel.cancelled() => Ok(AgentOutput::Error("Agent cancelled".to_string())),
    };

    *orchestrator.running.write().await = false;

    match result {
        Ok(AgentOutput::FinalAnswer { content, .. }) => Ok(content),
        Ok(AgentOutput::Error(e)) => Err(e),
        Ok(AgentOutput::ProposedChanges { .. }) => {
            Err("Unexpected output from Plan agent".to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
async fn cancel_agent(orchestrator: State<'_, Arc<AgentOrchestrator>>) -> Result<String, String> {
    orchestrator.cancel().await;
    Ok("Agent cancelled".to_string())
}

#[tauri::command]
async fn agent_status(orchestrator: State<'_, Arc<AgentOrchestrator>>) -> Result<bool, String> {
    Ok(orchestrator.is_running().await)
}

#[tauri::command]
async fn list_plans(state: State<'_, SharedState>) -> Result<Vec<String>, String> {
    let workspace = {
        let guard = state.lock().await;
        guard
            .workspace_path
            .as_ref()
            .ok_or("No hay workspace abierto")?
            .clone()
    };

    let plans_dir = workspace.join(".tundracode/plans");
    if !plans_dir.exists() {
        return Ok(Vec::new());
    }

    let mut plans = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&plans_dir) {
        for entry in entries.flatten() {
            if entry.path().extension().and_then(|e| e.to_str()) == Some("md") {
                if let Some(name) = entry.file_name().to_str() {
                    plans.push(name.to_string());
                }
            }
        }
    }
    plans.sort();
    Ok(plans)
}

#[tauri::command]
async fn load_plan(path: String, state: State<'_, SharedState>) -> Result<String, String> {
    let workspace = {
        let guard = state.lock().await;
        guard
            .workspace_path
            .as_ref()
            .ok_or("No hay workspace abierto")?
            .clone()
    };

    let plan_path = workspace.join(".tundracode/plans").join(&path);
    std::fs::read_to_string(&plan_path).map_err(|e| format!("Cannot read plan: {}", e))
}

#[tauri::command]
async fn read_memory(state: State<'_, SharedState>) -> Result<String, String> {
    let workspace = {
        let guard = state.lock().await;
        guard
            .workspace_path
            .as_ref()
            .ok_or("No hay workspace abierto")?
            .clone()
    };

    let memory_path = workspace.join(".tundracode/memory.md");
    std::fs::read_to_string(&memory_path).or_else(|_| Ok(String::new()))
}

#[tauri::command]
async fn write_memory(content: String, state: State<'_, SharedState>) -> Result<String, String> {
    let workspace = {
        let guard = state.lock().await;
        guard
            .workspace_path
            .as_ref()
            .ok_or("No hay workspace abierto")?
            .clone()
    };

    let memory_path = workspace.join(".tundracode/memory.md");
    if let Some(parent) = memory_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&memory_path, content).map_err(|e| format!("Cannot write memory: {}", e))?;
    Ok("Memory saved".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfigInput {
    pub agent_id: String,
    pub provider: String,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
}

#[tauri::command]
async fn save_agent_config(
    input: AgentConfigInput,
) -> Result<String, String> {
    let storage = tundracode_config::ConfigStorage::new()
        .map_err(|e| format!("Cannot open config storage: {}", e))?;
    let mut settings = storage.load()
        .map_err(|e| format!("Cannot load settings: {}", e))?;

    settings.agents.insert(input.agent_id.clone(), tundracode_config::AgentSettings {
        model: input.model,
        provider: input.provider,
        temperature: input.temperature,
        max_tokens: input.max_tokens,
    });

    storage.save(&settings)
        .map_err(|e| format!("Cannot save settings: {}", e))?;
    Ok("Agent config saved".to_string())
}

#[tauri::command]
async fn load_agent_configs() -> Result<std::collections::HashMap<String, tundracode_config::AgentSettings>, String> {
    let storage = tundracode_config::ConfigStorage::new()
        .map_err(|e| format!("Cannot open config storage: {}", e))?;
    let settings = storage.load()
        .map_err(|e| format!("Cannot load settings: {}", e))?;
    Ok(settings.agents)
}

#[tauri::command]
async fn save_last_workspace(
    path: String,
) -> Result<String, String> {
    let storage = tundracode_config::ConfigStorage::new()
        .map_err(|e| format!("Cannot open config storage: {}", e))?;
    let mut settings = storage.load()
        .map_err(|e| format!("Cannot load settings: {}", e))?;
    settings.last_workspace = Some(path);
    storage.save(&settings)
        .map_err(|e| format!("Cannot save settings: {}", e))?;
    Ok("Workspace saved".to_string())
}

#[tauri::command]
async fn load_last_workspace() -> Result<Option<String>, String> {
    let storage = tundracode_config::ConfigStorage::new()
        .map_err(|e| format!("Cannot open config storage: {}", e))?;
    let settings = storage.load()
        .map_err(|e| format!("Cannot load settings: {}", e))?;
    Ok(settings.last_workspace)
}



#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModelInfo {
    pub name: String,
    pub size: String,
    pub digest: String,
}

#[tauri::command]
async fn ollama_status() -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| format!("Failed to create client: {}", e))?;

    match client.get("http://localhost:11434/api/tags").send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                Ok("running".to_string())
            } else {
                Ok("stopped".to_string())
            }
        }
        Err(_) => Ok("not_available".to_string()),
    }
}

#[tauri::command]
async fn ollama_list_models() -> Result<Vec<OllamaModelInfo>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to create client: {}", e))?;

    let resp = client
        .get("http://localhost:11434/api/tags")
        .send()
        .await
        .map_err(|e| format!("Failed to connect to Ollama: {}", e))?;

    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let mut models = Vec::new();
    if let Some(models_array) = body.get("models").and_then(|m| m.as_array()) {
        for m in models_array {
            if let (Some(name), Some(size), Some(digest)) = (
                m.get("name").and_then(|v| v.as_str()),
                m.get("size").and_then(|v| v.as_u64()),
                m.get("digest").and_then(|v| v.as_str()),
            ) {
                let size_str = if size > 1_000_000_000 {
                    format!("{:.1} GB", size as f64 / 1_000_000_000.0)
                } else if size > 1_000_000 {
                    format!("{:.1} MB", size as f64 / 1_000_000.0)
                } else {
                    format!("{} KB", size / 1000)
                };
                models.push(OllamaModelInfo {
                    name: name.to_string(),
                    size: size_str,
                    digest: digest.to_string(),
                });
            }
        }
    }

    Ok(models)
}

#[tauri::command]
async fn ollama_pull_model(model: String) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| format!("Failed to create client: {}", e))?;

    let body = serde_json::json!({
        "name": model,
        "stream": false
    });

    let resp = client
        .post("http://localhost:11434/api/pull")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to pull model: {}", e))?;

    let status = resp.status();
    if !status.is_success() {
        let error_body = resp.text().await.unwrap_or_default();
        return Err(format!("Ollama pull failed ({}): {}", status, error_body));
    }

    Ok(format!("Model '{}' pulled successfully", model))
}

#[tauri::command]
async fn ollama_start_runtime() -> Result<String, String> {
    let ollama_bin = std::env::var("OLLAMA_BIN").unwrap_or_else(|_| "ollama".to_string());

    let status = tokio::process::Command::new(&ollama_bin)
        .arg("serve")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();

    match status {
        Ok(_) => Ok("Ollama runtime started".to_string()),
        Err(e) => Err(format!(
            "Failed to start Ollama: {}. Make sure Ollama is installed.",
            e
        )),
    }
}

#[tauri::command]
async fn ollama_stop_runtime() -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| format!("Failed to create client: {}", e))?;

    match client
        .post("http://localhost:11434/api/shutdown")
        .send()
        .await
    {
        Ok(_) => Ok("Ollama runtime stopped".to_string()),
        Err(e) => Err(format!("Failed to stop Ollama: {}", e)),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreeModelStatus {
    pub model: String,
    pub available: bool,
    pub latency_ms: u32,
    pub error: Option<String>,
}

#[tauri::command]
async fn get_keyring_status() -> Result<String, String> {
    let entry = keyring::Entry::new("tundracode", "test_entry")
        .map_err(|e| format!("Keyring init failed: {}", e))?;
    match entry.get_password() {
        Ok(_) => Ok("available".to_string()),
        Err(keyring::Error::NoEntry) => Ok("available".to_string()),
        Err(_) => {
            match entry.set_password("test") {
                Ok(_) => {
                    entry.delete_password().ok();
                    Ok("available".to_string())
                }
                Err(_) => Ok("unavailable".to_string()),
            }
        }
    }
}

#[tauri::command]
async fn get_free_models_status() -> Result<Vec<FreeModelStatus>, String> {
    let free_models = vec![
        "big-pickle",
        "deepseek-v4-flash-free",
        "mimo-v2.5-free",
        "nemotron-3-ultra-free",
    ];

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| format!("Failed to create client: {}", e))?;

    let mut results = Vec::new();

    for model in &free_models {
        let start = std::time::Instant::now();
        let body = serde_json::json!({
            "model": model,
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 1,
        });

        let resp = client
            .post("https://opencode.ai/zen/v1/chat/completions")
            .header("Content-Type", "application/json")
            .header("X-TundraCode-Client", "tundracode/0.1.0")
            .json(&body)
            .send()
            .await;

        let latency = start.elapsed().as_millis() as u32;

        match resp {
            Ok(r) => {
                if r.status().is_success() {
                    results.push(FreeModelStatus {
                        model: model.to_string(),
                        available: true,
                        latency_ms: latency,
                        error: None,
                    });
                } else {
                    let status = r.status();
                    let _err_text = r.text().await.unwrap_or_default();
                    let is_rate_limit = status == 429;
                    let is_unavailable = status == 404 || status == 503;
                    results.push(FreeModelStatus {
                        model: model.to_string(),
                        available: !is_unavailable,
                        latency_ms: latency,
                        error: if is_rate_limit {
                            Some("Rate limited".to_string())
                        } else if is_unavailable {
                            Some("Model unavailable".to_string())
                        } else {
                            Some(format!("Status {}", status))
                        },
                    });
                }
            }
            Err(e) => {
                results.push(FreeModelStatus {
                    model: model.to_string(),
                    available: false,
                    latency_ms: latency,
                    error: Some(e.to_string()),
                });
            }
        }
    }

    Ok(results)
}

fn detect_language(path: &std::path::Path) -> Option<String> {
    let ext = path.extension()?.to_string_lossy().to_string();
    match ext.as_str() {
        "rs" => Some("rust".to_string()),
        "js" | "jsx" => Some("javascript".to_string()),
        "ts" | "tsx" => Some("typescript".to_string()),
        "py" => Some("python".to_string()),
        "java" => Some("java".to_string()),
        "go" => Some("go".to_string()),
        "c" | "h" => Some("c".to_string()),
        "cpp" | "hpp" | "cc" => Some("cpp".to_string()),
        "html" => Some("html".to_string()),
        "css" => Some("css".to_string()),
        "json" => Some("json".to_string()),
        "md" => Some("markdown".to_string()),
        "toml" => Some("toml".to_string()),
        "yaml" | "yml" => Some("yaml".to_string()),
        _ => None,
    }
}

#[tauri::command]
async fn pick_directory(handle: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let result = handle.dialog().file().blocking_pick_folder();
    match result {
        Some(file_path) => {
            let path = file_path.into_path().map_err(|e| e.to_string())?;
            Ok(Some(path.to_string_lossy().to_string()))
        }
        None => Ok(None),
    }
}



#[tauri::command]
async fn get_providers() -> Result<Vec<ProviderInfo>, String> {
    Ok(get_all_providers())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfigInput {
    pub provider_id: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

#[tauri::command]
async fn save_provider_config(input: ProviderConfigInput) -> Result<String, String> {
    tracing::info!(
        "save_provider_config: provider={}, api_key_len={}, base_url={}",
        input.provider_id,
        input.api_key.as_ref().map(|k| k.len()).unwrap_or(0),
        input.base_url.as_deref().unwrap_or("(none)")
    );

    let _provider = get_provider_by_id(&input.provider_id)
        .ok_or(format!("Provider not found: {}", input.provider_id))?;

    let mut keyring_ok = true;
    let mut fallback_ok = false;

    if let Some(base_url) = &input.base_url {
        if !base_url.is_empty() {
            save_provider_base_url(&input.provider_id, base_url).await?;
        }
    }

    if let Some(api_key) = &input.api_key {
        if api_key.is_empty() {
            delete_provider_api_key(&input.provider_id).ok();
        } else {
            match set_api_key_in_keyring(&input.provider_id, api_key) {
                Ok(_) => {
                    tracing::info!("API key saved to keyring for provider {}", input.provider_id);
                }
                Err(e) => {
                    tracing::warn!(
                        "Keyring unavailable for {} ({}), falling back to file",
                        input.provider_id,
                        e
                    );
                    keyring_ok = false;
                    match save_provider_fallback(&input.provider_id, api_key, input.base_url.as_deref()).await {
                        Ok(_) => {
                            fallback_ok = true;
                            tracing::info!("API key saved to fallback file for provider {}", input.provider_id);
                        }
                        Err(fb_err) => {
                            tracing::error!(
                                "Failed to save API key for {} - keyring: {}, fallback: {}",
                                input.provider_id,
                                e,
                                fb_err
                            );
                            return Err(format!(
                                "Failed to save API key: keyring unavailable ({}) and fallback failed ({})",
                                e, fb_err
                            ));
                        }
                    }
                }
            }
        }
    }

    if !keyring_ok && fallback_ok {
        Ok("Config saved (keyring unavailable, stored in plaintext)".to_string())
    } else {
        Ok("Config saved".to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfigOutput {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

#[tauri::command]
async fn get_provider_config_cmd(provider_id: String) -> Result<ProviderConfigOutput, String> {
    let api_key = tundracode_models::credentials::get_api_key(&provider_id)
        .ok()
        .flatten();

    let base_url = match read_provider_fallback(&provider_id).await {
        Ok(config) => config.base_url,
        Err(_) => None,
    };

    Ok(ProviderConfigOutput { api_key, base_url })
}

#[tauri::command]
async fn delete_provider_api_key_cmd(provider_id: String) -> Result<String, String> {
    delete_provider_api_key(&provider_id)
        .map_err(|e| format!("Failed to delete API key: {}", e))?;
    delete_provider_fallback(&provider_id).await.ok();
    Ok("API key deleted".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestConnectionResult {
    pub success: bool,
    pub message: String,
    pub latency_ms: Option<u64>,
}

#[tauri::command]
async fn test_provider_connection(
    provider_id: String,
    api_key: String,
    base_url: Option<String>,
) -> Result<TestConnectionResult, String> {
    let provider =
        get_provider_by_id(&provider_id).ok_or(format!("Provider not found: {}", provider_id))?;

    let url = base_url
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| provider.base_url.clone());

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let start = std::time::Instant::now();

    let request = match provider_id.as_str() {
        "anthropic" => {
            let models_url = format!("{}/v1/models", url.trim_end_matches('/'));
            client
                .get(&models_url)
                .header("x-api-key", &api_key)
                .header("anthropic-version", "2023-06-01")
        }
        "google" => {
            client
                .get(format!(
                    "{}/v1beta/models?key={}",
                    url.trim_end_matches('/'),
                    api_key
                ))
        }
        _ => {
            let models_url = if url.ends_with("/models") || url.ends_with("/v1") {
                format!("{}/models", url.trim_end_matches('/'))
            } else {
                format!("{}/v1/models", url.trim_end_matches('/'))
            };
            client
                .get(&models_url)
                .bearer_auth(&api_key)
        }
    };

    let response = request
        .send()
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    let latency = start.elapsed().as_millis() as u64;
    let status = response.status();

    if status.is_success() {
        Ok(TestConnectionResult {
            success: true,
            message: format!("Connected to {} successfully", provider.name),
            latency_ms: Some(latency),
        })
    } else {
        let body = response.text().await.unwrap_or_default();
        let error_msg = if body.contains("Invalid API key") || body.contains("authentication") {
            "Invalid API key".to_string()
        } else if status.as_u16() == 429 {
            "Rate limited".to_string()
        } else {
            format!("HTTP {}: {}", status, &body[..body.len().min(200)])
        };
        Ok(TestConnectionResult {
            success: false,
            message: error_msg,
            latency_ms: Some(latency),
        })
    }
}

#[tauri::command]
async fn fetch_provider_models(provider_id: String) -> Result<Vec<ProviderModel>, String> {
    let provider =
        get_provider_by_id(&provider_id).ok_or(format!("Provider not found: {}", provider_id))?;

    let models_endpoint = provider.models_endpoint.ok_or(format!(
        "Provider {} does not support dynamic model listing",
        provider.name
    ))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let mut request = client.get(&models_endpoint);

    if provider.api_key_required {
        if let Ok(api_key) = get_api_key_from_keyring(&provider_id) {
            if provider_id == "anthropic" {
                request = request.header("x-api-key", &api_key);
                request = request.header("anthropic-version", "2023-06-01");
            } else if provider_id == "google" {
                request = request.query(&[("key", &api_key)]);
            } else {
                request = request.bearer_auth(&api_key);
            }
        }
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("Failed to fetch models: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("Provider returned error: {}", status));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let models = parse_models_response(&provider_id, &body);
    Ok(models)
}

fn parse_models_response(provider_id: &str, body: &serde_json::Value) -> Vec<ProviderModel> {
    match provider_id {
        "openai" | "alibaba" | "kimi" => {
            if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
                data.iter()
                    .filter_map(|m| {
                        let id = m.get("id").and_then(|v| v.as_str())?.to_string();
                        let name = m
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&id)
                            .to_string();
                        Some(ProviderModel {
                            id,
                            name,
                            description: None,
                        })
                    })
                    .collect()
            } else {
                Vec::new()
            }
        }
        "opencode-free" => {
            if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
                data.iter()
                    .filter_map(|m| {
                        let id = m.get("id").and_then(|v| v.as_str())?.to_string();
                        if !id.ends_with("-free") {
                            return None;
                        }
                        let name = m
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&id)
                            .to_string();
                        Some(ProviderModel {
                            id,
                            name,
                            description: None,
                        })
                    })
                    .collect()
            } else {
                Vec::new()
            }
        }
        "opencode-zen" | "opencode-go" => {
            if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
                data.iter()
                    .filter_map(|m| {
                        let id = m.get("id").and_then(|v| v.as_str())?.to_string();
                        if id.ends_with("-free") {
                            return None;
                        }
                        let name = m
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&id)
                            .to_string();
                        Some(ProviderModel {
                            id,
                            name,
                            description: None,
                        })
                    })
                    .collect()
            } else {
                Vec::new()
            }
        }
        "anthropic" => {
            if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
                data.iter()
                    .filter_map(|m| {
                        let id = m.get("id").and_then(|v| v.as_str())?.to_string();
                        let name = m
                            .get("display_name")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&id)
                            .to_string();
                        Some(ProviderModel {
                            id,
                            name,
                            description: None,
                        })
                    })
                    .collect()
            } else {
                Vec::new()
            }
        }
        "google" => {
            if let Some(models) = body.get("models").and_then(|m| m.as_array()) {
                models
                    .iter()
                    .filter_map(|m| {
                        let full_name = m.get("name").and_then(|v| v.as_str())?.to_string();
                        let id = full_name
                            .strip_prefix("models/")
                            .unwrap_or(&full_name)
                            .to_string();
                        let name = m
                            .get("displayName")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&id)
                            .to_string();
                        Some(ProviderModel {
                            id,
                            name,
                            description: None,
                        })
                    })
                    .collect()
            } else {
                Vec::new()
            }
        }
        _ => {
            if let Some(models) = body.get("models").and_then(|m| m.as_array()) {
                models
                    .iter()
                    .filter_map(|m| {
                        let id = m.get("id").and_then(|v| v.as_str())?.to_string();
                        let name = m
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&id)
                            .to_string();
                        Some(ProviderModel {
                            id,
                            name,
                            description: None,
                        })
                    })
                    .collect()
            } else {
                Vec::new()
            }
        }
    }
}

fn set_api_key_in_keyring(provider_id: &str, api_key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new("tundracode", &format!("{}_api_key", provider_id))
        .map_err(|e| format!("Keyring error: {}", e))?;
    entry
        .set_password(api_key)
        .map_err(|e| format!("Failed to set password: {}", e))
}

fn get_api_key_from_keyring(provider_id: &str) -> Result<String, String> {
    let entry = keyring::Entry::new("tundracode", &format!("{}_api_key", provider_id))
        .map_err(|e| format!("Keyring error: {}", e))?;
    entry
        .get_password()
        .map_err(|e| format!("Failed to get password: {}", e))
}

fn delete_provider_api_key(provider_id: &str) -> Result<(), String> {
    let entry = keyring::Entry::new("tundracode", &format!("{}_api_key", provider_id))
        .map_err(|e| format!("Keyring error: {}", e))?;
    entry
        .delete_password()
        .map_err(|e| format!("Failed to delete password: {}", e))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FallbackProviderConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

async fn save_provider_fallback(
    provider_id: &str,
    api_key: &str,
    base_url: Option<&str>,
) -> Result<(), String> {
    let config_dir = dirs::config_dir()
        .ok_or("Cannot find config directory".to_string())?
        .join("tundracode");
    tokio::fs::create_dir_all(&config_dir)
        .await
        .map_err(|e| format!("Failed to create config dir: {}", e))?;

    let config_path = config_dir.join("providers.json");
    let mut configs: std::collections::HashMap<String, FallbackProviderConfig> =
        if config_path.exists() {
            let content = tokio::fs::read_to_string(&config_path)
                .await
                .unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            std::collections::HashMap::new()
        };

    let entry = configs
        .entry(provider_id.to_string())
        .or_insert_with(|| FallbackProviderConfig {
            api_key: None,
            base_url: None,
        });
    entry.api_key = Some(api_key.to_string());
    if let Some(url) = base_url {
        entry.base_url = Some(url.to_string());
    }

    let content = serde_json::to_string_pretty(&configs)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    tokio::fs::write(&config_path, content)
        .await
        .map_err(|e| format!("Failed to write config: {}", e))
}

async fn read_provider_fallback(provider_id: &str) -> Result<FallbackProviderConfig, String> {
    let config_path = dirs::config_dir()
        .ok_or("Cannot find config directory".to_string())?
        .join("tundracode")
        .join("providers.json");

    if !config_path.exists() {
        return Err("Config file not found".to_string());
    }

    let content = tokio::fs::read_to_string(&config_path)
        .await
        .map_err(|e| format!("Failed to read config: {}", e))?;
    let configs: std::collections::HashMap<String, FallbackProviderConfig> =
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse config: {}", e))?;

    configs
        .get(provider_id)
        .cloned()
        .ok_or_else(|| "Provider not found in config".to_string())
}

async fn delete_provider_fallback(provider_id: &str) -> Result<(), String> {
    let config_path = dirs::config_dir()
        .ok_or("Cannot find config directory".to_string())?
        .join("tundracode")
        .join("providers.json");

    if !config_path.exists() {
        return Ok(());
    }

    let content = tokio::fs::read_to_string(&config_path)
        .await
        .map_err(|e| format!("Failed to read config: {}", e))?;
    let mut configs: std::collections::HashMap<String, FallbackProviderConfig> =
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse config: {}", e))?;

    configs.remove(provider_id);

    let new_content = serde_json::to_string_pretty(&configs)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    tokio::fs::write(&config_path, new_content)
        .await
        .map_err(|e| format!("Failed to write config: {}", e))
}

async fn save_provider_base_url(provider_id: &str, base_url: &str) -> Result<(), String> {
    let config_dir = dirs::config_dir()
        .ok_or("Cannot find config directory".to_string())?
        .join("tundracode");
    tokio::fs::create_dir_all(&config_dir)
        .await
        .map_err(|e| format!("Failed to create config dir: {}", e))?;

    let config_path = config_dir.join("providers.json");
    let mut configs: std::collections::HashMap<String, FallbackProviderConfig> =
        if config_path.exists() {
            let content = tokio::fs::read_to_string(&config_path)
                .await
                .unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            std::collections::HashMap::new()
        };

    let entry = configs
        .entry(provider_id.to_string())
        .or_insert_with(|| FallbackProviderConfig {
            api_key: None,
            base_url: None,
        });
    entry.base_url = Some(base_url.to_string());

    let content = serde_json::to_string_pretty(&configs)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    tokio::fs::write(&config_path, content)
        .await
        .map_err(|e| format!("Failed to write config: {}", e))
}



#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResult {
    pub content: String,
    pub tokens_used: u32,
    pub tool_log: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentChunkPayload {
    pub run_id: String,
    pub chunk: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentToolCallPayload {
    pub run_id: String,
    pub tool_name: String,
    pub call_id: String,
    pub file_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDonePayload {
    pub run_id: String,
    pub tokens_used: u32,
    pub finish_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentErrorPayload {
    pub run_id: String,
    pub error: String,
}

#[tauri::command]
async fn send_completion(
    run_id: String,
    provider_id: String,
    model_id: String,
    messages: Vec<ChatMessage>,
    system_prompt: Option<String>,
    state: State<'_, SharedState>,
    app_handle: AppHandle,
) -> Result<CompletionResult, String> {
    let workspace = {
        let guard = state.lock().await;
        guard
            .workspace_path
            .as_ref()
            .ok_or("No hay workspace abierto")?
            .clone()
    };

    let provider =
        get_provider_by_id(&provider_id).ok_or(format!("Provider not found: {}", provider_id))?;

    let api_key = tundracode_models::credentials::get_api_key(&provider_id)
        .ok()
        .flatten();

    let base_url = tundracode_models::credentials::get_base_url(&provider_id)
        .ok()
        .flatten()
        .unwrap_or_else(|| provider.base_url.clone());

    let model_config = ModelConfig {
        provider: provider_id.clone(),
        model: model_id,
        api_key,
        base_url: Some(base_url),
        temperature: 0.7,
        max_tokens: 4096,
    };

    let registry = ProviderRegistry::new();
    if !registry.has(&provider_id) {
        return Err(format!("Provider not registered: {}", provider_id));
    }

    let mut tool_registry = ToolRegistry::new();
    tool_registry.register_all_default();

    let tool_definitions: Vec<ToolDefinition> = tool_registry
        .list_tools()
        .iter()
        .filter_map(|name| {
            tool_registry.get(name).map(|tool| ToolDefinition {
                name: tool.name().to_string(),
                description: tool.description().to_string(),
                parameters: tool.parameters_schema(),
            })
        })
        .collect();

    let tool_context = ToolContext {
        workspace_path: workspace.to_string_lossy().to_string(),
        agent_id: "chat".to_string(),
    };

    let last_user = messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.clone())
        .unwrap_or_default();

    let system = system_prompt.unwrap_or_else(|| {
        "Eres un asistente de coding que usa herramientas para leer y modificar archivos. \
         Cuando hagas cambios, usa WriteFile/CreateFile/ApplyPatch. \
         Responde de forma concisa y explica que herramientas usaste."
            .to_string()
    });

    let (tx_chunk, mut rx_chunk) = mpsc::channel::<String>(64);
    let (tx_tool, mut rx_tool) = mpsc::channel::<ToolInvocation>(32);

    let app_for_chunks = app_handle.clone();
    let run_id_for_chunks = run_id.clone();
    let chunk_task = tokio::spawn(async move {
        while let Some(chunk) = rx_chunk.recv().await {
            let _ = app_for_chunks.emit(
                "agent-chunk",
                AgentChunkPayload {
                    run_id: run_id_for_chunks.clone(),
                    chunk,
                },
            );
        }
    });

    let app_for_tools = app_handle.clone();
    let run_id_for_tools = run_id.clone();
    let tool_task = tokio::spawn(async move {
        while let Some(inv) = rx_tool.recv().await {
            let _ = app_for_tools.emit(
                "agent-tool-call",
                AgentToolCallPayload {
                    run_id: run_id_for_tools.clone(),
                    tool_name: inv.tool_name.clone(),
                    call_id: inv.call_id.clone(),
                    file_path: inv.file_path.clone(),
                },
            );
        }
    });

    let agent_loop = AgentLoop::new().with_max_iterations(8);
    let run_config = RunConfig {
        provider_registry: &registry,
        tool_registry: &tool_registry,
        tool_context: &tool_context,
        provider_id: &provider_id,
        model_config: &model_config,
        system_prompt: &system,
        user_message: &last_user,
        tools: &tool_definitions,
    };

    let result = agent_loop.run(run_config).await;
    drop(tx_chunk);
    drop(tx_tool);
    let _ = chunk_task.await;
    let _ = tool_task.await;

    let output = match result {
        Ok(out) => out,
        Err(e) => {
            let _ = app_handle.emit(
                "agent-error",
                AgentErrorPayload {
                    run_id: run_id.clone(),
                    error: e.to_string(),
                },
            );
            return Err(format!("Completion failed: {}", e));
        }
    };

    let _ = app_handle.emit(
        "agent-done",
        AgentDonePayload {
            run_id: run_id.clone(),
            tokens_used: output.tokens_used,
            finish_reason: "stop".to_string(),
        },
    );

    let tool_log: Vec<String> = output
        .invocations
        .iter()
        .map(|inv| {
            format!(
                "Tool: {} | {} | call_id={} | args={}",
                inv.tool_name,
                if inv.success { "ok" } else { "err" },
                inv.call_id,
                inv.arguments
            )
        })
        .collect();

    Ok(CompletionResult {
        content: output.content,
        tokens_used: output.tokens_used,
        tool_log,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildResult {
    pub proposals: Vec<tundracode_agents::DiffProposal>,
    pub tool_log: Vec<String>,
}

#[tauri::command]
async fn run_build_agent(
    run_id: String,
    plan_description: String,
    plan_annotations: Option<String>,
    provider_id: String,
    model_id: String,
    state: State<'_, SharedState>,
    orchestrator: State<'_, Arc<AgentOrchestrator>>,
    app_handle: AppHandle,
) -> Result<BuildResult, String> {
    let workspace = {
        let guard = state.lock().await;
        guard
            .workspace_path
            .as_ref()
            .ok_or("No hay workspace abierto")?
            .clone()
    };

    let api_key = tundracode_models::credentials::get_api_key(&provider_id)
        .ok()
        .flatten();

    let provider =
        get_provider_by_id(&provider_id).ok_or(format!("Provider not found: {}", provider_id))?;

    let base_url = tundracode_models::credentials::get_base_url(&provider_id)
        .ok()
        .flatten()
        .unwrap_or_else(|| provider.base_url.clone());

    let per_agent = load_agent_settings_for("build").await.ok().flatten();
    let (temperature, max_tokens) = match per_agent {
        Some(s) => (s.temperature, s.max_tokens),
        None => (0.2, 8192),
    };

    let model_config = ModelConfig {
        provider: provider_id.clone(),
        model: model_id,
        api_key,
        base_url: Some(base_url),
        temperature,
        max_tokens,
    };

    let context = AgentContext {
        workspace_path: workspace.to_string_lossy().to_string(),
        model_config,
        autonomous_mode: false,
        budget_tokens: 200_000,
    };

    let input = AgentInput {
        user_message: plan_description,
        plan_annotations,
        memory_excerpt: None,
    };

    let cancel = CancellationToken::new();
    *orchestrator.cancel_token.write().await = Some(cancel.clone());
    *orchestrator.running.write().await = true;

    let agent = BuildAgent;
    let run_id_for_task = run_id.clone();
    let app_for_task = app_handle.clone();
    let result: anyhow::Result<AgentOutput> = tokio::spawn(async move {
        tokio::select! {
            output = agent.run(&context, input) => output,
            _ = cancel.cancelled() => {
                let _ = app_for_task.emit(
                    "agent-error",
                    AgentErrorPayload {
                        run_id: run_id_for_task,
                        error: "Build agent cancelled".to_string(),
                    },
                );
                Err(anyhow::anyhow!("Build agent cancelled"))
            }
        }
    })
    .await
    .map_err(|e| format!("Build task join failed: {}", e))?;

    *orchestrator.running.write().await = false;

    let output = result.map_err(|e| format!("Build agent failed: {}", e))?;

    let proposals = match output {
        AgentOutput::ProposedChanges { proposals, tool_log, invocations } => {
            persist_proposals(&proposals, &workspace).await.ok();
            for inv in &invocations {
                let _ = app_handle.emit(
                    "agent-tool-call",
                    AgentToolCallPayload {
                        run_id: run_id.clone(),
                        tool_name: inv.tool_name.clone(),
                        call_id: inv.call_id.clone(),
                        file_path: inv.file_path.clone(),
                    },
                );
            }
            let _ = app_handle.emit(
                "agent-done",
                AgentDonePayload {
                    run_id: run_id.clone(),
                    tokens_used: tool_log.len() as u32,
                    finish_reason: "stop".to_string(),
                },
            );
            BuildResult { proposals, tool_log }
        }
        AgentOutput::Error(e) => return Err(format!("Build agent error: {}", e)),
        _ => return Err("Unexpected output from build agent".to_string()),
    };

    Ok(proposals)
}

#[tauri::command]
async fn accept_diff(
    proposal_id: String,
    state: State<'_, SharedState>,
) -> Result<String, String> {
    let workspace = {
        let guard = state.lock().await;
        guard
            .workspace_path
            .as_ref()
            .ok_or("No hay workspace abierto")?
            .clone()
    };

    let proposals_dir = dirs::config_dir()
        .ok_or("Cannot find config directory")?
        .join("tundracode")
        .join("proposals");

    let proposal_path = proposals_dir.join(format!("{}.json", proposal_id));
    if !proposal_path.exists() {
        return Err(format!("Proposal not found: {}", proposal_id));
    }

    let content = tokio::fs::read_to_string(&proposal_path)
        .await
        .map_err(|e| format!("Failed to read proposal: {}", e))?;

    let proposal: tundracode_agents::DiffProposal =
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse proposal: {}", e))?;

    let tool_registry = ToolRegistry::new();
    let tool_context = ToolContext {
        workspace_path: workspace.to_string_lossy().to_string(),
        agent_id: "user".to_string(),
    };

    let params = match proposal.unified_diff.is_empty() {
        true => serde_json::json!({
            "path": proposal.file_path,
            "content": proposal.after
        }),
        false => serde_json::json!({
            "path": proposal.file_path,
            "diff": proposal.unified_diff
        }),
    };

    let tool_name = match proposal.unified_diff.is_empty() {
        true => "WriteFile",
        false => "ApplyPatch",
    };

    tool_registry
        .execute(&tool_context, tool_name, params)
        .await
        .map_err(|e| format!("Failed to apply: {}", e))?;

    tokio::fs::remove_file(&proposal_path)
        .await
        .map_err(|e| format!("Failed to remove proposal: {}", e))?;

    Ok(format!("Applied changes to {}", proposal.file_path))
}

#[tauri::command]
async fn reject_diff(proposal_id: String) -> Result<String, String> {
    let proposals_dir = dirs::config_dir()
        .ok_or("Cannot find config directory")?
        .join("tundracode")
        .join("proposals");

    let proposal_path = proposals_dir.join(format!("{}.json", proposal_id));
    if proposal_path.exists() {
        tokio::fs::remove_file(&proposal_path)
            .await
            .map_err(|e| format!("Failed to remove proposal: {}", e))?;
    }

    Ok("Proposal rejected and removed".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanComment {
    pub id: String,
    pub plan_path: String,
    pub line: u32,
    pub author: String,
    pub body: String,
    pub created_at: String,
    pub resolved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffComment {
    pub id: String,
    pub proposal_id: String,
    pub file_path: String,
    pub line: u32,
    pub author: String,
    pub body: String,
    pub created_at: String,
    pub resolved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddCommentInput {
    pub target_path: String,
    pub line: u32,
    pub body: String,
    pub author: Option<String>,
}

fn tundracode_home() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from(".")))
        .join("tundracode")
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn new_comment_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("cmt_{}", nanos)
}

async fn read_comments_file<T: for<'de> Deserialize<'de>>(file: &std::path::Path) -> Vec<T> {
    if !file.exists() {
        return Vec::new();
    }
    match tokio::fs::read_to_string(file).await {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

async fn write_comments_file<T: Serialize>(file: &std::path::Path, items: &[T]) -> Result<(), String> {
    if let Some(parent) = file.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Cannot create comments dir: {}", e))?;
    }
    let content = serde_json::to_string_pretty(items)
        .map_err(|e| format!("Cannot serialize comments: {}", e))?;
    tokio::fs::write(file, content)
        .await
        .map_err(|e| format!("Cannot write comments: {}", e))
}

#[tauri::command]
async fn add_plan_comment(
    input: AddCommentInput,
    state: State<'_, SharedState>,
) -> Result<PlanComment, String> {
    let _ = state; // workspace is implicit (comments live in ~/.tundracode)
    let file = tundracode_home().join("plan_comments.json");
    let mut comments: Vec<PlanComment> = read_comments_file(&file).await;

    let c = PlanComment {
        id: new_comment_id(),
        plan_path: input.target_path,
        line: input.line,
        author: input.author.unwrap_or_else(|| "user".to_string()),
        body: input.body,
        created_at: now_iso(),
        resolved: false,
    };
    comments.push(c.clone());
    write_comments_file(&file, &comments).await?;
    Ok(c)
}

#[tauri::command]
async fn list_plan_comments(
    plan_path: String,
) -> Result<Vec<PlanComment>, String> {
    let file = tundracode_home().join("plan_comments.json");
    let comments: Vec<PlanComment> = read_comments_file(&file).await;
    Ok(comments
        .into_iter()
        .filter(|c| c.plan_path == plan_path)
        .collect())
}

#[tauri::command]
async fn resolve_plan_comment(id: String) -> Result<String, String> {
    let file = tundracode_home().join("plan_comments.json");
    let mut comments: Vec<PlanComment> = read_comments_file(&file).await;
    if let Some(c) = comments.iter_mut().find(|c| c.id == id) {
        c.resolved = true;
        write_comments_file(&file, &comments).await?;
        Ok("Comment resolved".to_string())
    } else {
        Err(format!("Comment not found: {}", id))
    }
}

#[tauri::command]
async fn add_diff_comment(
    proposal_id: String,
    file_path: String,
    line: u32,
    body: String,
    author: Option<String>,
) -> Result<DiffComment, String> {
    let file = tundracode_home().join("diff_comments.json");
    let mut comments: Vec<DiffComment> = read_comments_file(&file).await;

    let c = DiffComment {
        id: new_comment_id(),
        proposal_id,
        file_path,
        line,
        author: author.unwrap_or_else(|| "user".to_string()),
        body,
        created_at: now_iso(),
        resolved: false,
    };
    comments.push(c.clone());
    write_comments_file(&file, &comments).await?;
    Ok(c)
}

#[tauri::command]
async fn list_diff_comments(proposal_id: String) -> Result<Vec<DiffComment>, String> {
    let file = tundracode_home().join("diff_comments.json");
    let comments: Vec<DiffComment> = read_comments_file(&file).await;
    Ok(comments
        .into_iter()
        .filter(|c| c.proposal_id == proposal_id)
        .collect())
}

#[tauri::command]
async fn resolve_diff_comment(id: String) -> Result<String, String> {
    let file = tundracode_home().join("diff_comments.json");
    let mut comments: Vec<DiffComment> = read_comments_file(&file).await;
    if let Some(c) = comments.iter_mut().find(|c| c.id == id) {
        c.resolved = true;
        write_comments_file(&file, &comments).await?;
        Ok("Comment resolved".to_string())
    } else {
        Err(format!("Comment not found: {}", id))
    }
}

async fn persist_proposals(
    proposals: &[tundracode_agents::DiffProposal],
    workspace: &std::path::Path,
) -> Result<(), String> {
    let dir = tundracode_home().join("proposals");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("Cannot create proposals dir: {}", e))?;
    let _ = workspace; // not used in path, but kept for future per-workspace storage
    for p in proposals {
        let file = dir.join(format!("{}.json", p.id));
        let content = serde_json::to_string_pretty(p)
            .map_err(|e| format!("Cannot serialize proposal: {}", e))?;
        tokio::fs::write(&file, content)
            .await
            .map_err(|e| format!("Cannot write proposal: {}", e))?;
    }
    Ok(())
}

async fn load_agent_settings_for(
    agent_id: &str,
) -> Result<Option<tundracode_config::AgentSettings>, String> {
    let storage = tundracode_config::ConfigStorage::new()
        .map_err(|e| format!("Cannot open config storage: {}", e))?;
    let settings = storage
        .load()
        .map_err(|e| format!("Cannot load settings: {}", e))?;
    Ok(settings.agents.get(agent_id).cloned())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt::init();

    let orchestrator = Arc::new(AgentOrchestrator::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Arc::new(Mutex::new(AppState::default())) as SharedState)
        .manage(orchestrator)
        .setup(|app| {
            #[cfg(target_os = "linux")]
            {
                if let Some(window) = app.get_webview_window("main") {
                    if is_wayland_session() {
                        let _ = window.set_decorations(false);
                        tracing::info!("Running on Wayland - decorations disabled");
                    } else {
                        let _ = window.set_decorations(true);
                        tracing::info!("Running on X11 - decorations enabled");
                    }
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_workspace,
            get_workspace,
            pick_directory,
            list_directory,
            read_file,
            write_file,
            get_git_status,
            git_stage,
            git_commit,
            get_lsp_status,
            detect_lsp_servers,
            get_window_info,
            run_agent_ask,
            generate_plan,
            run_build_agent,
            cancel_agent,
            agent_status,
            list_plans,
            load_plan,
            read_memory,
            write_memory,
            get_providers,
            save_provider_config,
            get_provider_config_cmd,
            delete_provider_api_key_cmd,
            test_provider_connection,
            fetch_provider_models,
            send_completion,
            save_agent_config,
            load_agent_configs,
            save_last_workspace,
            load_last_workspace,
            ollama_status,
            ollama_list_models,
            ollama_pull_model,
            ollama_start_runtime,
            ollama_stop_runtime,
            get_keyring_status,
            get_free_models_status,
            accept_diff,
            reject_diff,
            add_plan_comment,
            list_plan_comments,
            resolve_plan_comment,
            add_diff_comment,
            list_diff_comments,
            resolve_diff_comment,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
