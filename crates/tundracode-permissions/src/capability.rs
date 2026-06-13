use serde::{Deserialize, Serialize};
use globset::{Glob, GlobMatcher};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Capability {
    FileRead {
        path_pattern: Option<PathPattern>,
    },
    FileWrite {
        path_pattern: Option<PathPattern>,
    },
    FileDelete {
        path_pattern: Option<PathPattern>,
    },
    CommandExecute {
        allowed: Vec<CommandPattern>,
    },
    NetworkAccess {
        hosts: Vec<HostPattern>,
    },
    SubagentSpawn {
        allowed_profiles: Vec<String>,
    },
    SearchCodebase,
    SearchWeb,
    GetDiagnostics,
    ListDirectory {
        path_pattern: Option<PathPattern>,
    },
    ApplyPatch {
        path_pattern: Option<PathPattern>,
    },
}

impl Capability {
    pub fn name(&self) -> &'static str {
        match self {
            Capability::FileRead { .. } => "file_read",
            Capability::FileWrite { .. } => "file_write",
            Capability::FileDelete { .. } => "file_delete",
            Capability::CommandExecute { .. } => "command_execute",
            Capability::NetworkAccess { .. } => "network_access",
            Capability::SubagentSpawn { .. } => "subagent_spawn",
            Capability::SearchCodebase => "search_codebase",
            Capability::SearchWeb => "search_web",
            Capability::GetDiagnostics => "get_diagnostics",
            Capability::ListDirectory { .. } => "list_directory",
            Capability::ApplyPatch { .. } => "apply_patch",
        }
    }

    pub fn allows_path(&self, path: &Path) -> bool {
        match self {
            Capability::FileRead { path_pattern }
            | Capability::FileWrite { path_pattern }
            | Capability::FileDelete { path_pattern }
            | Capability::ListDirectory { path_pattern }
            | Capability::ApplyPatch { path_pattern } => {
                match path_pattern {
                    Some(pattern) => pattern.matches(path),
                    None => true,
                }
            }
            _ => true,
        }
    }

    pub fn allows_command(&self, cmd: &str) -> bool {
        match self {
            Capability::CommandExecute { allowed } => {
                if allowed.is_empty() {
                    return true;
                }
                allowed.iter().any(|p| p.matches(cmd))
            }
            _ => false,
        }
    }

    pub fn allows_host(&self, host: &str) -> bool {
        match self {
            Capability::NetworkAccess { hosts } => {
                if hosts.is_empty() {
                    return true;
                }
                hosts.iter().any(|p| p.matches(host))
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathPattern {
    #[serde(skip)]
    matcher: Option<GlobMatcher>,
    pattern: String,
}

impl PathPattern {
    pub fn new(pattern: &str) -> Result<Self, globset::Error> {
        let glob = Glob::new(pattern)?;
        let matcher = glob.compile_matcher();
        Ok(Self {
            matcher: Some(matcher),
            pattern: pattern.to_string(),
        })
    }

    pub fn matches(&self, path: &Path) -> bool {
        match &self.matcher {
            Some(m) => m.is_match(path),
            None => true,
        }
    }

    pub fn pattern(&self) -> &str {
        &self.pattern
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandPattern {
    pattern: String,
    #[serde(skip)]
    compiled: Option<regex::Regex>,
}

impl CommandPattern {
    pub fn exact(cmd: &str) -> Self {
        Self {
            pattern: cmd.to_string(),
            compiled: None,
        }
    }

    pub fn prefix(prefix: &str) -> Self {
        Self {
            pattern: format!("{}*", prefix),
            compiled: regex::Regex::new(&format!(r"^{}", regex::escape(prefix))).ok(),
        }
    }

    pub fn regex(pattern: &str) -> Result<Self, regex::Error> {
        let compiled = regex::Regex::new(pattern)?;
        Ok(Self {
            pattern: pattern.to_string(),
            compiled: Some(compiled),
        })
    }

    pub fn matches(&self, cmd: &str) -> bool {
        match &self.compiled {
            Some(re) => re.is_match(cmd),
            None => cmd.starts_with(&self.pattern.trim_end_matches('*')),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostPattern {
    pattern: String,
    #[serde(skip)]
    compiled: Option<regex::Regex>,
}

impl HostPattern {
    pub fn exact(host: &str) -> Self {
        Self {
            pattern: host.to_string(),
            compiled: None,
        }
    }

    pub fn wildcard(domain: &str) -> Self {
        Self {
            pattern: format!("*.{}", domain),
            compiled: regex::Regex::new(&format!(r"\.{}$", regex::escape(domain))).ok(),
        }
    }

    pub fn matches(&self, host: &str) -> bool {
        match &self.compiled {
            Some(re) => re.is_match(host),
            None => host == self.pattern,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileAction {
    Read,
    Write,
    Delete,
    List,
    Patch,
}
