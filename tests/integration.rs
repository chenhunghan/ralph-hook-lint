use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn run_binary(input: &str) -> String {
    run_binary_with_args(input, &[])
}

fn run_binary_debug(input: &str) -> String {
    run_binary_with_args(input, &["--debug"])
}

fn run_binary_lenient(input: &str) -> String {
    run_binary_with_args(input, &["--lenient", "--debug"])
}

fn run_binary_with_args(input: &str, args: &[&str]) -> String {
    let binary = env!("CARGO_BIN_EXE_ralph-hook-lint");
    let mut child = Command::new(binary)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn binary");

    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();

    let output = child.wait_with_output().expect("Failed to read output");
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn finds_package_json_directory() {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts/project");

    // Create a nested file path
    let file_path = fixture_dir.join("src/index.ts");
    let input = format!(
        r#"{{"tool_input":{{"file_path":"{}"}}}}"#,
        file_path.display()
    );

    let output = run_binary_debug(&input);

    // Should skip because no linter is installed, but should find package.json
    assert!(
        output.contains("no linter found") || output.contains("skipping lint"),
        "Unexpected output: {output}"
    );
}

#[test]
fn no_package_json_skips() {
    let input = r#"{"tool_input":{"file_path":"/tmp/no-package/file.ts"}}"#;
    let output = run_binary_debug(input);

    assert!(
        output.contains("no package.json found") || output.contains("skipping lint"),
        "Expected skip message, got: {output}"
    );
}

#[test]
fn unsupported_file_type_skips() {
    let input = r#"{"tool_input":{"file_path":"/some/path/file.py"}}"#;
    let output = run_binary_debug(input);

    assert!(
        output.contains("unsupported file type") || output.contains("skipping lint"),
        "Expected skip message for unsupported file, got: {output}"
    );
}

#[test]
fn missing_file_path_skips() {
    let input = r#"{"tool_input":{"other":"value"}}"#;
    let output = run_binary_debug(input);

    assert!(
        output.contains("no file_path provided"),
        "Expected no file_path message, got: {output}"
    );
}

#[test]
fn nested_projects_finds_closest_package_json() {
    // Structure:
    // nested/
    //   package.json         <- outer (should NOT be used)
    //   subproject/
    //     package.json       <- closest (should be used)
    //     src/
    //       index.ts         <- file being linted

    let fixture_dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts/nested/subproject");

    let file_path = fixture_dir.join("src/index.ts");
    let input = format!(
        r#"{{"tool_input":{{"file_path":"{}"}}}}"#,
        file_path.display()
    );

    let output = run_binary_debug(&input);

    // Valid outcomes: No linter found, Lint passed, or Lint errors (all mean package.json was found)
    assert!(
        output.contains("no linter found")
            || output.contains("lint passed")
            || output.contains("lint errors")
            || output.contains("skipping lint"),
        "Expected to find closest package.json, got: {output}"
    );

    // Verify npm prefix finds the closest package.json (subproject, not nested)
    let npm_output = Command::new("npm")
        .arg("prefix")
        .current_dir(fixture_dir.join("src"))
        .output()
        .expect("npm prefix failed");

    let prefix = String::from_utf8_lossy(&npm_output.stdout);
    assert!(
        prefix.trim().ends_with("subproject"),
        "npm prefix should find subproject (closest), got: {prefix}"
    );
}

#[test]
fn rust_project_finds_cargo_toml() {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rust/project");

    let file_path = fixture_dir.join("src/main.rs");
    let input = format!(
        r#"{{"tool_input":{{"file_path":"{}"}}}}"#,
        file_path.display()
    );

    let output = run_binary_debug(&input);

    // Should find Cargo.toml and run clippy (or report lint passed/errors)
    assert!(
        output.contains("clippy")
            || output.contains("lint passed")
            || output.contains("lint errors"),
        "Expected clippy to run for Rust project, got: {output}"
    );
}

#[test]
fn rust_monorepo_finds_crate_cargo_toml() {
    // Structure:
    // monorepo/
    //   Cargo.toml           <- workspace root
    //   crates/
    //     app/
    //       Cargo.toml       <- crate (should be found)
    //       src/
    //         lib.rs         <- file being linted

    let fixture_dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rust/monorepo/crates/app");

    let file_path = fixture_dir.join("src/lib.rs");
    let input = format!(
        r#"{{"tool_input":{{"file_path":"{}"}}}}"#,
        file_path.display()
    );

    let output = run_binary_debug(&input);

    // Should find Cargo.toml and run clippy
    assert!(
        output.contains("clippy")
            || output.contains("lint passed")
            || output.contains("lint errors"),
        "Expected clippy to run for Rust monorepo crate, got: {output}"
    );
}

#[test]
fn rust_file_no_cargo_toml_skips() {
    let input = r#"{"tool_input":{"file_path":"/tmp/no-cargo/file.rs"}}"#;
    let output = run_binary_debug(input);

    assert!(
        output.contains("unsupported file type")
            || output.contains("no project found")
            || output.contains("skipping lint"),
        "Expected skip message for Rust file without Cargo.toml, got: {output}"
    );
}

#[test]
fn no_debug_omits_system_message_on_continue() {
    let input = r#"{"tool_input":{"other":"value"}}"#;
    let output = run_binary(input);

    assert_eq!(
        output.trim(),
        r#"{"continue":true}"#,
        "Without --debug, continue responses should not contain systemMessage"
    );
}

#[test]
fn no_debug_skips_unsupported_without_system_message() {
    let input = r#"{"tool_input":{"file_path":"/tmp/no-cargo/file.rs"}}"#;
    let output = run_binary(input);

    assert_eq!(
        output.trim(),
        r#"{"continue":true}"#,
        "Without --debug, skip responses should not contain systemMessage"
    );
}

#[test]
fn lenient_flag_accepted_for_ts() {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts/project");
    let file_path = fixture_dir.join("src/index.ts");
    let input = format!(
        r#"{{"tool_input":{{"file_path":"{}"}}}}"#,
        file_path.display()
    );

    let output = run_binary_lenient(&input);

    // Should not crash; valid outcomes with --lenient
    assert!(
        output.contains("no linter found")
            || output.contains("lint passed")
            || output.contains("lint errors")
            || output.contains("skipping lint"),
        "Expected valid output with --lenient for TS, got: {output}"
    );
}

#[test]
fn lenient_flag_accepted_for_rust() {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rust/project");
    let file_path = fixture_dir.join("src/main.rs");
    let input = format!(
        r#"{{"tool_input":{{"file_path":"{}"}}}}"#,
        file_path.display()
    );

    let output = run_binary_lenient(&input);

    // Should run clippy with lenient flags without crashing
    assert!(
        output.contains("clippy")
            || output.contains("lint passed")
            || output.contains("lint errors"),
        "Expected clippy to run with --lenient for Rust, got: {output}"
    );
}

#[test]
fn lenient_without_debug_produces_valid_output() {
    let input = r#"{"tool_input":{"other":"value"}}"#;
    let output = run_binary_with_args(input, &["--lenient"]);

    assert_eq!(
        output.trim(),
        r#"{"continue":true}"#,
        "--lenient without --debug should produce clean JSON"
    );
}

// ── Collect / lint-collected integration tests ──

fn collect_temp_path(session_id: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("ralph-lint-{session_id}.txt"))
}

