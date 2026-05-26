use std::fmt::Write;
use std::path::Path;
use std::process::Command;

/// Result of running one or more linters.
///
/// Wire-protocol formatting lives outside this module — `Pass`/`Fail` here are
/// content-only so the same outcome can be serialized as either a Codex-style
/// `decision:block` JSON payload or an `asyncRewake` exit-2/stderr signal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LintOutcome {
    Pass { message: String },
    Fail { reason: String },
}

impl LintOutcome {
    pub const fn is_fail(&self) -> bool {
        matches!(self, Self::Fail { .. })
    }
}

pub fn run_js_lint(
    file_path: &str,
    project_root: &str,
    lenient: bool,
) -> Result<LintOutcome, Box<dyn std::error::Error>> {
    // Try linters in order: oxlint, biome, eslint
    let linters: &[(&str, &[&str])] = &[
        ("oxlint", &["{{file}}"]),
        ("biome", &["lint", "{{file}}"]),
        ("eslint", &["{{file}}"]),
    ];

    for (linter, args) in linters {
        let bin_path = format!("{project_root}/node_modules/.bin/{linter}");
        if Path::new(&bin_path).exists() {
            let mut actual_args: Vec<String> = args
                .iter()
                .map(|a| a.replace("{{file}}", file_path))
                .collect();

            if lenient {
                match *linter {
                    "oxlint" => {
                        actual_args.extend([
                            "--allow".into(),
                            "no-unused-vars".into(),
                            "--allow".into(),
                            "@typescript-eslint/no-unused-vars".into(),
                            "--allow".into(),
                            "no-undef".into(),
                        ]);
                    }
                    "biome" => {
                        actual_args.extend([
                            "--skip=correctness/noUnusedVariables".into(),
                            "--skip=correctness/noUnusedImports".into(),
                            "--skip=correctness/noUndeclaredVariables".into(),
                        ]);
                    }
                    "eslint" => {
                        actual_args.extend([
                            "--rule".into(),
                            "no-unused-vars: off".into(),
                            "--rule".into(),
                            "@typescript-eslint/no-unused-vars: off".into(),
                            "--rule".into(),
                            "no-undef: off".into(),
                            "--rule".into(),
                            "react/jsx-no-undef: off".into(),
                        ]);
                    }
                    _ => {}
                }
            }

            let output = Command::new(&bin_path)
                .args(&actual_args)
                .current_dir(project_root)
                .output()?;

            return Ok(build_outcome(
                linter,
                file_path,
                &String::from_utf8_lossy(&output.stdout),
                &String::from_utf8_lossy(&output.stderr),
                output.status.success(),
            ));
        }
    }

    // Try npm run lint
    let npm_lint = Command::new("npm")
        .args(["run", "lint", "--if-present", "--", file_path])
        .current_dir(project_root)
        .output();

    if let Ok(output) = npm_lint {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{stdout}{stderr}");
        if !combined.contains("Missing script") && !combined.contains("npm error") {
            return Ok(build_outcome(
                "npm run lint",
                file_path,
                &stdout,
                &stderr,
                output.status.success(),
            ));
        }
    }

    Ok(LintOutcome::Pass {
        message: format!("[ralph-hook-lint] no linter found for {file_path}."),
    })
}

pub fn run_rust_lint(
    file_path: &str,
    project_root: &str,
    lenient: bool,
) -> Result<LintOutcome, Box<dyn std::error::Error>> {
    run_rust_lint_multi(&[file_path.to_string()], project_root, lenient)
}

