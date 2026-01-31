use std::io::{self, Read};
use std::path::Path;
use std::process::Command;

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

    // Find the nearest package.json directory using npm prefix
    let file_dir = Path::new(&file_path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());

    let package_dir = Command::new("npm")
        .arg("prefix")
        .current_dir(&file_dir)
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        });

    let package_dir = match package_dir {
        Some(dir) if !dir.is_empty() => dir,
        _ => {
            return Ok(format!(
                r#"{{"continue":true,"systemMessage":"Skipping lint: no package.json found for {}."}}"#,
                escape_json(&file_path)
            ));
        }
    };

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

/// Extract file_path from JSON like {"tool_input":{"file_path":"/some/path"}}
fn extract_file_path(json: &str) -> Option<String> {
    let marker = r#""file_path":"#;
    let start = json.find(marker)? + marker.len();
    let rest = &json[start..];

    // Skip whitespace
    let rest = rest.trim_start();

    // Expect a quote
    if !rest.starts_with('"') {
        return None;
    }

    let rest = &rest[1..];
    let mut result = String::new();
    let mut chars = rest.chars();

    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(result),
            '\\' => {
                if let Some(escaped) = chars.next() {
                    match escaped {
                        'n' => result.push('\n'),
                        'r' => result.push('\r'),
                        't' => result.push('\t'),
                        '\\' => result.push('\\'),
                        '"' => result.push('"'),
                        '/' => result.push('/'),
                        _ => {
                            result.push('\\');
                            result.push(escaped);
                        }
                    }
                }
            }
            _ => result.push(c),
        }
    }
    None
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
