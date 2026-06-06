use async_trait::async_trait;
use serde_json::Value;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

use crate::{Tool, ToolContext, ToolError, ToolResult};

pub struct SearchCodebaseTool;

#[async_trait]
impl Tool for SearchCodebaseTool {
    fn name(&self) -> &'static str {
        "SearchCodebase"
    }
    fn description(&self) -> &'static str {
        "Busca en el codebase usando ripgrep. Encuentra patrones, usos, funciones relacionadas."
    }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Termino de busqueda" },
                "file_pattern": { "type": "string", "description": "Filtro de archivos (ej: '*.rs')" }
            },
            "required": ["query"]
        })
    }
    async fn execute(&self, context: &ToolContext, params: Value) -> Result<ToolResult, ToolError> {
        let query = params["query"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParameters("query required".to_string()))?;
        let file_pattern = params["file_pattern"].as_str();

        let workspace = Path::new(&context.workspace_path);

        let mut cmd = Command::new("rg");
        cmd.arg("--json")
            .arg("--line-number")
            .arg("--context")
            .arg("2")
            .arg("--max-count")
            .arg("50")
            .arg("--glob")
            .arg("!.tundracode/**")
            .arg("--glob")
            .arg("!.git/**")
            .arg("--glob")
            .arg("!target/**")
            .arg("--glob")
            .arg("!node_modules/**")
            .arg(query)
            .current_dir(workspace)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(pattern) = file_pattern {
            cmd.arg("--glob").arg(pattern);
        }

        let output = cmd
            .output()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to run rg: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() && !stderr.is_empty() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("ripgrep error: {}", stderr)),
                prior_content: None,
                file_path: None,
            });
        }

        if stdout.is_empty() {
            return Ok(ToolResult {
                success: true,
                output: format!("No results for '{}'", query),
                error: None,
                prior_content: None,
                file_path: None,
            });
        }

        let mut results = Vec::new();
        for line in stdout.lines() {
            if let Ok(json) = serde_json::from_str::<Value>(line) {
                if let Some(data) = json.get("data") {
                    if let (Some(path), Some(line_num), Some(text)) = (
                        data.get("path")
                            .and_then(|p| p.get("text"))
                            .and_then(|t| t.as_str()),
                        data.get("line_number").and_then(|l| l.as_u64()),
                        data.get("lines")
                            .and_then(|l| l.get("text"))
                            .and_then(|t| t.as_str()),
                    ) {
                        results.push(format!("{}:{}: {}", path, line_num, text.trim()));
                    }
                }
            }
        }

        if results.is_empty() {
            Ok(ToolResult {
                success: true,
                output: format!("No structured results for '{}'", query),
                error: None,
                prior_content: None,
                file_path: None,
            })
        } else {
            Ok(ToolResult {
                success: true,
                output: results.join("\n"),
                error: None,
                prior_content: None,
                file_path: None,
            })
        }
    }
}

pub struct SearchInWebTool;

#[async_trait]
impl Tool for SearchInWebTool {
    fn name(&self) -> &'static str {
        "SearchInWeb"
    }
    fn description(&self) -> &'static str {
        "Busqueda web para investigaciones. Usa DuckDuckGo HTML (best-effort, sin API key)."
    }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" }
            },
            "required": ["query"]
        })
    }
    async fn execute(
        &self,
        _context: &ToolContext,
        params: Value,
    ) -> Result<ToolResult, ToolError> {
        let query = params["query"].as_str().unwrap_or("");

        let url = format!(
            "https://html.duckduckgo.com/html/?q={}",
            urlencoding::encode(query)
        );

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to create client: {}", e)))?;

        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Search request failed: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Search returned status: {}", status)),
                prior_content: None,
                file_path: None,
            });
        }

        let html = response
            .text()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to read response: {}", e)))?;

        let results = parse_duckduckgo_results(&html);

        if results.is_empty() {
            Ok(ToolResult {
                success: true,
                output: format!("No web results for '{}'", query),
                error: None,
                prior_content: None,
                file_path: None,
            })
        } else {
            let formatted = results
                .iter()
                .map(|r| format!("- {} ({})\n  {}", r.title, r.url, r.snippet))
                .take(10)
                .collect::<Vec<_>>()
                .join("\n\n");
            Ok(ToolResult {
                success: true,
                output: formatted,
                error: None,
                prior_content: None,
                file_path: None,
            })
        }
    }
}

struct WebResult {
    title: String,
    url: String,
    snippet: String,
}

fn parse_duckduckgo_results(html: &str) -> Vec<WebResult> {
    let mut results = Vec::new();

    let doc = scraper::Html::parse_document(html);
    let result_selector = scraper::Selector::parse(".result").unwrap();
    let title_selector = scraper::Selector::parse(".result__a").unwrap();
    let snippet_selector = scraper::Selector::parse(".result__snippet").unwrap();

    for result in doc.select(&result_selector) {
        if let (Some(title_el), Some(snippet_el)) = (
            result.select(&title_selector).next(),
            result.select(&snippet_selector).next(),
        ) {
            let title = title_el
                .text()
                .collect::<Vec<_>>()
                .join(" ")
                .trim()
                .to_string();
            let url = title_el.value().attr("href").unwrap_or("").to_string();
            let snippet = snippet_el
                .text()
                .collect::<Vec<_>>()
                .join(" ")
                .trim()
                .to_string();

            if !title.is_empty() {
                results.push(WebResult {
                    title,
                    url,
                    snippet,
                });
            }
        }
    }

    results
}
