mod collect;
mod extract;
mod lint;
mod project;
mod snapshot;

use std::collections::{HashMap, HashSet};
use std::env;
use std::io::{self, Read};
use std::process;

use extract::{
    extract_cwd, extract_file_path, extract_session_id, extract_stop_hook_active, extract_turn_id,
};
use lint::{
    LintOutcome, continue_result, outcome_to_block_json, run_go_lint, run_java_lint, run_js_lint,
    run_python_lint, run_rust_lint, run_rust_lint_multi,
};
use project::{Lang, find_project_root};
use snapshot::{
    cleanup_snapshot, diff_changed_files, read_snapshot, scan_supported_files, write_snapshot,
};

/// What the binary should emit to stdout/stderr and which exit code to use.
///
/// Different hook events use different wire protocols:
/// - `PostToolUse` / Codex Stop / legacy single-file mode: JSON on stdout, exit 0.
/// - Claude `asyncRewake` Stop: stderr + exit 2 on failure, silent + exit 0 on pass.
pub enum HookOutput {
    /// Print to stdout and exit 0.
    Stdout(String),
    /// Write to stderr and exit 2 (asyncRewake wake-up signal).
    StderrExit2(String),
    /// Exit 0 with no output.
    Silent,
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return;
    }

    let debug = args.iter().any(|a| a == "--debug");
    let lenient = args.iter().any(|a| a == "--lenient");
    let collect_mode = args.iter().any(|a| a == "--collect");
    let lint_collected_mode = args.iter().any(|a| a == "--lint-collected");
    let snapshot_turn_mode = args.iter().any(|a| a == "--snapshot-turn");
    let lint_turn_mode = args.iter().any(|a| a == "--lint-turn");

    let result = if collect_mode {
        run_collect(debug)
    } else if lint_collected_mode {
        run_lint_collected(debug, lenient)
    } else if snapshot_turn_mode {
        run_snapshot_turn(debug)
    } else if lint_turn_mode {
        run_lint_turn(debug, lenient)
    } else {
        run(debug, lenient)
    };

    let output = match result {
        Ok(output) => output,
        Err(e) => {
            // asyncRewake mode swallows internal errors so a hook bug never wakes the agent.
            if lint_collected_mode {
                HookOutput::Silent
            } else {
                HookOutput::Stdout(continue_result(
                    debug,
                    &format!("[ralph-hook-lint] lint hook error: {e}"),
                ))
            }
        }
    };

    emit(output);
}

fn emit(output: HookOutput) {
    match output {
        HookOutput::Stdout(s) => println!("{s}"),
        HookOutput::StderrExit2(s) => {
            eprintln!("{s}");
            process::exit(2);
        }
        HookOutput::Silent => {}
    }
}

/// Collect mode: record the file path from stdin into the session temp file, return immediately.
fn run_collect(debug: bool) -> Result<HookOutput, Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    let session_id = match extract_session_id(&input) {
        Some(sid) if !sid.is_empty() => sid,
        _ => {
            return Ok(HookOutput::Stdout(continue_result(
                debug,
                "[ralph-hook-lint] no session_id, skipping collect.",
            )));
        }
    };

    let file_path = match extract_file_path(&input) {
        Some(fp) if !fp.is_empty() => fp,
        _ => {
            return Ok(HookOutput::Stdout(continue_result(
                debug,
                "[ralph-hook-lint] no file_path provided, skipping collect.",
            )));
        }
    };

    collect::record_path(&session_id, &file_path)?;

    Ok(HookOutput::Stdout(continue_result(
        debug,
        &format!("[ralph-hook-lint] collected {file_path} for deferred lint."),
    )))
}

/// Lint-collected mode (Claude `asyncRewake` Stop): lint files recorded during the turn.
/// On fail, write the reason to stderr and exit 2 so Claude is woken with the lint output
/// as a system reminder. On pass, exit 0 silently — the stop transition completes cleanly.
fn run_lint_collected(
    debug: bool,
    lenient: bool,
) -> Result<HookOutput, Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    let outcome = match extract_session_id(&input) {
        Some(sid) if !sid.is_empty() => {
            let paths = collect::read_and_cleanup(&sid)?;
            if paths.is_empty() {
                LintOutcome::Pass {
                    message: "[ralph-hook-lint] no files collected, skipping lint.".to_string(),
                }
            } else {
                lint_paths(&paths, lenient, "collected file(s)")
            }
        }
        _ => LintOutcome::Pass {
            message: "[ralph-hook-lint] no session_id, skipping lint-collected.".to_string(),
        },
    };

    Ok(outcome_to_async_rewake(outcome, debug))
}

