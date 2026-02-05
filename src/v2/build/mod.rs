//! Build System

mod builder;
mod metadata;

pub use builder::{BuildConfig, BuildResult, ComponentBuilder, Language};
pub use metadata::{BuildMetadata, ComponentManifest};

use crate::v2::registry::{LocalRegistry, PackageMetadata};
use crate::v2::{Error, Result};
use std::path::{Path, PathBuf};

pub fn build_all(
    config: &crate::v2::config::RunConfig,
    base_dir: &Path,
) -> Result<Vec<BuildResult>> {
    let mut results = Vec::new();

    let build_config = BuildConfig::from_run_config(config, base_dir)?;
    let builder = ComponentBuilder::new(build_config);

    for (name, component) in &config.components {
        if component.build.is_none() && component.source.is_none() && component.path.is_none() {
            continue;
        }

        let source_path = if let Some(ref source) = component.source {
            base_dir.join(source)
        } else if let Some(ref path) = component.path {
            base_dir.join(path)
        } else {
            base_dir.join(name)
        };

        if let Some(ref build_cmd) = component.build {
            run_custom_build(build_cmd, base_dir)?;

            let output_source = if let Some(ref path) = component.path {
                base_dir.join(path)
            } else if source_path
                .extension()
                .map(|e| e == "wasm")
                .unwrap_or(false)
            {
                source_path.clone()
            } else {
                return Err(Error::other(format!(
                    "Custom build for '{}' requires a .wasm path in component.path",
                    name
                )));
            };

            let result = builder.build(&output_source, name, Language::Wasm, None)?;
            results.push(result);
            continue;
        }

        let lang = if let Some(ref lang) = component.language {
            Language::from_str(lang)?
        } else {
            detect_language(&source_path)?
        };

        let result = builder.build(&source_path, name, lang, None)?;
        results.push(result);
    }

    Ok(results)
}

fn detect_language(path: &Path) -> Result<Language> {
    if path.extension().map(|e| e == "wasm").unwrap_or(false) {
        Ok(Language::Wasm)
    } else if path.extension().map(|e| e == "js").unwrap_or(false) {
        Ok(Language::JavaScript)
    } else if path.extension().map(|e| e == "ts").unwrap_or(false) {
        Ok(Language::TypeScript)
    } else if path.extension().map(|e| e == "go").unwrap_or(false) {
        Ok(Language::Go)
    } else if path.extension().map(|e| e == "zig").unwrap_or(false) {
        Ok(Language::Zig)
    } else if path.extension().map(|e| e == "py").unwrap_or(false) {
        Ok(Language::Python)
    } else if path.join("Cargo.toml").exists() {
        Ok(Language::Rust)
    } else if path.join("pyproject.toml").exists()
        || path.join("setup.py").exists()
        || path.join("app.py").exists()
        || path.join("main.py").exists()
        || path.join("__init__.py").exists()
    {
        Ok(Language::Python)
    } else if path.join("package.json").exists() {
        if path.join("tsconfig.json").exists() {
            Ok(Language::TypeScript)
        } else {
            Ok(Language::JavaScript)
        }
    } else if path.join("go.mod").exists() || path.join("main.go").exists() {
        Ok(Language::Go)
    } else if path.join("build.zig").exists() {
        Ok(Language::Zig)
    } else {
        Err(Error::other(format!(
            "Cannot detect language for {}. No project markers found.",
            path.display()
        )))
    }
}

fn run_custom_build(command: &str, base_dir: &Path) -> Result<()> {
    let mut cmd = if cfg!(windows) {
        let mut cmd = std::process::Command::new("cmd");
        cmd.arg("/C").arg(command);
        cmd
    } else {
        let mut cmd = std::process::Command::new("sh");
        cmd.arg("-c").arg(command);
        cmd
    };

    let status = cmd
        .current_dir(base_dir)
        .status()
        .map_err(|e| Error::other(format!("Custom build failed: {}", e)))?;

    if !status.success() {
        return Err(Error::other(format!(
            "Custom build command failed: {}",
            command
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_detect_language_by_extension() {
        let dir = tempdir().unwrap();
        let wasm_path = dir.path().join("comp.wasm");
        std::fs::write(&wasm_path, b"").unwrap();
        assert_eq!(detect_language(&wasm_path).unwrap(), Language::Wasm);

        let js_path = dir.path().join("index.js");
        std::fs::write(&js_path, b"").unwrap();
        assert_eq!(detect_language(&js_path).unwrap(), Language::JavaScript);
    }

    #[test]
    fn test_detect_language_by_project_files() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("go.mod"), b"module test").unwrap();
        assert_eq!(detect_language(dir.path()).unwrap(), Language::Go);
    }
}

pub fn build_and_publish(
    path: &Path,
    name: &str,
    version: &semver::Version,
    lang: Language,
    world: Option<&str>,
    output_dir: &Path,
    registry: Option<&mut LocalRegistry>,
) -> Result<BuildResult> {
    let config = BuildConfig {
        output_dir: output_dir.to_path_buf(),
        release: true,
        wit_dir: find_wit_dir(path).ok(),
        reproducible: false,
        source_date_epoch: None,
    };

    let builder = ComponentBuilder::new(config);
    let result = builder.build(path, name, lang, world)?;

    if let Some(registry) = registry {
        let bytes = std::fs::read(&result.output_path)?;
        let metadata = PackageMetadata {
            name: name.to_string(),
            version: version.to_string(),
            description: result.metadata.description.clone().unwrap_or_default(),
            sha256: result.metadata.sha256.clone(),
            dependencies: vec![],
            license: None,
            repository: None,
            wit: None,
            published_at: 0,
        };
        registry.publish(name, version, &bytes, metadata)?;
    }

    Ok(result)
}

fn find_wit_dir(path: &Path) -> Result<PathBuf> {
    let candidates = [
        path.join("wit"),
        path.join("../wit"),
        path.join("../../wit"),
    ];

    for candidate in candidates {
        if candidate.exists() {
            return Ok(candidate.canonicalize().map_err(|e| Error::Io(e))?);
        }
    }

    Err(Error::other(format!(
        "WIT directory not found near {}",
        path.display()
    )))
}
