//! Error Types

use std::path::PathBuf;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[non_exhaustive]
#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to initialize WASI runtime: {0}")]
    RuntimeInit(String),

    #[error("Component instantiation failed: {component} - {reason}")]
    ComponentInstantiation { component: String, reason: String },

    #[error("Component not found: {0}")]
    ComponentNotFound(String),

    #[error("Invalid component: {path} - {reason}")]
    InvalidComponent { path: PathBuf, reason: String },

    #[error("Component execution failed: {component} - {reason}")]
    ExecutionFailed { component: String, reason: String },

    #[error("Fuel exhausted: {component} used {used} of {limit} fuel units")]
    FuelExhausted {
        component: String,
        used: u64,
        limit: u64,
    },

    #[error("Timeout: {component} exceeded {limit_ms}ms limit")]
    Timeout { component: String, limit_ms: u64 },

    #[error("Memory exceeded: {component} used {used_bytes} of {limit_bytes} bytes")]
    MemoryExceeded {
        component: String,
        used_bytes: usize,
        limit_bytes: usize,
    },

    #[error("Stack overflow: {component} exceeded {limit} frames")]
    StackOverflow { component: String, limit: usize },

    #[error("Host call denied: {component} attempted {call} without capability")]
    HostCallDenied { component: String, call: String },

    #[error("Access denied: {component} cannot access {resource}")]
    AccessDenied { component: String, resource: String },

    #[error("Component panic: {component} - {message}")]
    ComponentPanic { component: String, message: String },

    #[error("WIT interface not found: {interface}")]
    WitInterfaceNotFound { interface: String },

    #[error("WIT type mismatch: expected {expected}, got {actual}")]
    WitTypeMismatch { expected: String, actual: String },

    #[error("WIT binding generation failed: {reason}")]
    WitBindingFailed { reason: String },

    #[error("Incompatible WIT interfaces: {from} cannot satisfy {to}")]
    WitIncompatible { from: String, to: String },

    #[error("Capability denied: {capability} for component {component}")]
    CapabilityDenied {
        capability: String,
        component: String,
    },

    #[error("Invalid capability grant: {0}")]
    InvalidCapability(String),

    #[error("Package not found: {name}@{version}")]
    PackageNotFound { name: String, version: String },

    #[error("Version resolution failed: {package} - {reason}")]
    VersionResolutionFailed { package: String, reason: String },

    #[error("Dependency cycle detected: {cycle}")]
    DependencyCycle { cycle: String },

    #[error("Hash mismatch for {package}: expected {expected}, got {actual}")]
    HashMismatch {
        package: String,
        expected: String,
        actual: String,
    },

    #[error("Registry unavailable: {url}")]
    RegistryUnavailable { url: String },

    #[error("Lockfile conflict: {reason}")]
    LockfileConflict { reason: String },

    #[error("Registry violation: {reason}")]
    RegistryViolation { reason: String },

    #[error("Version exists: {name}@{version} is immutable")]
    VersionExists { name: String, version: String },

    #[error("Invalid run.toml: {reason}")]
    InvalidConfig { reason: String },

    #[error("Missing required field: {field} in {file}")]
    MissingField { field: String, file: String },

    #[error("Component lifecycle error: {component} - {reason}")]
    LifecycleError { component: String, reason: String },

    #[error("Inter-component call failed: {caller} -> {callee}::{function} - {reason}")]
    InterComponentCallFailed {
        caller: String,
        callee: String,
        function: String,
        reason: String,
    },

    #[error("Docker fallback failed: {service} - {reason}")]
    DockerFallbackFailed { service: String, reason: String },

    #[error("Bridge connection failed: {reason}")]
    BridgeConnectionFailed { reason: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn other(msg: impl Into<String>) -> Self {
        Error::Other(msg.into())
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            Error::Io(_) | Error::Serialization(_) | Error::Other(_) => 1,
            Error::RuntimeInit(_) => 10,
            Error::ComponentInstantiation { .. } => 11,
            Error::ComponentNotFound(_) => 12,
            Error::InvalidComponent { .. } => 13,
            Error::ExecutionFailed { .. } => 14,
            Error::FuelExhausted { .. } => 20,
            Error::Timeout { .. } => 21,
            Error::MemoryExceeded { .. } => 22,
            Error::StackOverflow { .. } => 23,
            Error::HostCallDenied { .. } => 30,
            Error::AccessDenied { .. } => 31,
            Error::CapabilityDenied { .. } => 32,
            Error::InvalidCapability(_) => 33,
            Error::ComponentPanic { .. } => 40,
            Error::LifecycleError { .. } => 41,
            Error::InterComponentCallFailed { .. } => 42,
            Error::PackageNotFound { .. } => 50,
            Error::VersionResolutionFailed { .. } => 51,
            Error::DependencyCycle { .. } => 52,
            Error::HashMismatch { .. } => 53,
            Error::RegistryUnavailable { .. } => 54,
            Error::LockfileConflict { .. } => 55,
            Error::RegistryViolation { .. } => 56,
            Error::VersionExists { .. } => 57,
            Error::InvalidConfig { .. } => 60,
            Error::MissingField { .. } => 61,
            Error::WitInterfaceNotFound { .. } => 70,
            Error::WitTypeMismatch { .. } => 71,
            Error::WitBindingFailed { .. } => 72,
            Error::WitIncompatible { .. } => 73,
            Error::DockerFallbackFailed { .. } => 80,
            Error::BridgeConnectionFailed { .. } => 81,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Error::RuntimeInit(_) => "runtime_init",
            Error::ComponentInstantiation { .. } => "component_instantiation",
            Error::ComponentNotFound(_) => "component_not_found",
            Error::InvalidComponent { .. } => "invalid_component",
            Error::ExecutionFailed { .. } => "execution_failed",
            Error::FuelExhausted { .. } => "fuel_exhausted",
            Error::Timeout { .. } => "timeout",
            Error::MemoryExceeded { .. } => "memory_exceeded",
            Error::StackOverflow { .. } => "stack_overflow",
            Error::HostCallDenied { .. } => "host_call_denied",
            Error::AccessDenied { .. } => "access_denied",
            Error::ComponentPanic { .. } => "component_panic",
            Error::CapabilityDenied { .. } => "capability_denied",
            Error::InvalidCapability(_) => "invalid_capability",
            Error::PackageNotFound { .. } => "package_not_found",
            Error::VersionResolutionFailed { .. } => "version_resolution_failed",
            Error::DependencyCycle { .. } => "dependency_cycle",
            Error::HashMismatch { .. } => "hash_mismatch",
            Error::RegistryUnavailable { .. } => "registry_unavailable",
            Error::LockfileConflict { .. } => "lockfile_conflict",
            Error::RegistryViolation { .. } => "registry_violation",
            Error::VersionExists { .. } => "version_exists",
            Error::InvalidConfig { .. } => "invalid_config",
            Error::MissingField { .. } => "missing_field",
            Error::LifecycleError { .. } => "lifecycle_error",
            Error::InterComponentCallFailed { .. } => "inter_component_call_failed",
            Error::WitInterfaceNotFound { .. } => "wit_interface_not_found",
            Error::WitTypeMismatch { .. } => "wit_type_mismatch",
            Error::WitBindingFailed { .. } => "wit_binding_failed",
            Error::WitIncompatible { .. } => "wit_incompatible",
            Error::DockerFallbackFailed { .. } => "docker_fallback_failed",
            Error::BridgeConnectionFailed { .. } => "bridge_connection_failed",
            Error::Io(_) => "io_error",
            Error::Serialization(_) => "serialization_error",
            Error::Other(_) => "other",
        }
    }

    pub fn to_json(&self) -> String {
        format!(
            r#"{{"error":true,"kind":"{}","exit_code":{},"message":"{}"}}"#,
            self.kind(),
            self.exit_code(),
            self.to_string().replace('"', "\\\"")
        )
    }

    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Error::RegistryUnavailable { .. }
                | Error::DockerFallbackFailed { .. }
                | Error::BridgeConnectionFailed { .. }
        )
    }

    pub fn component_name(&self) -> Option<&str> {
        match self {
            Error::ComponentInstantiation { component, .. }
            | Error::ExecutionFailed { component, .. }
            | Error::CapabilityDenied { component, .. }
            | Error::LifecycleError { component, .. }
            | Error::FuelExhausted { component, .. }
            | Error::Timeout { component, .. }
            | Error::MemoryExceeded { component, .. }
            | Error::StackOverflow { component, .. }
            | Error::HostCallDenied { component, .. }
            | Error::AccessDenied { component, .. }
            | Error::ComponentPanic { component, .. } => Some(component),
            Error::ComponentNotFound(name) => Some(name),
            Error::InterComponentCallFailed { caller, .. } => Some(caller),
            _ => None,
        }
    }
}
