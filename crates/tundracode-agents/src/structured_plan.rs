use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanTask {
    pub number: usize,
    pub title: String,
    pub goal: String,
    pub files: Vec<String>,
    pub depends_on: Vec<usize>,
    pub acceptance_criteria: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanFrontmatter {
    pub generated_at: String,
    pub provider: String,
    pub estimated_build_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct ParsedPlan {
    pub frontmatter: Option<PlanFrontmatter>,
    pub tasks: Vec<PlanTask>,
    pub raw_content: String,
}

impl ParsedPlan {
    pub fn from_markdown(content: &str) -> Self {
        let frontmatter = parse_frontmatter(content);
        let tasks = parse_tasks(content);

        if tasks.is_empty() {
            ParsedPlan {
                frontmatter,
                tasks: vec![PlanTask {
                    number: 1,
                    title: "Implement plan".to_string(),
                    goal: "Implement the entire plan as described".to_string(),
                    files: Vec::new(),
                    depends_on: Vec::new(),
                    acceptance_criteria: "Plan implemented successfully".to_string(),
                }],
                raw_content: content.to_string(),
            }
        } else {
            ParsedPlan {
                frontmatter,
                tasks,
                raw_content: content.to_string(),
            }
        }
    }

    pub fn task_section(&self, task_number: usize) -> Option<String> {
        self.tasks
            .iter()
            .find(|t| t.number == task_number)
            .map(|task| {
                let mut section = format!("### Step {}: {}\n", task.number, task.title);
                if !task.goal.is_empty() {
                    section.push_str(&format!("- **Goal:** {}\n", task.goal));
                }
                if !task.files.is_empty() {
                    section.push_str(&format!("- **Files:** {}\n", task.files.join(", ")));
                }
                if !task.depends_on.is_empty() {
                    let deps: Vec<String> = task.depends_on.iter().map(|d| d.to_string()).collect();
                    section.push_str(&format!("- **Depends on:** {}\n", deps.join(", ")));
                }
                if !task.acceptance_criteria.is_empty() {
                    section.push_str(&format!(
                        "- **Acceptance:** {}\n",
                        task.acceptance_criteria
                    ));
                }
                section
            })
    }

    pub fn context_summary(&self, completed_tasks: &[usize]) -> String {
        let mut summary = String::new();
        for &num in completed_tasks {
            if let Some(task) = self.tasks.iter().find(|t| t.number == num) {
                summary.push_str(&format!(
                    "Step {}: {} ({}). ",
                    task.number, task.title, task.goal
                ));
            }
        }
        if summary.is_empty() {
            "No previous steps completed yet.".to_string()
        } else {
            summary
        }
    }
}

fn parse_frontmatter(content: &str) -> Option<PlanFrontmatter> {
    if !content.starts_with("---") {
        return None;
    }

    let rest = &content[3..];
    let end_idx = rest.find("\n---")?;
    let yaml_block = &rest[..end_idx];

    let mut generated_at = String::new();
    let mut provider = String::new();
    let mut estimated_build_tokens = 0;

    for line in yaml_block.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("generated_at:") {
            generated_at = val.trim().trim_matches('"').trim_matches('\'').to_string();
        } else if let Some(val) = line.strip_prefix("provider:") {
            provider = val.trim().trim_matches('"').trim_matches('\'').to_string();
        } else if let Some(val) = line.strip_prefix("estimated_build_tokens:") {
            estimated_build_tokens = val.trim().parse().unwrap_or(0);
        }
    }

    if provider.is_empty() {
        return None;
    }

    Some(PlanFrontmatter {
        generated_at,
        provider,
        estimated_build_tokens,
    })
}

fn parse_tasks(content: &str) -> Vec<PlanTask> {
    let mut tasks = Vec::new();
    let mut current_task: Option<PlanTask> = None;
    let mut current_field = String::new();
    let mut current_field_value = String::new();

    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Support both "### Task N:" and "### Step N:" headers
        let task_prefix = if line.starts_with("### Task ") {
            Some(line.trim_start_matches("### Task "))
        } else if line.starts_with("### Step ") {
            Some(line.trim_start_matches("### Step "))
        } else {
            None
        };

        if let Some(header) = task_prefix {
            if let Some(task) = current_task.take() {
                flush_field(&mut current_task, &current_field, &current_field_value);
                tasks.push(task);
            }

            let parts: Vec<&str> = header.splitn(2, ':').collect();
            let number = parts[0]
                .trim()
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0);
            let title = parts.get(1).map(|s| s.trim().to_string()).unwrap_or_default();

            current_task = Some(PlanTask {
                number,
                title,
                goal: String::new(),
                files: Vec::new(),
                depends_on: Vec::new(),
                acceptance_criteria: String::new(),
            });
            current_field.clear();
            current_field_value.clear();
        } else if current_task.is_some() {
            let trimmed = line.trim();

            if trimmed.starts_with("- **Goal:**") {
                flush_field(&mut current_task, &current_field, &current_field_value);
                current_field = "goal".to_string();
                current_field_value = trimmed.trim_start_matches("- **Goal:**").trim().to_string();
            } else if trimmed.starts_with("- **Archivos:**") || trimmed.starts_with("- **Files:**") {
                flush_field(&mut current_task, &current_field, &current_field_value);
                current_field = "files".to_string();
                let prefix = if trimmed.starts_with("- **Archivos:**") {
                    "- **Archivos:**"
                } else {
                    "- **Files:**"
                };
                current_field_value = trimmed
                    .trim_start_matches(prefix)
                    .trim()
                    .to_string();
            } else if trimmed.starts_with("- **Herramientas:**") || trimmed.starts_with("- **Tools:**") {
                flush_field(&mut current_task, &current_field, &current_field_value);
                current_field = "tools".to_string();
                current_field_value.clear();
            } else if trimmed.starts_with("- **Depende de:**") || trimmed.starts_with("- **Depends on:**") || trimmed.starts_with("- **Dependencies:**") {
                flush_field(&mut current_task, &current_field, &current_field_value);
                current_field = "depends".to_string();
                let prefix = if trimmed.starts_with("- **Depende de:**") {
                    "- **Depende de:**"
                } else if trimmed.starts_with("- **Depends on:**") {
                    "- **Depends on:**"
                } else {
                    "- **Dependencies:**"
                };
                current_field_value = trimmed
                    .trim_start_matches(prefix)
                    .trim()
                    .to_string();
            } else if trimmed.starts_with("- **Criterio de aceptacion:**") || trimmed.starts_with("- **Acceptance:**") || trimmed.starts_with("- **Acceptance criteria:**") {
                flush_field(&mut current_task, &current_field, &current_field_value);
                current_field = "acceptance".to_string();
                let prefix = if trimmed.starts_with("- **Criterio de aceptacion:**") {
                    "- **Criterio de aceptacion:**"
                } else if trimmed.starts_with("- **Acceptance criteria:**") {
                    "- **Acceptance criteria:**"
                } else {
                    "- **Acceptance:**"
                };
                current_field_value = trimmed
                    .trim_start_matches(prefix)
                    .trim()
                    .to_string();
            } else if !trimmed.is_empty()
                && !trimmed.starts_with("###")
                && !trimmed.starts_with("##")
                && !trimmed.starts_with("#")
                && !trimmed.starts_with("- **")
            {
                if !current_field_value.is_empty() {
                    current_field_value.push(' ');
                }
                current_field_value.push_str(trimmed);
            }
        }

        i += 1;
    }

    if let Some(task) = current_task.take() {
        flush_field(&mut current_task, &current_field, &current_field_value);
        tasks.push(task);
    }

    tasks
}

