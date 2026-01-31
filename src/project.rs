use std::path::Path;
use std::process::Command;

/// Project information for a detected language/ecosystem
#[derive(Debug)]
pub struct ProjectInfo {
    /// Root directory of the project
    pub root: String,
    /// Detected language/ecosystem (reserved for future use)
    #[allow(dead_code)]
    pub lang: Lang,
}

/// Supported languages/ecosystems
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    JavaScript,
    Rust,
}

/// Detect language from file extension
pub fn detect_lang(file_path: &str) -> Option<Lang> {
    let js_extensions = [".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs"];
    let rust_extensions = [".rs"];

    if js_extensions.iter().any(|ext| file_path.ends_with(ext)) {
        Some(Lang::JavaScript)
    } else if rust_extensions.iter().any(|ext| file_path.ends_with(ext)) {
        Some(Lang::Rust)
    } else {
        None
    }
}

/// Find the nearest project root for the given file path.
/// Returns None if no project root is found or file type is unsupported.
pub fn find_project_root(file_path: &str) -> Option<ProjectInfo> {
    let lang = detect_lang(file_path)?;
    let file_dir = Path::new(file_path)
        .parent()
        .map_or_else(|| ".".to_string(), |p| p.to_string_lossy().to_string());

    match lang {
        Lang::JavaScript => find_npm_root(&file_dir).map(|root| ProjectInfo { root, lang }),
        Lang::Rust => find_cargo_root(&file_dir).map(|root| ProjectInfo { root, lang }),
    }
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
                if root.is_empty() { None } else { Some(root) }
            } else {
                None
            }
        })
}

/// Find the nearest Cargo.toml directory by walking up the directory tree
fn find_cargo_root(dir: &str) -> Option<String> {
    let mut current = Path::new(dir);
    loop {
        let cargo_toml = current.join("Cargo.toml");
        if cargo_toml.exists() {
            return Some(current.to_string_lossy().to_string());
        }
        current = current.parent()?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_project_root_for_js_file() {
        let fixture_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts/project");

        // Use a file path in the same directory as package.json
        let file_path = fixture_dir.join("index.ts");
        let result = find_project_root(&file_path.to_string_lossy());

        assert!(result.is_some(), "Expected to find project root");
        let info = result.unwrap();
        assert_eq!(info.lang, Lang::JavaScript);
        assert!(
            info.root.ends_with("project"),
            "Expected project, got: {}",
            info.root
        );
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
        assert!(
            info.root.ends_with("subproject"),
            "Expected subproject, got: {}",
            info.root
        );
    }

    #[test]
    fn find_project_root_no_project() {
        let result = find_project_root("/tmp/nonexistent/path/file.ts");
        assert!(result.is_none());
    }

    #[test]
    fn find_project_root_for_rust_file() {
        let fixture_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rust/project");

        let file_path = fixture_dir.join("src/main.rs");
        let result = find_project_root(&file_path.to_string_lossy());

        assert!(result.is_some(), "Expected to find project root");
        let info = result.unwrap();
        assert_eq!(info.lang, Lang::Rust);
        assert!(
            info.root.ends_with("project"),
            "Expected project, got: {}",
            info.root
        );
    }

    #[test]
    fn find_project_root_rust_monorepo() {
        let fixture_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/rust/monorepo/crates/app");

        let file_path = fixture_dir.join("src/lib.rs");
        let result = find_project_root(&file_path.to_string_lossy());

        assert!(result.is_some());
        let info = result.unwrap();
        assert_eq!(info.lang, Lang::Rust);
        // Should find the crate's Cargo.toml, not the workspace root
        assert!(
            info.root.ends_with("app"),
            "Expected app crate, got: {}",
            info.root
        );
    }

    #[test]
    fn detect_lang_js() {
        assert_eq!(detect_lang("/path/to/file.js"), Some(Lang::JavaScript));
        assert_eq!(detect_lang("/path/to/file.ts"), Some(Lang::JavaScript));
        assert_eq!(detect_lang("/path/to/file.tsx"), Some(Lang::JavaScript));
        assert_eq!(detect_lang("/path/to/file.jsx"), Some(Lang::JavaScript));
        assert_eq!(detect_lang("/path/to/file.mjs"), Some(Lang::JavaScript));
        assert_eq!(detect_lang("/path/to/file.cjs"), Some(Lang::JavaScript));
    }

    #[test]
    fn detect_lang_rust() {
        assert_eq!(detect_lang("/path/to/file.rs"), Some(Lang::Rust));
    }

    #[test]
    fn detect_lang_unsupported() {
        assert_eq!(detect_lang("/path/to/file.py"), None);
        assert_eq!(detect_lang("/path/to/file.go"), None);
        assert_eq!(detect_lang("/path/to/file.txt"), None);
    }
}
