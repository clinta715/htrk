// Plugin discovery — scan standard search paths for .clap (and future .vst3) files.
// Phase 1: filesystem scan only (list .clap files).
// Phase 2+: probe each plugin via CLAP factory to extract descriptors.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::default_search_paths;

/// Result of a discovery scan: the set of .clap files found across all search roots.
#[derive(Clone, Debug, Default)]
pub struct ScanResult {
    pub clap_files: Vec<PathBuf>,
    pub errors: Vec<ScanError>,
}

#[derive(Clone, Debug)]
pub struct ScanError {
    pub path: PathBuf,
    pub message: String,
}

impl ScanResult {
    pub fn total_found(&self) -> usize {
        self.clap_files.len()
    }
}

/// Scan all standard CLAP search paths for `.clap` files.
/// Returns a list of candidate plugin files; probing each one for metadata
/// happens in Phase 2 (via the CLAP factory).
pub fn scan_default_paths() -> ScanResult {
    scan_paths(&default_search_paths())
}

/// Scan a specific list of paths for `.clap` files.
/// Skips directories that cannot be read, recording the error.
pub fn scan_paths(paths: &[PathBuf]) -> ScanResult {
    let mut result = ScanResult::default();
    let mut seen: HashSet<PathBuf> = HashSet::new();

    for path in paths {
        if !path.exists() {
            continue;
        }
        match scan_single_path(path, &mut seen) {
            Ok(found) => result.clap_files.extend(found),
            Err(e) => result.errors.push(e),
        }
    }

    result.clap_files.sort();
    result
}

fn scan_single_path(
    root: &Path,
    seen: &mut HashSet<PathBuf>,
) -> Result<Vec<PathBuf>, ScanError> {
    let mut found = Vec::new();
    walk_dir(root, &mut |entry| {
        if let Some(ext) = entry.extension().and_then(|e| e.to_str()) {
            if ext.eq_ignore_ascii_case("clap") {
                let canonical = entry.canonicalize().unwrap_or_else(|_| entry.to_path_buf());
                if seen.insert(canonical.clone()) {
                    found.push(canonical);
                }
            }
        }
    })
    .map_err(|e| ScanError {
        path: root.to_path_buf(),
        message: e,
    })?;
    Ok(found)
}

fn walk_dir<F: FnMut(&Path)>(dir: &Path, on_entry: &mut F) -> Result<(), String> {
    let read = std::fs::read_dir(dir).map_err(|e| format!("read_dir: {e}"))?;
    for entry_result in read {
        let entry = match entry_result {
            Ok(e) => e,
            Err(e) => {
                eprintln!("[plugin-discovery] read_dir entry error: {e}");
                continue;
            }
        };
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(e) => {
                eprintln!("[plugin-discovery] file_type error: {e}");
                continue;
            }
        };
        if file_type.is_dir() {
            walk_dir(&path, on_entry)?;
        } else if file_type.is_file() {
            on_entry(&path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_scan_empty_dir() {
        let tmp = std::env::temp_dir().join(format!("htrk-plugins-test-{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let result = scan_paths(&[tmp.clone()]);
        assert!(result.clap_files.is_empty());
        assert!(result.errors.is_empty());
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_scan_finds_clap_files() {
        let tmp = std::env::temp_dir().join(format!("htrk-plugins-test-{}-2", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("fake.clap"), b"fake").unwrap();
        fs::write(tmp.join("not_a_plugin.txt"), b"x").unwrap();
        let result = scan_paths(&[tmp.clone()]);
        assert_eq!(result.clap_files.len(), 1);
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_scan_skips_missing_paths() {
        let nonexistent = std::path::PathBuf::from("Z:\\does\\not\\exist\\at\\all");
        let result = scan_paths(&[nonexistent]);
        assert!(result.clap_files.is_empty());
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_scan_dedupes_across_paths() {
        let tmp = std::env::temp_dir().join(format!("htrk-plugins-test-{}-3", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("plugin.clap"), b"x").unwrap();
        let result = scan_paths(&[tmp.clone(), tmp.clone()]);
        assert_eq!(result.clap_files.len(), 1);
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_scan_recursive() {
        let tmp = std::env::temp_dir().join(format!("htrk-plugins-test-{}-4", std::process::id()));
        let sub = tmp.join("nested");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("deep.clap"), b"x").unwrap();
        let result = scan_paths(&[tmp.clone()]);
        assert_eq!(result.clap_files.len(), 1);
        let _ = fs::remove_dir_all(&tmp);
    }

    /// Integration test: scan the system's CLAP directory.
    /// Skipped if the path doesn't exist (non-Windows or CLAP not installed).
    #[test]
    fn test_scan_system_clap_dir() {
        let system_path = std::path::Path::new(r"C:\Program Files\Common Files\CLAP");
        if !system_path.exists() {
            eprintln!("[skip] System CLAP dir not found");
            return;
        }
        let result = scan_paths(&[system_path.to_path_buf()]);
        eprintln!("[ok] Found {} .clap entries in system CLAP dir", result.total_found());
        eprintln!("     errors: {}", result.errors.len());
        assert!(result.total_found() >= 10, "Expected at least 10 .clap files on this system");
    }
}
