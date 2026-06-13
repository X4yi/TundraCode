use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::capability::{Capability, FileAction};
use crate::policy::PermissionPolicy;
use tundracode_security::path_guard::{ensure_within_workspace, is_tundracode_path};

#[derive(Debug, Clone, Error)]
pub enum PermissionError {
    #[error("capability '{0}' not granted")]
    CapabilityDenied(String),

    #[error("path '{0}' is not allowed for action {1:?}")]
    PathDenied(PathBuf, FileAction),

    #[error("path '{0}' escapes workspace")]
    PathEscapesWorkspace(PathBuf),

    #[error("path '{0}' is a TundraCode internal path")]
    PathIsInternal(PathBuf),

    #[error("command '{0}' is not allowed")]
    CommandDenied(String),

    #[error("network access to host '{0}' is not allowed")]
    NetworkDenied(String),

    #[error("dry_run mode: write operation blocked")]
    DryRunBlocked,

    #[error("subagent profile '{0}' is not allowed")]
    SubagentProfileDenied(String),
}

#[derive(Debug, Clone)]
pub enum PermissionResult {
    Allowed,
    Denied(String),
}

impl PermissionResult {
    pub fn is_allowed(&self) -> bool {
        matches!(self, PermissionResult::Allowed)
    }

    pub fn into_result(self) -> Result<(), String> {
        match self {
            PermissionResult::Allowed => Ok(()),
            PermissionResult::Denied(e) => Err(e),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ToolAction {
    ReadFile { path: PathBuf },
    WriteFile { path: PathBuf },
    DeleteFile { path: PathBuf },
    ListDirectory { path: PathBuf },
    ApplyPatch { path: PathBuf },
    RunCommand { command: String, args: Vec<String> },
    SearchCodebase,
    SearchWeb { host: Option<String> },
    GetDiagnostics,
    SpawnSubagent { profile_id: String },
}

pub struct PermissionGuard {
    policy: PermissionPolicy,
    workspace: PathBuf,
}

impl PermissionGuard {
    pub fn new(policy: PermissionPolicy, workspace: PathBuf) -> Self {
        Self { policy, workspace }
    }

    pub fn policy(&self) -> &PermissionPolicy {
        &self.policy
    }

    pub fn check(&self, action: &ToolAction) -> PermissionResult {
        if self.policy.dry_run {
            if matches!(
                action,
                ToolAction::WriteFile { .. }
                    | ToolAction::DeleteFile { .. }
                    | ToolAction::ApplyPatch { .. }
            ) {
                return PermissionResult::Denied("dry_run mode: write operation blocked".to_string());
            }
        }

        match action {
            ToolAction::ReadFile { path } => self.check_file(path, FileAction::Read),
            ToolAction::WriteFile { path } => self.check_file(path, FileAction::Write),
            ToolAction::DeleteFile { path } => self.check_file(path, FileAction::Delete),
            ToolAction::ListDirectory { path } => self.check_file(path, FileAction::List),
            ToolAction::ApplyPatch { path } => self.check_file(path, FileAction::Patch),
            ToolAction::RunCommand { command, .. } => self.check_command(command),
            ToolAction::SearchCodebase => self.check_capability("search_codebase"),
            ToolAction::SearchWeb { host } => self.check_network(host.as_deref()),
            ToolAction::GetDiagnostics => self.check_capability("get_diagnostics"),
            ToolAction::SpawnSubagent { profile_id } => self.check_subagent_profile(profile_id),
        }
    }

    fn check_file(&self, path: &Path, action: FileAction) -> PermissionResult {
        if let Err(e) = ensure_within_workspace(path, &self.workspace) {
            return PermissionResult::Denied(format!("path escapes workspace: {}", e));
        }

        if is_tundracode_path(path) {
            return PermissionResult::Denied(format!("path '{}' is a TundraCode internal path", path.display()));
        }

        let cap_name = match action {
            FileAction::Read => "file_read",
            FileAction::Write => "file_write",
            FileAction::Delete => "file_delete",
            FileAction::List => "list_directory",
            FileAction::Patch => "apply_patch",
        };

        let cap = self.policy.get_capability(cap_name);
        match cap {
            Some(c) => {
                if c.allows_path(path) {
                    PermissionResult::Allowed
                } else {
                    PermissionResult::Denied(format!("path '{}' not allowed for action {:?}", path.display(), action))
                }
            }
            None => PermissionResult::Denied(format!("capability '{}' not granted", cap_name)),
        }
    }

    fn check_command(&self, command: &str) -> PermissionResult {
        let cap = self.policy.get_capability("command_execute");
        match cap {
            Some(c) => {
                if c.allows_command(command) {
                    PermissionResult::Allowed
                } else {
                    PermissionResult::Denied(format!("command '{}' not allowed", command))
                }
            }
            None => PermissionResult::Denied("capability 'command_execute' not granted".to_string()),
        }
    }

    fn check_network(&self, host: Option<&str>) -> PermissionResult {
        let cap = self.policy.get_capability("network_access");
        match cap {
            Some(c) => {
                match host {
                    Some(h) => {
                        if c.allows_host(h) {
                            PermissionResult::Allowed
                        } else {
                            PermissionResult::Denied(format!("network access to host '{}' not allowed", h))
                        }
                    }
                    None => PermissionResult::Allowed,
                }
            }
            None => {
                if self.policy.has_capability("search_web") {
                    PermissionResult::Allowed
                } else {
                    PermissionResult::Denied("capability 'network_access' not granted".to_string())
                }
            }
        }
    }

    fn check_subagent_profile(&self, profile_id: &str) -> PermissionResult {
        let cap = self.policy.get_capability("subagent_spawn");
        match cap {
            Some(Capability::SubagentSpawn { allowed_profiles }) => {
                if allowed_profiles.is_empty() || allowed_profiles.contains(&profile_id.to_string()) {
                    PermissionResult::Allowed
                } else {
                    PermissionResult::Denied(format!("subagent profile '{}' not allowed", profile_id))
                }
            }
            _ => PermissionResult::Denied("capability 'subagent_spawn' not granted".to_string()),
        }
    }

    fn check_capability(&self, name: &str) -> PermissionResult {
        if self.policy.has_capability(name) {
            PermissionResult::Allowed
        } else {
            PermissionResult::Denied(format!("capability '{}' not granted", name))
        }
    }

    pub fn is_dry_run(&self) -> bool {
        self.policy.dry_run
    }

    pub fn max_iterations(&self) -> usize {
        self.policy.max_iterations
    }

    pub fn budget_tokens(&self) -> u32 {
        self.policy.budget_tokens
    }
}
