use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const MAX_CACHE_BYTES: u64 = 500 * 1024 * 1024;
const MAX_CACHE_ENTRIES: usize = 200;

/// Summary information for the persistent build cache.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CacheStats {
    /// Number of indexed cache entries.
    pub entries: usize,
    /// Total bytes tracked by the cache index.
    pub total_bytes: u64,
    /// Entry count by language namespace.
    pub by_language: Vec<(String, usize)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheIndex {
    entries: HashMap<String, CacheIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheIndexEntry {
    lang: String,
    path: PathBuf,
    size: u64,
    atime: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheMeta {
    lang: String,
    source_hash: u64,
    toolchain: String,
    created_at: u64,
    size: u64,
}

/// Return the root directory for run-kit cache data.
pub fn root_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("run-kit")
}

/// Return cache statistics from the current index.
pub fn stats() -> Result<CacheStats> {
    let index = read_index()?;
    let mut by_lang: HashMap<String, usize> = HashMap::new();
    let mut total = 0;
    for entry in index.entries.values() {
        *by_lang.entry(entry.lang.clone()).or_insert(0) += 1;
        total += entry.size;
    }
    let mut by_language = by_lang.into_iter().collect::<Vec<_>>();
    by_language.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(CacheStats {
        entries: index.entries.len(),
        total_bytes: total,
        by_language,
    })
}

/// Clear every persistent build cache entry.
pub fn clear() -> Result<()> {
    let root = root_dir();
    if root.exists() {
        fs::remove_dir_all(&root)
            .with_context(|| format!("failed to remove {}", root.display()))?;
    }
    Ok(())
}

/// Clear entries belonging to a single language namespace.
pub fn clear_lang(lang: &str) -> Result<()> {
    let mut index = read_index()?;
    let ids = index
        .entries
        .iter()
        .filter_map(|(id, entry)| (entry.lang == lang).then_some(id.clone()))
        .collect::<Vec<_>>();
    for id in ids {
        if let Some(entry) = index.entries.remove(&id) {
            let entry_dir = entry.path.parent().unwrap_or(entry.path.as_path());
            let _ = fs::remove_dir_all(entry_dir);
        }
    }
    write_index(&index)
}

/// Look up a cached binary path for a language namespace and source hash.
pub fn lookup(namespace: &str, source_hash: u64) -> Option<PathBuf> {
    let toolchain = toolchain_fingerprint(namespace);
    let id = entry_id(namespace, source_hash, &toolchain);
    let path = entry_path(namespace, &id);
    if !path.exists() {
        return None;
    }

    if let Ok(mut index) = read_index() {
        let size = path.metadata().map(|m| m.len()).unwrap_or(0);
        index.entries.insert(
            id,
            CacheIndexEntry {
                lang: namespace.to_string(),
                path: path.clone(),
                size,
                atime: now_secs(),
            },
        );
        let _ = write_index(&index);
    }
    Some(path)
}

/// Store a compiled binary for a language namespace and source hash.
pub fn store(namespace: &str, source_hash: u64, binary: &Path) -> Option<PathBuf> {
    let toolchain = toolchain_fingerprint(namespace);
    let id = entry_id(namespace, source_hash, &toolchain);
    let path = entry_path(namespace, &id);
    let entry_dir = path.parent()?;
    fs::create_dir_all(entry_dir).ok()?;
    fs::copy(binary, &path).ok()?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o755));
    }

    let size = path.metadata().map(|m| m.len()).unwrap_or(0);
    let meta = CacheMeta {
        lang: namespace.to_string(),
        source_hash,
        toolchain,
        created_at: now_secs(),
        size,
    };
    let meta_path = entry_dir.join("meta.json");
    if let Ok(text) = serde_json::to_string_pretty(&meta) {
        let _ = fs::write(meta_path, text);
    }

    if let Ok(mut index) = read_index() {
        index.entries.insert(
            id,
            CacheIndexEntry {
                lang: namespace.to_string(),
                path: path.clone(),
                size,
                atime: now_secs(),
            },
        );
        let _ = evict_if_needed(&mut index);
        let _ = write_index(&index);
    }

    Some(path)
}

