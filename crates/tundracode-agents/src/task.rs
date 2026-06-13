use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::structured_plan::PlanTask;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
    Paused,
}

impl TaskStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
        )
    }

    pub fn is_active(&self) -> bool {
        matches!(self, TaskStatus::Running)
    }

    pub fn checkbox(&self) -> &'static str {
        match self {
            TaskStatus::Completed => "[x]",
            TaskStatus::Failed => "[!]",
            TaskStatus::Cancelled => "[-]",
            TaskStatus::Running => "[>]",
            TaskStatus::Paused => "[||]",
            TaskStatus::Pending => "[ ]",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub number: usize,
    pub id: String,
    pub title: String,
    pub objective: String,
    pub status: TaskStatus,
    pub created_at: String,
    pub updated_at: String,
    pub result_summary: Option<String>,
    pub error: Option<String>,
    pub dependencies: Vec<usize>,
    pub files: Vec<String>,
    pub assigned_agent: Option<String>,
    pub token_usage: u32,
    pub acceptance_criteria: String,
}

impl Task {
    pub fn from_plan_task(plan: PlanTask, index: usize) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            number: plan.number,
            id: format!("task_{}", index + 1),
            title: plan.title.clone(),
            objective: plan.goal.clone(),
            status: TaskStatus::Pending,
            created_at: now.clone(),
            updated_at: now,
            result_summary: None,
            error: None,
            dependencies: plan.depends_on.clone(),
            files: plan.files.clone(),
            assigned_agent: None,
            token_usage: 0,
            acceptance_criteria: plan.acceptance_criteria.clone(),
        }
    }

    pub fn new_standalone(title: String, objective: String, index: usize) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            number: index + 1,
            id: format!("task_{}", index + 1),
            title,
            objective,
            status: TaskStatus::Pending,
            created_at: now.clone(),
            updated_at: now,
            result_summary: None,
            error: None,
            dependencies: vec![],
            files: vec![],
            assigned_agent: None,
            token_usage: 0,
            acceptance_criteria: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStore {
    tasks: Vec<Task>,
    current_task: Option<usize>,
}

