//! Deployment
//!
//! `run deploy` builds and packages components for local, edge, or registry targets.

mod edge;

pub use edge::{EdgeDeployment, EdgeProvider, deploy_edge, generate_edge_manifest};

use crate::v2::build::build_all;
use crate::v2::config::{DeployConfig, RunConfig};
use crate::v2::registry::{ComponentPackage, Dependency, RegistryClient, RegistryConfig};
use crate::v2::{Error, Result};
use semver::Version;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeployTarget {
    Local,
    Edge,
    Registry,
}

impl DeployTarget {
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "local" => Ok(Self::Local),
            "edge" => Ok(Self::Edge),
            "registry" => Ok(Self::Registry),
            other => Err(Error::other(format!("Unknown deploy target '{}'", other))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DeployOptions {
    pub project_dir: PathBuf,
    pub target: Option<String>,
    pub profile: Option<String>,
    pub output_dir: Option<PathBuf>,
    pub component: Option<String>,
    pub build: bool,
    pub registry_url: Option<String>,
    pub auth_token: Option<String>,
    pub provider: Option<String>,
}

#[derive(Debug, Serialize)]
struct DeployManifest {
    project: String,
    version: String,
    target: String,
    created_at: u64,
    components: Vec<DeployComponent>,
}

#[derive(Debug, Serialize)]
struct DeployComponent {
    name: String,
    path: String,
    sha256: String,
    size: usize,
    language: Option<String>,
}

pub async fn run_deploy(options: DeployOptions) -> Result<()> {
    let config_path = options.project_dir.join("run.toml");
    let config = RunConfig::load(&config_path)?;

    let deploy_profile = options
        .profile
        .as_ref()
        .and_then(|name| config.deploy.get(name))
        .or_else(|| config.deploy.values().next());

    let target = if let Some(ref t) = options.target {
        DeployTarget::from_str(t)?
    } else if let Some(profile) = deploy_profile {
        DeployTarget::from_str(&profile.target_type)?
    } else {
        DeployTarget::Local
    };

    if options.build {
        build_all(&config, &options.project_dir)?;
    }

    match target {
        DeployTarget::Local => {
            let output_dir = resolve_output_dir(&options, deploy_profile, &options.project_dir)?;
            package_local(
                &config,
                &options.project_dir,
                &output_dir,
                options.component.as_deref(),
                target,
            )?;
            println!("Deploy bundle created at {}", output_dir.display());
            Ok(())
        }
        DeployTarget::Edge => deploy_edge_target(&config, &options, deploy_profile).await,
        DeployTarget::Registry => publish_registry(&config, &options, deploy_profile).await,
    }
}

fn resolve_output_dir(
    options: &DeployOptions,
    profile: Option<&DeployConfig>,
    project_dir: &Path,
) -> Result<PathBuf> {
    if let Some(ref output) = options.output_dir {
        return Ok(output.clone());
    }
    if let Some(profile) = profile {
        if let Some(path) = profile.options.get("output_dir") {
            return Ok(project_dir.join(path));
        }
    }
    Ok(project_dir.join("dist").join("deploy"))
}

fn package_local(
    config: &RunConfig,
    project_dir: &Path,
    output_dir: &Path,
    component_filter: Option<&str>,
    target: DeployTarget,
) -> Result<()> {
    std::fs::create_dir_all(output_dir)?;
    let mut components = Vec::new();

    for (name, comp) in &config.components {
        if let Some(filter) = component_filter {
            if name != filter {
                continue;
            }
        }

        let wasm_path = resolve_component_path(config, project_dir, name)?;
        let bytes = std::fs::read(&wasm_path)?;
        let sha256 = crate::v2::registry::compute_sha256(&bytes);
        let size = bytes.len();
        let dest = output_dir.join(format!("{}.wasm", name));
        std::fs::copy(&wasm_path, &dest)?;

        components.push(DeployComponent {
            name: name.to_string(),
            path: dest.file_name().unwrap().to_string_lossy().to_string(),
            sha256,
            size,
            language: comp.language.clone(),
        });
    }

    let manifest = DeployManifest {
        project: config.project.name.clone(),
        version: config.project.version.clone(),
        target: match target {
            DeployTarget::Local => "local",
            DeployTarget::Edge => "edge",
            DeployTarget::Registry => "registry",
        }
        .to_string(),
        created_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        components,
    };

    let manifest_path = output_dir.join("deploy.json");
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| Error::other(format!("Failed to serialize manifest: {}", e)))?;
    std::fs::write(&manifest_path, manifest_json)?;

    let config_dest = output_dir.join("run.toml");
    std::fs::copy(project_dir.join("run.toml"), config_dest)?;

    Ok(())
}

async fn publish_registry(
    config: &RunConfig,
    options: &DeployOptions,
    profile: Option<&DeployConfig>,
) -> Result<()> {
    let target_url = options
        .registry_url
        .clone()
        .or_else(|| profile.and_then(|p| p.options.get("registry_url").cloned()))
        .unwrap_or_else(|| config.registry.url.clone());

    let auth_token = options
        .auth_token
        .clone()
        .or_else(|| config.registry.auth_token.clone())
        .or_else(|| profile.and_then(|p| p.options.get("auth_token").cloned()));

    let mut registry_config = RegistryConfig::with_url(&target_url);
    registry_config.mirrors = config.registry.mirrors.clone();
    registry_config.auth_token = auth_token;

    let client = RegistryClient::new(registry_config);

    for (name, _comp) in &config.components {
        if let Some(filter) = options.component.as_deref() {
            if name != filter {
                continue;
            }
        }

        let wasm_path = resolve_component_path(config, &options.project_dir, name)?;
        let bytes = std::fs::read(&wasm_path)?;
        let version = Version::parse(&config.project.version)
            .map_err(|e| Error::other(format!("Invalid project version: {}", e)))?;

        let mut dependencies = Vec::new();
        for (dep, dep_config) in &config.dependencies {
            let dep_req = Dependency::new(dep, &dep_config.version)?;
            dependencies.push(dep_req);
        }

        let package = ComponentPackage {
            name: format!("{}:{}", config.project.name, name),
            version: version.clone(),
            description: config.project.description.clone().unwrap_or_default(),
            sha256: crate::v2::registry::compute_sha256(&bytes),
            download_url: String::new(),
            wit_url: None,
            dependencies,
            targets: vec!["wasm32-wasip1".to_string()],
            license: config.project.license.clone(),
            repository: config.project.repository.clone(),
            size: bytes.len(),
            published_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };

        client.publish(&package, &bytes).await?;
        println!("Published {}@{}", package.name, package.version);
    }

    Ok(())
}

fn resolve_component_path(
    config: &RunConfig,
    project_dir: &Path,
    component: &str,
) -> Result<PathBuf> {
    let comp_config = config
        .components
        .get(component)
        .ok_or_else(|| Error::ComponentNotFound(component.to_string()))?;

    if let Some(ref path) = comp_config.path {
        return Ok(project_dir.join(path));
    }

    if let Some(ref source) = comp_config.source {
        let source_path = project_dir.join(source);
        if source_path
            .extension()
            .map(|e| e == "wasm")
            .unwrap_or(false)
        {
            return Ok(source_path);
        }
    }

    let output_dir = project_dir.join(&config.build.output_dir);
    Ok(output_dir.join(format!("{}.wasm", component)))
}

async fn deploy_edge_target(
    config: &RunConfig,
    options: &DeployOptions,
    profile: Option<&DeployConfig>,
) -> Result<()> {
    let provider_str = options
        .provider
        .as_ref()
        .or_else(|| profile.and_then(|p| p.options.get("provider")))
        .ok_or_else(|| Error::other("provider required for edge deployment"))?;

    let provider = EdgeProvider::from_str(provider_str)?;

    for (name, _comp) in &config.components {
        if let Some(filter) = options.component.as_deref() {
            if name != filter {
                continue;
            }
        }

        let wasm_path = resolve_component_path(config, &options.project_dir, name)?;
        let bytes = std::fs::read(&wasm_path)?;
        let sha256 = crate::v2::registry::compute_sha256(&bytes);

        let mut deploy_options = profile.map(|p| p.options.clone()).unwrap_or_default();

        // Override with CLI options if provided
        if let Some(ref url) = options.registry_url {
            deploy_options.insert("registry_url".to_string(), url.clone());
        }
        if let Some(ref token) = options.auth_token {
            deploy_options.insert("api_token".to_string(), token.clone());
        }

        let deployment = EdgeDeployment {
            provider,
            name: format!("{}-{}", config.project.name, name),
            component_path: wasm_path.display().to_string(),
            options: deploy_options,
        };

        let url = deploy_edge(deployment).await?;
        println!("Deployed {} to {}", name, url);

        // Save manifest
        let manifest = generate_edge_manifest(name, provider, &url, &wasm_path, &sha256);
        let manifest_path = options
            .project_dir
            .join(".run")
            .join("deploy")
            .join(format!("{}.json", name));
        std::fs::create_dir_all(manifest_path.parent().unwrap())?;
        let manifest_json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| Error::other(format!("Failed to serialize manifest: {}", e)))?;
        std::fs::write(&manifest_path, manifest_json)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_parse() {
        assert!(matches!(
            DeployTarget::from_str("local").unwrap(),
            DeployTarget::Local
        ));
        assert!(matches!(
            DeployTarget::from_str("edge").unwrap(),
            DeployTarget::Edge
        ));
        assert!(matches!(
            DeployTarget::from_str("registry").unwrap(),
            DeployTarget::Registry
        ));
    }
}
