//! Docker Compose Compatibility
//!
//! Parse docker-compose.yml and convert to Run + Docker hybrid mode

use crate::v2::bridge::{Bridge, DockerConfig};
use crate::v2::config::RunConfig;
use crate::v2::{Error, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct DockerCompose {
    pub version: Option<String>,
    pub services: HashMap<String, ComposeService>,
}

#[derive(Debug, Deserialize)]
pub struct ComposeService {
    pub image: Option<String>,
    pub build: Option<ComposeBuild>,
    pub ports: Option<Vec<String>>,
    pub environment: Option<HashMap<String, String>>,
    pub volumes: Option<Vec<String>>,
    pub command: Option<Vec<String>>,
    pub depends_on: Option<Vec<String>>,
    pub restart: Option<String>,
    pub mem_limit: Option<String>,
    pub cpus: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ComposeBuild {
    Simple(String),
    Complex {
        context: String,
        dockerfile: Option<String>,
    },
}

impl DockerCompose {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        serde_yaml::from_str(&content)
            .map_err(|e| Error::other(format!("Invalid docker-compose.yml: {}", e)))
    }

    pub fn to_run_config(&self, name: &str) -> RunConfig {
        let mut config = RunConfig::default();
        config.project.name = name.to_string();

        for (service_name, service) in &self.services {
            if is_wasm_compatible_service(service) {
                // Convert to Run component
                config.components.insert(
                    service_name.clone(),
                    crate::v2::config::ComponentConfig {
                        path: service
                            .build
                            .as_ref()
                            .map(|_| format!("target/wasm/{}.wasm", service_name)),
                        source: service.build.as_ref().and_then(|b| match b {
                            ComposeBuild::Simple(s) => Some(s.clone()),
                            ComposeBuild::Complex { context, .. } => Some(context.clone()),
                        }),
                        language: None,
                        build: None,
                        capabilities: extract_capabilities(service),
                        env: service.environment.clone().unwrap_or_default(),
                        dependencies: service.depends_on.clone().unwrap_or_default(),
                        health_check: None,
                        restart: service.restart.clone(),
                    },
                );
            } else if is_docker_only_service(service) {
                // Keep as Docker service
                config.dev.services.push(service_name.clone());

                if let Some(ref image) = service.image {
                    let (_image_name, _tag) = parse_image_tag(image);
                    config.docker.services.insert(
                        service_name.clone(),
                        crate::v2::config::DockerService {
                            url: format!(
                                "http://{}:{}",
                                service_name,
                                extract_first_port(service).unwrap_or(80)
                            ),
                            env_var: Some(format!("{}_URL", service_name.to_uppercase())),
                        },
                    );
                }
            }
        }

        config
    }

    pub fn start_hybrid(&self, bridge: &mut Bridge) -> Result<()> {
        for (service_name, service) in &self.services {
            if is_docker_only_service(service) {
                let config = service_to_docker_config(service)?;
                bridge.start_service(service_name, config)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ComposeAnalysis {
    pub total: usize,
    pub wasm_components: Vec<String>,
    pub docker_services: Vec<String>,
}

pub fn analyze_compose(compose_path: &Path) -> Result<ComposeAnalysis> {
    let compose = DockerCompose::load(compose_path)?;
    let mut wasm_components = Vec::new();
    let mut docker_services = Vec::new();

    for (service_name, service) in &compose.services {
        if is_wasm_compatible_service(service) {
            wasm_components.push(service_name.clone());
        } else if is_docker_only_service(service) {
            docker_services.push(service_name.clone());
        }
    }

    wasm_components.sort();
    docker_services.sort();

    Ok(ComposeAnalysis {
        total: compose.services.len(),
        wasm_components,
        docker_services,
    })
}

fn is_wasm_compatible_service(service: &ComposeService) -> bool {
    service.build.is_some() && service.image.is_none()
}

fn is_docker_only_service(service: &ComposeService) -> bool {
    if let Some(ref image) = service.image {
        let image_lower = image.to_lowercase();
        image_lower.contains("postgres")
            || image_lower.contains("redis")
            || image_lower.contains("mysql")
            || image_lower.contains("mongo")
            || image_lower.contains("kafka")
            || image_lower.contains("elastic")
    } else {
        false
    }
}

fn service_to_docker_config(service: &ComposeService) -> Result<DockerConfig> {
    let (image, tag) = if let Some(ref img) = service.image {
        parse_image_tag(img)
    } else {
        return Err(Error::other("Service has no image"));
    };

    let mut ports = HashMap::new();
    if let Some(ref port_mappings) = service.ports {
        for mapping in port_mappings {
            if let Some((host, container)) = parse_port_mapping(mapping) {
                ports.insert(container, host);
            }
        }
    }

    let mut volumes = Vec::new();
    if let Some(ref vol_mappings) = service.volumes {
        for mapping in vol_mappings {
            if let Some((host, container)) = parse_volume_mapping(mapping) {
                volumes.push((host, container));
            }
        }
    }

    Ok(DockerConfig {
        image,
        tag,
        env: service.environment.clone().unwrap_or_default(),
        ports,
        volumes,
        command: service.command.clone(),
        restart: service.restart.clone().unwrap_or_else(|| "no".to_string()),
        memory: service.mem_limit.clone(),
        cpus: service.cpus,
    })
}

fn parse_image_tag(image: &str) -> (String, String) {
    if let Some(colon_pos) = image.rfind(':') {
        let name = image[..colon_pos].to_string();
        let tag = image[colon_pos + 1..].to_string();
        (name, tag)
    } else {
        (image.to_string(), "latest".to_string())
    }
}

fn parse_port_mapping(mapping: &str) -> Option<(u16, u16)> {
    let parts: Vec<&str> = mapping.split(':').collect();
    if parts.len() >= 2 {
        let host = parts[0].parse().ok()?;
        let container = parts[1].parse().ok()?;
        Some((host, container))
    } else {
        let port = mapping.parse().ok()?;
        Some((port, port))
    }
}

fn parse_volume_mapping(mapping: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = mapping.split(':').collect();
    if parts.len() >= 2 {
        Some((parts[0].to_string(), parts[1].to_string()))
    } else {
        None
    }
}

fn extract_capabilities(service: &ComposeService) -> Vec<String> {
    let mut caps = vec!["stdout".to_string(), "stderr".to_string()];

    if let Some(ref ports) = service.ports {
        for port_str in ports {
            if let Some((_, port)) = parse_port_mapping(port_str) {
                caps.push(format!("net:listen:{}", port));
            }
        }
    }

    if service.environment.is_some() {
        caps.push("env:*".to_string());
    }

    caps
}

fn extract_first_port(service: &ComposeService) -> Option<u16> {
    service
        .ports
        .as_ref()?
        .first()
        .and_then(|s| parse_port_mapping(s))
        .map(|(_, port)| port)
}

pub fn migrate_compose_to_run(compose_path: &Path, output_path: &Path) -> Result<()> {
    let compose = DockerCompose::load(compose_path)?;
    let project_name = compose_path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("my-app");

    let run_config = compose.to_run_config(project_name);
    run_config.save(output_path)?;

    println!("Migrated docker-compose.yml to {}", output_path.display());
    println!("\nWASI components:");
    for (name, _) in &run_config.components {
        println!("  - {}", name);
    }
    println!("\nDocker services:");
    for service in &run_config.dev.services {
        println!("  - {}", service);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_image_tag() {
        let (image, tag) = parse_image_tag("postgres:15");
        assert_eq!(image, "postgres");
        assert_eq!(tag, "15");

        let (image, tag) = parse_image_tag("redis");
        assert_eq!(image, "redis");
        assert_eq!(tag, "latest");
    }

    #[test]
    fn test_parse_port_mapping() {
        let (host, container) = parse_port_mapping("3000:8080").unwrap();
        assert_eq!(host, 3000);
        assert_eq!(container, 8080);

        let (host, container) = parse_port_mapping("5432").unwrap();
        assert_eq!(host, 5432);
        assert_eq!(container, 5432);
    }
}
