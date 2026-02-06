//! File Watcher
//!
//! Watches for file changes to trigger hot reload.

use crate::v2::{Error, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub enum WatchEvent {
    Modified(PathBuf),

    Created(PathBuf),

    Deleted(PathBuf),
}

pub struct FileWatcher {
    base_dir: PathBuf,

    patterns: Vec<String>,

    running: Arc<std::sync::atomic::AtomicBool>,

    last_modified: Arc<Mutex<HashMap<PathBuf, std::time::SystemTime>>>,

    pending_events: Arc<Mutex<HashMap<PathBuf, (WatchEvent, Instant)>>>,

    debounce_ms: u64,

    thread_handle: Option<std::thread::JoinHandle<()>>,
}

impl FileWatcher {
    pub fn new(base_dir: &Path, patterns: Vec<String>) -> Result<Self> {
        Ok(Self {
            base_dir: base_dir.to_path_buf(),
            patterns,
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            last_modified: Arc::new(Mutex::new(HashMap::new())),
            pending_events: Arc::new(Mutex::new(HashMap::new())),
            debounce_ms: 200, // 200ms debounce by default
            thread_handle: None,
        })
    }

    pub fn with_debounce(base_dir: &Path, patterns: Vec<String>, debounce_ms: u64) -> Result<Self> {
        Ok(Self {
            base_dir: base_dir.to_path_buf(),
            patterns,
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            last_modified: Arc::new(Mutex::new(HashMap::new())),
            pending_events: Arc::new(Mutex::new(HashMap::new())),
            debounce_ms,
            thread_handle: None,
        })
    }

    pub fn start<F>(&mut self, callback: F) -> Result<()>
    where
        F: Fn(WatchEvent) + Send + 'static,
    {
        self.running
            .store(true, std::sync::atomic::Ordering::SeqCst);

        self.scan_files()?;

        let base_dir = self.base_dir.clone();
        let patterns = self.patterns.clone();
        let running = Arc::clone(&self.running);
        let last_modified = Arc::clone(&self.last_modified);
        let pending_events = Arc::clone(&self.pending_events);
        let debounce_ms = self.debounce_ms;

        let handle = std::thread::spawn(move || {
            while running.load(std::sync::atomic::Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(100));

                if let Ok(events) = check_for_changes(&base_dir, &patterns, &last_modified) {
                    let mut pending = pending_events.lock().unwrap();
                    for event in events {
                        let path = match &event {
                            WatchEvent::Modified(p)
                            | WatchEvent::Created(p)
                            | WatchEvent::Deleted(p) => p.clone(),
                        };
                        pending.insert(path, (event, Instant::now()));
                    }
                }

                let now = Instant::now();
                let debounce_duration = Duration::from_millis(debounce_ms);
                let mut pending = pending_events.lock().unwrap();
                let ready: Vec<_> = pending
                    .iter()
                    .filter(|(_, (_, ts))| now.duration_since(*ts) >= debounce_duration)
                    .map(|(path, (event, _))| (path.clone(), event.clone()))
                    .collect();

                for (path, event) in ready {
                    pending.remove(&path);
                    callback(event);
                }
            }
        });

        self.thread_handle = Some(handle);
        Ok(())
    }

    pub fn stop(&mut self) {
        self.running
            .store(false, std::sync::atomic::Ordering::SeqCst);

        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }

    fn scan_files(&self) -> Result<()> {
        let mut last_modified = self.last_modified.lock().unwrap();

        for pattern in &self.patterns {
            let paths = glob_files(&self.base_dir, pattern)?;
            for path in paths {
                if let Ok(metadata) = std::fs::metadata(&path) {
                    if let Ok(modified) = metadata.modified() {
                        last_modified.insert(path, modified);
                    }
                }
            }
        }

        Ok(())
    }

    #[allow(dead_code)]
    fn matches(&self, path: &Path) -> bool {
        let rel_path = path.strip_prefix(&self.base_dir).unwrap_or(path);
        let path_str = rel_path.to_string_lossy();

        for pattern in &self.patterns {
            if glob_matches(pattern, &path_str) {
                return true;
            }
        }
        false
    }
}

impl Drop for FileWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

fn check_for_changes(
    base_dir: &Path,
    patterns: &[String],
    last_modified: &Arc<Mutex<HashMap<PathBuf, std::time::SystemTime>>>,
) -> Result<Vec<WatchEvent>> {
    let mut events = Vec::new();
    let mut current_files: HashMap<PathBuf, std::time::SystemTime> = HashMap::new();

    for pattern in patterns {
        let paths = glob_files(base_dir, pattern)?;
        for path in paths {
            if let Ok(metadata) = std::fs::metadata(&path) {
                if let Ok(modified) = metadata.modified() {
                    current_files.insert(path, modified);
                }
            }
        }
    }

    let mut last = last_modified.lock().unwrap();

    for (path, modified) in &current_files {
        match last.get(path) {
            Some(last_mod) if modified > last_mod => {
                events.push(WatchEvent::Modified(path.clone()));
            }
            None => {
                events.push(WatchEvent::Created(path.clone()));
            }
            _ => {}
        }
    }

    for path in last.keys() {
        if !current_files.contains_key(path) {
            events.push(WatchEvent::Deleted(path.clone()));
        }
    }

    *last = current_files;

    Ok(events)
}

