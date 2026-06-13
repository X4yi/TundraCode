mod credentials;
mod orchestrator;
mod streaming;

use credentials::{
    delete_provider_api_key, delete_provider_fallback, get_api_key_from_keyring,
    read_provider_fallback, save_provider_base_url, save_provider_fallback,
    set_api_key_in_keyring,
};
use orchestrator::{AgentOrchestrator, BuildSession};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use streaming::{create_on_event, create_on_event_no_chunks, CompactedPayload, SubagentPayload};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;
use tundracode_agents::{
    r#loop::{AgentLoop, RunConfig},
    Agent, AgentContext, AgentInput, AgentOutput, AskAgent, BuildAgent, BuildConfig, BuildMode,
    DiffProposal, ParsedPlan, PlanAgent, TaskStore,
};
use tundracode_models::{
    get_all_providers, get_provider_by_id, lookup_model_context, ModelConfig, ProviderInfo,
    ProviderModel, ProviderRegistry, ToolDefinition,
};
use tundracode_tools::{ToolContext, ToolRegistry};

#[derive(Default)]
struct AppState {
    workspace_path: Option<std::path::PathBuf>,
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
    reasoning_effort: Option<String>,
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
    };

    let context = AgentContext {
        workspace_path: workspace.to_string_lossy().to_string(),
        model_config,
        build_mode: BuildMode::ReviewRequired,
        budget_tokens: u32::MAX,
        reasoning_effort,
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
    run_id: String,
    description: String,
    provider_id: String,
    model_id: String,
    reasoning_effort: Option<String>,
    state: State<'_, SharedState>,
    orchestrator: State<'_, Arc<AgentOrchestrator>>,
    app_handle: AppHandle,
) -> Result<serde_json::Value, String> {
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
    };

    let context = AgentContext {
        workspace_path: workspace.to_string_lossy().to_string(),
        model_config,
        build_mode: BuildMode::ReviewRequired,
        budget_tokens: u32::MAX,
        reasoning_effort,
    };

    let description_clone = description.clone();
    let input = AgentInput {
        user_message: description,
        plan_annotations: None,
        memory_excerpt: None,
    };

    let agent = PlanAgent;
    let run_id_for_task = run_id.clone();
    let app_for_task = app_handle.clone();

    let streaming = create_on_event_no_chunks(run_id.clone(), app_handle.clone());
    let on_event = streaming;

    let plans_dir = workspace.join(".tundracode/plans");
    let _ = tokio::fs::create_dir_all(&plans_dir).await;

    let pre_existing_files: std::collections::HashSet<String> = {
        let mut files = std::collections::HashSet::new();
        if let Ok(entries) = std::fs::read_dir(&plans_dir) {
            for entry in entries.flatten() {
                if let Ok(name) = entry.file_name().into_string() {
                    if name.ends_with(".md") {
                        files.insert(name);
                    }
                }
            }
        }
        files
    };

    let result: anyhow::Result<AgentOutput> = tokio::spawn(async move {
        tokio::select! {
            output = agent.run_with_streaming(&context, input, Some(Box::new(on_event))) => output,
            _ = cancel.cancelled() => {
                let _ = app_for_task.emit(
                    "agent-error",
                    AgentErrorPayload {
                        run_id: run_id_for_task,
                        error: "Plan agent cancelled".to_string(),
                    },
                );
                Err(anyhow::anyhow!("Plan agent cancelled"))
            }
        }
    })
    .await
    .map_err(|e| format!("Plan task join failed: {}", e))?;

    *orchestrator.running.write().await = false;

    match result {
        Ok(AgentOutput::FinalAnswer { content, tokens_used }) => {
            let mut title = content
                .lines()
                .find(|l| l.trim().starts_with('#'))
                .map(|l| l.trim_start_matches('#').trim().to_string())
                .unwrap_or_else(|| description_clone.chars().take(60).collect());

            let summary = content
                .lines()
                .skip_while(|l| l.trim().starts_with('#') || l.trim().is_empty() || *l == "---")
                .take(3)
                .collect::<Vec<_>>()
                .join(" ");

            let mut file_path: Option<String> = None;
            let mut plan_content = content.clone();

            if let Ok(entries) = std::fs::read_dir(&plans_dir) {
                let mut newest: Option<(std::time::SystemTime, std::path::PathBuf)> = None;
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("md") {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if pre_existing_files.contains(&name) {
                            continue;
                        }
                        if let Ok(meta) = entry.metadata() {
                            if let Ok(modified) = meta.modified() {
                                let is_newer = match &newest {
                                    None => true,
                                    Some((existing_time, _)) => modified.duration_since(*existing_time).is_ok(),
                                };
                                if is_newer {
                                    newest = Some((modified, path));
                                }
                            }
                        }
                    }
                }

                if let Some((_, path)) = newest {
                    if let Ok(file_content) = tokio::fs::read_to_string(&path).await {
                        file_path = Some(path.to_string_lossy().to_string());
                        plan_content = file_content;

                        if let Some(file_title) = plan_content
                            .lines()
                            .find(|l| l.trim().starts_with('#'))
                            .map(|l| l.trim_start_matches('#').trim().to_string())
                        {
                            let _ = std::mem::replace(&mut title, file_title);
                        }
                    }
                }
            }

            if file_path.is_none() {
                let slug = title
                    .chars()
                    .filter(|c| c.is_alphanumeric() || *c == ' ')
                    .collect::<String>()
                    .trim()
                    .replace(' ', "-")
                    .to_lowercase()
                    .chars()
                    .take(40)
                    .collect::<String>();
                let filename = format!("{}.md", slug);
                let plan_path = plans_dir.join(&filename);

                if tokio::fs::write(&plan_path, &plan_content).await.is_ok() {
                    file_path = Some(plan_path.to_string_lossy().to_string());
                }
            }

            let _ = app_handle.emit(
                "agent-done",
                AgentDonePayload {
                    run_id,
                    tokens_used,
                    finish_reason: "stop".to_string(),
                },
            );

            Ok(serde_json::json!({
                "title": title,
                "file_path": file_path,
                "summary": summary,
                "content": plan_content,
            }))
        }
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