/// Run clippy once and filter output for all given file paths.
pub fn run_rust_lint_multi(
    file_paths: &[String],
    project_root: &str,
    lenient: bool,
) -> Result<LintOutcome, Box<dyn std::error::Error>> {
    let mut clippy_args = vec!["clippy", "--message-format=short", "--", "-D", "warnings"];
    if lenient {
        clippy_args.extend([
            "-A",
            "unused_variables",
            "-A",
            "unused_imports",
            "-A",
            "dead_code",
        ]);
    }
    let output = Command::new("cargo")
        .args(&clippy_args)
        .current_dir(project_root)
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let refs: Vec<&str> = file_paths.iter().map(String::as_str).collect();
    let file_errors = filter_clippy_output_multi(&stdout, &stderr, &refs, project_root);

    let label = if file_paths.len() == 1 {
        file_paths[0].clone()
    } else {
        format!("{} files", file_paths.len())
    };

    if file_errors.is_empty() {
        Ok(LintOutcome::Pass {
            message: format!("[ralph-hook-lint] lint passed for {label} using clippy."),
        })
    } else {
        Ok(LintOutcome::Fail {
            reason: format!(
                "[ralph-hook-lint] lint errors in {label} using clippy:\n\n{file_errors}\n\nFix lint errors."
            ),
        })
    }
}

pub fn run_python_lint(
    file_path: &str,
    project_root: &str,
    lenient: bool,
) -> Result<LintOutcome, Box<dyn std::error::Error>> {
    // Try linters in order of speed: ruff (fastest), mypy, pylint, flake8
    let linters: &[(&str, &[&str])] = &[
        ("ruff", &["check", "--output-format=concise", "{{file}}"]),
        ("mypy", &["{{file}}"]),
        ("pylint", &["--output-format=text", "{{file}}"]),
        ("flake8", &["{{file}}"]),
    ];

    let venv_dirs = [".venv/bin", "venv/bin", ".env/bin", "env/bin"];

    for (linter, args) in linters {
        let mut bin_path: Option<String> = None;

        for venv_dir in &venv_dirs {
            let venv_path = format!("{project_root}/{venv_dir}/{linter}");
            if Path::new(&venv_path).exists() {
                bin_path = Some(venv_path);
                break;
            }
        }

        if bin_path.is_none() {
            if let Ok(output) = Command::new("which").arg(linter).output() {
                if output.status.success() {
                    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !path.is_empty() {
                        bin_path = Some(path);
                    }
                }
            }
        }

        if let Some(bin) = bin_path {
            let mut actual_args: Vec<String> = args
                .iter()
                .map(|a| a.replace("{{file}}", file_path))
                .collect();

            if lenient {
                match *linter {
                    "ruff" => {
                        actual_args.extend(["--ignore".into(), "F841,F401,F821".into()]);
                    }
                    "pylint" => {
                        actual_args.extend(["--disable=W0611,W0612,E0602".into()]);
                    }
                    "flake8" => {
                        actual_args.extend(["--extend-ignore=F841,F401,F821".into()]);
                    }
                    _ => {} // mypy doesn't check unused vars
                }
            }

            let output = Command::new(&bin)
                .args(&actual_args)
                .current_dir(project_root)
                .output()?;

            return Ok(build_outcome(
                linter,
                file_path,
                &String::from_utf8_lossy(&output.stdout),
                &String::from_utf8_lossy(&output.stderr),
                output.status.success(),
            ));
        }
    }

    Ok(LintOutcome::Pass {
        message: format!(
            "[ralph-hook-lint] no Python linter found for {file_path}. Install ruff for best performance: pip install ruff"
        ),
    })
}

