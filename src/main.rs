mod extract;
mod project;

use std::io::{self, Read};
use std::path::Path;
use std::process::Command;

use extract::extract_file_path;
use project::find_project_root;

fn main() {
    let result = run();
    match result {
        Ok(output) => println!("{}", output),
        Err(e) => println!(r#"{{"continue":true,"systemMessage":"Lint hook error: {}"}}"#, escape_json(&e.to_string())),
    }
}

fn run() -> Result<String, Box<dyn std::error::Error>> {
    // Read input from stdin
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    // Extract file_path from tool_input.file_path using simple string search
    let file_path = extract_file_path(&input);

    let file_path = match file_path {
        Some(fp) if !fp.is_empty() => fp,
        _ => return Ok(r#"{"continue":true,"systemMessage":"no file_path provided, skipping lint hook."}"#.to_string()),
    };

    // Skip non-JS/TS files
    if !is_js_ts_file(&file_path) {
        return Ok(format!(
            r#"{{"continue":true,"systemMessage":"Skipping lint: {} is not a JS/TS file."}}"#,
            escape_json(&file_path)
        ));
    }

    // Find the nearest project root
    let project = match find_project_root(&file_path) {
        Some(p) => p,
        None => {
            return Ok(format!(
                r#"{{"continue":true,"systemMessage":"Skipping lint: no package.json found for {}."}}"#,
                escape_json(&file_path)
            ));
        }
    };

    let package_dir = project.root;

    // Try linters in order: oxlint, biome, eslint
    let linters: &[(&str, &[&str])] = &[
        ("oxlint", &["{{file}}"]),
        ("biome", &["lint", "{{file}}"]),
        ("eslint", &["{{file}}"]),
    ];

    for (linter, args) in linters {
        let bin_path = format!("{}/node_modules/.bin/{}", package_dir, linter);
        if Path::new(&bin_path).exists() {
            let actual_args: Vec<String> = args
                .iter()
                .map(|a| a.replace("{{file}}", &file_path))
                .collect();

            let output = Command::new(&bin_path)
                .args(&actual_args)
                .current_dir(&package_dir)
                .output()?;

            return Ok(output_lint_result(
                linter,
                &file_path,
                &String::from_utf8_lossy(&output.stdout),
                &String::from_utf8_lossy(&output.stderr),
                output.status.success(),
            ));
        }
    }

    // Try npm run lint
    let npm_lint = Command::new("npm")
        .args(["run", "lint", "--if-present", "--", &file_path])
        .current_dir(&package_dir)
        .output();

    if let Ok(output) = npm_lint {
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if !combined.contains("Missing script") && !combined.contains("npm error") {
            return Ok(output_lint_result(
                "npm run lint",
                &file_path,
                &String::from_utf8_lossy(&output.stdout),
                &String::from_utf8_lossy(&output.stderr),
                output.status.success(),
            ));
        }
    }

    // No linter found
    Ok(format!(
        r#"{{"continue":true,"systemMessage":"No linter found for {}."}}"#,
        escape_json(&file_path)
    ))
}

fn is_js_ts_file(path: &str) -> bool {
    let extensions = [".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs"];
    extensions.iter().any(|ext| path.ends_with(ext))
}

fn escape_json(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => result.push_str(r#"\""#),
            '\\' => result.push_str(r#"\\"#),
            '\n' => result.push_str(r#"\n"#),
            '\r' => result.push_str(r#"\r"#),
            '\t' => result.push_str(r#"\t"#),
            c if c.is_control() => {
                result.push_str(&format!(r#"\u{:04x}"#, c as u32));
            }
            c => result.push(c),
        }
    }
    result
}

fn output_lint_result(linter: &str, file_path: &str, stdout: &str, stderr: &str, success: bool) -> String {
    if success {
        format!(
            r#"{{"continue":true,"systemMessage":"Lint passed for {} using {}."}}"#,
            escape_json(file_path),
            escape_json(linter)
        )
    } else {
        let output = if !stdout.is_empty() && !stderr.is_empty() {
            format!("{}\n{}", stdout, stderr)
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