async fn execute_build_sequential(
    workspace: std::path::PathBuf,
    context: AgentContext,
    parsed_plan: ParsedPlan,
    mut task_store: TaskStore,
    run_id: String,
    cancel: CancellationToken,
    app_handle: AppHandle,
    orchestrator: &Arc<AgentOrchestrator>,
) -> anyhow::Result<(String, Vec<DiffProposal>, u32)> {
    let agent = BuildAgent;
    let mut all_proposals: Vec<DiffProposal> = Vec::new();
    let mut total_tokens: u32 = 0;
    let build_config = BuildConfig::detect(&workspace.to_string_lossy());

    loop {
        if cancel.is_cancelled() {
            let _ = app_handle.emit(
                "agent-error",
                AgentErrorPayload {
                    run_id: run_id.clone(),
                    error: "Build cancelled".to_string(),
                },
            );
            return Ok(("Build cancelled".to_string(), all_proposals, total_tokens));
        }

        if task_store.is_complete() {
            break;
        }

        if task_store.has_blocked_tasks() && task_store.next_available_task().is_none() {
            break;
        }

        let available = match task_store.next_available_task() {
            Some(t) => t.clone(),
            None => break,
        };

        task_store.mark_running(available.number);

        let _ = app_handle.emit(
            "agent-task-progress",
            AgentTaskProgressPayload {
                run_id: run_id.clone(),
                task_number: available.number,
                total_tasks: task_store.all_tasks().len(),
                task_title: available.title.clone(),
                status: "running".to_string(),
            },
        );

        let completed_nums = task_store.completed_task_numbers();
        let context_summary = parsed_plan.context_summary(&completed_nums);
        let task_section = parsed_plan
            .task_section(available.number)
            .unwrap_or_default();

        let user_message = format!(
            "Contexto de tasks anteriores:\n{}\n\nTask actual a implementar:\n{}\n\nImplementa SOLO esta task.",
            context_summary, task_section
        );

        let provider_registry = ProviderRegistry::new();
        let mut tool_registry = ToolRegistry::new();
        #[allow(deprecated)]
        tool_registry.register_subset_legacy(&[
            "ReadFile", "WriteFile", "ApplyPatch", "CreateFile",
            "DeleteFile", "ListDirectory", "RunCommand", "GetDiagnostics",
        ]);

        let tool_context = tundracode_tools::ToolContext {
            workspace_path: context.workspace_path.clone(),
            agent_id: "build".to_string(),
            dry_run: context.build_mode == BuildMode::ReviewRequired,
        };

        let tools: Vec<tundracode_models::ToolDefinition> = agent
            .allowed_tools()
            .iter()
            .filter_map(|name| {
                tool_registry.get(name).map(|tool| tundracode_models::ToolDefinition {
                    name: tool.name().to_string(),
                    description: tool.description().to_string(),
                    parameters: tool.parameters_schema(),
                })
            })
            .collect();

        let mut agent_loop = AgentLoop::new()
            .with_max_iterations(15)
            .with_budget_tokens(u32::MAX);

        let run_config = RunConfig {
            provider_registry: &provider_registry,
            tool_registry: &tool_registry,
            tool_context: &tool_context,
            provider_id: &context.model_config.provider,
            model_config: &context.model_config,
            system_prompt: &agent.system_prompt(),
            user_message: &user_message,
            tools: &tools,
            reasoning_effort: context.reasoning_effort.clone(),
            on_event: None,
        };

        let run_output = match agent_loop.run(run_config).await {
            Ok(out) => out,
            Err(e) => {
                task_store.mark_failed(available.number, e.to_string());
                let _ = app_handle.emit(
                    "agent-task-complete",
                    AgentTaskCompletePayload {
                        run_id: run_id.clone(),
                        task_number: available.number,
                        success: false,
                        diff_count: 0,
                        error: Some(e.to_string()),
                    },
                );
                continue;
            }
        };

        total_tokens += run_output.tokens_used;

        let task_proposals: Vec<DiffProposal> = run_output
            .invocations
            .iter()
            .enumerate()
            .filter_map(|(idx, inv)| {
                if !inv.success {
                    return None;
                }
                let path = inv.file_path.clone().or_else(|| {
                    inv.arguments
                        .get("path")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })?;
                let before = inv.before.clone().unwrap_or_default();
                let after = inv.after.clone().unwrap_or_default();
                if before == after {
                    return None;
                }

                let kind = match inv.tool_name.as_str() {
                    "CreateFile" => tundracode_agents::DiffKind::Create,
                    "DeleteFile" => tundracode_agents::DiffKind::Delete,
                    _ => tundracode_agents::DiffKind::Modify,
                };

                let unified = if before.is_empty() {
                    let mut out = String::new();
                    out.push_str(&format!("--- /dev/null\n+++ b/{}\n", path));
                    out.push_str("@@ -0,0 +1,");
                    out.push_str(&after.lines().count().to_string());
                    out.push_str(" @@\n");
                    for line in after.lines() {
                        out.push('+');
                        out.push_str(line);
                        out.push('\n');
                    }
                    out
                } else {
                    tundracode_tools::generate_unified_diff(
                        &before, &after,
                        &format!("a/{}", path), &format!("b/{}", path),
                    )
                };

                Some(DiffProposal {
                    id: format!("proposal_t{}_{}", available.number, idx + 1),
                    file_path: path,
                    kind,
                    unified_diff: unified,
                    requires_user_confirmation: true,
                    before,
                    after,
                    tool_call_id: inv.call_id.clone(),
                    task_number: Some(available.number),
                })
            })
            .collect();

        // Autonomous mode: verify with build/test
        if context.build_mode == BuildMode::Autonomous && !build_config.build_command.is_empty() {
            let mut compile_ok = false;
            for attempt in 0..3 {
                let cmd_to_run = if attempt == 0 {
                    build_config.build_command.clone()
                } else if build_config.has_tests {
                    build_config.test_command.clone()
                } else {
                    build_config.build_command.clone()
                };

                if cmd_to_run.is_empty() {
                    compile_ok = true;
                    break;
                }

                let result = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(&cmd_to_run)
                    .current_dir(&workspace)
                    .output();

                match result {
                    Ok(output) if output.status.success() => {
                        compile_ok = true;
                        break;
                    }
                    Ok(output) => {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        if attempt < 2 {
                            // LLM will auto-correct in next iteration
                            // For now, we just retry the build
                            continue;
                        } else {
                            task_store.mark_failed(
                                available.number,
                                format!("Build/test failed after 3 attempts: {}", stderr),
                            );
                            let _ = app_handle.emit(
                                "agent-task-complete",
                                AgentTaskCompletePayload {
                                    run_id: run_id.clone(),
                                    task_number: available.number,
                                    success: false,
                                    diff_count: task_proposals.len(),
                                    error: Some(format!("Build/test failed: {}", stderr)),
                                },
                            );
                            continue;
                        }
                    }
                    Err(e) => {
                        if attempt < 2 {
                            continue;
                        } else {
                            task_store.mark_failed(available.number, e.to_string());
                            let _ = app_handle.emit(
                                "agent-task-complete",
                                AgentTaskCompletePayload {
                                    run_id: run_id.clone(),
                                    task_number: available.number,
                                    success: false,
                                    diff_count: task_proposals.len(),
                                    error: Some(e.to_string()),
                                },
                            );
                            continue;
                        }
                    }
                }
            }

            if compile_ok {
                all_proposals.extend(task_proposals.clone());
                task_store.mark_completed(available.number, String::new());
                let _ = app_handle.emit(
                    "agent-task-complete",
                    AgentTaskCompletePayload {
                        run_id: run_id.clone(),
                        task_number: available.number,
                        success: true,
                        diff_count: task_proposals.len(),
                        error: None,
                    },
                );
            }
            continue;
        }

        // ReviewRequired mode: pause for review
        all_proposals.extend(task_proposals.clone());

        if context.build_mode == BuildMode::ReviewRequired && !task_proposals.is_empty() {
            let diff_count = task_proposals.len();
            task_store.mark_paused(available.number);

            let _ = app_handle.emit(
                "agent-task-paused",
                AgentTaskPausedPayload {
                    run_id: run_id.clone(),
                    task_number: available.number,
                    task_title: available.title.clone(),
                    proposals: task_proposals.clone(),
                },
            );

            let _ = orchestrator.store_build_session(BuildSession {
                run_id: run_id.clone(),
                task_store,
                parsed_plan,
                current_proposals: task_proposals,
                cancel_token: cancel,
                context,
                input: AgentInput {
                    user_message: String::new(),
                    plan_annotations: None,
                    memory_excerpt: None,
                },
            }).await;

            return Ok((format!("Task {} paused for review ({} diffs)", available.number, diff_count), all_proposals, total_tokens));
        }

        task_store.mark_completed(available.number, String::new());
        let _ = app_handle.emit(
            "agent-task-complete",
            AgentTaskCompletePayload {
                run_id: run_id.clone(),
                task_number: available.number,
                success: true,
                diff_count: task_proposals.len(),
                error: None,
            },
        );
    }

    let _ = app_handle.emit(
        "agent-done",
        AgentDonePayload {
            run_id: run_id.clone(),
            tokens_used: total_tokens,
            finish_reason: "stop".to_string(),
        },
    );

    Ok((format!("Build complete. {} proposals generated.", all_proposals.len()), all_proposals, total_tokens))
}