pub fn run_java_lint(
    file_path: &str,
    project_root: &str,
    lenient: bool,
) -> Result<LintOutcome, Box<dyn std::error::Error>> {
    // PMD/SpotBugs don't support clean CLI-level rule suppression
    let _ = lenient;
    let pom_path = Path::new(project_root).join("pom.xml");
    let gradle_path = Path::new(project_root).join("build.gradle");
    let gradle_kts_path = Path::new(project_root).join("build.gradle.kts");

    // --batch-mode disables ANSI color output (the official Maven way).
    let maven_linters: &[(&str, &[&str], &str)] = &[
        (
            "pmd:check",
            &["--batch-mode", "pmd:check", "-q"],
            "No plugin found for prefix 'pmd'",
        ),
        (
            "spotbugs:check",
            &["--batch-mode", "spotbugs:check", "-q"],
            "No plugin found for prefix 'spotbugs'",
        ),
    ];

    let gradle_linters: &[(&str, &str)] = &[
        ("pmdMain", "Task 'pmdMain' not found"),
        ("spotbugsMain", "Task 'spotbugsMain' not found"),
    ];

    if pom_path.exists() {
        for (name, args, not_found_msg) in maven_linters {
            let output = Command::new("mvn")
                .args(*args)
                .current_dir(project_root)
                .output()?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            if stderr.contains("Unknown lifecycle phase") || stderr.contains(not_found_msg) {
                continue;
            }

            return Ok(build_outcome(
                &format!("mvn {name}"),
                file_path,
                &stdout,
                &stderr,
                output.status.success(),
            ));
        }

        return Ok(LintOutcome::Pass {
            message: format!(
                "[ralph-hook-lint] no Java linter configured for {file_path}. Add maven-pmd-plugin or spotbugs-maven-plugin to pom.xml."
            ),
        });
    }

    if gradle_path.exists() || gradle_kts_path.exists() {
        let gradle_cmd = if Path::new(project_root).join("gradlew").exists() {
            "./gradlew"
        } else {
            "gradle"
        };

        for (task, not_found_msg) in gradle_linters {
            let output = Command::new(gradle_cmd)
                .args([*task, "--console=plain", "-q"])
                .current_dir(project_root)
                .output()?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            if stderr.contains(not_found_msg) {
                continue;
            }

            return Ok(build_outcome(
                &format!("{gradle_cmd} {task}"),
                file_path,
                &stdout,
                &stderr,
                output.status.success(),
            ));
        }

        return Ok(LintOutcome::Pass {
            message: format!(
                "[ralph-hook-lint] no Java linter configured for {file_path}. Add pmd or spotbugs plugin to build.gradle."
            ),
        });
    }

    Ok(LintOutcome::Pass {
        message: format!(
            "[ralph-hook-lint] no Java build tool found for {file_path}. Add pom.xml or build.gradle."
        ),
    })
}

pub fn run_go_lint(
    file_path: &str,
    project_root: &str,
    lenient: bool,
) -> Result<LintOutcome, Box<dyn std::error::Error>> {
    // Go compiles at the package level (all .go files in a directory together).
    // Linting individual files causes false positives (undefined symbols from
    // sibling files in the same package). Lint the package directory instead.
    let pkg_dir = go_package_dir(file_path, project_root);

    let linters: &[(&str, &[&str])] = &[
        ("golangci-lint", &["run", "--fast", "{{pkg}}"]),
        ("staticcheck", &["{{pkg}}"]),
    ];

    for (linter, args) in linters {
        if let Ok(output) = Command::new("which").arg(linter).output() {
            if output.status.success() {
                let mut actual_args: Vec<String> = args
                    .iter()
                    .map(|a| a.replace("{{pkg}}", &pkg_dir))
                    .collect();

                if lenient && *linter == "golangci-lint" {
                    actual_args.push("--disable=unused".into());
                }

                let output = Command::new(linter)
                    .args(&actual_args)
                    .current_dir(project_root)
                    .output()?;

                return Ok(build_outcome(
                    linter,
                    file_path,
                    &String::from_utf8_lossy(&output.stdout),
                    &String::from_utf8_lossy(&output.stderr),
                    output.status.success(),
                ));
            }
        }
    }

    // Fallback to go vet (always available with Go installation)
    if let Ok(output) = Command::new("which").arg("go").output() {
        if output.status.success() {
            let output = Command::new("go")
                .args(["vet", &pkg_dir])
                .current_dir(project_root)
                .output()?;

            return Ok(build_outcome(
                "go vet",
                file_path,
                &String::from_utf8_lossy(&output.stdout),
                &String::from_utf8_lossy(&output.stderr),
                output.status.success(),
            ));
        }
    }

    Ok(LintOutcome::Pass {
        message: format!(
            "[ralph-hook-lint] no Go linter found for {file_path}. Install golangci-lint for best results: https://golangci-lint.run"
        ),
    })
}

