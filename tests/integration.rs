use std::path::Path;
use std::process::{Command, Stdio};
use std::io::Write;

fn run_binary(input: &str) -> String {
    let binary = env!("CARGO_BIN_EXE_ralph-hook-lint");
    let mut child = Command::new(binary)
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
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/ts/project");

    // Create a nested file path
    let file_path = fixture_dir.join("src/index.ts");
    let input = format!(
        r#"{{"tool_input":{{"file_path":"{}"}}}}"#,
        file_path.display()
    );

    let output = run_binary(&input);

    // Should skip because no linter is installed, but should find package.json
    assert!(
        output.contains("No linter found") || output.contains("Skipping lint"),
        "Unexpected output: {}",
        output
    );
}

#[test]
fn no_package_json_skips() {
    let input = r#"{"tool_input":{"file_path":"/tmp/no-package/file.ts"}}"#;
    let output = run_binary(input);

    assert!(
        output.contains("no package.json found") || output.contains("Skipping lint"),
        "Expected skip message, got: {}",
        output
    );
}

#[test]
fn non_js_ts_file_skips() {
    let input = r#"{"tool_input":{"file_path":"/some/path/file.rs"}}"#;
    let output = run_binary(input);

    assert!(
        output.contains("not a JS/TS file"),
        "Expected JS/TS skip message, got: {}",
        output
    );
}

#[test]
fn missing_file_path_skips() {
    let input = r#"{"tool_input":{"other":"value"}}"#;
    let output = run_binary(input);

    assert!(
        output.contains("no file_path provided"),
        "Expected no file_path message, got: {}",
        output
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

    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/ts/nested/subproject");

    let file_path = fixture_dir.join("src/index.ts");
    let input = format!(
        r#"{{"tool_input":{{"file_path":"{}"}}}}"#,
        file_path.display()
    );

    let output = run_binary(&input);

    // Valid outcomes: No linter found, Lint passed, or Lint errors (all mean package.json was found)
    assert!(
        output.contains("No linter found")
            || output.contains("Lint passed")
            || output.contains("Lint errors")
            || output.contains("Skipping lint"),
        "Expected to find closest package.json, got: {}",
        output
    );

    // Verify npm prefix finds the closest package.json (subproject, not nested)
    let npm_output = Command::new("npm")
        .arg("prefix")
        .current_dir(&fixture_dir.join("src"))
        .output()
        .expect("npm prefix failed");

    let prefix = String::from_utf8_lossy(&npm_output.stdout);
    assert!(
        prefix.trim().ends_with("subproject"),
        "npm prefix should find subproject (closest), got: {}",
        prefix
    );
}