#[tauri::command]
async fn resume_build(
    input: ResumeBuildInput,
    state: State<'_, SharedState>,
    orchestrator: State<'_, Arc<AgentOrchestrator>>,
    app_handle: AppHandle,
) -> Result<String, String> {
    let workspace = {
        let guard = state.lock().await;
        guard
            .workspace_path
            .as_ref()
            .ok_or("No hay workspace abierto")?
            .clone()
    };

    let session = orchestrator
        .take_build_session(&input.run_id)
        .await
        .ok_or_else(|| format!("No active build session found for run_id: {}", input.run_id))?;

    if input.action == "accept_all_task" {
        for proposal in &session.current_proposals {
            let file_path = workspace.join(&proposal.file_path);
            if let Some(parent) = file_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&file_path, &proposal.after);
        }
    }

    let run_id = session.run_id.clone();
    let cancel = session.cancel_token.clone();
    let context = session.context.clone();
    let parsed_plan = session.parsed_plan.clone();
    let mut task_store = session.task_store;

    let current_task_num = task_store
        .current_task_number()
        .ok_or("No current task in session")?;

    if input.action == "accept_all_task" {
        task_store.mark_completed(current_task_num, String::new());
    } else {
        task_store.mark_failed(current_task_num, "User rejected changes".to_string());
    }

    let run_id_clone = run_id.clone();
    let app_clone = app_handle.clone();
    let orch_clone = orchestrator.inner().clone();
    let workspace_for_persist = workspace.clone();

    let result = tokio::spawn(async move {
        execute_build_sequential(
            workspace, context, parsed_plan, task_store,
            run_id_clone, cancel, app_clone, &orch_clone,
        ).await
    })
    .await
    .map_err(|e| format!("Resume build task join failed: {}", e))?;

    *orchestrator.running.write().await = false;

    match result {
        Ok((msg, proposals, _tokens)) => {
            if !proposals.is_empty() {
                let _ = persist_proposals(&proposals, &workspace_for_persist).await;
            }
            Ok(msg)
        }
        Err(e) => Err(e.to_string()),
    }
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
async fn open_plan_in_editor(
    path: String,
) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("Cannot read plan file: {}", e))
}

