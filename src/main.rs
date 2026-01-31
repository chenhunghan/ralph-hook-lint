mod extract;
mod lint;
mod project;

use std::env;
use std::io::{self, Read};

use extract::extract_file_path;
use lint::{escape_json, run_go_lint, run_java_lint, run_js_lint, run_python_lint, run_rust_lint};
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
        Lang::Python => run_python_lint(&file_path, &project.root),
        Lang::Java => run_java_lint(&file_path, &project.root),
        Lang::Go => run_go_lint(&file_path, &project.root),
    }
}