impl TaskStore {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            current_task: None,
        }
    }

    pub fn from_plan_tasks(plan_tasks: Vec<PlanTask>) -> Self {
        let tasks = plan_tasks
            .into_iter()
            .enumerate()
            .map(|(i, p)| Task::from_plan_task(p, i))
            .collect();

        Self {
            tasks,
            current_task: None,
        }
    }

    pub fn add_task(&mut self, title: String, objective: String, dependencies: Vec<usize>) -> &Task {
        let index = self.tasks.len();
        let task = Task::new_standalone(title, objective, index);
        let task = Task {
            dependencies,
            ..task
        };
        self.tasks.push(task);
        self.tasks.last().unwrap()
    }

    pub fn get_task(&self, id: &str) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
    }

    pub fn get_task_by_number(&self, number: usize) -> Option<&Task> {
        self.tasks.iter().find(|t| t.number == number)
    }

    pub fn get_task_mut(&mut self, id: &str) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|t| t.id == id)
    }

    pub fn get_task_by_number_mut(&mut self, number: usize) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|t| t.number == number)
    }

    pub fn mark_running(&mut self, number: usize) -> bool {
        if let Some(task) = self.get_task_by_number_mut(number) {
            task.status = TaskStatus::Running;
            task.updated_at = chrono::Utc::now().to_rfc3339();
            self.current_task = Some(number);
            true
        } else {
            false
        }
    }

    pub fn mark_completed(&mut self, number: usize, summary: String) -> bool {
        if let Some(task) = self.get_task_by_number_mut(number) {
            task.status = TaskStatus::Completed;
            task.result_summary = Some(summary);
            task.updated_at = chrono::Utc::now().to_rfc3339();
            if self.current_task == Some(number) {
                self.current_task = None;
            }
            true
        } else {
            false
        }
    }

    pub fn mark_failed(&mut self, number: usize, error: String) -> bool {
        if let Some(task) = self.get_task_by_number_mut(number) {
            task.status = TaskStatus::Failed;
            task.error = Some(error);
            task.updated_at = chrono::Utc::now().to_rfc3339();
            if self.current_task == Some(number) {
                self.current_task = None;
            }
            true
        } else {
            false
        }
    }

    pub fn mark_cancelled(&mut self, number: usize) -> bool {
        if let Some(task) = self.get_task_by_number_mut(number) {
            task.status = TaskStatus::Cancelled;
            task.updated_at = chrono::Utc::now().to_rfc3339();
            if self.current_task == Some(number) {
                self.current_task = None;
            }
            true
        } else {
            false
        }
    }

    pub fn mark_paused(&mut self, number: usize) -> bool {
        if let Some(task) = self.get_task_by_number_mut(number) {
            task.status = TaskStatus::Paused;
            task.updated_at = chrono::Utc::now().to_rfc3339();
            true
        } else {
            false
        }
    }

    pub fn next_available_task(&self) -> Option<&Task> {
        for task in &self.tasks {
            if task.status != TaskStatus::Pending {
                continue;
            }

            let deps_satisfied = task.dependencies.iter().all(|dep_num| {
                self.tasks
                    .iter()
                    .find(|t| t.number == *dep_num)
                    .map(|t| t.status == TaskStatus::Completed)
                    .unwrap_or(false)
            });

            if deps_satisfied {
                return Some(task);
            }
        }
        None
    }

    pub fn is_complete(&self) -> bool {
        self.tasks.iter().all(|t| t.status.is_terminal())
    }

    pub fn progress(&self) -> (usize, usize) {
        let completed = self
            .tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Completed)
            .count();
        (completed, self.tasks.len())
    }

    pub fn progress_summary(&self) -> String {
        let total = self.tasks.len();
        let completed = self
            .tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Completed)
            .count();
        let failed = self
            .tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Failed)
            .count();
        let pending = self
            .tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Pending)
            .count();
        let running = self
            .tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Running)
            .count();

        format!(
            "{}/{} completed, {} running, {} failed, {} pending",
            completed, total, running, failed, pending
        )
    }

    pub fn compact_summary(&self) -> String {
        let mut lines = Vec::new();
        for task in &self.tasks {
            lines.push(format!("{} {}", task.status.checkbox(), task.title));
        }
        lines.join("\n")
    }

    pub fn current_task_number(&self) -> Option<usize> {
        self.current_task
    }

    pub fn all_tasks(&self) -> &[Task] {
        &self.tasks
    }

    pub fn completed_task_numbers(&self) -> Vec<usize> {
        self.tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Completed)
            .map(|t| t.number)
            .collect()
    }

    pub fn has_blocked_tasks(&self) -> bool {
        self.tasks.iter().any(|t| {
            if t.status != TaskStatus::Pending {
                return false;
            }
            t.dependencies.iter().any(|dep_num| {
                self.tasks
                    .iter()
                    .find(|t| t.number == *dep_num)
                    .map(|t| t.status == TaskStatus::Failed)
                    .unwrap_or(false)
            })
        })
    }

    pub fn total_tokens(&self) -> u32 {
        self.tasks.iter().map(|t| t.token_usage).sum()
    }

    pub fn save_to_file(&self, path: &Path) -> Result<(), String> {
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize tasks: {}", e))?;
        std::fs::write(path, content).map_err(|e| format!("Failed to write tasks: {}", e))
    }

    pub fn load_from_file(path: &Path) -> Result<Self, String> {
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("Failed to read tasks: {}", e))?;
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse tasks: {}", e))
    }
}