/// Return an incremental workspace path managed by the build cache.
pub fn workspace(namespace: &str, source_hash: u64) -> Result<PathBuf> {
    let toolchain = toolchain_fingerprint(namespace);
    let id = entry_id(namespace, source_hash, &toolchain);
    let dir = root_dir()
        .join("builds")
        .join(namespace)
        .join(id)
        .join("workspace");
    fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    Ok(dir)
}

fn entry_id(namespace: &str, source_hash: u64, toolchain: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"run-kit/v1");
    hasher.update(namespace.as_bytes());
    hasher.update(&source_hash.to_le_bytes());
    hasher.update(toolchain.as_bytes());
    let hash = hasher.finalize();
    hash.to_hex()[..16].to_string()
}

fn entry_path(namespace: &str, id: &str) -> PathBuf {
    let suffix = std::env::consts::EXE_SUFFIX;
    root_dir()
        .join("builds")
        .join(namespace)
        .join(id)
        .join(format!("bin{suffix}"))
}

fn index_path() -> PathBuf {
    root_dir().join("index.json")
}

fn read_index() -> Result<CacheIndex> {
    let path = index_path();
    if !path.exists() {
        return Ok(CacheIndex {
            entries: HashMap::new(),
        });
    }
    let text =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
}

fn write_index(index: &CacheIndex) -> Result<()> {
    let path = index_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(index)?;
    fs::write(&path, text).with_context(|| format!("failed to write {}", path.display()))
}

fn evict_if_needed(index: &mut CacheIndex) -> Result<()> {
    loop {
        let total = index.entries.values().map(|entry| entry.size).sum::<u64>();
        if total <= MAX_CACHE_BYTES && index.entries.len() <= MAX_CACHE_ENTRIES {
            break;
        }
        let Some((oldest_id, oldest)) = index
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.atime)
            .map(|(id, entry)| (id.clone(), entry.clone()))
        else {
            break;
        };
        index.entries.remove(&oldest_id);
        let entry_dir = oldest.path.parent().unwrap_or(oldest.path.as_path());
        let _ = fs::remove_dir_all(entry_dir);
    }
    Ok(())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn toolchain_fingerprint(namespace: &str) -> String {
    let lang = namespace.split('-').next().unwrap_or(namespace);
    let candidates: &[(&str, &[&str])] = match lang {
        "rust" => &[("rustc", &["--version"])],
        "go" => &[("go", &["version"])],
        "c" => &[
            ("cc", &["--version"]),
            ("clang", &["--version"]),
            ("gcc", &["--version"]),
        ],
        "cpp" => &[
            ("c++", &["--version"]),
            ("clang++", &["--version"]),
            ("g++", &["--version"]),
        ],
        "java" => &[("java", &["-version"])],
        "kotlin" => &[("kotlinc", &["-version"])],
        "zig" => &[("zig", &["version"])],
        _ => &[],
    };

    for (program, args) in candidates {
        let resolved = which::which(program).unwrap_or_else(|_| PathBuf::from(program));
        if let Ok(output) = std::process::Command::new(resolved).args(*args).output() {
            let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&output.stderr));
            let version = text.lines().next().unwrap_or("").trim();
            if !version.is_empty() {
                return version.to_string();
            }
        }
    }

    "unknown-toolchain".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_id_is_stable() {
        assert_eq!(
            entry_id("rust", 42, "rustc 1.0"),
            entry_id("rust", 42, "rustc 1.0")
        );
    }

    #[test]
    fn entry_id_changes_with_toolchain() {
        assert_ne!(
            entry_id("rust", 42, "rustc 1.0"),
            entry_id("rust", 42, "rustc 2.0")
        );
    }

    #[test]
    fn stats_empty_index_is_valid() {
        let index = CacheIndex {
            entries: HashMap::new(),
        };
        assert!(index.entries.is_empty());
    }
}
