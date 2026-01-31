mod extract;
mod project;

use std::env;
use std::fmt::Write;
use std::io::{self, Read};
use std::path::Path;
use std::process::Command;

use extract::extract_file_path;
use project::{Lang, find_project_root};

fn main() {
    // Handle --version flag
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 && (args[1] == "--version" || args[1] == "-V") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return;
    }

    let result = run();
    match result {
        Ok(output) => println!("{output}"),
        Err(e) => println!(
            r#"{{"continue":true,"systemMessage":"Lint hook error: {}"}}"#,
            escape_json(&e.to_string())
        ),
    }
}

fn run() -> Result<String, Box<dyn std::error::Error>> {
    // Read input from stdin
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    // Extract file_path from tool_input.file_path using simple string search
    let file_path = extract_file_path(&input);

    let file_path =
        match file_path {
            Some(fp) if !fp.is_empty() => fp,
            _ => return Ok(
                r#"{"continue":true,"systemMessage":"no file_path provided, skipping lint hook."}"#
                    .to_string(),
            ),
        };

    // Find the nearest project root (also validates file type)
    let Some(project) = find_project_root(&file_path) else {
        return Ok(format!(
            r#"{{"continue":true,"systemMessage":"Skipping lint: unsupported file type or no project found for {}."}}"#,
            escape_json(&file_path)
        ));
    };

    match project.lang {
        Lang::JavaScript => run_js_lint(&file_path, &project.root),
        Lang::Rust => run_rust_lint(&file_path, &project.root),
    }
}

fn run_js_lint(file_path: &str, project_root: &str) -> Result<String, Box<dyn std::error::Error>> {
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
            ));
        }
    }

    // No linter found
    Ok(format!(
        r#"{{"continue":true,"systemMessage":"No linter found for {}."}}"#,
        escape_json(file_path)
    ))
}

fn run_rust_lint(
    file_path: &str,
    project_root: &str,
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
        Ok(format!(
            r#"{{"continue":true,"systemMessage":"Lint passed for {} using clippy."}}"#,
            escape_json(file_path)
        ))
    } else {
        Ok(format!(
            r#"{{"decision":"block","reason":"Lint errors in {} using clippy:\n\n{}\n\nFix lint errors."}}"#,
            escape_json(file_path),
            escape_json(&file_errors)
        ))
    }
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

fn escape_json(s: &str) -> String {
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

fn output_lint_result(
    linter: &str,
    file_path: &str,
    stdout: &str,
    stderr: &str,
    success: bool,
) -> String {
    if success {
        format!(
            r#"{{"continue":true,"systemMessage":"Lint passed for {} using {}."}}"#,
            escape_json(file_path),
            escape_json(linter)
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
            r#"{{"decision":"block","reason":"Lint errors in {} using {}:\n\n{}\n\nFix lint errors."}}"#,
            escape_json(file_path),
            escape_json(linter),
            escape_json(output.trim())
        )
    }
}
