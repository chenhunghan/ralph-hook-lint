mod extract;
mod lint;
mod project;

use std::env;
use std::io::{self, Read};

use extract::extract_file_path;
use lint::{
    continue_result, run_go_lint, run_java_lint, run_js_lint, run_python_lint, run_rust_lint,
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

    let result = run(debug, lenient);
    match result {
        Ok(output) => println!("{output}"),
        Err(e) => println!(
            "{}",
            continue_result(debug, &format!("[ralph-hook-lint] lint hook error: {e}"))
        ),
    }
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
