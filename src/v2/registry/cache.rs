//! Component Cache
//!
//! Local cache for downloaded components to enable offline use
//! and faster subsequent installs.

use super::Lockfile;
use crate::v2::{Error, Result};
use semver::{Version, VersionReq};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
#[derive(Debug, Clone)]
pub struct CacheConfig {
    pub cache_dir: PathBuf,

    pub max_size: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            cache_dir: PathBuf::from(".run/cache"),
            max_size: 1024 * 1024 * 1024, // 1 GB
        }
    }
}
pub struct ComponentCache {
    config: CacheConfig,

    index: HashMap<String, Vec<CachedEntry>>,
}

#[derive(Debug, Clone)]
struct CachedEntry {
    version: Version,
    path: PathBuf,
    size: usize,
    last_used: u64,
}

impl ComponentCache {
    pub fn new(config: CacheConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.cache_dir)?;

        let mut cache = Self {
            config,
            index: HashMap::new(),
        };

        cache.rebuild_index()?;
        Ok(cache)
    }
    fn rebuild_index(&mut self) -> Result<()> {
        self.index.clear();

        for entry in std::fs::read_dir(&self.config.cache_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().map(|e| e == "wasm").unwrap_or(false) {
                if let Some((name, version)) = parse_cache_filename(&path) {
                    let metadata = std::fs::metadata(&path)?;

                    let cached = CachedEntry {
                        version,
                        path: path.clone(),
                        size: metadata.len() as usize,
                        last_used: metadata
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs())
                            .unwrap_or(0),
                    };

                    self.index.entry(name).or_default().push(cached);
                }
            }
        }

        for entries in self.index.values_mut() {
            entries.sort_by(|a, b| b.version.cmp(&a.version));
        }

        Ok(())
    }
    pub fn get(&self, name: &str, version_req: &VersionReq) -> Result<Option<PathBuf>> {
        let entries = match self.index.get(name) {
            Some(e) => e,
            None => return Ok(None),
        };

        for entry in entries {
            if version_req.matches(&entry.version) {
                if entry.path.exists() {
                    return Ok(Some(entry.path.clone()));
                }
            }
        }

        Ok(None)
    }
    pub fn get_exact(&self, name: &str, version: &Version) -> Result<Option<PathBuf>> {
        let version_req = VersionReq::parse(&format!("={}", version))
            .map_err(|e| Error::other(format!("Invalid version: {}", e)))?;
        self.get(name, &version_req)
    }
    pub fn store(&mut self, name: &str, version: &Version, bytes: &[u8]) -> Result<PathBuf> {
        let current_size = self.total_size();
        if current_size + bytes.len() > self.config.max_size {
            self.evict(bytes.len())?;
        }

        let filename = format!("{}@{}.wasm", safe_filename(name), version);
        let path = self.config.cache_dir.join(&filename);

        std::fs::write(&path, bytes)?;

        let entry = CachedEntry {
            version: version.clone(),
            path: path.clone(),
            size: bytes.len(),
            last_used: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        self.index.entry(name.to_string()).or_default().push(entry);

        Ok(path)
    }
    pub fn remove(&mut self, name: &str) -> Result<()> {
        if let Some(entries) = self.index.remove(name) {
            for entry in entries {
                let _ = std::fs::remove_file(&entry.path);
            }
        }
        Ok(())
    }
    pub fn remove_version(&mut self, name: &str, version: &Version) -> Result<()> {
        if let Some(entries) = self.index.get_mut(name) {
            entries.retain(|e| {
                if e.version == *version {
                    let _ = std::fs::remove_file(&e.path);
                    false
                } else {
                    true
                }
            });
        }
        Ok(())
    }
    pub fn list_all(&self) -> Result<Vec<(String, Version)>> {
        let mut result = Vec::new();
        for (name, entries) in &self.index {
            for entry in entries {
                result.push((name.clone(), entry.version.clone()));
            }
        }
        result.sort();
        Ok(result)
    }
    pub fn total_size(&self) -> usize {
        self.index
            .values()
            .flat_map(|entries| entries.iter())
            .map(|e| e.size)
            .sum()
    }
    pub fn stats(&self) -> CacheStats {
        let component_count = self.index.len();
        let total_entries: usize = self.index.values().map(|e| e.len()).sum();
        let total_size = self.total_size();

        CacheStats {
            component_count,
            total_entries,
            total_size,
            max_size: self.config.max_size,
        }
    }
    fn evict(&mut self, needed: usize) -> Result<()> {
        let mut all_entries: Vec<(String, CachedEntry)> = self
            .index
            .iter()
            .flat_map(|(name, entries)| entries.iter().map(move |e| (name.clone(), e.clone())))
            .collect();

        all_entries.sort_by(|a, b| a.1.last_used.cmp(&b.1.last_used));

        let mut freed = 0;
        for (name, entry) in all_entries {
            if freed >= needed {
                break;
            }

            if std::fs::remove_file(&entry.path).is_ok() {
                freed += entry.size;

                if let Some(entries) = self.index.get_mut(&name) {
                    entries.retain(|e| e.version != entry.version);
                }
            }
        }

        Ok(())
    }
    pub fn clean_unused(&self, lockfile: Option<&Lockfile>) -> Result<usize> {
        let mut removed = 0;

        let locked_packages: std::collections::HashSet<_> = lockfile
            .map(|l| l.components().map(|c| c.name.clone()).collect())
            .unwrap_or_default();

        for (name, entries) in &self.index {
            if !locked_packages.contains(name) {
                for entry in entries {
                    if std::fs::remove_file(&entry.path).is_ok() {
                        removed += 1;
                    }
                }
            }
        }

        Ok(removed)
    }
    pub fn clear(&mut self) -> Result<()> {
        for entries in self.index.values() {
            for entry in entries {
                let _ = std::fs::remove_file(&entry.path);
            }
        }
        self.index.clear();
        Ok(())
    }
}
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub component_count: usize,

    pub total_entries: usize,

    pub total_size: usize,

    pub max_size: usize,
}

impl CacheStats {
    pub fn usage_percent(&self) -> f64 {
        (self.total_size as f64 / self.max_size as f64) * 100.0
    }
}
fn parse_cache_filename(path: &Path) -> Option<(String, Version)> {
    let stem = path.file_stem()?.to_str()?;
    let at_pos = stem.rfind('@')?;

    let name = &stem[..at_pos];
    let version_str = &stem[at_pos + 1..];

    let version = Version::parse(version_str).ok()?;
    Some((unsafe_filename(name), version))
}
fn safe_filename(name: &str) -> String {
    name.replace(':', "__").replace('/', "_").replace('\\', "_")
}
fn unsafe_filename(safe: &str) -> String {
    safe.replace("__", ":").replace('_', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_filename() {
        assert_eq!(safe_filename("wasi:http"), "wasi__http");
        assert_eq!(safe_filename("my/package"), "my_package");
    }

    #[test]
    fn test_unsafe_filename() {
        assert_eq!(unsafe_filename("wasi__http"), "wasi:http");
    }

    #[test]
    fn test_parse_cache_filename() {
        let path = PathBuf::from("/cache/wasi__http@0.2.0.wasm");
        let (name, version) = parse_cache_filename(&path).unwrap();
        assert_eq!(name, "wasi:http");
        assert_eq!(version, Version::new(0, 2, 0));
    }
}
