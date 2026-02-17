mod collect;
mod extract;
mod lint;
mod project;

use std::collections::{HashMap, HashSet};
use std::env;
use std::io::{self, Read};

use extract::{extract_file_path, extract_session_id};
use lint::{
    continue_result, escape_json, run_go_lint, run_java_lint, run_js_lint, run_python_lint,
    run_rust_lint, run_rust_lint_multi,
};
use project::{Lang, find_project_root};

fn main() {
    let args: Vec<String> = env::args().collect();

    // Handle --version flag
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return;
    }

    let debug = args.iter().any(|a| a == "--debug");
    let lenient = args.iter().any(|a| a == "--lenient");
    let collect_mode = args.iter().any(|a| a == "--collect");
    let lint_collected_mode = args.iter().any(|a| a == "--lint-collected");

    let result = if collect_mode {
        run_collect(debug)
    } else if lint_collected_mode {
        run_lint_collected(debug, lenient)
    } else {
        run(debug, lenient)
    };

    match result {
        Ok(output) => println!("{output}"),
        Err(e) => println!(
            "{}",
            continue_result(debug, &format!("[ralph-hook-lint] lint hook error: {e}"))
        ),
    }
}

/// Collect mode: record the file path from stdin into the session temp file, return immediately.
fn run_collect(debug: bool) -> Result<String, Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    let session_id = match extract_session_id(&input) {
        Some(sid) if !sid.is_empty() => sid,
        _ => {
            return Ok(continue_result(
                debug,
                "[ralph-hook-lint] no session_id, skipping collect.",
            ));
        }
    };

    let file_path = match extract_file_path(&input) {
        Some(fp) if !fp.is_empty() => fp,
        _ => {
            return Ok(continue_result(
                debug,
                "[ralph-hook-lint] no file_path provided, skipping collect.",
            ));
        }
    };

    collect::record_path(&session_id, &file_path)?;

    Ok(continue_result(
        debug,
        &format!("[ralph-hook-lint] collected {file_path} for deferred lint."),
    ))
}

/// Lint-collected mode: read all collected paths, lint each, aggregate errors.
fn run_lint_collected(debug: bool, lenient: bool) -> Result<String, Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    let session_id = match extract_session_id(&input) {
        Some(sid) if !sid.is_empty() => sid,
        _ => {
            return Ok(continue_result(
                debug,
                "[ralph-hook-lint] no session_id, skipping lint-collected.",
            ));
        }
    };

    let paths = collect::read_and_cleanup(&session_id)?;

    if paths.is_empty() {
        return Ok(continue_result(
            debug,
            "[ralph-hook-lint] no files collected, skipping lint.",
        ));
    }

    let mut errors: Vec<String> = Vec::new();
    // Group Rust files by project root so clippy runs once and filters for all files.
    let mut rust_projects: HashMap<String, Vec<String>> = HashMap::new();
    // Track Java projects already linted to avoid redundant maven/gradle runs.
    let mut java_projects: HashSet<String> = HashSet::new();

    for file_path in &paths {
        let Some(project) = find_project_root(file_path) else {
            continue;
        };

        match project.lang {
            Lang::Rust => {
                rust_projects
                    .entry(project.root)
                    .or_default()
                    .push(file_path.clone());
            }
            Lang::Java => {
                if !java_projects.insert(project.root.clone()) {
                    continue;
                }
                collect_lint_errors(
                    run_java_lint(file_path, &project.root, debug, lenient),
                    file_path,
                    &mut errors,
                );
            }
            _ => {
                let result = match project.lang {
                    Lang::JavaScript => run_js_lint(file_path, &project.root, debug, lenient),
                    Lang::Python => run_python_lint(file_path, &project.root, debug, lenient),
                    Lang::Go => run_go_lint(file_path, &project.root, debug, lenient),
                    _ => unreachable!(),
                };
                collect_lint_errors(result, file_path, &mut errors);
            }
        }
    }

    // Run clippy once per Rust project, filtering output for all collected files.
    for (root, files) in &rust_projects {
        collect_lint_errors(
            run_rust_lint_multi(files, root, debug, lenient),
            &root.clone(),
            &mut errors,
        );
    }

    if errors.is_empty() {
        Ok(continue_result(
            debug,
            &format!(
                "[ralph-hook-lint] all {} collected file(s) passed lint.",
                paths.len()
            ),
        ))
    } else {
        let combined = errors.join("\n\n---\n\n");
        Ok(format!(
            r#"{{"decision":"block","reason":"{}"}}"#,
            escape_json(&combined)
        ))
    }
}

/// Push the reason from a block result into the errors vec, or ignore continues.
fn collect_lint_errors(
    result: Result<String, Box<dyn std::error::Error>>,
    label: &str,
    errors: &mut Vec<String>,
) {
    match result {
        Ok(output) if output.contains(r#""decision":"block"#) => {
            if let Some(reason) = extract_reason(&output) {
                errors.push(reason);
            } else {
                errors.push(output);
            }
        }
        Ok(_) => {}
        Err(e) => {
            errors.push(format!("[ralph-hook-lint] error linting {label}: {e}"));
        }
    }
}

/// Extract the `reason` value from a block JSON response.
fn extract_reason(json: &str) -> Option<String> {
    extract::extract_reason_field(json)
}

fn run(debug: bool, lenient: bool) -> Result<String, Box<dyn std::error::Error>> {
    // Read input from stdin
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    // Extract file_path from tool_input.file_path using simple string search
    let file_path = extract_file_path(&input);

    let file_path = match file_path {
        Some(fp) if !fp.is_empty() => fp,
        _ => {
            return Ok(continue_result(
                debug,
                "[ralph-hook-lint] no file_path provided, skipping lint hook.",
            ));
        }
    };

    // Find the nearest project root (also validates file type)
    let Some(project) = find_project_root(&file_path) else {
        return Ok(continue_result(
            debug,
            &format!(
                "[ralph-hook-lint] skipping lint: unsupported file type or no project found for {file_path}."
            ),
        ));
    };

    match project.lang {
        Lang::JavaScript => run_js_lint(&file_path, &project.root, debug, lenient),
        Lang::Rust => run_rust_lint(&file_path, &project.root, debug, lenient),
        Lang::Python => run_python_lint(&file_path, &project.root, debug, lenient),
        Lang::Java => run_java_lint(&file_path, &project.root, debug, lenient),
        Lang::Go => run_go_lint(&file_path, &project.root, debug, lenient),
    }
}
