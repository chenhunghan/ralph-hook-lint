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
    Python,
    Java,
}

/// Detect language from file extension
pub fn detect_lang(file_path: &str) -> Option<Lang> {
    let js_extensions = [".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs"];
    let rust_extensions = [".rs"];
    let python_extensions = [".py", ".pyi"];
    let java_extensions = [".java"];

    if js_extensions.iter().any(|ext| file_path.ends_with(ext)) {
        Some(Lang::JavaScript)
    } else if rust_extensions.iter().any(|ext| file_path.ends_with(ext)) {
        Some(Lang::Rust)
    } else if python_extensions.iter().any(|ext| file_path.ends_with(ext)) {
        Some(Lang::Python)
    } else if java_extensions.iter().any(|ext| file_path.ends_with(ext)) {
        Some(Lang::Java)
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
        Lang::Python => find_python_root(&file_dir).map(|root| ProjectInfo { root, lang }),
        Lang::Java => find_java_root(&file_dir).map(|root| ProjectInfo { root, lang }),
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

/// Find the nearest Python project root by walking up the directory tree
/// Looks for pyproject.toml, setup.py, setup.cfg, or requirements.txt
fn find_python_root(dir: &str) -> Option<String> {
    let markers = [
        "pyproject.toml",
        "setup.py",
        "setup.cfg",
        "requirements.txt",
    ];
    let mut current = Path::new(dir);
    loop {
        for marker in &markers {
            if current.join(marker).exists() {
                return Some(current.to_string_lossy().to_string());
            }
        }
        current = current.parent()?;
    }
}

/// Find the nearest Java project root by walking up the directory tree
/// Looks for pom.xml, build.gradle, or build.gradle.kts
fn find_java_root(dir: &str) -> Option<String> {
    let markers = ["pom.xml", "build.gradle", "build.gradle.kts"];
    let mut current = Path::new(dir);
    loop {
        for marker in &markers {
            if current.join(marker).exists() {
                return Some(current.to_string_lossy().to_string());
            }
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
    fn detect_lang_python() {
        assert_eq!(detect_lang("/path/to/file.py"), Some(Lang::Python));
        assert_eq!(detect_lang("/path/to/file.pyi"), Some(Lang::Python));
    }

    #[test]
    fn detect_lang_unsupported() {
        assert_eq!(detect_lang("/path/to/file.go"), None);
        assert_eq!(detect_lang("/path/to/file.txt"), None);
        assert_eq!(detect_lang("/path/to/file.rb"), None);
    }

    #[test]
    fn find_project_root_for_python_file() {
        let fixture_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/python/project");

        let file_path = fixture_dir.join("src/main.py");
        let result = find_project_root(&file_path.to_string_lossy());

        assert!(result.is_some(), "Expected to find project root");
        let info = result.unwrap();
        assert_eq!(info.lang, Lang::Python);
        assert!(
            info.root.ends_with("project"),
            "Expected project, got: {}",
            info.root
        );
    }

    #[test]
    fn find_project_root_python_monorepo() {
        let fixture_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/python/monorepo/packages/app");

        let file_path = fixture_dir.join("src/lib.py");
        let result = find_project_root(&file_path.to_string_lossy());

        assert!(result.is_some());
        let info = result.unwrap();
        assert_eq!(info.lang, Lang::Python);
        // Should find the package's pyproject.toml, not the monorepo root
        assert!(
            info.root.ends_with("app"),
            "Expected app package, got: {}",
            info.root
        );
    }

    #[test]
    fn detect_lang_java() {
        assert_eq!(detect_lang("/path/to/file.java"), Some(Lang::Java));
    }

    #[test]
    fn find_project_root_for_java_file() {
        let fixture_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/java/project");

        let file_path = fixture_dir.join("src/main/java/App.java");
        let result = find_project_root(&file_path.to_string_lossy());

        assert!(result.is_some(), "Expected to find project root");
        let info = result.unwrap();
        assert_eq!(info.lang, Lang::Java);
        assert!(
            info.root.ends_with("project"),
            "Expected project, got: {}",
            info.root
        );
    }

    #[test]
    fn find_project_root_java_monorepo() {
        let fixture_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/java/monorepo/modules/app");

        let file_path = fixture_dir.join("src/main/java/Lib.java");
        let result = find_project_root(&file_path.to_string_lossy());

        assert!(result.is_some());
        let info = result.unwrap();
        assert_eq!(info.lang, Lang::Java);
        // Should find the module's pom.xml, not the monorepo root
        assert!(
            info.root.ends_with("app"),
            "Expected app module, got: {}",
            info.root
        );
    }
}