/// Serialize a `LintOutcome` for the `asyncRewake` protocol.
fn outcome_to_async_rewake(outcome: LintOutcome, debug: bool) -> HookOutput {
    match outcome {
        LintOutcome::Pass { message } => {
            if debug {
                // Debug messages on success are dropped by asyncRewake (only exit-2 wakes Claude),
                // but writing to stderr is harmless and useful when running the binary by hand.
                eprintln!("{message}");
            }
            HookOutput::Silent
        }
        LintOutcome::Fail { reason } => HookOutput::StderrExit2(reason),
    }
}

fn lint_paths(paths: &[String], lenient: bool, success_scope: &str) -> LintOutcome {
    let mut errors: Vec<String> = Vec::new();
    // Group Rust files by project root so clippy runs once and filters for all files.
    let mut rust_projects: HashMap<String, Vec<String>> = HashMap::new();
    // Track Java projects already linted to avoid redundant maven/gradle runs.
    let mut java_projects: HashSet<String> = HashSet::new();
    // Track Go packages already linted to avoid redundant per-file runs
    // (Go lints at the package/directory level, not per-file).
    let mut go_packages: HashSet<String> = HashSet::new();

    for file_path in paths {
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
                    run_java_lint(file_path, &project.root, lenient),
                    file_path,
                    &mut errors,
                );
            }
            Lang::Go => {
                // Go lints at the package (directory) level. Deduplicate so we
                // don't re-lint the same package for every changed file in it.
                let pkg_key = format!(
                    "{}:{}",
                    project.root,
                    std::path::Path::new(file_path.as_str())
                        .parent()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default()
                );
                if !go_packages.insert(pkg_key) {
                    continue;
                }
                collect_lint_errors(
                    run_go_lint(file_path, &project.root, lenient),
                    file_path,
                    &mut errors,
                );
            }
            _ => {
                let result = match project.lang {
                    Lang::JavaScript => run_js_lint(file_path, &project.root, lenient),
                    Lang::Python => run_python_lint(file_path, &project.root, lenient),
                    _ => unreachable!(),
                };
                collect_lint_errors(result, file_path, &mut errors);
            }
        }
    }

    // Run clippy once per Rust project, filtering output for all collected files.
    for (root, files) in &rust_projects {
        collect_lint_errors(
            run_rust_lint_multi(files, root, lenient),
            &root.clone(),
            &mut errors,
        );
    }

    if errors.is_empty() {
        LintOutcome::Pass {
            message: format!(
                "[ralph-hook-lint] all {} {} passed lint.",
                paths.len(),
                success_scope
            ),
        }
    } else {
        LintOutcome::Fail {
            reason: errors.join("\n\n---\n\n"),
        }
    }
}

fn run_snapshot_turn(debug: bool) -> Result<HookOutput, Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    let session_id = match extract_session_id(&input) {
        Some(sid) if !sid.is_empty() => sid,
        _ => {
            return Ok(HookOutput::Stdout(continue_result(
                debug,
                "[ralph-hook-lint] no session_id, skipping turn snapshot.",
            )));
        }
    };

    let turn_id = match extract_turn_id(&input) {
        Some(turn_id) if !turn_id.is_empty() => turn_id,
        _ => {
            return Ok(HookOutput::Stdout(continue_result(
                debug,
                "[ralph-hook-lint] no turn_id, skipping turn snapshot.",
            )));
        }
    };

    let cwd = match extract_cwd(&input) {
        Some(cwd) if !cwd.is_empty() => cwd,
        _ => {
            return Ok(HookOutput::Stdout(continue_result(
                debug,
                "[ralph-hook-lint] no cwd, skipping turn snapshot.",
            )));
        }
    };

    let count = write_snapshot(&session_id, &turn_id, &cwd)?;
    Ok(HookOutput::Stdout(continue_result(
        debug,
        &format!("[ralph-hook-lint] captured baseline for {count} file(s)."),
    )))
}