impl Default for TaskStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_plan_tasks() -> Vec<PlanTask> {
        vec![
            PlanTask {
                number: 1,
                title: "Task A".to_string(),
                goal: "Do A".to_string(),
                files: vec!["src/a.rs".to_string()],
                depends_on: vec![],
                acceptance_criteria: "A works".to_string(),
            },
            PlanTask {
                number: 2,
                title: "Task B".to_string(),
                goal: "Do B".to_string(),
                files: vec!["src/b.rs".to_string()],
                depends_on: vec![1],
                acceptance_criteria: "B works".to_string(),
            },
            PlanTask {
                number: 3,
                title: "Task C".to_string(),
                goal: "Do C".to_string(),
                files: vec!["src/c.rs".to_string()],
                depends_on: vec![1],
                acceptance_criteria: "C works".to_string(),
            },
            PlanTask {
                number: 4,
                title: "Task D".to_string(),
                goal: "Do D".to_string(),
                files: vec!["src/d.rs".to_string()],
                depends_on: vec![2, 3],
                acceptance_criteria: "D works".to_string(),
            },
        ]
    }

    #[test]
    fn test_create_task() {
        let mut store = TaskStore::new();
        store.add_task("Task 1".to_string(), "Do something".to_string(), vec![]);
        assert_eq!(store.all_tasks().len(), 1);
        assert_eq!(store.all_tasks()[0].status, TaskStatus::Pending);
    }

    #[test]
    fn test_from_plan_tasks() {
        let store = TaskStore::from_plan_tasks(sample_plan_tasks());
        assert_eq!(store.all_tasks().len(), 4);
        assert_eq!(store.all_tasks()[0].number, 1);
        assert_eq!(store.all_tasks()[1].dependencies, vec![1]);
    }

    #[test]
    fn test_dependency_resolution() {
        let mut store = TaskStore::from_plan_tasks(sample_plan_tasks());

        let next = store.next_available_task();
        assert!(next.is_some());
        assert_eq!(next.unwrap().number, 1);

        store.mark_running(1);
        store.mark_completed(1, "Done".to_string());

        let next = store.next_available_task();
        assert!(next.is_some());
        let num = next.unwrap().number;
        assert!(num == 2 || num == 3);
    }

    #[test]
    fn test_compact_summary() {
        let mut store = TaskStore::from_plan_tasks(sample_plan_tasks());
        store.mark_running(1);
        store.mark_completed(1, "Done".to_string());

        let summary = store.compact_summary();
        assert!(summary.contains("[x] Task A"));
        assert!(summary.contains("[ ] Task B"));
    }

    #[test]
    fn test_progress() {
        let mut store = TaskStore::from_plan_tasks(sample_plan_tasks());

        store.mark_running(1);
        store.mark_completed(1, "Done".to_string());
        store.mark_failed(2, "Error".to_string());

        assert!(store.progress_summary().contains("1/4 completed"));
        assert!(store.progress_summary().contains("1 failed"));
        assert!(!store.is_complete());
    }

    #[test]
    fn test_failed_task_blocks_dependents() {
        let mut store = TaskStore::from_plan_tasks(sample_plan_tasks());

        store.mark_running(1);
        store.mark_failed(1, "Build error".to_string());

        assert!(store.next_available_task().is_none());
        assert!(!store.is_complete());
        assert!(store.has_blocked_tasks());
    }

    #[test]
    fn test_sequential_completion() {
        let mut store = TaskStore::from_plan_tasks(sample_plan_tasks());

        store.mark_running(1);
        store.mark_completed(1, "Done".to_string());

        store.mark_running(2);
        store.mark_completed(2, "Done".to_string());

        store.mark_running(3);
        store.mark_completed(3, "Done".to_string());

        let next = store.next_available_task();
        assert!(next.is_some());
        assert_eq!(next.unwrap().number, 4);

        store.mark_running(4);
        store.mark_completed(4, "Done".to_string());

        assert!(store.is_complete());
        assert_eq!(store.progress(), (4, 4));
    }
}
