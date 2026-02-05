//! Local Filesystem Registry

use super::{compute_sha256, ComponentPackage, Dependency};
use crate::v2::{Error, Result};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
#[derive(Debug, Clone)]
pub struct LocalRegistryConfig {
    pub registry_dir: PathBuf,
}

impl Default for LocalRegistryConfig {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        Self {
            registry_dir: PathBuf::from(home).join(".run").join("registry"),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageMetadata {
    pub name: String,

    pub version: String,

    pub description: String,

    pub sha256: String,

    #[serde(default)]
    pub dependencies: Vec<DependencySpec>,

    pub license: Option<String>,

    pub repository: Option<String>,

    pub wit: Option<String>,

    #[serde(default)]
    pub published_at: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencySpec {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub optional: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RegistryIndex {
    pub version: u32,

    pub packages: HashMap<String, Vec<String>>,
}
pub struct LocalRegistry {
    config: LocalRegistryConfig,
    index: RegistryIndex,
}

impl LocalRegistry {
    pub fn new(config: LocalRegistryConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.registry_dir)?;
        std::fs::create_dir_all(config.registry_dir.join("packages"))?;

        let mut registry = Self {
            config,
            index: RegistryIndex::default(),
        };

        registry.load_index()?;
        Ok(registry)
    }
    pub fn open_default() -> Result<Self> {
        Self::new(LocalRegistryConfig::default())
    }
    fn load_index(&mut self) -> Result<()> {
        let index_path = self.config.registry_dir.join("index.toml");

        if index_path.exists() {
            let content = std::fs::read_to_string(&index_path)?;
            self.index = toml::from_str(&content)
                .map_err(|e| Error::other(format!("Failed to parse registry index: {}", e)))?;
        } else {
            self.index = RegistryIndex {
                version: 1,
                packages: HashMap::new(),
            };
            self.save_index()?;
        }

        Ok(())
    }
    fn save_index(&self) -> Result<()> {
        let index_path = self.config.registry_dir.join("index.toml");
        let content = toml::to_string_pretty(&self.index)
            .map_err(|e| Error::other(format!("Failed to serialize registry index: {}", e)))?;
        std::fs::write(&index_path, content)?;
        Ok(())
    }
    fn package_dir(&self, name: &str) -> PathBuf {
        let safe_name = safe_package_name(name);
        self.config.registry_dir.join("packages").join(safe_name)
    }
    fn version_dir(&self, name: &str, version: &Version) -> PathBuf {
        self.package_dir(name).join(version.to_string())
    }
    pub fn get_versions(&self, name: &str) -> Result<Vec<Version>> {
        match self.index.packages.get(name) {
            Some(versions) => {
                let mut parsed: Vec<Version> = versions
                    .iter()
                    .filter_map(|v| Version::parse(v).ok())
                    .collect();
                parsed.sort();
                Ok(parsed)
            }
            None => Ok(vec![]),
        }
    }
    pub fn get_latest_version(&self, name: &str) -> Result<Version> {
        let versions = self.get_versions(name)?;
        versions
            .into_iter()
            .max()
            .ok_or_else(|| Error::PackageNotFound {
                name: name.to_string(),
                version: "*".to_string(),
            })
    }
    pub fn get_metadata(&self, name: &str, version: &Version) -> Result<PackageMetadata> {
        let metadata_path = self.version_dir(name, version).join("metadata.toml");

        if !metadata_path.exists() {
            return Err(Error::PackageNotFound {
                name: name.to_string(),
                version: version.to_string(),
            });
        }

        let content = std::fs::read_to_string(&metadata_path)?;
        toml::from_str(&content)
            .map_err(|e| Error::other(format!("Failed to parse package metadata: {}", e)))
    }
    pub fn get_info(&self, name: &str, version: &Version) -> Result<ComponentPackage> {
        let metadata = self.get_metadata(name, version)?;
        let version_dir = self.version_dir(name, version);

        Ok(ComponentPackage {
            name: metadata.name,
            version: Version::parse(&metadata.version)
                .map_err(|e| Error::other(format!("Invalid version: {}", e)))?,
            description: metadata.description,
            sha256: metadata.sha256,
            download_url: format!("file://{}", version_dir.join("component.wasm").display()),
            wit_url: metadata
                .wit
                .map(|w| format!("file://{}", version_dir.join(w).display())),
            dependencies: metadata
                .dependencies
                .iter()
                .map(|d| Dependency {
                    name: d.name.clone(),
                    version_req: VersionReq::parse(&d.version).unwrap_or(VersionReq::STAR),
                    optional: d.optional,
                    features: vec![],
                })
                .collect(),
            targets: vec!["wasm32-wasip2".to_string()],
            license: metadata.license,
            repository: metadata.repository,
            size: std::fs::metadata(version_dir.join("component.wasm"))
                .map(|m| m.len() as usize)
                .unwrap_or(0),
            published_at: metadata.published_at,
        })
    }
    pub fn get_component(&self, name: &str, version: &Version) -> Result<Vec<u8>> {
        let component_path = self.version_dir(name, version).join("component.wasm");

        if !component_path.exists() {
            return Err(Error::PackageNotFound {
                name: name.to_string(),
                version: version.to_string(),
            });
        }

        Ok(std::fs::read(&component_path)?)
    }
    pub fn get_component_verified(&self, name: &str, version: &Version) -> Result<Vec<u8>> {
        let metadata = self.get_metadata(name, version)?;
        let bytes = self.get_component(name, version)?;

        let actual_hash = compute_sha256(&bytes);
        if actual_hash != metadata.sha256 {
            return Err(Error::HashMismatch {
                package: name.to_string(),
                expected: metadata.sha256,
                actual: actual_hash,
            });
        }

        Ok(bytes)
    }
    pub fn publish(
        &mut self,
        name: &str,
        version: &Version,
        component_bytes: &[u8],
        metadata: PackageMetadata,
    ) -> Result<()> {
        let version_dir = self.version_dir(name, version);
        std::fs::create_dir_all(&version_dir)?;

        let component_path = version_dir.join("component.wasm");
        std::fs::write(&component_path, component_bytes)?;

        let actual_hash = compute_sha256(component_bytes);
        let mut metadata = metadata;
        metadata.sha256 = actual_hash;
        metadata.name = name.to_string();
        metadata.version = version.to_string();
        metadata.published_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let metadata_content = toml::to_string_pretty(&metadata)
            .map_err(|e| Error::other(format!("Failed to serialize metadata: {}", e)))?;
        std::fs::write(version_dir.join("metadata.toml"), metadata_content)?;

        self.index
            .packages
            .entry(name.to_string())
            .or_default()
            .push(version.to_string());
        self.save_index()?;

        Ok(())
    }
    pub fn import_from_file(
        &mut self,
        name: &str,
        version: &Version,
        component_path: &Path,
        description: &str,
    ) -> Result<()> {
        let bytes = std::fs::read(component_path)?;

        let metadata = PackageMetadata {
            name: name.to_string(),
            version: version.to_string(),
            description: description.to_string(),
            sha256: String::new(), // Will be computed
            dependencies: vec![],
            license: None,
            repository: None,
            wit: None,
            published_at: 0,
        };

        self.publish(name, version, &bytes, metadata)
    }
    pub fn remove_version(&mut self, name: &str, version: &Version) -> Result<()> {
        let version_dir = self.version_dir(name, version);

        if version_dir.exists() {
            std::fs::remove_dir_all(&version_dir)?;
        }

        if let Some(versions) = self.index.packages.get_mut(name) {
            versions.retain(|v| v != &version.to_string());
            if versions.is_empty() {
                self.index.packages.remove(name);
            }
        }
        self.save_index()?;

        Ok(())
    }
    pub fn remove_package(&mut self, name: &str) -> Result<()> {
        let package_dir = self.package_dir(name);

        if package_dir.exists() {
            std::fs::remove_dir_all(&package_dir)?;
        }

        self.index.packages.remove(name);
        self.save_index()?;

        Ok(())
    }
    pub fn list_packages(&self) -> Vec<String> {
        self.index.packages.keys().cloned().collect()
    }
    pub fn search(&self, query: &str) -> Vec<(String, Version)> {
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        for (name, versions) in &self.index.packages {
            if name.to_lowercase().contains(&query_lower) {
                if let Some(latest) = versions.iter().filter_map(|v| Version::parse(v).ok()).max() {
                    results.push((name.clone(), latest));
                }
            }
        }

        results.sort_by(|a, b| a.0.cmp(&b.0));
        results
    }
    pub fn verify_all(&self) -> Vec<(String, Version, String)> {
        let mut errors = Vec::new();

        for (name, versions) in &self.index.packages {
            for version_str in versions {
                if let Ok(version) = Version::parse(version_str) {
                    if let Err(e) = self.get_component_verified(name, &version) {
                        errors.push((name.clone(), version, e.to_string()));
                    }
                }
            }
        }

        errors
    }
    pub fn stats(&self) -> RegistryStats {
        let package_count = self.index.packages.len();
        let version_count: usize = self.index.packages.values().map(|v| v.len()).sum();

        let total_size = self
            .index
            .packages
            .iter()
            .flat_map(|(name, versions)| {
                versions.iter().filter_map(|v| {
                    Version::parse(v).ok().and_then(|ver| {
                        let path = self.version_dir(name, &ver).join("component.wasm");
                        std::fs::metadata(&path).ok().map(|m| m.len() as usize)
                    })
                })
            })
            .sum();

        RegistryStats {
            package_count,
            version_count,
            total_size,
        }
    }
}
#[derive(Debug, Clone)]
pub struct RegistryStats {
    pub package_count: usize,
    pub version_count: usize,
    pub total_size: usize,
}
fn safe_package_name(name: &str) -> String {
    name.replace(':', "__")
        .replace('/', "_")
        .replace('\\', "_")
        .replace('@', "_at_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_safe_package_name() {
        assert_eq!(safe_package_name("wasi:http"), "wasi__http");
        assert_eq!(safe_package_name("my/package"), "my_package");
        assert_eq!(safe_package_name("pkg@1.0"), "pkg_at_1.0");
    }

    #[test]
    fn test_local_registry_create() {
        let dir = tempdir().unwrap();
        let config = LocalRegistryConfig {
            registry_dir: dir.path().to_path_buf(),
        };

        let registry = LocalRegistry::new(config).unwrap();
        assert!(registry.list_packages().is_empty());
    }

    #[test]
    fn test_local_registry_publish_and_get() {
        let dir = tempdir().unwrap();
        let config = LocalRegistryConfig {
            registry_dir: dir.path().to_path_buf(),
        };

        let mut registry = LocalRegistry::new(config).unwrap();

        let wasm_bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

        let metadata = PackageMetadata {
            name: "test-component".to_string(),
            version: "1.0.0".to_string(),
            description: "Test component".to_string(),
            sha256: String::new(),
            dependencies: vec![],
            license: Some("MIT".to_string()),
            repository: None,
            wit: None,
            published_at: 0,
        };

        registry
            .publish(
                "test-component",
                &Version::new(1, 0, 0),
                &wasm_bytes,
                metadata,
            )
            .unwrap();

        let packages = registry.list_packages();
        assert!(packages.contains(&"test-component".to_string()));

        let versions = registry.get_versions("test-component").unwrap();
        assert_eq!(versions, vec![Version::new(1, 0, 0)]);

        let retrieved = registry
            .get_component_verified("test-component", &Version::new(1, 0, 0))
            .unwrap();
        assert_eq!(retrieved, wasm_bytes);
    }

    #[test]
    fn test_local_registry_hash_verification() {
        let dir = tempdir().unwrap();
        let config = LocalRegistryConfig {
            registry_dir: dir.path().to_path_buf(),
        };

        let mut registry = LocalRegistry::new(config).unwrap();

        let wasm_bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

        let metadata = PackageMetadata {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            description: "Test".to_string(),
            sha256: String::new(),
            dependencies: vec![],
            license: None,
            repository: None,
            wit: None,
            published_at: 0,
        };

        registry
            .publish("test", &Version::new(1, 0, 0), &wasm_bytes, metadata)
            .unwrap();

        let component_path = registry
            .version_dir("test", &Version::new(1, 0, 0))
            .join("component.wasm");
        std::fs::write(&component_path, b"tampered").unwrap();

        let result = registry.get_component_verified("test", &Version::new(1, 0, 0));
        assert!(result.is_err());
    }
}
