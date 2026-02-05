use std::collections::HashSet;
use std::path::PathBuf;

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Capability {
    FileRead(PathBuf),
    FileWrite(PathBuf),
    DirCreate(PathBuf),
    DirRead(PathBuf),
    NetConnect { host: String, port: u16 },
    NetListen { port: u16 },
    DnsResolve,
    EnvRead(String),
    EnvReadAll,
    Args,
    Cwd,
    Clock,
    Random,
    Stdin,
    Stdout,
    Stderr,
    ComponentCall { component: String, function: String },
    ComponentCallAny { component: String },
    Exit,
    Subprocess,
    Unrestricted,
}

impl Capability {
    pub fn allows(&self, other: &Capability) -> bool {
        if self == other {
            return true;
        }
        match (self, other) {
            (Capability::Unrestricted, _) => true,
            (Capability::DirRead(parent), Capability::FileRead(child)) => child.starts_with(parent),
            (Capability::FileWrite(parent), Capability::FileWrite(child)) => {
                child.starts_with(parent)
            }
            (
                Capability::ComponentCallAny { component: c1 },
                Capability::ComponentCall { component: c2, .. },
            ) => c1 == c2,
            (Capability::EnvReadAll, Capability::EnvRead(_)) => true,
            _ => false,
        }
    }

    pub fn description(&self) -> String {
        match self {
            Capability::FileRead(p) => format!("read {}", p.display()),
            Capability::FileWrite(p) => format!("write {}", p.display()),
            Capability::DirCreate(p) => format!("mkdir {}", p.display()),
            Capability::DirRead(p) => format!("ls {}", p.display()),
            Capability::NetConnect { host, port } => format!("connect {}:{}", host, port),
            Capability::NetListen { port } => format!("listen {}", port),
            Capability::DnsResolve => "dns".to_string(),
            Capability::EnvRead(var) => format!("env {}", var),
            Capability::EnvReadAll => "env *".to_string(),
            Capability::Args => "args".to_string(),
            Capability::Cwd => "cwd".to_string(),
            Capability::Clock => "clock".to_string(),
            Capability::Random => "random".to_string(),
            Capability::Stdin => "stdin".to_string(),
            Capability::Stdout => "stdout".to_string(),
            Capability::Stderr => "stderr".to_string(),
            Capability::ComponentCall {
                component,
                function,
            } => format!("{}::{}", component, function),
            Capability::ComponentCallAny { component } => format!("{}::*", component),
            Capability::Exit => "exit".to_string(),
            Capability::Subprocess => "subprocess".to_string(),
            Capability::Unrestricted => "unrestricted".to_string(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CapabilitySet {
    capabilities: HashSet<Capability>,
}

impl CapabilitySet {
    pub fn new() -> Self {
        Self {
            capabilities: HashSet::new(),
        }
    }

    pub fn deterministic() -> Self {
        let mut set = Self::new();
        set.grant(Capability::Stdout);
        set.grant(Capability::Stderr);
        set
    }

    pub fn cli_default() -> Self {
        let mut set = Self::deterministic();
        set.grant(Capability::Stdin);
        set.grant(Capability::Args);
        set.grant(Capability::Exit);
        set
    }

    pub fn dev_default() -> Self {
        let mut set = Self::cli_default();
        set.grant(Capability::Cwd);
        set.grant(Capability::Clock);
        set
    }

    pub fn service_default() -> Self {
        Self::deterministic()
    }

    pub fn unrestricted() -> Self {
        let mut set = Self::new();
        set.grant(Capability::Unrestricted);
        set
    }

    pub fn grant(&mut self, cap: Capability) {
        self.capabilities.insert(cap);
    }
    pub fn revoke(&mut self, cap: &Capability) {
        self.capabilities.remove(cap);
    }
    pub fn has(&self, cap: &Capability) -> bool {
        self.capabilities.iter().any(|c| c.allows(cap))
    }

    pub fn check(&self, cap: &Capability) -> Result<(), CapabilityError> {
        if self.has(cap) {
            Ok(())
        } else {
            Err(CapabilityError::Denied(cap.clone()))
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Capability> {
        self.capabilities.iter()
    }

    pub fn merge(&mut self, other: &CapabilitySet) {
        for cap in &other.capabilities {
            self.capabilities.insert(cap.clone());
        }
    }

    pub fn intersect(&self, other: &CapabilitySet) -> CapabilitySet {
        let mut result = CapabilitySet::new();
        for cap in &self.capabilities {
            if other.has(cap) {
                result.grant(cap.clone());
            }
        }
        result
    }
}

#[derive(Debug)]
pub enum CapabilityError {
    Denied(Capability),
    InvalidCapability(String),
}

impl std::fmt::Display for CapabilityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CapabilityError::Denied(cap) => write!(f, "denied: {}", cap.description()),
            CapabilityError::InvalidCapability(msg) => write!(f, "invalid: {}", msg),
        }
    }
}

impl std::error::Error for CapabilityError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyMode {
    Production,
    Development,
}

#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct SecurityPolicy {
    pub mode: PolicyMode,
    pub cli_default: CapabilitySet,
    pub service_default: CapabilitySet,
    pub max_memory: usize,
    pub max_execution_time_ms: u64,
    pub max_fuel: u64,
    pub allow_unrestricted: bool,
    pub allowed_hosts: Vec<String>,
    pub blocked_hosts: Vec<String>,
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self::production()
    }
}

impl SecurityPolicy {
    pub fn production() -> Self {
        Self {
            mode: PolicyMode::Production,
            cli_default: CapabilitySet::deterministic(),
            service_default: CapabilitySet::deterministic(),
            max_memory: 256 * 1024 * 1024,
            max_execution_time_ms: 30_000,
            max_fuel: 10_000_000_000,
            allow_unrestricted: false,
            allowed_hosts: vec![],
            blocked_hosts: vec![],
        }
    }

