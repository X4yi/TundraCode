use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::AppHandle;
use tauri::Emitter;
use tundracode_models::StreamEvent;

use crate::{AgentChunkPayload, AgentErrorPayload, AgentToolCallPayload};

#[derive(serde::Serialize, Clone)]
pub struct CompactedPayload {
    pub run_id: String,
    pub message: String,
}

#[derive(serde::Serialize, Clone)]
pub struct SubagentPayload {
    pub run_id: String,
    pub agent: String,
    pub task: Option<String>,
    pub duration_ms: Option<u64>,
    pub success: Option<bool>,
    pub findings: Option<Vec<String>>,
}

type ToolCallState = Arc<Mutex<HashMap<String, (String, Option<String>, Instant, serde_json::Value)>>>;

fn extract_param(args: &serde_json::Value, keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Some(v) = args.get(k).and_then(|v| v.as_str()).map(|s| s.to_string()) {
            return Some(v);
        }
    }
    None
}

fn make_summary(tool_name: &str, args: &serde_json::Value, success: bool) -> String {
    if !success { return format!("{} failed", tool_name); }
    match tool_name {
        "ReadFile" => {
            extract_param(args, &["p","path"]).map(|p| format!("Read {}", p))
            .unwrap_or_else(|| "Read file".to_string())
        }
        "WriteFile" => {
            let path = extract_param(args, &["p","path"]).unwrap_or_default();
            let len = args.get("c").or_else(|| args.get("content"))
                .and_then(|v| v.as_str()).map(|s| s.len()).unwrap_or(0);
            if path.is_empty() { format!("Wrote {} chars", len) }
            else { format!("Wrote {} chars to {}", len, path) }
        }
        "CreateFile" => {
            extract_param(args, &["p","path"]).map(|p| format!("Created {}", p))
            .unwrap_or_else(|| "Created file".to_string())
        }
        "DeleteFile" => {
            extract_param(args, &["p","path"]).map(|p| format!("Deleted {}", p))
            .unwrap_or_else(|| "Deleted file".to_string())
        }
        "ApplyPatch" => {
            extract_param(args, &["p","path"]).map(|p| format!("Patched {}", p))
            .unwrap_or_else(|| "Applied patch".to_string())
        }
        "RunCommand" => {
            extract_param(args, &["c","command"]).map(|c| format!("`{}`", c))
            .unwrap_or_else(|| "Ran command".to_string())
        }
        "GetDiagnostics" => {
            extract_param(args, &["f","file_path"]).map(|f| format!("Diagnostics for {}", f))
            .unwrap_or_else(|| "Got diagnostics".to_string())
        }
        "SearchCodebase" => {
            extract_param(args, &["q","query"]).map(|q| format!("Searched \"{}\"", q))
            .unwrap_or_else(|| "Searched codebase".to_string())
        }
        "SearchInWeb" => {
            extract_param(args, &["q","query"]).map(|q| format!("Web search \"{}\"", q))
            .unwrap_or_else(|| "Web search".to_string())
        }
        "ListDirectory" => {
            extract_param(args, &["p","path"]).map(|p| format!("Listed {}", p))
            .unwrap_or_else(|| "Listed directory".to_string())
        }
        "GetWorkspace" => "Scanned workspace".to_string(),
        _ => format!("{} completed", tool_name),
    }
}

fn make_output_preview(tool_name: &str, args: &serde_json::Value) -> Option<String> {
    match tool_name {
        "RunCommand" => {
            let cmd = extract_param(args, &["c","command"]).unwrap_or_default();
            let cmd_args = args.get("a").or_else(|| args.get("args"))
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(" "))
                .unwrap_or_default();
            Some(if cmd_args.is_empty() { cmd } else { format!("{} {}", cmd, cmd_args) })
        }
        "ReadFile" | "WriteFile" | "ApplyPatch" | "CreateFile" | "DeleteFile" => {
            extract_param(args, &["p","path"])
        }
        "SearchCodebase" => extract_param(args, &["q","query"]),
        "SearchInWeb" => extract_param(args, &["q","query"]),
        "GetDiagnostics" => extract_param(args, &["f","file_path"]),
        "ListDirectory" => extract_param(args, &["p","path"]),
        _ => None,
    }
}

fn extract_file_path(args: &serde_json::Value) -> Option<String> {
    extract_param(args, &["p","path","f","file_path"])
}

fn merge_args(accumulated: &serde_json::Value, delta: &str) -> serde_json::Value {
    if accumulated.get("_raw").and_then(|v| v.as_str()).is_some() {
        let mut new = accumulated.clone();
        if let Some(obj) = new.as_object_mut() {
            if let Some(raw) = obj.get_mut("_raw") {
                if let Some(s) = raw.as_str() {
                    *raw = serde_json::Value::String(s.to_string() + delta);
                }
            }
        }
        return new;
    }
    let out = serde_json::json!({"_raw": delta});
    out
}