/// Derive the Go package import path (relative to project root) from a file path.
/// Returns a `./`-prefixed directory suitable for `go vet`, `staticcheck`, etc.
/// e.g. `/home/user/project/cmd/root.go` with root `/home/user/project` → `./cmd`
fn go_package_dir(file_path: &str, project_root: &str) -> String {
    let file = Path::new(file_path);
    let root = Path::new(project_root);
    file.parent()
        .and_then(|p| p.strip_prefix(root).ok())
        .map_or_else(
            || "./...".to_string(),
            |rel| {
                let rel_str = rel.to_string_lossy();
                if rel_str.is_empty() {
                    ".".to_string()
                } else {
                    format!("./{rel_str}")
                }
            },
        )
}

fn build_outcome(
    linter: &str,
    file_path: &str,
    stdout: &str,
    stderr: &str,
    success: bool,
) -> LintOutcome {
    if success {
        LintOutcome::Pass {
            message: format!("[ralph-hook-lint] lint passed for {file_path} using {linter}."),
        }
    } else {
        let output = if !stdout.is_empty() && !stderr.is_empty() {
            format!("{stdout}\n{stderr}")
        } else if !stdout.is_empty() {
            stdout.to_string()
        } else {
            stderr.to_string()
        };

        LintOutcome::Fail {
            reason: format!(
                "[ralph-hook-lint] lint errors in {file_path} using {linter}:\n\n{}\n\nFix lint errors.",
                output.trim()
            ),
        }
    }
}

