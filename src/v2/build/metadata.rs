//! Build Metadata
//!
//! Metadata for built components, used for deterministic builds
//! and lockfile integration.

use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildMetadata {
    pub name: String,

    pub sha256: String,

    pub size: usize,

    pub language: String,

    #[serde(default)]
    pub exports: Vec<String>,

    #[serde(default)]
    pub imports: Vec<String>,

    pub description: Option<String>,

    pub built_at: u64,
}

impl BuildMetadata {
    pub fn load(path: &std::path::Path) -> crate::v2::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        toml::from_str(&content)
            .map_err(|e| crate::v2::Error::other(format!("Failed to parse metadata: {}", e)))
    }

    pub fn save(&self, path: &std::path::Path) -> crate::v2::Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| crate::v2::Error::other(format!("Failed to serialize metadata: {}", e)))?;
        std::fs::write(path, content)?;
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentManifest {
    pub package: PackageInfo,

    #[serde(default)]
    pub dependencies: std::collections::HashMap<String, DependencyInfo>,

    #[serde(default)]
    pub build: Option<BuildInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub name: String,

    pub version: String,

    pub description: Option<String>,

    pub license: Option<String>,

    #[serde(default)]
    pub authors: Vec<String>,

    pub repository: Option<String>,

    pub world: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyInfo {
    pub version: String,

    #[serde(default)]
    pub optional: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildInfo {
    pub language: String,

    pub entry: Option<String>,

    pub wit_dir: Option<String>,
}

impl ComponentManifest {
    pub fn load(path: &std::path::Path) -> crate::v2::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        toml::from_str(&content)
            .map_err(|e| crate::v2::Error::other(format!("Failed to parse manifest: {}", e)))
    }

    pub fn save(&self, path: &std::path::Path) -> crate::v2::Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| crate::v2::Error::other(format!("Failed to serialize manifest: {}", e)))?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_build_metadata_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.meta.toml");

        let metadata = BuildMetadata {
            name: "test-component".to_string(),
            sha256: "abc123".to_string(),
            size: 1024,
            language: "rust".to_string(),
            exports: vec!["run:calc/calculator@0.1.0".to_string()],
            imports: vec!["wasi:cli/environment@0.2.3".to_string()],
            description: Some("Test component".to_string()),
            built_at: 1234567890,
        };

        metadata.save(&path).unwrap();
        let loaded = BuildMetadata::load(&path).unwrap();

        assert_eq!(loaded.name, metadata.name);
        assert_eq!(loaded.sha256, metadata.sha256);
        assert_eq!(loaded.exports, metadata.exports);
    }

    #[test]
    fn test_component_manifest_parse() {
        let toml_str = r#"
[package]
name = "my-component"
version = "1.0.0"
description = "A test component"
world = "calculator-impl"

[dependencies]
"wasi:http" = { version = "^0.2.0" }

[build]
language = "rust"
"#;

        let manifest: ComponentManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.package.name, "my-component");
        assert_eq!(manifest.package.version, "1.0.0");
        assert!(manifest.dependencies.contains_key("wasi:http"));
    }
}
