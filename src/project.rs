use std::path::Path;
use std::process::Command;

/// Project information for a detected language/ecosystem
#[derive(Debug)]
pub struct ProjectInfo {
    /// Root directory of the project
    pub root: String,
    /// Detected language/ecosystem
    pub lang: Lang,
}

/// Supported languages/ecosystems
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    JavaScript,
    // Future: Python, Go, Rust, etc.
}

/// Find the nearest project root for the given file path.
/// Returns None if no project root is found.
pub fn find_project_root(file_path: &str) -> Option<ProjectInfo> {
    let file_dir = Path::new(file_path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());

    // Try JavaScript/TypeScript (npm)
    if let Some(root) = find_npm_root(&file_dir) {
        return Some(ProjectInfo {
            root,
            lang: Lang::JavaScript,
        });
    }

    // Future: Add other languages here
    // if let Some(root) = find_python_root(&file_dir) { ... }
    // if let Some(root) = find_go_root(&file_dir) { ... }

    None
}

/// Find the nearest package.json directory using npm prefix
fn find_npm_root(dir: &str) -> Option<String> {
    Command::new("npm")
        .arg("prefix")
        .current_dir(dir)
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                let root = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if !root.is_empty() {
                    Some(root)
                } else {
                    None
                }
            } else {
                None
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_project_root_for_js_file() {
        let fixture_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/ts/project");

        // Use a file path in the same directory as package.json
        let file_path = fixture_dir.join("index.ts");
        let result = find_project_root(&file_path.to_string_lossy());

        assert!(result.is_some(), "Expected to find project root");
        let info = result.unwrap();
        assert_eq!(info.lang, Lang::JavaScript);
        assert!(info.root.ends_with("project"), "Expected project, got: {}", info.root);
    }

    #[test]
    fn find_project_root_nested() {
        let fixture_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/ts/nested/subproject");

        let file_path = fixture_dir.join("src/index.ts");
        let result = find_project_root(&file_path.to_string_lossy());

        assert!(result.is_some());
        let info = result.unwrap();
        assert_eq!(info.lang, Lang::JavaScript);
        assert!(info.root.ends_with("subproject"), "Expected subproject, got: {}", info.root);
    }

    #[test]
    fn find_project_root_no_project() {
        let result = find_project_root("/tmp/nonexistent/path/file.ts");
        assert!(result.is_none());
    }
}