#[tauri::command]
async fn implement_plan_with_agent(
    plan_path: String,
    task_numbers: Option<Vec<usize>>,
    provider_id: String,
    model_id: String,
    reasoning_effort: Option<String>,
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

    let plan_content = std::fs::read_to_string(&plan_path)
        .map_err(|e| format!("Cannot read plan file: {}", e))?;

    let parsed_plan = ParsedPlan::from_markdown(&plan_content);

    let tasks = if let Some(ref numbers) = task_numbers {
        if !numbers.is_empty() {
            parsed_plan
                .tasks
                .iter()
                .filter(|t| numbers.contains(&t.number))
                .cloned()
                .collect()
        } else {
            parsed_plan.tasks.clone()
        }
    } else {
        parsed_plan.tasks.clone()
    };

    if tasks.is_empty() {
        return Err("No tasks found in plan".to_string());
    }

    let task_store = TaskStore::from_plan_tasks(tasks);

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
    };

    let context = AgentContext {
        workspace_path: workspace.to_string_lossy().to_string(),
        model_config,
        build_mode: BuildMode::ReviewRequired,
        budget_tokens: 256_000,
        reasoning_effort,
    };

    let run_id = format!("build_{}", chrono::Utc::now().format("%Y%m%d%H%M%S"));
    let run_id_clone = run_id.clone();
    let app_clone = app_handle.clone();
    let orch_clone = orchestrator.inner().clone();
    let workspace_for_persist = workspace.clone();

    let result = tokio::spawn(async move {
        execute_build_sequential(
            workspace,
            context,
            parsed_plan,
            task_store,
            run_id_clone,
            cancel,
            app_clone,
            &orch_clone,
        )
        .await
    })
    .await
    .map_err(|e| format!("Build task join failed: {}", e))?;

    *orchestrator.running.write().await = false;

    match result {
        Ok((msg, proposals, _tokens)) => {
            let _ = persist_proposals(&proposals, &workspace_for_persist).await;
            let _ = app_handle.emit(
                "agent-chunk",
                AgentChunkPayload {
                    run_id,
                    chunk: msg.clone(),
                },
            );
            Ok(BuildResult {
                proposals,
                tool_log: vec![msg],
            })
        }
        Err(e) => Err(format!("Build failed: {}", e)),
    }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupTask {
    pub name: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IconsManifest {
    pub version: String,
    pub icons: HashMap<String, String>,
    pub filenames: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupResult {
    pub tasks: Vec<StartupTask>,
    pub icons_dir: Option<String>,
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
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| format!("Failed to create client: {}", e))?;

    let models_response = client
        .get("https://opencode.ai/zen/v1/models")
        .header("X-TundraCode-Client", "tundracode/0.1.0")
        .send()
        .await;

    let free_models: Vec<String> = match models_response {
        Ok(resp) => {
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
                data.iter()
                    .filter_map(|m| {
                        let id = m.get("id").and_then(|v| v.as_str())?;
                        if id.ends_with("-free") {
                            Some(id.to_string())
                        } else {
                            None
                        }
                    })
                    .collect()
            } else {
                vec![
                    "big-pickle".to_string(),
                    "deepseek-v4-flash-free".to_string(),
                    "mimo-v2.5-free".to_string(),
                    "nemotron-3-ultra-free".to_string(),
                ]
            }
        }
        Err(_) => {
            vec![
                "big-pickle".to_string(),
                "deepseek-v4-flash-free".to_string(),
                "mimo-v2.5-free".to_string(),
                "nemotron-3-ultra-free".to_string(),
            ]
        }
    };

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

#[tauri::command]
async fn run_startup_tasks() -> Result<StartupResult, String> {
    let mut tasks = Vec::new();
    let mut icons_dir = None;

    let icons_result = ensure_icons_cache().await;
    match icons_result {
        Ok(dir) => {
            icons_dir = Some(dir.clone());
            tasks.push(StartupTask {
                name: "icons".to_string(),
                status: "ok".to_string(),
                message: format!("Icons cached at {}", dir),
            });
        }
        Err(e) => {
            tasks.push(StartupTask {
                name: "icons".to_string(),
                status: "error".to_string(),
                message: e,
            });
        }
    }

    let providers = get_all_providers();
    let mut model_cache: ModelCacheData = HashMap::new();

    let cache_path = cache_dir().join("models_cache.json");
    if let Ok(data) = tokio::fs::read_to_string(&cache_path).await {
        if let Ok(cached) = serde_json::from_str::<ModelCacheData>(&data) {
            model_cache = cached;
        }
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    for provider in &providers {
        if !provider.api_key_required {
            continue;
        }

        let has_key = get_api_key_from_keyring(&provider.id).is_ok();
        if !has_key {
            continue;
        }

        if let Some(endpoint) = &provider.models_endpoint {
            if let Ok(api_key) = get_api_key_from_keyring(&provider.id) {
                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(10))
                    .build();

                if let Ok(client) = client {
                    let mut request = client.get(endpoint);
                    if provider.id == "anthropic" {
                        request = request.header("x-api-key", &api_key);
                        request = request.header("anthropic-version", "2023-06-01");
                    } else if provider.id == "google" {
                        request = request.query(&[("key", &api_key)]);
                    } else {
                        request = request.bearer_auth(&api_key);
                    }

                    if let Ok(response) = request.send().await {
                        if let Ok(body) = response.json::<serde_json::Value>().await {
                            let models = parse_models_response(&provider.id, &body);
                            if !models.is_empty() {
                                model_cache.insert(
                                    provider.id.clone(),
                                    PerProviderCache {
                                        models: models.clone(),
                                        cached_at: now,
                                    },
                                );
                                tasks.push(StartupTask {
                                    name: format!("models_{}", provider.id),
                                    status: "ok".to_string(),
                                    message: format!("{}: {} models", provider.name, models.len()),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    let cache_dir_path = ensure_cache_dir().await.unwrap_or_default();
    let cache_file = cache_dir_path.join("models_cache.json");
    if let Ok(data) = serde_json::to_string(&model_cache) {
        let _ = tokio::fs::write(&cache_file, data).await;
    }

    Ok(StartupResult {
        tasks,
        icons_dir,
    })
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
                            context_window: None,
                            max_output_tokens: None,
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
                            context_window: None,
                            max_output_tokens: None,
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
                            context_window: None,
                            max_output_tokens: None,
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
                        let context_window = m.get("max_input_tokens").and_then(|v| v.as_u64()).map(|v| v as u32);
                        let max_output_tokens = m.get("max_tokens").and_then(|v| v.as_u64()).map(|v| v as u32);
                        Some(ProviderModel {
                            id,
                            name,
                            description: None,
                            context_window,
                            max_output_tokens,
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
                        let context_window = m.get("inputTokenLimit").and_then(|v| v.as_u64()).map(|v| v as u32);
                        let max_output_tokens = m.get("outputTokenLimit").and_then(|v| v.as_u64()).map(|v| v as u32);
                        Some(ProviderModel {
                            id,
                            name,
                            description: None,
                            context_window,
                            max_output_tokens,
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
                            context_window: None,
                            max_output_tokens: None,
                        })
                    })
                    .collect()
            } else {
                Vec::new()
            }
        }
    }
}



#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerProviderCache {
    pub models: Vec<ProviderModel>,
    pub cached_at: u64,
}

type ModelCacheData = HashMap<String, PerProviderCache>;

fn cache_dir() -> std::path::PathBuf {
    let base = dirs::cache_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    base.join("tundracode")
}

async fn ensure_cache_dir() -> Result<std::path::PathBuf, String> {
    let dir = cache_dir();
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("Failed to create cache dir: {}", e))?;
    Ok(dir)
}

#[tauri::command]
async fn load_cached_models() -> Result<Option<ModelCacheData>, String> {
    let path = cache_dir().join("models_cache.json");
    match tokio::fs::read_to_string(&path).await {
        Ok(data) => {
            serde_json::from_str(&data).map(Some).map_err(|e| format!("Failed to parse model cache: {}", e))
        }
        Err(_) => Ok(None),
    }
}

#[tauri::command]
async fn save_cached_models(cache: ModelCacheData) -> Result<(), String> {
    let dir = ensure_cache_dir().await?;
    let path = dir.join("models_cache.json");
    let data = serde_json::to_string(&cache).map_err(|e| format!("Failed to serialize model cache: {}", e))?;
    tokio::fs::write(&path, &data)
        .await
        .map_err(|e| format!("Failed to write model cache: {}", e))
}

#[tauri::command]
async fn ensure_icons_cache() -> Result<String, String> {
    let icons_dir = cache_dir().join("icons");
    let manifest_path = icons_dir.join("manifest.json");

    if manifest_path.exists() {
        return Ok(icons_dir.to_string_lossy().to_string());
    }

    tokio::fs::create_dir_all(&icons_dir)
        .await
        .map_err(|e| format!("Failed to create icons dir: {}", e))?;

    let zip_url = "https://github.com/vscode-icons/vscode-icons/releases/latest/download/vscode-icons-master.zip";
    let zip_path = cache_dir().join("icons_download.zip");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(zip_url)
        .send()
        .await
        .map_err(|e| format!("Failed to download icons: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("Failed to download icons: HTTP {}", status));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read icons download: {}", e))?;

    tokio::fs::write(&zip_path, &bytes)
        .await
        .map_err(|e| format!("Failed to write icons zip: {}", e))?;

    let zip_file = std::fs::File::open(&zip_path)
        .map_err(|e| format!("Failed to open icons zip: {}", e))?;
    let mut archive = zip::ZipArchive::new(zip_file)
        .map_err(|e| format!("Failed to read icons zip: {}", e))?;

    let mut manifest = IconsManifest {
        version: "1".to_string(),
        icons: HashMap::new(),
        filenames: HashMap::new(),
    };

    let mut extracted_files: Vec<(String, Vec<u8>)> = Vec::new();

    {
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)
                .map_err(|e| format!("Failed to read zip entry: {}", e))?;

            let entry_path = entry.mangled_name().to_string_lossy().to_string();

            if entry_path.ends_with('/') {
                continue;
            }

            let file_name = std::path::Path::new(&entry_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            if !file_name.ends_with(".svg") {
                continue;
            }

            if entry_path.contains("icons/") && !entry_path.contains("__MACOSX") {
                let mut content = Vec::new();
                std::io::Read::read_to_end(&mut entry, &mut content)
                    .map_err(|e| format!("Failed to read zip entry content: {}", e))?;

                extracted_files.push((file_name, content));
            }
        }
    }

    for (file_name, content) in extracted_files {
        let icon_name = file_name.replace(".svg", "");
        let clean_name = icon_name
            .strip_prefix("file_type_")
            .unwrap_or(&icon_name)
            .to_string();

        let target_path = icons_dir.join(&file_name);
        tokio::fs::write(&target_path, &content)
            .await
            .map_err(|e| format!("Failed to write icon file: {}", e))?;

        manifest.icons.insert(clean_name.clone(), file_name.clone());

        if clean_name == "rust" {
            manifest.filenames.insert("Cargo.toml".to_string(), file_name.clone());
            manifest.filenames.insert("Cargo.lock".to_string(), file_name.clone());
        } else if clean_name == "javascript" {
            manifest.filenames.insert("package.json".to_string(), file_name.clone());
            manifest.filenames.insert("package-lock.json".to_string(), file_name.clone());
        } else if clean_name == "typescript" {
            manifest.filenames.insert("tsconfig.json".to_string(), file_name.clone());
        } else if clean_name == "docker" {
            manifest.filenames.insert("Dockerfile".to_string(), file_name.clone());
            manifest.filenames.insert("docker-compose.yml".to_string(), file_name.clone());
        } else if clean_name == "git" {
            manifest.filenames.insert(".gitignore".to_string(), file_name.clone());
            manifest.filenames.insert(".gitattributes".to_string(), file_name.clone());
            manifest.filenames.insert(".gitmodules".to_string(), file_name.clone());
        } else if clean_name == "go" {
            manifest.filenames.insert("go.mod".to_string(), file_name.clone());
            manifest.filenames.insert("go.sum".to_string(), file_name.clone());
        }
    }

    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("Failed to serialize manifest: {}", e))?;
    tokio::fs::write(&manifest_path, manifest_json)
        .await
        .map_err(|e| format!("Failed to write manifest: {}", e))?;

    tokio::fs::remove_file(&zip_path).await.ok();

    Ok(icons_dir.to_string_lossy().to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    pub path: String,
    pub is_directory: bool,
    pub size: u64,
}

#[tauri::command]
async fn get_project_structure(workspace: String) -> Result<Vec<IndexEntry>, String> {
    let mut entries = Vec::new();
    let root = std::path::Path::new(&workspace);
    if !root.exists() {
        return Err("Workspace does not exist".to_string());
    }
    let mut dirs = vec![root.to_path_buf()];
    let ignored = [".git", "node_modules", "target", ".tundracode", "__pycache__", ".DS_Store", "vendor"];

    while let Some(dir) = dirs.pop() {
        let mut read_dir = tokio::fs::read_dir(&dir)
            .await
            .map_err(|e| format!("Failed to read dir {:?}: {}", dir, e))?;

        while let Some(entry) = read_dir.next_entry().await.map_err(|e| format!("Read dir error: {}", e))? {
            let is_dir = entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
            let name = entry.file_name().to_string_lossy().to_string();

            if ignored.contains(&name.as_str()) || name.starts_with('.') {
                continue;
            }

            let path = entry.path().to_string_lossy().to_string();
            let size = if is_dir { 0u64 } else { entry.metadata().await.map(|m| m.len()).unwrap_or(0) };

            entries.push(IndexEntry { path, is_directory: is_dir, size });

            if is_dir {
                dirs.push(entry.path());
            }
        }
    }

    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
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
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentToolCallPayload {
    Start {
        run_id: String,
        call_id: String,
        tool_name: String,
        file_path: Option<String>,
        arguments: Option<serde_json::Value>,
    },
    Progress {
        run_id: String,
        call_id: String,
        message: String,
        progress: Option<f32>,
    },
    Complete {
        run_id: String,
        call_id: String,
        tool_name: String,
        result_summary: String,
        output_preview: Option<String>,
        duration_ms: u64,
        success: bool,
        file_path: Option<String>,
    },
    Error {
        run_id: String,
        call_id: String,
        tool_name: String,
        error: String,
        duration_ms: u64,
    },
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelContextInfo {
    pub max_context_tokens: u32,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateSessionTitleInput {
    pub user_message: String,
    pub provider_id: String,
    pub model_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSessionInput {
    pub filename: String,
    pub history_json: String,
    pub model: String,
    pub tokens: u64,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTaskProgressPayload {
    pub run_id: String,
    pub task_number: usize,
    pub total_tasks: usize,
    pub task_title: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTaskCompletePayload {
    pub run_id: String,
    pub task_number: usize,
    pub success: bool,
    pub diff_count: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTaskPausedPayload {
    pub run_id: String,
    pub task_number: usize,
    pub task_title: String,
    pub proposals: Vec<tundracode_agents::DiffProposal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeBuildInput {
    pub run_id: String,
    pub action: String,
}

#[tauri::command]
async fn send_completion(
    run_id: String,
    provider_id: String,
    model_id: String,
    reasoning_effort: Option<String>,
    messages: Vec<ChatMessage>,
    system_prompt: Option<String>,
    agent_id: Option<String>,
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
    };

    let registry = ProviderRegistry::new();
    if !registry.has(&provider_id) {
        return Err(format!("Provider not registered: {}", provider_id));
    }

    let mut tool_registry = ToolRegistry::new();
    
    // If agent_id is provided, restrict tools to profile's allowed_tools
    if let Some(ref aid) = agent_id {
        use tundracode_agents::profile::AgentProfileRegistry;
        let profile_registry = AgentProfileRegistry::new();
        if let Some(profile) = profile_registry.get(aid) {
            #[allow(deprecated)]
            tool_registry.register_subset_legacy(&profile.allowed_tools.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        } else {
            tool_registry.register_all_default();
        }
    } else {
        tool_registry.register_all_default();
    }

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
        dry_run: false,
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

    let on_event = {
        let tx_chunk = tx_chunk.clone();
        let run_id = run_id.clone();
        let app_handle = app_handle.clone();
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};
        use std::time::Instant;
        
        let tool_call_state: Arc<Mutex<HashMap<String, (String, Option<String>, Instant, serde_json::Value)>>> = Arc::new(Mutex::new(HashMap::new()));
        
        move |event: tundracode_models::StreamEvent| {
            use tundracode_models::StreamEvent;
            match event {
                StreamEvent::Token(t) => {
                    let _ = tx_chunk.try_send(t);
                }
                StreamEvent::ReasoningToken(t) => {
                    let _ = app_handle.emit(
                        "agent-reasoning",
                        AgentChunkPayload {
                            run_id: run_id.clone(),
                            chunk: t,
                        },
                    );
                }
                StreamEvent::ToolCallStart { name, call_id, file_path, .. } => {
                    let start_time = Instant::now();
                    let mut state = tool_call_state.lock().unwrap();
                    state.insert(call_id.clone(), (name.clone(), file_path.clone(), start_time, serde_json::Value::Object(serde_json::Map::new())));
                    drop(state);
                    
                    let _ = app_handle.emit(
                        "agent-tool-call",
                        AgentToolCallPayload::Start {
                            run_id: run_id.clone(),
                            call_id,
                            tool_name: name,
                            file_path,
                            arguments: None,
                        },
                    );
                }
                StreamEvent::ToolCallDelta { call_id, arguments_delta } => {
                    let mut state = tool_call_state.lock().unwrap();
                    if let Some((_, _, _, args)) = state.get_mut(&call_id) {
                        if let Some(obj) = args.as_object_mut() {
                            if let Some(raw) = obj.get_mut("_raw") {
                                if let Some(s) = raw.as_str() {
                                    *raw = serde_json::Value::String(s.to_string() + &arguments_delta);
                                }
                            } else {
                                obj.insert("_raw".to_string(), serde_json::Value::String(arguments_delta));
                            }
                        }
                    }
                    drop(state);
                }
                StreamEvent::ToolCallEnd { call_id, file_path } => {
                    let (tool_name, original_file_path, start_time, _args) = {
                        let mut state = tool_call_state.lock().unwrap();
                        state.remove(&call_id).unwrap_or_else(|| (String::new(), file_path.clone(), Instant::now(), serde_json::Value::Object(serde_json::Map::new())))
                    };
                    
                    let duration_ms = start_time.elapsed().as_millis() as u64;
                    let final_file_path = file_path.or(original_file_path);
                    
                    let _ = app_handle.emit(
                        "agent-tool-call",
                        AgentToolCallPayload::Complete {
                            run_id: run_id.clone(),
                            call_id: call_id.clone(),
                            tool_name: tool_name.clone(),
                            result_summary: format!("{} completed", tool_name),
                            output_preview: None,
                            duration_ms,
                            success: true,
                            file_path: final_file_path,
                        },
                    );
                }
                StreamEvent::Done(_) => {}
                StreamEvent::Error(e) => {
                    let _ = app_handle.emit(
                        "agent-error",
                        AgentErrorPayload {
                            run_id: run_id.clone(),
                            error: e,
                        },
                    );
                }
                StreamEvent::ContextCompacted { message } => {
                    let _ = app_handle.emit(
                        "agent-compacted",
                        CompactedPayload {
                            run_id: run_id.clone(),
                            message,
                        },
                    );
                }
                StreamEvent::SubagentStart { agent_id, task } => {
                    let _ = app_handle.emit(
                        "subagent-start",
                        SubagentPayload {
                            run_id: run_id.clone(),
                            agent: agent_id,
                            task: Some(task),
                            duration_ms: None,
                            success: None,
                            findings: None,
                        },
                    );
                }
                StreamEvent::SubagentComplete { agent_id, duration_ms, success } => {
                    let _ = app_handle.emit(
                        "subagent-complete",
                        SubagentPayload {
                            run_id: run_id.clone(),
                            agent: agent_id,
                            task: None,
                            duration_ms: Some(duration_ms),
                            success: Some(success),
                            findings: None,
                        },
                    );
                }
            }
        }
    };

    let mut agent_loop = AgentLoop::new().with_max_iterations(20);
    let run_config = RunConfig {
        provider_registry: &registry,
        tool_registry: &tool_registry,
        tool_context: &tool_context,
        provider_id: &provider_id,
        model_config: &model_config,
        system_prompt: &system,
        user_message: &last_user,
        tools: &tool_definitions,
        reasoning_effort,
        on_event: Some(Box::new(on_event)),
    };

    let result = agent_loop.run(run_config).await;
    drop(tx_chunk);
    let _ = chunk_task.await;

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
    reasoning_effort: Option<String>,
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

    let model_config = ModelConfig {
        provider: provider_id.clone(),
        model: model_id,
        api_key,
        base_url: Some(base_url),
    };

    let context = AgentContext {
        workspace_path: workspace.to_string_lossy().to_string(),
        model_config,
        build_mode: BuildMode::ReviewRequired,
        budget_tokens: u32::MAX,
        reasoning_effort,
    };

    let input = AgentInput {
        user_message: plan_description.clone(),
        plan_annotations,
        memory_excerpt: None,
    };

    let parsed_plan = ParsedPlan::from_markdown(&plan_description);
    let use_sequential = parsed_plan.tasks.len() >= 2;

    let cancel = CancellationToken::new();
    *orchestrator.cancel_token.write().await = Some(cancel.clone());
    *orchestrator.running.write().await = true;

    if use_sequential {
        let _on_event = create_on_event(run_id.clone(), app_handle.clone());

        let tasks_clone = parsed_plan.tasks.clone();
        let task_store = TaskStore::from_plan_tasks(tasks_clone);
        let result = execute_build_sequential(
            workspace.clone(), context, parsed_plan, task_store,
            run_id.clone(), cancel, app_handle.clone(), &orchestrator,
        ).await;

        *orchestrator.running.write().await = false;
        return result.map(|(summary, proposals, tokens)| {
            for proposal in &proposals {
                let _ = app_handle.emit("agent-tool-call", AgentToolCallPayload::Complete {
                    run_id: run_id.clone(),
                    call_id: proposal.id.clone(),
                    tool_name: "apply_diff".to_string(),
                    result_summary: format!("{:?} {}", proposal.kind, proposal.file_path),
                    output_preview: None,
                    duration_ms: 0,
                    success: true,
                    file_path: Some(proposal.file_path.clone()),
                });
            }
            let _ = app_handle.emit("agent-done", AgentDonePayload {
                run_id: run_id.clone(),
                tokens_used: tokens,
                finish_reason: "stop".to_string(),
            });
            let tool_log: Vec<String> = summary.lines().map(|l| l.to_string()).collect();
            BuildResult { proposals, tool_log }
        }).map_err(|e| format!("Build agent failed: {}", e));
    }

    let agent = BuildAgent;
    let run_id_for_task = run_id.clone();
    let app_for_task = app_handle.clone();

    let on_event = create_on_event(run_id.clone(), app_handle.clone());

    let result: anyhow::Result<AgentOutput> = tokio::spawn(async move {
        tokio::select! {
            output = agent.run_with_streaming(&context, input, Some(Box::new(on_event))) => output,
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
        AgentOutput::ProposedChanges { proposals, tool_log, invocations, tokens_used } => {
            persist_proposals(&proposals, &workspace).await.ok();
            for inv in &invocations {
                let success = inv.success;
                let summary = if success {
                    format!("{} completed", inv.tool_name)
                } else {
                    format!("{} failed", inv.tool_name)
                };
                let output_preview = if inv.output.len() > 200 {
                    Some(format!("{}...", &inv.output[..200]))
                } else if inv.output.is_empty() {
                    None
                } else {
                    Some(inv.output.clone())
                };
                let _ = app_handle.emit(
                    "agent-tool-call",
                    AgentToolCallPayload::Complete {
                        run_id: run_id.clone(),
                        call_id: inv.call_id.clone(),
                        tool_name: inv.tool_name.clone(),
                        result_summary: summary,
                        output_preview,
                        duration_ms: 0,
                        success,
                        file_path: inv.file_path.clone(),
                    },
                );
            }
            let _ = app_handle.emit(
                "agent-done",
                AgentDonePayload {
                    run_id: run_id.clone(),
                    tokens_used,
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

    let proposals_dir = workspace
        .join(".tundracode")
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

    let file_path = workspace.join(&proposal.file_path);

    match proposal.kind {
        tundracode_agents::DiffKind::Create | tundracode_agents::DiffKind::Modify => {
            if let Some(parent) = file_path.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| format!("Cannot create directory: {}", e))?;
            }
            tokio::fs::write(&file_path, &proposal.after)
                .await
                .map_err(|e| format!("Cannot write file: {}", e))?;
        }
        tundracode_agents::DiffKind::Delete => {
            if file_path.exists() {
                tokio::fs::remove_file(&file_path)
                    .await
                    .map_err(|e| format!("Cannot delete file: {}", e))?;
            }
        }
    }

    tokio::fs::remove_file(&proposal_path)
        .await
        .map_err(|e| format!("Failed to remove proposal: {}", e))?;

    Ok(format!("Applied changes to {}", proposal.file_path))
}

#[tauri::command]
async fn reject_diff(proposal_id: String, state: State<'_, SharedState>) -> Result<String, String> {
    let workspace = {
        let guard = state.lock().await;
        guard
            .workspace_path
            .as_ref()
            .ok_or("No hay workspace abierto")?
            .clone()
    };

    let proposals_dir = workspace
        .join(".tundracode")
        .join("proposals");

    let proposal_path = proposals_dir.join(format!("{}.json", proposal_id));

    if proposal_path.exists() {
        let content = tokio::fs::read_to_string(&proposal_path)
            .await
            .map_err(|e| format!("Failed to read proposal: {}", e))?;

        let proposal: tundracode_agents::DiffProposal =
            serde_json::from_str(&content).map_err(|e| format!("Failed to parse proposal: {}", e))?;

        let file_path = workspace.join(&proposal.file_path);

        match proposal.kind {
            tundracode_agents::DiffKind::Create => {
                if file_path.exists() {
                    let _ = tokio::fs::remove_file(&file_path).await;
                }
            }
            tundracode_agents::DiffKind::Modify => {
                if !proposal.before.is_empty() {
                    if let Some(parent) = file_path.parent() {
                        let _ = tokio::fs::create_dir_all(parent).await;
                    }
                    let _ = tokio::fs::write(&file_path, &proposal.before).await;
                }
            }
            tundracode_agents::DiffKind::Delete => {
                if !proposal.before.is_empty() && !file_path.exists() {
                    if let Some(parent) = file_path.parent() {
                        let _ = tokio::fs::create_dir_all(parent).await;
                    }
                    let _ = tokio::fs::write(&file_path, &proposal.before).await;
                }
            }
        }

        tokio::fs::remove_file(&proposal_path)
            .await
            .map_err(|e| format!("Failed to remove proposal: {}", e))?;
    }

    Ok("Proposal rejected and rolled back".to_string())
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

fn sessions_dir() -> std::path::PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let flatpak_dir = home.join(".var/app/com.tundracode.dev");
    if flatpak_dir.exists() {
        flatpak_dir.join("sessions")
    } else {
        tundracode_home().join("sessions")
    }
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
    let dir = workspace.join(".tundracode").join("proposals");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("Cannot create proposals dir: {}", e))?;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub filename: String,
    pub title: String,
    pub date: String,
    pub model: String,
    pub tokens: u64,
    pub mode: String,
}

#[tauri::command]
async fn save_session(
    title: String,
    history_json: String,
    model: String,
    tokens: u64,
    mode: String,
) -> Result<String, String> {
    let dir = sessions_dir();
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("Cannot create sessions dir: {}", e))?;

    let slug = title
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-')
        .take(50)
        .collect::<String>()
        .trim()
        .replace(' ', "-")
        .to_lowercase();
    let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let filename = format!("{}-{}.md", ts, slug);

    let history: Vec<serde_json::Value> = serde_json::from_str(&history_json)
        .map_err(|e| format!("Cannot parse history: {}", e))?;

    let mut content = format!(
        "# Session: {}\n- Date: {}\n- Model: {}\n- Tokens: {}\n- Mode: {}\n\n---\n\n",
        title,
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S"),
        model,
        tokens,
        mode,
    );

    for msg in &history {
        let role = msg["role"].as_str().unwrap_or("user");
        let text = msg["content"].as_str().unwrap_or("");
        let label = if role == "user" { "User" } else { "Assistant" };
        content.push_str(&format!("## {}\n{}\n\n", label, text));
    }

    let path = dir.join(&filename);
    tokio::fs::write(&path, content)
        .await
        .map_err(|e| format!("Cannot write session: {}", e))?;

    Ok(filename)
}

#[tauri::command]
async fn load_session(
    filename: String,
) -> Result<SessionInfo, String> {
    let dir = sessions_dir();
    let path = dir.join(&filename);
    let content = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("Cannot read session: {}", e))?;

    let mut title = String::new();
    let mut date = String::new();
    let mut model = String::new();
    let mut tokens = 0u64;
    let mut mode = String::new();
    let mut history = Vec::new();

    let mut current_role = String::new();
    let mut current_text = String::new();

    for line in content.lines() {
        if let Some(t) = line.strip_prefix("# Session: ") {
            title = t.to_string();
        } else if let Some(d) = line.strip_prefix("- Date: ") {
            date = d.to_string();
        } else if let Some(m) = line.strip_prefix("- Model: ") {
            model = m.to_string();
        } else if let Some(t) = line.strip_prefix("- Tokens: ") {
            tokens = t.parse().unwrap_or(0);
        } else if let Some(m) = line.strip_prefix("- Mode: ") {
            mode = m.to_string();
        } else if line == "---" {
            continue;
        } else if let Some(r) = line.strip_prefix("## ") {
            if !current_role.is_empty() && !current_text.trim().is_empty() {
                history.push(serde_json::json!({
                    "role": current_role,
                    "content": current_text.trim(),
                }));
            }
            current_role = if r == "User" { "user".to_string() } else { "assistant".to_string() };
            current_text.clear();
        } else if !line.starts_with("- ") && !line.starts_with("# ") {
            if !current_text.is_empty() {
                current_text.push('\n');
            }
            current_text.push_str(line);
        }
    }

    if !current_role.is_empty() && !current_text.trim().is_empty() {
        history.push(serde_json::json!({
            "role": current_role,
            "content": current_text.trim(),
        }));
    }

    Ok(SessionInfo {
        filename,
        title,
        date,
        model,
        tokens,
        mode,
    })
}

#[tauri::command]
async fn load_session_history(
    filename: String,
) -> Result<Vec<serde_json::Value>, String> {
    let dir = sessions_dir();
    let path = dir.join(&filename);
    let content = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("Cannot read session: {}", e))?;

    let mut history = Vec::new();
    let mut current_role = String::new();
    let mut current_text = String::new();

    for line in content.lines() {
        if line.starts_with("# Session:") || line.starts_with("- ") || line == "---" {
            continue;
        } else if let Some(r) = line.strip_prefix("## ") {
            if !current_role.is_empty() && !current_text.trim().is_empty() {
                history.push(serde_json::json!({
                    "role": current_role,
                    "content": current_text.trim(),
                }));
            }
            current_role = if r == "User" { "user".to_string() } else { "assistant".to_string() };
            current_text.clear();
        } else {
            if !current_text.is_empty() {
                current_text.push('\n');
            }
            current_text.push_str(line);
        }
    }

    if !current_role.is_empty() && !current_text.trim().is_empty() {
        history.push(serde_json::json!({
            "role": current_role,
            "content": current_text.trim(),
        }));
    }

    Ok(history)
}

#[tauri::command]
async fn list_sessions() -> Result<Vec<SessionInfo>, String> {
    let dir = sessions_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries = tokio::fs::read_dir(&dir)
        .await
        .map_err(|e| format!("Cannot read sessions dir: {}", e))?;

    let mut sessions = Vec::new();
    while let Some(entry) = entries.next_entry().await.map_err(|e| format!("Read dir error: {}", e))? {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".md") {
            continue;
        }
        let content = tokio::fs::read_to_string(entry.path())
            .await
            .unwrap_or_default();

        let mut title = name.clone();
        let mut date = String::new();
        let mut model = String::new();
        let mut tokens = 0u64;
        let mut mode = String::new();

        for line in content.lines() {
            if let Some(t) = line.strip_prefix("# Session: ") {
                title = t.to_string();
            } else if let Some(d) = line.strip_prefix("- Date: ") {
                date = d.to_string();
            } else if let Some(m) = line.strip_prefix("- Model: ") {
                model = m.to_string();
            } else if let Some(t) = line.strip_prefix("- Tokens: ") {
                tokens = t.parse().unwrap_or(0);
            } else if let Some(m) = line.strip_prefix("- Mode: ") {
                mode = m.to_string();
            }
        }

        sessions.push(SessionInfo {
            filename: name,
            title,
            date,
            model,
            tokens,
            mode,
        });
    }

    sessions.sort_by(|a, b| b.date.cmp(&a.date));
    Ok(sessions)
}

#[tauri::command]
async fn delete_session(
    filename: String,
) -> Result<String, String> {
    let dir = sessions_dir();
    let path = dir.join(&filename);
    if !path.exists() {
        return Err("Session not found".to_string());
    }
    tokio::fs::remove_file(&path)
        .await
        .map_err(|e| format!("Cannot delete session: {}", e))?;
    Ok("Session deleted".to_string())
}

#[tauri::command]
async fn get_model_context_info(
    provider_id: String,
    model_id: String,
) -> Result<ModelContextInfo, String> {
    // First check if we have real data from fetch_provider_models
    // For now, use the registry lookup
    if let Some(ctx) = lookup_model_context(&provider_id, &model_id) {
        return Ok(ModelContextInfo {
            max_context_tokens: ctx,
            source: "registry".to_string(),
        });
    }

    // For opencode providers, use default
    if provider_id.starts_with("opencode") {
        return Ok(ModelContextInfo {
            max_context_tokens: 128_000,
            source: "registry".to_string(),
        });
    }

    Ok(ModelContextInfo {
        max_context_tokens: 0,
        source: "unknown".to_string(),
    })
}

#[tauri::command]
async fn generate_session_title(
    user_message: String,
    provider_id: String,
    model_id: String,
) -> Result<String, String> {
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
        provider: provider_id,
        model: model_id,
        api_key,
        base_url: Some(base_url),
    };

    let prompt = format!(
        "Genera un titulo descriptivo en maximo 6 palabras para una sesion de coding cuyo primer mensaje fue: '{}'. Responde SOLO con el titulo, sin comillas ni puntos.",
        user_message.chars().take(200).collect::<String>()
    );

    let registry = ProviderRegistry::new();
    let provider = registry
        .get(&model_config.provider)
        .ok_or_else(|| format!("Provider not found: {}", model_config.provider))?;

    let request = tundracode_models::CompletionRequest {
        conversation: {
            let mut conv = tundracode_models::Conversation::new();
            conv.add_message(tundracode_models::MessageRole::User, prompt);
            conv
        },
        system_prompt: Some("Eres un asistente que genera titulos cortos y descriptivos.".to_string()),
        reasoning_effort: None,
    };

    // Use a timeout of 5 seconds
    let timeout_duration = std::time::Duration::from_secs(5);

    match tokio::time::timeout(timeout_duration, provider.complete(&model_config, request, None)).await {
        Ok(Ok((response, _))) => {
            let title = response.content.trim().trim_matches('"').trim_matches('\'').trim().to_string();
            if title.is_empty() {
                Ok(user_message.chars().take(50).collect())
            } else {
                Ok(title)
            }
        }
        _ => {
            // Fallback to truncating the message
            Ok(user_message.chars().take(50).collect())
        }
    }
}

#[tauri::command]
async fn update_session(
    input: UpdateSessionInput,
) -> Result<String, String> {
    let dir = sessions_dir();
    let path = dir.join(&input.filename);

    if !path.exists() {
        return Err("Session file not found".to_string());
    }

    let history: Vec<serde_json::Value> = serde_json::from_str(&input.history_json)
        .map_err(|e| format!("Cannot parse history: {}", e))?;

    let mut content = format!(
        "# Session: {}\n- Date: {}\n- Model: {}\n- Tokens: {}\n- Mode: {}\n\n---\n\n",
        input.filename
            .strip_suffix(".md")
            .and_then(|s| s.splitn(2, '-').nth(1))
            .unwrap_or(&input.filename)
            .replace('-', " "),
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S"),
        input.model,
        input.tokens,
        input.mode,
    );

    for msg in &history {
        let role = msg["role"].as_str().unwrap_or("user");
        let text = msg["content"].as_str().unwrap_or("");
        let label = if role == "user" { "User" } else { "Assistant" };
        content.push_str(&format!("## {}\n{}\n\n", label, text));
    }

    tokio::fs::write(&path, content)
        .await
        .map_err(|e| format!("Cannot write session: {}", e))?;

    Ok("Session updated".to_string())
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
            open_plan_in_editor,
            implement_plan_with_agent,
            resume_build,
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
            save_session,
            load_session,
            load_session_history,
            list_sessions,
            delete_session,
            get_model_context_info,
            generate_session_title,
            update_session,
            get_project_structure,
            load_cached_models,
            save_cached_models,
            ensure_icons_cache,
            run_startup_tasks,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