fn parse_accumulated_args(accumulated: &serde_json::Value) -> (serde_json::Value, Option<String>) {
    if let Some(raw) = accumulated.get("_raw").and_then(|v| v.as_str()) {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(raw) {
            let fp = extract_file_path(&parsed);
            return (parsed, fp);
        }
    }
    let fp = extract_file_path(accumulated);
    (accumulated.clone(), fp)
}

pub fn create_on_event(run_id: String, app_handle: AppHandle) -> impl FnMut(StreamEvent) + Send {
    let tool_call_state: ToolCallState = Arc::new(Mutex::new(HashMap::new()));

    move |event: StreamEvent| {
        match event {
            StreamEvent::Token(t) => {
                let _ = app_handle.emit(
                    "agent-chunk",
                    AgentChunkPayload {
                        run_id: run_id.clone(),
                        chunk: t,
                    },
                );
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
            StreamEvent::ToolCallStart { name, call_id, file_path, arguments } => {
                let start_time = Instant::now();
                let mut state = tool_call_state.lock().unwrap();
                state.insert(
                    call_id.clone(),
                    (
                        name.clone(),
                        file_path.clone(),
                        start_time,
                        arguments.unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
                    ),
                );
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
                    *args = merge_args(args, &arguments_delta);
                }
                drop(state);
            }
            StreamEvent::ToolCallEnd { call_id, file_path } => {
                let (tool_name, original_file_path, start_time, accumulated_args) = {
                    let mut state = tool_call_state.lock().unwrap();
                    state.remove(&call_id).unwrap_or_else(|| {
                        (
                            String::new(),
                            file_path.clone(),
                            Instant::now(),
                            serde_json::Value::Object(serde_json::Map::new()),
                        )
                    })
                };
                let duration_ms = start_time.elapsed().as_millis() as u64;
                let (parsed_args, args_file_path) = parse_accumulated_args(&accumulated_args);
                let final_file_path = file_path.or(original_file_path).or(args_file_path);

                let result_summary = make_summary(&tool_name, &parsed_args, true);
                let output_preview = make_output_preview(&tool_name, &parsed_args);

                let _ = app_handle.emit(
                    "agent-tool-call",
                    AgentToolCallPayload::Complete {
                        run_id: run_id.clone(),
                        call_id,
                        tool_name: tool_name.clone(),
                        result_summary,
                        output_preview,
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
}

pub fn create_on_event_no_chunks(
    run_id: String,
    app_handle: AppHandle,
) -> impl FnMut(StreamEvent) + Send {
    let tool_call_state: ToolCallState = Arc::new(Mutex::new(HashMap::new()));

    move |event: StreamEvent| {
        match event {
            StreamEvent::Token(_) => {}
            StreamEvent::ReasoningToken(t) => {
                let _ = app_handle.emit(
                    "agent-reasoning",
                    AgentChunkPayload {
                        run_id: run_id.clone(),
                        chunk: t,
                    },
                );
            }
            StreamEvent::ToolCallStart { name, call_id, file_path, arguments } => {
                let start_time = Instant::now();
                let mut state = tool_call_state.lock().unwrap();
                state.insert(
                    call_id.clone(),
                    (
                        name.clone(),
                        file_path.clone(),
                        start_time,
                        arguments.unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
                    ),
                );
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
                    *args = merge_args(args, &arguments_delta);
                }
                drop(state);
            }
            StreamEvent::ToolCallEnd { call_id, file_path } => {
                let (tool_name, original_file_path, start_time, accumulated_args) = {
                    let mut state = tool_call_state.lock().unwrap();
                    state.remove(&call_id).unwrap_or_else(|| {
                        (
                            String::new(),
                            file_path.clone(),
                            Instant::now(),
                            serde_json::Value::Object(serde_json::Map::new()),
                        )
                    })
                };
                let duration_ms = start_time.elapsed().as_millis() as u64;
                let (parsed_args, args_file_path) = parse_accumulated_args(&accumulated_args);
                let final_file_path = file_path.or(original_file_path).or(args_file_path);

                let result_summary = make_summary(&tool_name, &parsed_args, true);
                let output_preview = make_output_preview(&tool_name, &parsed_args);

                let _ = app_handle.emit(
                    "agent-tool-call",
                    AgentToolCallPayload::Complete {
                        run_id: run_id.clone(),
                        call_id,
                        tool_name: tool_name.clone(),
                        result_summary,
                        output_preview,
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
}
