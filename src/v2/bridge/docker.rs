//! Docker Bridge
//!
//! Connect to external Docker services.

use super::ConnectionInfo;
use crate::v2::{Error, Result};
use std::collections::HashMap;
use std::process::{Command, Stdio};
#[derive(Debug, Clone)]
pub struct DockerConfig {
    pub image: String,

    pub tag: String,

    pub env: HashMap<String, String>,

    pub ports: HashMap<u16, u16>,

    pub volumes: Vec<(String, String)>,

    pub command: Option<Vec<String>>,

    pub restart: String,

    pub memory: Option<String>,

    pub cpus: Option<f64>,
}

impl DockerConfig {
    pub fn postgres(password: &str) -> Self {
        let mut env = HashMap::new();
        env.insert("POSTGRES_PASSWORD".to_string(), password.to_string());

        let mut ports = HashMap::new();
        ports.insert(5432, 5432);

        Self {
            image: "postgres".to_string(),
            tag: "15".to_string(),
            env,
            ports,
            volumes: vec![],
            command: None,
            restart: "no".to_string(),
            memory: None,
            cpus: None,
        }
    }
    pub fn redis() -> Self {
        let mut ports = HashMap::new();
        ports.insert(6379, 6379);

        Self {
            image: "redis".to_string(),
            tag: "7".to_string(),
            env: HashMap::new(),
            ports,
            volumes: vec![],
            command: None,
            restart: "no".to_string(),
            memory: None,
            cpus: None,
        }
    }
    pub fn mysql(root_password: &str) -> Self {
        let mut env = HashMap::new();
        env.insert("MYSQL_ROOT_PASSWORD".to_string(), root_password.to_string());

        let mut ports = HashMap::new();
        ports.insert(3306, 3306);

        Self {
            image: "mysql".to_string(),
            tag: "8".to_string(),
            env,
            ports,
            volumes: vec![],
            command: None,
            restart: "no".to_string(),
            memory: None,
            cpus: None,
        }
    }
}

impl Default for DockerConfig {
    fn default() -> Self {
        Self {
            image: String::new(),
            tag: "latest".to_string(),
            env: HashMap::new(),
            ports: HashMap::new(),
            volumes: vec![],
            command: None,
            restart: "no".to_string(),
            memory: None,
            cpus: None,
        }
    }
}
pub struct DockerService {
    name: String,

    config: DockerConfig,

    container_id: Option<String>,

    running: bool,
}

impl DockerService {
    pub fn new(name: &str, config: DockerConfig) -> Result<Self> {
        Ok(Self {
            name: name.to_string(),
            config,
            container_id: None,
            running: false,
        })
    }
    pub fn start(&mut self) -> Result<()> {
        if self.running {
            return Ok(());
        }

        let container_name = format!("run-{}", self.name);
        let image = format!("{}:{}", self.config.image, self.config.tag);

        let mut cmd = Command::new("docker");
        cmd.arg("run")
            .arg("-d")
            .arg("--name")
            .arg(&container_name)
            .arg("--rm"); // Auto-remove when stopped

        for (key, value) in &self.config.env {
            cmd.arg("-e").arg(format!("{}={}", key, value));
        }

        for (container_port, host_port) in &self.config.ports {
            cmd.arg("-p")
                .arg(format!("{}:{}", host_port, container_port));
        }

        for (host_path, container_path) in &self.config.volumes {
            cmd.arg("-v")
                .arg(format!("{}:{}", host_path, container_path));
        }

        if let Some(ref memory) = self.config.memory {
            cmd.arg("--memory").arg(memory);
        }
        if let Some(cpus) = self.config.cpus {
            cmd.arg("--cpus").arg(cpus.to_string());
        }

        cmd.arg(&image);

        if let Some(ref command) = self.config.command {
            for arg in command {
                cmd.arg(arg);
            }
        }

        let output = cmd.output().map_err(|e| Error::DockerFallbackFailed {
            service: self.name.clone(),
            reason: format!("Failed to run docker: {}", e),
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::DockerFallbackFailed {
                service: self.name.clone(),
                reason: stderr.to_string(),
            });
        }

        let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();

        self.container_id = Some(container_id);
        self.running = true;

        self.wait_healthy()?;

        Ok(())
    }
    pub fn stop(&mut self) -> Result<()> {
        if !self.running {
            return Ok(());
        }

        if let Some(ref container_id) = self.container_id {
            let _ = Command::new("docker")
                .args(["stop", container_id])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }

        self.container_id = None;
        self.running = false;
        Ok(())
    }
    pub fn is_running(&self) -> bool {
        self.running
    }
    pub fn container_id(&self) -> Option<&str> {
        self.container_id.as_deref()
    }
    pub fn ports(&self) -> HashMap<u16, u16> {
        self.config.ports.clone()
    }
    pub fn connection_info(&self) -> ConnectionInfo {
        ConnectionInfo {
            host: "127.0.0.1".to_string(),
            ports: self.config.ports.clone(),
            env: self.config.env.clone(),
        }
    }
    fn wait_healthy(&self) -> Result<()> {
        let container_id = if let Some(ref id) = self.container_id {
            id.clone()
        } else {
            std::thread::sleep(std::time::Duration::from_secs(2));
            return Ok(());
        };

        let start = std::time::Instant::now();
        loop {
            if start.elapsed().as_secs() > 30 {
                return Err(Error::other("Docker health check timed out"));
            }

            let output = Command::new("docker")
                .args([
                    "inspect",
                    "--format",
                    "{{.State.Health.Status}}",
                    &container_id,
                ])
                .output();

            match output {
                Ok(out) if out.status.success() => {
                    let status = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if status.is_empty() || status == "<no value>" {
                        break;
                    }
                    if status == "healthy" {
                        break;
                    }
                }
                _ => {
                    break;
                }
            }

            std::thread::sleep(std::time::Duration::from_secs(1));
        }

        Ok(())
    }
    pub fn logs(&self, tail: usize) -> Result<String> {
        let container_id = self
            .container_id
            .as_ref()
            .ok_or_else(|| Error::other("Container not running"))?;

        let output = Command::new("docker")
            .args(["logs", "--tail", &tail.to_string(), container_id])
            .output()
            .map_err(|e| Error::other(format!("Failed to get logs: {}", e)))?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
    pub fn exec(&self, command: &[&str]) -> Result<String> {
        let container_id = self
            .container_id
            .as_ref()
            .ok_or_else(|| Error::other("Container not running"))?;

        let mut cmd = Command::new("docker");
        cmd.arg("exec").arg(container_id);
        for arg in command {
            cmd.arg(arg);
        }

        let output = cmd
            .output()
            .map_err(|e| Error::other(format!("Failed to exec: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::other(format!("Exec failed: {}", stderr)));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

impl Drop for DockerService {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_postgres_config() {
        let config = DockerConfig::postgres("secret");
        assert_eq!(config.image, "postgres");
        assert_eq!(config.tag, "15");
        assert!(config.env.contains_key("POSTGRES_PASSWORD"));
        assert!(config.ports.contains_key(&5432));
    }

    #[test]
    fn test_redis_config() {
        let config = DockerConfig::redis();
        assert_eq!(config.image, "redis");
        assert!(config.ports.contains_key(&6379));
    }
}