fn filter_clippy_output_multi(
    stdout: &str,
    stderr: &str,
    file_paths: &[&str],
    project_root: &str,
) -> String {
    let combined = format!("{stderr}\n{stdout}");

    // Clippy outputs paths relative to the project root (e.g. "src/lib.rs:10:5").
    // Absolute paths from the caller rarely match, so we also build relative paths
    // by stripping the project_root prefix.  Bare filenames are kept as a last-resort
    // fallback for unusual path formats.
    let prefix = if project_root.ends_with('/') {
        project_root.to_string()
    } else {
        format!("{project_root}/")
    };

    let relative_paths: Vec<&str> = file_paths
        .iter()
        .filter_map(|fp| fp.strip_prefix(&prefix))
        .collect();

    let file_names: Vec<&str> = file_paths
        .iter()
        .map(|fp| {
            Path::new(fp)
                .file_name()
                .map_or(*fp, |n| n.to_str().unwrap_or(fp))
        })
        .collect();

    combined
        .lines()
        .filter(|line| {
            file_paths.iter().any(|fp| line.contains(fp))
                || relative_paths.iter().any(|rp| line.contains(rp))
                || file_names.iter().any(|name| line.contains(name))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// --- Wire-protocol helpers (used at the boundary in main.rs) ---

pub fn escape_json(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => result.push_str(r#"\""#),
            '\\' => result.push_str(r"\\"),
            '\n' => result.push_str(r"\n"),
            '\r' => result.push_str(r"\r"),
            '\t' => result.push_str(r"\t"),
            c if c.is_control() => {
                let _ = write!(result, r"\u{:04x}", c as u32);
            }
            c => result.push(c),
        }
    }
    result
}

/// Build a `{"continue":true}` response, including `systemMessage` only in debug mode.
pub fn continue_result(debug: bool, message: &str) -> String {
    if debug {
        format!(
            r#"{{"continue":true,"systemMessage":"{}"}}"#,
            escape_json(message)
        )
    } else {
        r#"{"continue":true}"#.to_string()
    }
}

/// Serialize a `LintOutcome` as the synchronous Codex `decision:block` JSON protocol.
/// Pass → `{"continue":true}` (with optional `systemMessage` in debug mode).
/// Fail → `{"decision":"block","reason":"..."}`.
pub fn outcome_to_block_json(outcome: &LintOutcome, debug: bool) -> String {
    match outcome {
        LintOutcome::Pass { message } => continue_result(debug, message),
        LintOutcome::Fail { reason } => {
            format!(
                r#"{{"decision":"block","reason":"{}"}}"#,
                escape_json(reason)
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_json_simple_string() {
        assert_eq!(escape_json("hello"), "hello");
    }

    #[test]
    fn test_escape_json_quotes() {
        assert_eq!(escape_json(r#"say "hello""#), r#"say \"hello\""#);
    }

    #[test]
    fn test_escape_json_backslash() {
        assert_eq!(escape_json(r"path\to\file"), r"path\\to\\file");
    }

    #[test]
    fn test_escape_json_newlines() {
        assert_eq!(escape_json("line1\nline2"), r"line1\nline2");
    }

    #[test]
    fn test_escape_json_tabs() {
        assert_eq!(escape_json("col1\tcol2"), r"col1\tcol2");
    }

    #[test]
    fn test_escape_json_carriage_return() {
        assert_eq!(escape_json("line1\r\nline2"), r"line1\r\nline2");
    }

    #[test]
    fn test_escape_json_mixed() {
        assert_eq!(
            escape_json("Error: \"file\\not\\found\"\n"),
            r#"Error: \"file\\not\\found\"\n"#
        );
    }

    #[test]
    fn test_build_outcome_success() {
        let outcome = build_outcome("eslint", "src/app.js", "", "", true);
        assert_eq!(
            outcome,
            LintOutcome::Pass {
                message: "[ralph-hook-lint] lint passed for src/app.js using eslint.".to_string()
            }
        );
    }

    #[test]
    fn test_build_outcome_failure_stdout_only() {
        let outcome = build_outcome("eslint", "src/app.js", "error on line 1", "", false);
        match outcome {
            LintOutcome::Fail { reason } => {
                assert!(reason.contains("error on line 1"));
                assert!(reason.contains("eslint"));
                assert!(reason.contains("src/app.js"));
                assert!(reason.contains("Fix lint errors."));
            }
            LintOutcome::Pass { .. } => panic!("expected Fail outcome"),
        }
    }

    #[test]
    fn test_build_outcome_failure_stderr_only() {
        let outcome = build_outcome("eslint", "src/app.js", "", "error on line 2", false);
        match outcome {
            LintOutcome::Fail { reason } => assert!(reason.contains("error on line 2")),
            LintOutcome::Pass { .. } => panic!("expected Fail outcome"),
        }
    }

    #[test]
    fn test_build_outcome_failure_both_streams() {
        let outcome = build_outcome("eslint", "src/app.js", "stdout err", "stderr err", false);
        match outcome {
            LintOutcome::Fail { reason } => {
                assert!(reason.contains("stdout err"));
                assert!(reason.contains("stderr err"));
            }
            LintOutcome::Pass { .. } => panic!("expected Fail outcome"),
        }
    }

    #[test]
    fn test_outcome_to_block_json_pass_no_debug() {
        let outcome = LintOutcome::Pass {
            message: "anything".to_string(),
        };
        assert_eq!(
            outcome_to_block_json(&outcome, false),
            r#"{"continue":true}"#
        );
    }

    #[test]
    fn test_outcome_to_block_json_pass_debug() {
        let outcome = LintOutcome::Pass {
            message: "lint passed".to_string(),
        };
        assert_eq!(
            outcome_to_block_json(&outcome, true),
            r#"{"continue":true,"systemMessage":"lint passed"}"#
        );
    }

    #[test]
    fn test_outcome_to_block_json_fail() {
        let outcome = LintOutcome::Fail {
            reason: "lint failed".to_string(),
        };
        assert_eq!(
            outcome_to_block_json(&outcome, false),
            r#"{"decision":"block","reason":"lint failed"}"#
        );
    }

    #[test]
    fn test_outcome_to_block_json_fail_escapes_special_chars() {
        let outcome = LintOutcome::Fail {
            reason: "error: \"unexpected\"\n".to_string(),
        };
        let serialized = outcome_to_block_json(&outcome, false);
        assert!(serialized.contains(r#"\"unexpected\""#));
        assert!(serialized.contains(r"\n"));
    }

    #[test]
    fn test_continue_result_debug() {
        let result = continue_result(true, "[ralph-hook-lint] some message");
        assert_eq!(
            result,
            r#"{"continue":true,"systemMessage":"[ralph-hook-lint] some message"}"#
        );
    }

    #[test]
    fn test_continue_result_no_debug() {
        let result = continue_result(false, "[ralph-hook-lint] some message");
        assert_eq!(result, r#"{"continue":true}"#);
    }

    #[test]
    fn test_lint_outcome_is_fail() {
        assert!(LintOutcome::Fail { reason: "x".into() }.is_fail());
        assert!(
            !LintOutcome::Pass {
                message: "x".into()
            }
            .is_fail()
        );
    }

    #[test]
    fn test_filter_clippy_output_matches_relative_path() {
        let stderr = "warning: unused variable\n  --> src/main.rs:10:5\nerror: something else";
        let result = filter_clippy_output_multi("", stderr, &["/project/src/main.rs"], "/project");
        assert!(result.contains("src/main.rs:10:5"));
        assert!(!result.contains("unused variable"));
    }

    #[test]
    fn test_filter_clippy_output_matches_filename_fallback() {
        let stderr = "warning: unused in main.rs\n  --> other/main.rs:5:1";
        let result = filter_clippy_output_multi("", stderr, &["/project/src/main.rs"], "/project");
        assert!(result.contains("main.rs"));
    }

    #[test]
    fn test_filter_clippy_output_empty_when_no_match() {
        let stderr = "warning: in other.rs:10:5";
        let result = filter_clippy_output_multi("", stderr, &["/project/src/main.rs"], "/project");
        assert!(result.is_empty() || !result.contains("other.rs"));
    }

    #[test]
    fn test_filter_clippy_output_multi_matches_multiple_files() {
        let stderr = "  --> src/main.rs:10:5\n  --> src/lib.rs:20:3\n  --> src/other.rs:1:1";
        let result = filter_clippy_output_multi(
            "",
            stderr,
            &["/project/src/main.rs", "/project/src/lib.rs"],
            "/project",
        );
        assert!(result.contains("src/main.rs:10:5"));
        assert!(result.contains("src/lib.rs:20:3"));
        assert!(!result.contains("src/other.rs"));
    }

    #[test]
    fn test_filter_clippy_workspace_no_cross_crate_leak() {
        let stderr = "  --> src/lib.rs:10:5\n  --> /ws/crates/core/src/lib.rs:20:3";
        let result = filter_clippy_output_multi(
            "",
            stderr,
            &["/ws/crates/app/src/lib.rs"],
            "/ws/crates/app",
        );
        assert!(result.contains("src/lib.rs:10:5"));
    }

    #[test]
    fn test_go_package_dir_subdir() {
        assert_eq!(
            go_package_dir("/home/user/project/cmd/root.go", "/home/user/project"),
            "./cmd"
        );
    }

    #[test]
    fn test_go_package_dir_root() {
        assert_eq!(
            go_package_dir("/home/user/project/main.go", "/home/user/project"),
            "."
        );
    }

    #[test]
    fn test_go_package_dir_nested() {
        assert_eq!(
            go_package_dir(
                "/home/user/project/internal/api/handler.go",
                "/home/user/project"
            ),
            "./internal/api"
        );
    }

    #[test]
    fn test_go_package_dir_fallback() {
        assert_eq!(
            go_package_dir("/other/path/file.go", "/home/user/project"),
            "./..."
        );
    }
}