fn glob_files(base_dir: &Path, pattern: &str) -> Result<Vec<PathBuf>> {
    let mut results = Vec::new();

    fn walk_dir(
        dir: &Path,
        base_dir: &Path,
        pattern: &str,
        results: &mut Vec<PathBuf>,
    ) -> std::io::Result<()> {
        if !dir.is_dir() {
            return Ok(());
        }

        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(err) => {
                if err.kind() == std::io::ErrorKind::PermissionDenied {
                    return Ok(());
                }
                return Err(err);
            }
        };

        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    if err.kind() == std::io::ErrorKind::PermissionDenied {
                        continue;
                    }
                    return Err(err);
                }
            };
            let path = entry.path();
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            if file_name.starts_with('.') {
                continue;
            }

            if path.is_dir() {
                if pattern.contains("**") {
                    walk_dir(&path, base_dir, pattern, results)?;
                }
            } else {
                let rel_path = path.strip_prefix(base_dir).unwrap_or(&path);
                let rel_path = rel_path.to_string_lossy();
                if glob_matches(pattern, &rel_path) {
                    results.push(path);
                }
            }
        }

        Ok(())
    }

    walk_dir(base_dir, base_dir, pattern, &mut results).map_err(|e| Error::Io(e))?;

    Ok(results)
}

fn glob_matches(pattern: &str, path: &str) -> bool {
    let path = path.replace('\\', "/");
    let pattern = pattern.replace('\\', "/");

    glob_match_recursive(&pattern, &path)
}

fn glob_match_recursive(pattern: &str, path: &str) -> bool {
    if pattern.contains("**/") {
        let parts: Vec<&str> = pattern.splitn(2, "**/").collect();
        if parts.len() == 2 {
            let prefix = parts[0];
            let suffix = parts[1];

            if !prefix.is_empty() && !path.starts_with(prefix) {
                return false;
            }

            let remaining = if prefix.is_empty() {
                path.to_string()
            } else {
                path.strip_prefix(prefix).unwrap_or(path).to_string()
            };

            if glob_match_recursive(suffix, &remaining) {
                return true;
            }

            for (i, _) in remaining.match_indices('/') {
                if glob_match_recursive(suffix, &remaining[i + 1..]) {
                    return true;
                }
            }

            return false;
        }
    }

    if pattern.contains('*') && !pattern.contains("**") {
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            return path.starts_with(parts[0]) && path.ends_with(parts[1]);
        }
    }

    if pattern.contains('?') {
        let regex_pattern = pattern
            .replace(".", "\\.")
            .replace('*', "[^/]*")
            .replace('?', ".");

        if let Ok(regex) = regex::Regex::new(&format!("^{}$", regex_pattern)) {
            return regex.is_match(path);
        }
    }

    pattern == path
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_glob_matches() {
        assert!(glob_matches("*.rs", "main.rs"));
        assert!(glob_matches("src/**/*.rs", "src/lib.rs"));
        assert!(glob_matches("*.wit", "hello.wit"));
        assert!(!glob_matches("*.rs", "main.txt"));
    }

    #[test]
    fn test_glob_files_relative() {
        let temp = TempDir::new().unwrap();
        let base = temp.path();
        std::fs::create_dir_all(base.join("src")).unwrap();
        std::fs::write(base.join("src/lib.rs"), "fn main() {}").unwrap();

        let files = glob_files(base, "src/**/*.rs").unwrap();
        assert!(files.iter().any(|p| p.ends_with("src/lib.rs")));
    }

    #[cfg(unix)]
    #[test]
    fn test_glob_files_permission_denied() {
        use std::os::unix::fs::PermissionsExt;

        struct PermGuard {
            path: PathBuf,
            mode: u32,
        }

        impl Drop for PermGuard {
            fn drop(&mut self) {
                let _ = std::fs::set_permissions(
                    &self.path,
                    std::fs::Permissions::from_mode(self.mode),
                );
            }
        }

        let temp = TempDir::new().unwrap();
        let base = temp.path();
        std::fs::create_dir_all(base.join("src")).unwrap();
        std::fs::write(base.join("src/main.rs"), "fn main() {}").unwrap();

        let secret_dir = base.join("secret");
        std::fs::create_dir_all(&secret_dir).unwrap();
        std::fs::write(secret_dir.join("secret.rs"), "fn main() {}").unwrap();

        let original_mode = std::fs::metadata(&secret_dir).unwrap().permissions().mode();
        let _guard = PermGuard {
            path: secret_dir.clone(),
            mode: original_mode,
        };
        std::fs::set_permissions(&secret_dir, std::fs::Permissions::from_mode(0o000)).unwrap();

        let files = glob_files(base, "**/*.rs").unwrap();
        assert!(files.iter().any(|p| p.ends_with("src/main.rs")));
    }
}
