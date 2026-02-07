use std::fmt::Write;
use std::path::Path;
use std::process::Command;

pub fn run_js_lint(
    file_path: &str,
    project_root: &str,
    debug: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    // Try linters in order: oxlint, biome, eslint
    let linters: &[(&str, &[&str])] = &[
        ("oxlint", &["{{file}}"]),
        ("biome", &["lint", "{{file}}"]),
        ("eslint", &["{{file}}"]),
    ];

    for (linter, args) in linters {
        let bin_path = format!("{project_root}/node_modules/.bin/{linter}");
        if Path::new(&bin_path).exists() {
            let actual_args: Vec<String> = args
                .iter()
                .map(|a| a.replace("{{file}}", file_path))
                .collect();

            let output = Command::new(&bin_path)
                .args(&actual_args)
                .current_dir(project_root)
                .output()?;

            return Ok(output_lint_result(
                linter,
                file_path,
                &String::from_utf8_lossy(&output.stdout),
                &String::from_utf8_lossy(&output.stderr),
                output.status.success(),
                debug,
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
            return Ok(output_lint_result(
                "npm run lint",
                file_path,
                &stdout,
                &stderr,
                output.status.success(),
                debug,
            ));
        }
    }

    // No linter found
    Ok(continue_result(
        debug,
        &format!("[ralph-hook-lint] no linter found for {}.", file_path),
    ))
}

pub fn run_rust_lint(
    file_path: &str,
    project_root: &str,
    debug: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    // Run clippy on the specific file
    let output = Command::new("cargo")
        .args(["clippy", "--message-format=short", "--", "-D", "warnings"])
        .current_dir(project_root)
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Filter output to only show errors related to the specific file
    let file_errors = filter_clippy_output(&stdout, &stderr, file_path);

    if file_errors.is_empty() {
        Ok(continue_result(
            debug,
            &format!(
                "[ralph-hook-lint] lint passed for {} using clippy.",
                file_path
            ),
        ))
    } else {
        Ok(format!(
            r#"{{"decision":"block","reason":"[ralph-hook-lint] lint errors in {} using clippy:\n\n{}\n\nFix lint errors."}}"#,
            escape_json(file_path),
            escape_json(&file_errors)
        ))
    }
}

pub fn run_python_lint(
    file_path: &str,
    project_root: &str,
    debug: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    // Try linters in order of speed: ruff (fastest), mypy, pylint, flake8
    let linters: &[(&str, &[&str])] = &[
        ("ruff", &["check", "--output-format=concise", "{{file}}"]),
        ("mypy", &["{{file}}"]),
        ("pylint", &["--output-format=text", "{{file}}"]),
        ("flake8", &["{{file}}"]),
    ];

    // Check for virtual environment paths first, then system paths
    let venv_dirs = [".venv/bin", "venv/bin", ".env/bin", "env/bin"];

    for (linter, args) in linters {
        // Try virtual environment first
        let mut bin_path: Option<String> = None;

        for venv_dir in &venv_dirs {
            let venv_path = format!("{project_root}/{venv_dir}/{linter}");
            if Path::new(&venv_path).exists() {
                bin_path = Some(venv_path);
                break;
            }
        }

        // Fall back to system PATH
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
            let actual_args: Vec<String> = args
                .iter()
                .map(|a| a.replace("{{file}}", file_path))
                .collect();

            let output = Command::new(&bin)
                .args(&actual_args)
                .current_dir(project_root)
                .output()?;

            return Ok(output_lint_result(
                linter,
                file_path,
                &String::from_utf8_lossy(&output.stdout),
                &String::from_utf8_lossy(&output.stderr),
                output.status.success(),
                debug,
            ));
        }
    }

    // No linter found
    Ok(continue_result(
        debug,
        &format!(
            "[ralph-hook-lint] no Python linter found for {}. Install ruff for best performance: pip install ruff",
            file_path
        ),
    ))
}

pub fn run_java_lint(
    file_path: &str,
    project_root: &str,
    debug: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    // Detect build tool: Maven or Gradle
    let pom_path = Path::new(project_root).join("pom.xml");
    let gradle_path = Path::new(project_root).join("build.gradle");
    let gradle_kts_path = Path::new(project_root).join("build.gradle.kts");

    // Linters to try in order: pmd (fast), spotbugs (thorough)
    let maven_linters: &[(&str, &[&str], &str)] = &[
        (
            "pmd:check",
            &["pmd:check", "-q"],
            "No plugin found for prefix 'pmd'",
        ),
        (
            "spotbugs:check",
            &["spotbugs:check", "-q"],
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

            // Check if plugin exists
            if stderr.contains("Unknown lifecycle phase") || stderr.contains(not_found_msg) {
                continue;
            }

            return Ok(output_lint_result(
                &format!("mvn {name}"),
                file_path,
                &stdout,
                &stderr,
                output.status.success(),
                debug,
            ));
        }

        return Ok(continue_result(
            debug,
            &format!(
                "[ralph-hook-lint] no Java linter configured for {}. Add maven-pmd-plugin or spotbugs-maven-plugin to pom.xml.",
                file_path
            ),
        ));
    }

    if gradle_path.exists() || gradle_kts_path.exists() {
        let gradle_cmd = if Path::new(project_root).join("gradlew").exists() {
            "./gradlew"
        } else {
            "gradle"
        };

        for (task, not_found_msg) in gradle_linters {
            let output = Command::new(gradle_cmd)
                .args([*task, "-q"])
                .current_dir(project_root)
                .output()?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            // Check if task exists
            if stderr.contains(not_found_msg) {
                continue;
            }

            return Ok(output_lint_result(
                &format!("{gradle_cmd} {task}"),
                file_path,
                &stdout,
                &stderr,
                output.status.success(),
                debug,
            ));
        }

        return Ok(continue_result(
            debug,
            &format!(
                "[ralph-hook-lint] no Java linter configured for {}. Add pmd or spotbugs plugin to build.gradle.",
                file_path
            ),
        ));
    }

    // No build tool found
    Ok(continue_result(
        debug,
        &format!(
            "[ralph-hook-lint] no Java build tool found for {}. Add pom.xml or build.gradle.",
            file_path
        ),
    ))
}

pub fn run_go_lint(
    file_path: &str,
    project_root: &str,
    debug: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    // Try linters in order: golangci-lint (comprehensive), staticcheck, go vet
    let linters: &[(&str, &[&str])] = &[
        ("golangci-lint", &["run", "--fast", "{{file}}"]),
        ("staticcheck", &["{{file}}"]),
    ];

    for (linter, args) in linters {
        // Check if linter exists in PATH
        if let Ok(output) = Command::new("which").arg(linter).output() {
            if output.status.success() {
                let actual_args: Vec<String> = args
                    .iter()
                    .map(|a| a.replace("{{file}}", file_path))
                    .collect();

                let output = Command::new(linter)
                    .args(&actual_args)
                    .current_dir(project_root)
                    .output()?;

                return Ok(output_lint_result(
                    linter,
                    file_path,
                    &String::from_utf8_lossy(&output.stdout),
                    &String::from_utf8_lossy(&output.stderr),
                    output.status.success(),
                    debug,
                ));
            }
        }
    }

    // Fallback to go vet (always available with Go installation)
    if let Ok(output) = Command::new("which").arg("go").output() {
        if output.status.success() {
            let output = Command::new("go")
                .args(["vet", file_path])
                .current_dir(project_root)
                .output()?;

            return Ok(output_lint_result(
                "go vet",
                file_path,
                &String::from_utf8_lossy(&output.stdout),
                &String::from_utf8_lossy(&output.stderr),
                output.status.success(),
                debug,
            ));
        }
    }

    // No linter found
    Ok(continue_result(
        debug,
        &format!(
            "[ralph-hook-lint] no Go linter found for {}. Install golangci-lint for best results: https://golangci-lint.run",
            file_path
        ),
    ))
}

fn filter_clippy_output(stdout: &str, stderr: &str, file_path: &str) -> String {
    let combined = format!("{stderr}\n{stdout}");
    let file_name = Path::new(file_path)
        .file_name()
        .map_or(file_path, |n| n.to_str().unwrap_or(file_path));

    combined
        .lines()
        .filter(|line| {
            // Include lines that reference our file or are continuation/context lines
            line.contains(file_path) || line.contains(file_name)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

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

fn output_lint_result(
    linter: &str,
    file_path: &str,
    stdout: &str,
    stderr: &str,
    success: bool,
    debug: bool,
) -> String {
    if success {
        continue_result(
            debug,
            &format!(
                "[ralph-hook-lint] lint passed for {} using {}.",
                file_path, linter
            ),
        )
    } else {
        let output = if !stdout.is_empty() && !stderr.is_empty() {
            format!("{stdout}\n{stderr}")
        } else if !stdout.is_empty() {
            stdout.to_string()
        } else {
            stderr.to_string()
        };

        format!(
            r#"{{"decision":"block","reason":"[ralph-hook-lint] lint errors in {} using {}:\n\n{}\n\nFix lint errors."}}"#,
            escape_json(file_path),
            escape_json(linter),
            escape_json(output.trim())
        )
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
    fn test_output_lint_result_success_debug() {
        let result = output_lint_result("eslint", "src/app.js", "", "", true, true);
        assert_eq!(
            result,
            r#"{"continue":true,"systemMessage":"[ralph-hook-lint] lint passed for src/app.js using eslint."}"#
        );
    }

    #[test]
    fn test_output_lint_result_success_no_debug() {
        let result = output_lint_result("eslint", "src/app.js", "", "", true, false);
        assert_eq!(result, r#"{"continue":true}"#);
    }

    #[test]
    fn test_output_lint_result_failure_stdout_only() {
        let result = output_lint_result("eslint", "src/app.js", "error on line 1", "", false, true);
        assert_eq!(
            result,
            r#"{"decision":"block","reason":"[ralph-hook-lint] lint errors in src/app.js using eslint:\n\nerror on line 1\n\nFix lint errors."}"#
        );
    }

    #[test]
    fn test_output_lint_result_failure_stderr_only() {
        let result = output_lint_result("eslint", "src/app.js", "", "error on line 2", false, true);
        assert_eq!(
            result,
            r#"{"decision":"block","reason":"[ralph-hook-lint] lint errors in src/app.js using eslint:\n\nerror on line 2\n\nFix lint errors."}"#
        );
    }

    #[test]
    fn test_output_lint_result_failure_both() {
        let result = output_lint_result(
            "eslint",
            "src/app.js",
            "stdout err",
            "stderr err",
            false,
            true,
        );
        assert_eq!(
            result,
            r#"{"decision":"block","reason":"[ralph-hook-lint] lint errors in src/app.js using eslint:\n\nstdout err\nstderr err\n\nFix lint errors."}"#
        );
    }

    #[test]
    fn test_output_lint_result_failure_no_debug_still_blocks() {
        let result =
            output_lint_result("eslint", "src/app.js", "error on line 1", "", false, false);
        assert_eq!(
            result,
            r#"{"decision":"block","reason":"[ralph-hook-lint] lint errors in src/app.js using eslint:\n\nerror on line 1\n\nFix lint errors."}"#
        );
    }

    #[test]
    fn test_output_lint_result_escapes_special_chars() {
        let result = output_lint_result(
            "eslint",
            "src/app.js",
            "error: \"unexpected\"\n",
            "",
            false,
            true,
        );
        assert!(result.contains(r#"\"unexpected\""#));
        assert!(result.contains(r"\n"));
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
    fn test_filter_clippy_output_matches_full_path() {
        let stderr = "warning: unused variable\n  --> src/main.rs:10:5\nerror: something else";
        let result = filter_clippy_output("", stderr, "src/main.rs");
        assert!(result.contains("src/main.rs:10:5"));
        assert!(!result.contains("unused variable"));
    }

    #[test]
    fn test_filter_clippy_output_matches_filename() {
        let stderr = "warning: unused in main.rs\n  --> other/main.rs:5:1";
        let result = filter_clippy_output("", stderr, "/project/src/main.rs");
        assert!(result.contains("main.rs"));
    }

    #[test]
    fn test_filter_clippy_output_empty_when_no_match() {
        let stderr = "warning: in other.rs:10:5";
        let result = filter_clippy_output("", stderr, "src/main.rs");
        assert!(result.is_empty() || !result.contains("other.rs"));
    }
}
