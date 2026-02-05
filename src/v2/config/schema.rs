//! Configuration Schema Types
//!
//! Additional schema types and validation.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct WorldConfig {
    pub name: String,

    pub imports: Vec<String>,

    pub exports: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct NetCapability {
    pub action: String,

    pub host: Option<String>,

    pub port: u16,
}

#[derive(Debug, Clone)]
pub struct FsCapability {
    pub action: String,

    pub path: String,
}

pub fn parse_capability(cap: &str) -> Option<CapabilitySpec> {
    let parts: Vec<&str> = cap.split(':').collect();
    if parts.is_empty() {
        return None;
    }

    match parts[0] {
        "net" => {
            if parts.len() < 3 {
                return None;
            }
            Some(CapabilitySpec::Network(NetCapability {
                action: parts[1].to_string(),
                host: if parts.len() > 3 {
                    Some(parts[2].to_string())
                } else {
                    None
                },
                port: parts.last()?.parse().ok()?,
            }))
        }
        "fs" => {
            if parts.len() < 3 {
                return None;
            }
            Some(CapabilitySpec::Filesystem(FsCapability {
                action: parts[1].to_string(),
                path: parts[2..].join(":"), // Handle paths with colons
            }))
        }
        "env" => Some(CapabilitySpec::Environment(
            parts.get(1).unwrap_or(&"*").to_string(),
        )),
        "clock" => Some(CapabilitySpec::Clock),
        "random" => Some(CapabilitySpec::Random),
        "stdin" => Some(CapabilitySpec::Stdin),
        "stdout" => Some(CapabilitySpec::Stdout),
        "stderr" => Some(CapabilitySpec::Stderr),
        "all" => Some(CapabilitySpec::All),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub enum CapabilitySpec {
    Network(NetCapability),
    Filesystem(FsCapability),
    Environment(String), // Variable name or "*" for all
    Clock,
    Random,
    Stdin,
    Stdout,
    Stderr,
    All,
}

#[derive(Debug, Clone)]
pub struct DeployConfig {
    pub name: String,

    pub target_type: String,

    pub options: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct PluginConfig {
    pub path: Option<String>,

    pub package: Option<String>,

    pub version: Option<String>,

    pub enabled: bool,

    pub hooks: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TestCaseConfig {
    pub component: String,

    pub function: String,

    pub args: Vec<String>,

    pub expect: Option<String>,

    pub expect_exit: Option<i32>,

    pub expect_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ServiceDef {
    pub name: String,

    pub component: String,

    pub port: Option<u16>,

    pub depends_on: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_net_capability() {
        let spec = parse_capability("net:listen:8080").unwrap();
        match spec {
            CapabilitySpec::Network(net) => {
                assert_eq!(net.action, "listen");
                assert_eq!(net.port, 8080);
            }
            _ => panic!("Expected Network capability"),
        }
    }

    #[test]
    fn test_parse_fs_capability() {
        let spec = parse_capability("fs:read:/data/files").unwrap();
        match spec {
            CapabilitySpec::Filesystem(fs) => {
                assert_eq!(fs.action, "read");
                assert_eq!(fs.path, "/data/files");
            }
            _ => panic!("Expected Filesystem capability"),
        }
    }

    #[test]
    fn test_parse_env_capability() {
        let spec = parse_capability("env:API_KEY").unwrap();
        match spec {
            CapabilitySpec::Environment(var) => {
                assert_eq!(var, "API_KEY");
            }
            _ => panic!("Expected Environment capability"),
        }
    }
}