    pub fn development() -> Self {
        Self {
            mode: PolicyMode::Development,
            cli_default: CapabilitySet::dev_default(),
            service_default: CapabilitySet::dev_default(),
            max_memory: 512 * 1024 * 1024,
            max_execution_time_ms: 60_000,
            max_fuel: 0,
            allow_unrestricted: true,
            allowed_hosts: vec!["*".to_string()],
            blocked_hosts: vec![],
        }
    }

    pub fn strict() -> Self {
        Self::production()
    }

    pub fn is_host_allowed(&self, host: &str) -> bool {
        if self.blocked_hosts.iter().any(|h| host_matches(h, host)) {
            return false;
        }
        self.allowed_hosts.iter().any(|h| host_matches(h, host))
    }

    pub fn is_dev(&self) -> bool {
        self.mode == PolicyMode::Development
    }
}

fn host_matches(pattern: &str, host: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if pattern.starts_with("*.") {
        return host.ends_with(&pattern[1..]);
    }
    pattern == host
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_allows() {
        let file_read = Capability::FileRead(PathBuf::from("/data/file.txt"));
        assert!(file_read.allows(&file_read));
        let dir_read = Capability::DirRead(PathBuf::from("/data"));
        assert!(dir_read.allows(&Capability::FileRead(PathBuf::from("/data/file.txt"))));
        assert!(Capability::Unrestricted.allows(&file_read));
    }

    #[test]
    fn test_capability_set() {
        let caps = CapabilitySet::cli_default();
        assert!(caps.has(&Capability::Stdout));
        assert!(!caps.has(&Capability::NetConnect {
            host: "localhost".to_string(),
            port: 80
        }));
    }

    #[test]
    fn test_host_matching() {
        assert!(host_matches("*", "example.com"));
        assert!(host_matches("*.example.com", "api.example.com"));
        assert!(!host_matches("*.example.com", "example.com"));
    }
}