#[test]
fn collect_records_file_path() {
    let sid = format!("integ-collect-{}", std::process::id());
    let _ = fs::remove_file(collect_temp_path(&sid));

    let input = format!(
        r#"{{"session_id":"{sid}","tool_name":"Edit","tool_input":{{"file_path":"/tmp/test.rs"}}}}"#,
    );
    let output = run_binary_with_args(&input, &["--collect"]);

    assert_eq!(
        output.trim(),
        r#"{"continue":true}"#,
        "collect mode should return continue, got: {output}"
    );

    // Verify the temp file was created with the path
    let contents = fs::read_to_string(collect_temp_path(&sid)).unwrap();
    assert!(
        contents.contains("/tmp/test.rs"),
        "temp file should contain the path, got: {contents}"
    );

    // Cleanup
    let _ = fs::remove_file(collect_temp_path(&sid));
}

#[test]
fn collect_deduplicates() {
    let sid = format!("integ-dedup-{}", std::process::id());
    let _ = fs::remove_file(collect_temp_path(&sid));

    let input = format!(
        r#"{{"session_id":"{sid}","tool_name":"Edit","tool_input":{{"file_path":"/tmp/dup.rs"}}}}"#,
    );

    // Record same path twice
    run_binary_with_args(&input, &["--collect"]);
    run_binary_with_args(&input, &["--collect"]);

    let contents = fs::read_to_string(collect_temp_path(&sid)).unwrap();
    let lines: Vec<&str> = contents.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "Should have exactly one entry after dedup, got: {lines:?}"
    );

    let _ = fs::remove_file(collect_temp_path(&sid));
}

#[test]
fn lint_collected_no_files() {
    // Use a fresh session_id with no collected files
    let sid = format!("integ-empty-{}", std::process::id());
    let _ = fs::remove_file(collect_temp_path(&sid));

    let input = format!(r#"{{"session_id":"{sid}"}}"#);
    let output = run_binary_with_args(&input, &["--lint-collected", "--debug"]);

    assert!(
        output.contains("no files collected") || output.contains(r#""continue":true"#),
        "lint-collected with no files should continue, got: {output}"
    );
}

#[test]
fn lint_collected_cleans_up() {
    let sid = format!("integ-cleanup-{}", std::process::id());
    let _ = fs::remove_file(collect_temp_path(&sid));

    // Collect a file that won't match any project (so lint just skips it)
    let collect_input = format!(
        r#"{{"session_id":"{sid}","tool_name":"Edit","tool_input":{{"file_path":"/tmp/no-project/fake.rs"}}}}"#,
    );
    run_binary_with_args(&collect_input, &["--collect"]);
    assert!(
        collect_temp_path(&sid).exists(),
        "temp file should exist after collect"
    );

    // Now run lint-collected — should clean up the temp file
    let lint_input = format!(r#"{{"session_id":"{sid}"}}"#);
    let output = run_binary_with_args(&lint_input, &["--lint-collected"]);

    assert!(
        output.contains(r#""continue":true"#),
        "lint-collected should continue for unsupported files, got: {output}"
    );
    assert!(
        !collect_temp_path(&sid).exists(),
        "temp file should be deleted after lint-collected"
    );
}
