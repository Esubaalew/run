//! Docker Bridge (Hybrid Mode)
//!
//! Provides fallback to Docker for components that cannot be compiled to WASM.
//! This enables gradual migration and support for legacy dependencies.
//!
//! Key principle: Docker is OPTIONAL, never REQUIRED by default.

pub mod compose;
mod docker;
mod proxy;

pub use compose::{DockerCompose, migrate_compose_to_run};
pub use docker::{DockerConfig, DockerService};
pub use proxy::{BridgeProxy, ProxyConfig};

use crate::v2::{Error, Result};
use std::collections::HashMap;
use std::process::{Command, Stdio};
#[derive(Debug, Clone)]
pub struct BridgeConfig {
    pub enabled: bool,

    pub docker_socket: String,

    pub network: String,

    pub timeout_secs: u64,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            docker_socket: "/var/run/docker.sock".to_string(),
            network: "run-bridge".to_string(),
            timeout_secs: 30,
        }
    }
}
pub struct Bridge {
    config: BridgeConfig,

    services: HashMap<String, DockerService>,

    docker_available: bool,
}

impl Bridge {
    pub fn new(config: BridgeConfig) -> Result<Self> {
        let docker_available = check_docker_available();

        Ok(Self {
            config,
            services: HashMap::new(),
            docker_available,
        })
    }
    pub fn is_docker_available(&self) -> bool {
        self.docker_available
    }
    pub fn start_service(&mut self, name: &str, config: DockerConfig) -> Result<()> {
        if !self.config.enabled {
            return Err(Error::other("Docker bridge is disabled"));
        }

        if !self.docker_available {
            return Err(Error::DockerFallbackFailed {
                service: name.to_string(),
                reason: "Docker is not available".to_string(),
            });
        }

        let mut service = DockerService::new(name, config)?;
        service.start()?;

        self.services.insert(name.to_string(), service);
        Ok(())
    }
    pub fn stop_service(&mut self, name: &str) -> Result<()> {
        if let Some(mut service) = self.services.remove(name) {
            service.stop()?;
        }
        Ok(())
    }
    pub fn stop_all(&mut self) -> Result<()> {
        let names: Vec<String> = self.services.keys().cloned().collect();
        for name in names {
            self.stop_service(&name)?;
        }
        Ok(())
    }
    pub fn get_connection(&self, name: &str) -> Option<ConnectionInfo> {
        self.services.get(name).map(|s| s.connection_info())
    }
    pub fn list_services(&self) -> Vec<&str> {
        self.services.keys().map(|s| s.as_str()).collect()
    }
    pub fn needs_docker(dependency: &str) -> bool {
        let docker_only = [
            "postgres",
            "postgresql",
            "mysql",
            "mariadb",
            "mongodb",
            "redis",
            "kafka",
            "elasticsearch",
            "rabbitmq",
            "memcached",
        ];

        docker_only
            .iter()
            .any(|d| dependency.to_lowercase().contains(d))
    }
    pub fn status(&self) -> BridgeStatus {
        let mut services = Vec::new();

        for (name, service) in &self.services {
            services.push(ServiceStatus {
                name: name.clone(),
                running: service.is_running(),
                container_id: service.container_id().map(|s| s.to_string()),
                ports: service.ports(),
            });
        }

        BridgeStatus {
            docker_available: self.docker_available,
            services,
        }
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        let _ = self.stop_all();
    }
}
fn check_docker_available() -> bool {
    Command::new("docker")
        .arg("info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub host: String,

    pub ports: HashMap<u16, u16>,

    pub env: HashMap<String, String>,
}
#[derive(Debug, Clone)]
pub struct BridgeStatus {
    pub docker_available: bool,
    pub services: Vec<ServiceStatus>,
}
#[derive(Debug, Clone)]
pub struct ServiceStatus {
    pub name: String,
    pub running: bool,
    pub container_id: Option<String>,
    pub ports: HashMap<u16, u16>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_needs_docker() {
        assert!(Bridge::needs_docker("postgres:15"));
        assert!(Bridge::needs_docker("redis:7"));
        assert!(!Bridge::needs_docker("wasi:http"));
    }

    #[test]
    fn test_bridge_config() {
        let config = BridgeConfig::default();
        assert!(config.enabled);
    }
}
