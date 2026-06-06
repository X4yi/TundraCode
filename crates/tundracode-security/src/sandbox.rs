use std::path::{Path, PathBuf};

const DANGEROUS_COMMANDS: &[&str] = &[
    "sudo ",
    "sudo\t",
    "rm -rf /",
    "rm -rf ~",
    "rm -rf $HOME",
    "rm -rf /home",
    ":(){:|:&};:",
    "mkfs",
    "dd if=",
    "chmod 777 /",
    "chown root",
    "mount ",
    "umount ",
    "iptables ",
    "kill -9 1",
];

pub struct CommandSandbox {
    workspace_path: PathBuf,
    allow_network: bool,
}

impl CommandSandbox {
    pub fn new(workspace_path: impl Into<PathBuf>) -> Self {
        Self {
            workspace_path: workspace_path.into(),
            allow_network: false,
        }
    }

    pub fn with_network(mut self, allow: bool) -> Self {
        self.allow_network = allow;
        self
    }

    pub fn is_path_allowed(&self, path: &Path) -> bool {
        path.starts_with(&self.workspace_path)
    }

    pub fn is_network_allowed(&self) -> bool {
        self.allow_network
    }

    pub fn validate_command(&self, command: &str, args: &[String]) -> Result<(), String> {
        let full_command = format!(
            "{} {}",
            command,
            args.join(" ")
        )
        .trim()
        .to_string();

        for pattern in DANGEROUS_COMMANDS {
            if full_command.contains(pattern) {
                return Err(format!(
                    "Command blocked by sandbox: contains dangerous pattern '{}'",
                    pattern
                ));
            }
        }

        Ok(())
    }

    pub fn sanitize_env(&self) -> Vec<(String, String)> {
        let mut allowed = Vec::new();

        for key in &["PATH", "HOME", "USER", "LANG", "TERM", "SHELL"] {
            if let Ok(val) = std::env::var(key) {
                allowed.push((key.to_string(), val));
            }
        }

        allowed
    }
}