/// Lint-turn mode (Codex Stop): synchronous `decision:block` JSON protocol.
fn run_lint_turn(debug: bool, lenient: bool) -> Result<HookOutput, Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    let session_id = match extract_session_id(&input) {
        Some(sid) if !sid.is_empty() => sid,
        _ => {
            return Ok(HookOutput::Stdout(continue_result(
                debug,
                "[ralph-hook-lint] no session_id, skipping turn lint.",
            )));
        }
    };

    let turn_id = match extract_turn_id(&input) {
        Some(turn_id) if !turn_id.is_empty() => turn_id,
        _ => {
            return Ok(HookOutput::Stdout(continue_result(
                debug,
                "[ralph-hook-lint] no turn_id, skipping turn lint.",
            )));
        }
    };

    let cwd = match extract_cwd(&input) {
        Some(cwd) if !cwd.is_empty() => cwd,
        _ => {
            return Ok(HookOutput::Stdout(continue_result(
                debug,
                "[ralph-hook-lint] no cwd, skipping turn lint.",
            )));
        }
    };

    let stop_hook_active = extract_stop_hook_active(&input).unwrap_or(false);
    let baseline = read_snapshot(&session_id, &turn_id)?;

    if baseline.is_empty() {
        return Ok(HookOutput::Stdout(continue_result(
            debug,
            "[ralph-hook-lint] no turn snapshot found, skipping turn lint.",
        )));
    }

    let current = scan_supported_files(&cwd)?;
    let changed_files = diff_changed_files(&baseline, &current);

    if changed_files.is_empty() {
        cleanup_snapshot(&session_id, &turn_id)?;
        return Ok(HookOutput::Stdout(continue_result(
            debug,
            "[ralph-hook-lint] no supported files changed this turn.",
        )));
    }

    let outcome = lint_paths(&changed_files, lenient, "changed file(s)");
    if !outcome.is_fail() {
        cleanup_snapshot(&session_id, &turn_id)?;
        return Ok(HookOutput::Stdout(outcome_to_block_json(&outcome, debug)));
    }

    if stop_hook_active {
        cleanup_snapshot(&session_id, &turn_id)?;
        return Ok(HookOutput::Stdout(
            r#"{"continue":true,"systemMessage":"[ralph-hook-lint] lint still failing after one Stop continuation; skipping a second auto-continue to avoid a loop."}"#
                .to_string(),
        ));
    }

    Ok(HookOutput::Stdout(outcome_to_block_json(&outcome, debug)))
}

/// Push the reason from a `Fail` outcome into the errors vec, or ignore passes.
fn collect_lint_errors(
    result: Result<LintOutcome, Box<dyn std::error::Error>>,
    label: &str,
    errors: &mut Vec<String>,
) {
    match result {
        Ok(LintOutcome::Fail { reason }) => errors.push(reason),
        Ok(LintOutcome::Pass { .. }) => {}
        Err(e) => errors.push(format!("[ralph-hook-lint] error linting {label}: {e}")),
    }
}

/// Default mode: lint a single file from `tool_input.file_path`.
/// Used by the optional eager-on-edit `PostToolUse` setup described in the README.
fn run(debug: bool, lenient: bool) -> Result<HookOutput, Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    let file_path = extract_file_path(&input);

    let file_path = match file_path {
        Some(fp) if !fp.is_empty() => fp,
        _ => {
            return Ok(HookOutput::Stdout(continue_result(
                debug,
                "[ralph-hook-lint] no file_path provided, skipping lint hook.",
            )));
        }
    };

    let Some(project) = find_project_root(&file_path) else {
        return Ok(HookOutput::Stdout(continue_result(
            debug,
            &format!(
                "[ralph-hook-lint] skipping lint: unsupported file type or no project found for {file_path}."
            ),
        )));
    };

    let outcome = match project.lang {
        Lang::JavaScript => run_js_lint(&file_path, &project.root, lenient)?,
        Lang::Rust => run_rust_lint(&file_path, &project.root, lenient)?,
        Lang::Python => run_python_lint(&file_path, &project.root, lenient)?,
        Lang::Java => run_java_lint(&file_path, &project.root, lenient)?,
        Lang::Go => run_go_lint(&file_path, &project.root, lenient)?,
    };

    Ok(HookOutput::Stdout(outcome_to_block_json(&outcome, debug)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn async_rewake_pass_is_silent() {
        let outcome = LintOutcome::Pass {
            message: "ok".to_string(),
        };
        assert!(matches!(
            outcome_to_async_rewake(outcome, false),
            HookOutput::Silent
        ));
    }

    #[test]
    fn async_rewake_pass_with_debug_is_still_silent_on_stdout() {
        // Debug message goes to stderr; the wire result is still Silent (exit 0).
        let outcome = LintOutcome::Pass {
            message: "ok".to_string(),
        };
        assert!(matches!(
            outcome_to_async_rewake(outcome, true),
            HookOutput::Silent
        ));
    }

    #[test]
    fn async_rewake_fail_writes_reason_to_stderr_with_exit_2() {
        let outcome = LintOutcome::Fail {
            reason: "lint exploded".to_string(),
        };
        match outcome_to_async_rewake(outcome, false) {
            HookOutput::StderrExit2(message) => assert_eq!(message, "lint exploded"),
            _ => panic!("expected StderrExit2"),
        }
    }

    #[test]
    fn collect_lint_errors_appends_fail_reason() {
        let mut errors = Vec::new();
        collect_lint_errors(
            Ok(LintOutcome::Fail {
                reason: "boom".to_string(),
            }),
            "src/x.rs",
            &mut errors,
        );
        assert_eq!(errors, vec!["boom".to_string()]);
    }

    #[test]
    fn collect_lint_errors_ignores_pass() {
        let mut errors = Vec::new();
        collect_lint_errors(
            Ok(LintOutcome::Pass {
                message: "ok".to_string(),
            }),
            "src/x.rs",
            &mut errors,
        );
        assert!(errors.is_empty());
    }
}
