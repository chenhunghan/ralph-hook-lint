use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::project::detect_lang;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fingerprint {
    modified_ns: u128,
    len: u64,
}

impl Fingerprint {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        let modified_ns = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |duration| duration.as_nanos());

        Self {
            modified_ns,
            len: metadata.len(),
        }
    }
}

fn should_skip_dir(name: &OsStr) -> bool {
    matches!(
        name.to_str(),
        Some(
            ".git"
                | ".hg"
                | ".svn"
                | ".idea"
                | ".vscode"
                | ".codex"
                | ".claude"
                | ".next"
                | ".nuxt"
                | ".turbo"
                | ".venv"
                | ".mypy_cache"
                | ".pytest_cache"
                | ".ruff_cache"
                | "__pycache__"
                | "node_modules"
                | "target"
                | "dist"
                | "build"
                | "coverage"
                | "out"
                | "vendor"
        )
    )
}

fn collect_supported_files(
    dir: &Path,
    files: &mut HashMap<String, Fingerprint>,
) -> Result<(), Box<dyn std::error::Error>> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();

        if file_type.is_dir() {
            if should_skip_dir(&entry.file_name()) {
                continue;
            }
            collect_supported_files(&path, files)?;
            continue;
        }

        if !file_type.is_file() {
            continue;
        }

        let path_str = path.to_string_lossy().to_string();
        if detect_lang(&path_str).is_none() {
            continue;
        }

        let metadata = entry.metadata()?;
        files.insert(path_str, Fingerprint::from_metadata(&metadata));
    }

    Ok(())
}

pub fn scan_supported_files(
    cwd: &str,
) -> Result<HashMap<String, Fingerprint>, Box<dyn std::error::Error>> {
    let mut files = HashMap::new();
    collect_supported_files(Path::new(cwd), &mut files)?;
    Ok(files)
}

pub fn snapshot_path(session_id: &str, turn_id: &str) -> PathBuf {
    std::env::temp_dir().join(format!("ralph-lint-turn-{session_id}-{turn_id}.txt"))
}

pub fn write_snapshot(
    session_id: &str,
    turn_id: &str,
    cwd: &str,
) -> Result<usize, Box<dyn std::error::Error>> {
    let snapshot = scan_supported_files(cwd)?;
    let path = snapshot_path(session_id, turn_id);
    let mut entries: Vec<_> = snapshot.into_iter().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let count = entries.len();

    let mut file = fs::File::create(path)?;
    for (file_path, fingerprint) in entries {
        writeln!(
            file,
            "{}\t{}\t{}",
            file_path, fingerprint.modified_ns, fingerprint.len
        )?;
    }

    Ok(count)
}

pub fn read_snapshot(
    session_id: &str,
    turn_id: &str,
) -> Result<HashMap<String, Fingerprint>, Box<dyn std::error::Error>> {
    let path = snapshot_path(session_id, turn_id);
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut snapshot = HashMap::new();

    for line in reader.lines() {
        let line = line?;
        if line.is_empty() {
            continue;
        }

        let mut parts = line.splitn(3, '\t');
        let Some(file_path) = parts.next() else {
            continue;
        };
        let Some(modified_ns) = parts.next() else {
            continue;
        };
        let Some(len) = parts.next() else {
            continue;
        };

        let Ok(modified_ns) = modified_ns.parse::<u128>() else {
            continue;
        };
        let Ok(len) = len.parse::<u64>() else {
            continue;
        };

        snapshot.insert(file_path.to_string(), Fingerprint { modified_ns, len });
    }

    Ok(snapshot)
}

pub fn cleanup_snapshot(session_id: &str, turn_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let path = snapshot_path(session_id, turn_id);
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub fn diff_changed_files(
    baseline: &HashMap<String, Fingerprint>,
    current: &HashMap<String, Fingerprint>,
) -> Vec<String> {
    let mut changed = current
        .iter()
        .filter_map(|(file_path, fingerprint)| match baseline.get(file_path) {
            None => Some(file_path.clone()),
            Some(existing) if existing != fingerprint => Some(file_path.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    changed.sort();
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ralph-lint-snapshot-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn scan_supported_files_ignores_skipped_dirs() {
        let dir = temp_dir("scan");
        let src_dir = dir.join("src");
        let node_modules_dir = dir.join("node_modules");
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&node_modules_dir).unwrap();
        fs::write(src_dir.join("main.rs"), "fn main() {}\n").unwrap();
        fs::write(node_modules_dir.join("ignored.ts"), "export {};\n").unwrap();

        let files = scan_supported_files(&dir.to_string_lossy()).unwrap();

        assert!(files.contains_key(&src_dir.join("main.rs").to_string_lossy().to_string()));
        assert!(
            !files.contains_key(
                &node_modules_dir
                    .join("ignored.ts")
                    .to_string_lossy()
                    .to_string()
            )
        );
    }

    #[test]
    fn diff_changed_files_detects_new_and_modified_files() {
        let dir = temp_dir("diff");
        fs::create_dir_all(dir.join("src")).unwrap();
        let file_path = dir.join("src/lib.rs");
        fs::write(&file_path, "pub fn before() {}\n").unwrap();

        let baseline = scan_supported_files(&dir.to_string_lossy()).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(5));
        fs::write(&file_path, "pub fn after() {}\n").unwrap();
        let new_file = dir.join("src/new.ts");
        fs::write(&new_file, "export const value = 1;\n").unwrap();

        let current = scan_supported_files(&dir.to_string_lossy()).unwrap();
        let changed = diff_changed_files(&baseline, &current);

        assert_eq!(
            changed,
            vec![
                file_path.to_string_lossy().to_string(),
                new_file.to_string_lossy().to_string()
            ]
        );
    }

    #[test]
    fn write_and_read_snapshot_round_trip() {
        let dir = temp_dir("roundtrip");
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/main.py"), "print('hello')\n").unwrap();

        let session_id = format!("session-{}", std::process::id());
        let turn_id = "turn-1";
        let count = write_snapshot(&session_id, turn_id, &dir.to_string_lossy()).unwrap();
        let snapshot = read_snapshot(&session_id, turn_id).unwrap();

        assert_eq!(count, 1);
        assert_eq!(snapshot.len(), 1);

        cleanup_snapshot(&session_id, turn_id).unwrap();
        let _ = fs::remove_dir_all(dir);
    }
}
