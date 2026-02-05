//! Orchestrator
//!
//! Component lifecycle and inter-component calls.

mod lifecycle;
mod logging;
mod router;
#[cfg(test)]
mod tests;

pub use lifecycle::{ComponentStatus, HealthCheck, Orchestrator};
pub use logging::{LogAggregator, LogEntry, LogLevel};
pub use router::{CallRouter, RouteTarget};

use crate::v2::runtime::{ExecutionResult, InstanceHandle, RuntimeEngine};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    pub health_checks: bool,

    pub health_check_interval: Duration,

    pub restart_policy: RestartPolicy,

    pub max_restart_attempts: u32,

    pub log_buffer_size: usize,

    pub metrics_enabled: bool,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            health_checks: true,
            health_check_interval: Duration::from_secs(30),
            restart_policy: RestartPolicy::OnFailure,
            max_restart_attempts: 3,
            log_buffer_size: 10_000,
            metrics_enabled: true,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartPolicy {
    Never,

    OnFailure,

    Always,
}
#[derive(Debug)]
pub struct ManagedComponent {
    pub id: String,

    pub handle: Option<InstanceHandle>,

    pub status: ComponentStatus,

    pub restart_count: u32,

    pub last_health_check: Option<Instant>,

    pub healthy: bool,

    pub dependencies: Vec<String>,

    pub dependents: Vec<String>,
}
#[derive(Debug, Clone, Default)]
pub struct ComponentMetrics {
    pub call_count: u64,

    pub error_count: u64,

    pub total_time_ms: u64,

    pub avg_time_ms: f64,

    pub restart_count: u32,

    pub uptime_ms: u64,
}
#[derive(Debug, Clone, Default)]
pub struct OrchestratorMetrics {
    pub components_running: usize,
    pub components_stopped: usize,
    pub components_failed: usize,

    pub total_calls: u64,

    pub total_errors: u64,

    pub uptime_ms: u64,
}
#[derive(Debug, Clone)]
pub enum OrchestratorEvent {
    ComponentStarted {
        id: String,
    },

    ComponentStopped {
        id: String,
        exit_code: i32,
    },

    ComponentFailed {
        id: String,
        error: String,
    },

    ComponentRestarted {
        id: String,
        attempt: u32,
    },

    HealthCheckPassed {
        id: String,
    },

    HealthCheckFailed {
        id: String,
        reason: String,
    },

    ComponentCall {
        from: String,
        to: String,
        function: String,
    },
}
pub type EventCallback = Box<dyn Fn(&OrchestratorEvent) + Send + Sync>;

pub fn create_orchestrator(
    runtime: Arc<Mutex<RuntimeEngine>>,
    config: OrchestratorConfig,
) -> Orchestrator {
    Orchestrator::new(runtime, config)
}
