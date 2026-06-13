use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildConfig {
    pub project_type: ProjectType,
    pub build_command: String,
    pub test_command: String,
    pub has_tests: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectType {
    RustCargo,
    NodeNpm,
    NodeYarn,
    Python,
    Go,
    Unknown,
}

impl BuildConfig {
    pub fn detect(workspace_path: &str) -> Self {
        let workspace = Path::new(workspace_path);

        if workspace.join("Cargo.toml").exists() {
            return Self::detect_rust(workspace);
        }
        if workspace.join("package.json").exists() {
            return Self::detect_node(workspace);
        }
        if workspace.join("go.mod").exists() {
            return Self::detect_go();
        }
        if workspace.join("pyproject.toml").exists()
            || workspace.join("setup.py").exists()
            || workspace.join("requirements.txt").exists()
        {
            return Self::detect_python(workspace);
        }

        BuildConfig {
            project_type: ProjectType::Unknown,
            build_command: String::new(),
            test_command: String::new(),
            has_tests: false,
        }
    }

    fn detect_rust(workspace: &Path) -> Self {
        let has_tests = Self::rust_has_tests(workspace);
        BuildConfig {
            project_type: ProjectType::RustCargo,
            build_command: "cargo build".to_string(),
            test_command: if has_tests {
                "cargo test".to_string()
            } else {
                "cargo check".to_string()
            },
            has_tests,
        }
    }

    fn rust_has_tests(workspace: &Path) -> bool {
        if workspace.join("tests").is_dir() {
            return true;
        }
        if let Ok(entries) = std::fs::read_dir(workspace.join("src")) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with("test_") || name.ends_with("_test.rs") {
                        return true;
                    }
                }
            }
        }
        if let Ok(content) = std::fs::read_to_string(workspace.join("Cargo.toml")) {
            if content.contains("[dev-dependencies]") {
                return true;
            }
        }
        false
    }

    fn detect_node(workspace: &Path) -> Self {
        let has_yarn = workspace.join("yarn.lock").exists();
        let has_tests = Self::node_has_tests(workspace);
        let pkg = Self::read_package_json(workspace);

        let build_cmd = if has_yarn {
            "yarn build"
        } else {
            "npm run build"
        };

        let test_cmd = if has_yarn {
            "yarn test"
        } else {
            "npm test"
        };

        let build_command = if let Some(ref p) = pkg {
            if p.scripts.contains_key("build") {
                build_cmd.to_string()
            } else {
                String::new()
            }
        } else {
            build_cmd.to_string()
        };

        let test_command = if has_tests {
            test_cmd.to_string()
        } else {
            String::new()
        };

        BuildConfig {
            project_type: if has_yarn {
                ProjectType::NodeYarn
            } else {
                ProjectType::NodeNpm
            },
            build_command,
            test_command,
            has_tests,
        }
    }

    fn node_has_tests(workspace: &Path) -> bool {
        if workspace.join("test").is_dir() || workspace.join("tests").is_dir() {
            return true;
        }
        if let Ok(content) = std::fs::read_to_string(workspace.join("package.json")) {
            if content.contains("\"test\"") || content.contains("\"jest\"") || content.contains("\"vitest\"") || content.contains("\"mocha\"") {
                return true;
            }
        }
        false
    }

    fn read_package_json(workspace: &Path) -> Option<PackageJson> {
        let path = workspace.join("package.json");
        let content = std::fs::read_to_string(&path).ok()?;
        let pkg: PackageJson = serde_json::from_str(&content).ok()?;
        Some(pkg)
    }

    fn detect_python(workspace: &Path) -> Self {
        let has_tests = Self::python_has_tests(workspace);
        let test_cmd = if workspace.join("pytest.ini").exists()
            || workspace.join("pyproject.toml").exists()
        {
            "pytest"
        } else if workspace.join("tox.ini").exists() {
            "tox"
        } else {
            "python -m unittest discover"
        };

        BuildConfig {
            project_type: ProjectType::Python,
            build_command: String::new(),
            test_command: if has_tests {
                test_cmd.to_string()
            } else {
                String::new()
            },
            has_tests,
        }
    }

    fn python_has_tests(workspace: &Path) -> bool {
        if workspace.join("tests").is_dir() || workspace.join("test").is_dir() {
            return true;
        }
        if let Ok(entries) = std::fs::read_dir(workspace) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with("test_") && name.ends_with(".py") {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn detect_go() -> Self {
        BuildConfig {
            project_type: ProjectType::Go,
            build_command: "go build ./...".to_string(),
            test_command: "go test ./...".to_string(),
            has_tests: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct PackageJson {
    #[serde(default)]
    scripts: std::collections::HashMap<String, String>,
}

impl PackageJson {
    fn new() -> Self {
        PackageJson {
            scripts: std::collections::HashMap::new(),
        }
    }
}

impl Default for PackageJson {
    fn default() -> Self {
        Self::new()
    }
}

/// Generates AGENTS.md content for a project by detecting conventions.
pub fn generate_agents_md(workspace_path: &str) -> String {
    let config = BuildConfig::detect(workspace_path);
    let type_name = match config.project_type {
        ProjectType::RustCargo => "Rust (Cargo)",
        ProjectType::NodeNpm => "Node.js (npm)",
        ProjectType::NodeYarn => "Node.js (Yarn)",
        ProjectType::Python => "Python",
        ProjectType::Go => "Go",
        ProjectType::Unknown => "Unknown",
    };

    let mut lines = Vec::new();
    lines.push("# Project Conventions".to_string());
    lines.push(String::new());
    lines.push(format!("Project type: {}", type_name));
    lines.push(String::new());

    if !config.build_command.is_empty() {
        lines.push(format!("Build: `{}`", config.build_command));
    }
    if !config.test_command.is_empty() {
        lines.push(format!("Test: `{}`", config.test_command));
    }
    if config.has_tests {
        lines.push("Tests: Yes".to_string());
    }
    lines.push(String::new());

    // Detect linter/formatter
    let workspace = Path::new(workspace_path);
    match config.project_type {
        ProjectType::RustCargo => {
            lines.push("Lint: `cargo clippy`".to_string());
            lines.push("Format: `cargo fmt`".to_string());
            lines.push("Type check: `cargo check`".to_string());
        }
        ProjectType::NodeNpm | ProjectType::NodeYarn => {
            if workspace.join("tsconfig.json").exists() {
                lines.push("Type check: `npx tsc --noEmit`".to_string());
            }
            if workspace.join(".eslintrc").exists()
                || workspace.join(".eslintrc.json").exists()
                || workspace.join("eslint.config.js").exists()
            {
                lines.push("Lint: `npx eslint .`".to_string());
            }
            if workspace.join(".prettierrc").exists() || workspace.join(".prettierrc.json").exists()
            {
                lines.push("Format: `npx prettier --check .`".to_string());
            }
        }
        ProjectType::Python => {
            if workspace.join("pyproject.toml").exists() {
                lines.push("Lint: `ruff check .`".to_string());
                lines.push("Format: `ruff format .`".to_string());
            } else {
                lines.push("Lint: `pylint .`".to_string());
                lines.push("Format: `black .`".to_string());
            }
        }
        ProjectType::Go => {
            lines.push("Lint: `go vet ./...`".to_string());
            lines.push("Format: `gofmt -l .`".to_string());
        }
        ProjectType::Unknown => {
            lines.push("No project type detected.".to_string());
            lines.push("Edit this file to add build/lint/test commands.".to_string());
        }
    }

    lines.push(String::new());
    lines.push("## Coding Conventions".to_string());
    lines.push(String::new());
    lines.push("- Follow existing code style in the project.".to_string());
    lines.push("- Do not add comments unless asked.".to_string());
    lines.push("- Use existing libraries and patterns.".to_string());
    lines.push("- Always run lint and type check after changes.".to_string());
    lines.push(String::new());
    lines.push("## Memory".to_string());
    lines.push(String::new());
    lines.push("Edit `.tundracode/memory.md` to persist project context across sessions.".to_string());

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0) as u64;
        let dir = std::env::temp_dir().join(format!("tundra_test_{}_{}", std::process::id(), nanos));
        let _ = fs::create_dir_all(&dir);
        dir.to_string_lossy().to_string()
    }

    #[test]
    fn test_detect_rust_project() {
        let dir = temp_dir();
        let _ = fs::write(format!("{}/Cargo.toml", dir), "[package]\nname = \"test\"");
        let _ = fs::create_dir_all(format!("{}/tests", dir));

        let config = BuildConfig::detect(&dir);
        assert_eq!(config.project_type, ProjectType::RustCargo);
        assert_eq!(config.build_command, "cargo build");
        assert!(config.has_tests);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_detect_node_project() {
        let dir = temp_dir();
        let _ = fs::write(
            format!("{}/package.json", dir),
            r#"{"scripts": {"build": "tsc", "test": "jest"}}"#,
        );

        let config = BuildConfig::detect(&dir);
        assert!(matches!(
            config.project_type,
            ProjectType::NodeNpm | ProjectType::NodeYarn
        ));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_detect_unknown_project() {
        let dir = temp_dir();
        let config = BuildConfig::detect(&dir);
        assert_eq!(config.project_type, ProjectType::Unknown);
        assert!(config.build_command.is_empty());
        assert!(config.test_command.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }
}