fn flush_field(
    current_task: &mut Option<PlanTask>,
    field: &str,
    value: &str,
) {
    let task = match current_task.as_mut() {
        Some(t) => t,
        None => return,
    };

    let value = value.trim();
    if value.is_empty() {
        return;
    }

    match field {
        "goal" => task.goal = value.to_string(),
        "files" => {
            task.files = value
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
        "depends" => {
            if value.to_lowercase() == "ninguna" || value.to_lowercase() == "none" {
                task.depends_on = Vec::new();
            } else {
                task.depends_on = value
                    .split(|c: char| !c.is_ascii_digit())
                    .filter(|s| !s.is_empty())
                    .filter_map(|s| s.parse::<usize>().ok())
                    .collect();
            }
        }
        "acceptance" => task.acceptance_criteria = value.to_string(),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_plan() -> String {
        r#"---
generated_at: 2026-06-08T00:00:00Z
provider: openai/gpt-4
estimated_build_tokens: 15000
---

## Stack
Rust + Tauri

## Implementation Steps

### Step 1: Create module structure
**Goal:** Create the structured_plan module
**Files:** crates/tundracode-agents/src/structured_plan.rs, crates/tundracode-agents/src/lib.rs

### Step 2: Implement parser
**Goal:** Parse markdown plan into structured tasks
**Files:** crates/tundracode-agents/src/structured_plan.rs

### Step 3: Add tests
**Goal:** Add comprehensive unit tests
**Files:** crates/tundracode-agents/src/structured_plan.rs
"#
        .to_string()
    }

    #[test]
    fn test_parse_plan_with_frontmatter() {
        let plan = ParsedPlan::from_markdown(&sample_plan());
        assert!(plan.frontmatter.is_some());
        let fm = plan.frontmatter.unwrap();
        assert_eq!(fm.provider, "openai/gpt-4");
        assert_eq!(fm.estimated_build_tokens, 15000);
    }

    #[test]
    fn test_parse_step_headers() {
        let plan = ParsedPlan::from_markdown(&sample_plan());
        assert_eq!(plan.tasks.len(), 3);

        let t1 = &plan.tasks[0];
        assert_eq!(t1.number, 1);
        assert_eq!(t1.title, "Create module structure");
        assert!(t1.goal.contains("structured_plan module"));
    }

    #[test]
    fn test_parse_task_headers_backward_compat() {
        let content = r#"
### Task 1: Old format task
- **Goal:** Do the thing
- **Archivos:** src/main.rs
- **Depende de:** ninguna
- **Criterio de aceptacion:** It works
"#
        .to_string();
        let plan = ParsedPlan::from_markdown(&content);
        assert_eq!(plan.tasks.len(), 1);
        assert_eq!(plan.tasks[0].number, 1);
        assert_eq!(plan.tasks[0].title, "Old format task");
    }

    #[test]
    fn test_parse_plan_without_tasks() {
        let content = "Just a plain text plan without structured tasks.".to_string();
        let plan = ParsedPlan::from_markdown(&content);
        assert_eq!(plan.tasks.len(), 1);
        assert_eq!(plan.tasks[0].number, 1);
        assert_eq!(plan.tasks[0].title, "Implement plan");
    }

    #[test]
    fn test_task_section() {
        let plan = ParsedPlan::from_markdown(&sample_plan());
        let section = plan.task_section(2);
        assert!(section.is_some());
        let section = section.unwrap();
        assert!(section.contains("Step 2"));
        assert!(section.contains("Implement parser"));
    }

    #[test]
    fn test_context_summary() {
        let plan = ParsedPlan::from_markdown(&sample_plan());
        let summary = plan.context_summary(&[1, 2]);
        assert!(summary.contains("Step 1"));
        assert!(summary.contains("Step 2"));
    }
}
