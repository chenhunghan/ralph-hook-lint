use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

/// Returns the temp file path for a given session: `<temp_dir>/ralph-lint-<session_id>.txt`
pub fn temp_path(session_id: &str) -> PathBuf {
    std::env::temp_dir().join(format!("ralph-lint-{session_id}.txt"))
}

/// Append `file_path` to the session's temp file, skipping if already present.
pub fn record_path(session_id: &str, file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let path = temp_path(session_id);

    // Read existing entries to check for duplicates
    let existing: Vec<String> = if path.exists() {
        let file = fs::File::open(&path)?;
        BufReader::new(file)
            .lines()
            .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };

    if existing.iter().any(|line| line == file_path) {
        return Ok(());
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    writeln!(file, "{file_path}")?;
    Ok(())
}

/// Read all recorded paths, then delete the temp file. Returns an empty vec if the file
/// does not exist.
pub fn read_and_cleanup(session_id: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let path = temp_path(session_id);

    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = fs::File::open(&path)?;
    let paths: Vec<String> = BufReader::new(file)
        .lines()
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|l| !l.is_empty())
        .collect();

    fs::remove_file(&path)?;
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_session() -> String {
        format!("test-{}", std::process::id())
    }

    #[test]
    fn record_and_read_single_path() {
        let sid = format!("{}-single", unique_session());
        // Ensure clean state
        let _ = fs::remove_file(temp_path(&sid));

        record_path(&sid, "/tmp/a.rs").unwrap();
        let paths = read_and_cleanup(&sid).unwrap();
        assert_eq!(paths, vec!["/tmp/a.rs"]);
        // File should be deleted
        assert!(!temp_path(&sid).exists());
    }

    #[test]
    fn dedup_same_path() {
        let sid = format!("{}-dedup", unique_session());
        let _ = fs::remove_file(temp_path(&sid));

        record_path(&sid, "/tmp/b.rs").unwrap();
        record_path(&sid, "/tmp/b.rs").unwrap();
        record_path(&sid, "/tmp/c.rs").unwrap();

        let paths = read_and_cleanup(&sid).unwrap();
        assert_eq!(paths, vec!["/tmp/b.rs", "/tmp/c.rs"]);
    }

    #[test]
    fn read_and_cleanup_nonexistent() {
        let sid = "nonexistent-session-xyz";
        let paths = read_and_cleanup(sid).unwrap();
        assert!(paths.is_empty());
    }

    #[test]
    fn cleanup_deletes_file() {
        let sid = format!("{}-cleanup", unique_session());
        let _ = fs::remove_file(temp_path(&sid));

        record_path(&sid, "/tmp/d.rs").unwrap();
        assert!(temp_path(&sid).exists());

        let _ = read_and_cleanup(&sid).unwrap();
        assert!(!temp_path(&sid).exists());
    }
}
