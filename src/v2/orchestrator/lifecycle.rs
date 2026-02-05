//! Lifecycle Management

use super::*;
use crate::v2::runtime::{
    Capability, CapabilitySet, ComponentValue, InstanceHandle, RuntimeEngine,
};
use crate::v2::{Error, Result};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentStatus {
    Pending,

    Starting,

    Running,

    Paused,

    Stopping,

    Stopped,

    Failed,

    Restarting,
}

impl std::fmt::Display for ComponentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComponentStatus::Pending => write!(f, "pending"),
            ComponentStatus::Starting => write!(f, "starting"),
            ComponentStatus::Running => write!(f, "running"),
            ComponentStatus::Paused => write!(f, "paused"),
            ComponentStatus::Stopping => write!(f, "stopping"),
            ComponentStatus::Stopped => write!(f, "stopped"),
            ComponentStatus::Failed => write!(f, "failed"),
            ComponentStatus::Restarting => write!(f, "restarting"),
        }
    }
}
#[derive(Debug, Clone)]
pub struct HealthCheck {
    pub function: String,

    pub expected: Option<ComponentValue>,

    pub timeout: Duration,

    pub interval: Duration,
}

impl Default for HealthCheck {
    fn default() -> Self {
        Self {
            function: "health".to_string(),
            expected: None,
            timeout: Duration::from_secs(5),
            interval: Duration::from_secs(30),
        }
    }
}
pub struct Orchestrator {
    runtime: Arc<Mutex<RuntimeEngine>>,

    config: OrchestratorConfig,

    components: RwLock<HashMap<String, ManagedComponent>>,

    metrics: RwLock<HashMap<String, ComponentMetrics>>,

    router: CallRouter,

    logs: LogAggregator,

    listeners: RwLock<Vec<EventCallback>>,

    start_time: Instant,
}

impl Orchestrator {
    pub fn new(runtime: Arc<Mutex<RuntimeEngine>>, config: OrchestratorConfig) -> Self {
        Self {
            runtime,
            config: config.clone(),
            components: RwLock::new(HashMap::new()),
            metrics: RwLock::new(HashMap::new()),
            router: CallRouter::new(),
            logs: LogAggregator::new(config.log_buffer_size),
            listeners: RwLock::new(Vec::new()),
            start_time: Instant::now(),
        }
    }
    pub fn register(&self, component_id: &str, dependencies: Vec<String>) -> Result<()> {
        let mut components = self.components.write().unwrap();

        if components.contains_key(component_id) {
            return Err(Error::other(format!(
                "Component '{}' already registered",
                component_id
            )));
        }

        for dep in &dependencies {
            if !components.contains_key(dep) {}
        }

        let managed = ManagedComponent {
            id: component_id.to_string(),
            handle: None,
            status: ComponentStatus::Pending,
            restart_count: 0,
            last_health_check: None,
            healthy: true,
            dependencies,
            dependents: Vec::new(),
        };

        components.insert(component_id.to_string(), managed);

        let mut metrics = self.metrics.write().unwrap();
        metrics.insert(component_id.to_string(), ComponentMetrics::default());

        Ok(())
    }
    pub fn start(&self, component_id: &str, capabilities: CapabilitySet) -> Result<()> {
        let dependencies = {
            let components = self.components.read().unwrap();
            let component = components
                .get(component_id)
                .ok_or_else(|| Error::ComponentNotFound(component_id.to_string()))?;
            component.dependencies.clone()
        };

        {
            let components = self.components.read().unwrap();
            for dep_id in &dependencies {
                if let Some(dep) = components.get(dep_id) {
                    if dep.status != ComponentStatus::Running {
                        return Err(Error::LifecycleError {
                            component: component_id.to_string(),
                            reason: format!("Dependency '{}' is not running", dep_id),
                        });
                    }
                }
            }
        }

        let handle = {
            let mut components = self.components.write().unwrap();
            let component = components
                .get_mut(component_id)
                .ok_or_else(|| Error::ComponentNotFound(component_id.to_string()))?;

            component.status = ComponentStatus::Starting;

            let runtime = self.runtime.lock().unwrap();
            let handle = runtime.instantiate(component_id, capabilities)?;

            component.handle = Some(handle.clone());
            component.status = ComponentStatus::Running;
            component.healthy = true;

            handle
        };

        self.router.register(component_id, handle);

        self.emit_event(OrchestratorEvent::ComponentStarted {
            id: component_id.to_string(),
        });

        self.logs
            .log(component_id, LogLevel::Info, "Component started");

        Ok(())
    }
    pub fn stop(&self, component_id: &str) -> Result<i32> {
        let dependents = {
            let components = self.components.read().unwrap();
            let component = components
                .get(component_id)
                .ok_or_else(|| Error::ComponentNotFound(component_id.to_string()))?;
            component.dependents.clone()
        };

        {
            let components = self.components.read().unwrap();
            for dep_id in &dependents {
                if let Some(dep) = components.get(dep_id) {
                    if dep.status == ComponentStatus::Running {
                        return Err(Error::LifecycleError {
                            component: component_id.to_string(),
                            reason: format!("Dependent '{}' is still running", dep_id),
                        });
                    }
                }
            }
        }

        let mut components = self.components.write().unwrap();
        let component = components
            .get_mut(component_id)
            .ok_or_else(|| Error::ComponentNotFound(component_id.to_string()))?;

        component.status = ComponentStatus::Stopping;

        let exit_code = if let Some(ref handle) = component.handle {
            let runtime = self.runtime.lock().unwrap();
            runtime.terminate(handle)?;
            0
        } else {
            0
        };

        component.status = ComponentStatus::Stopped;
        component.handle = None;

        self.router.unregister(component_id);

        self.emit_event(OrchestratorEvent::ComponentStopped {
            id: component_id.to_string(),
            exit_code,
        });

        self.logs
            .log(component_id, LogLevel::Info, "Component stopped");

        Ok(exit_code)
    }
    pub fn restart(&self, component_id: &str, capabilities: CapabilitySet) -> Result<()> {
        let restart_count = {
            let mut components = self.components.write().unwrap();
            let component = components
                .get_mut(component_id)
                .ok_or_else(|| Error::ComponentNotFound(component_id.to_string()))?;

            if component.restart_count >= self.config.max_restart_attempts {
                return Err(Error::LifecycleError {
                    component: component_id.to_string(),
                    reason: format!(
                        "Maximum restart attempts ({}) exceeded",
                        self.config.max_restart_attempts
                    ),
                });
            }

            component.status = ComponentStatus::Restarting;
            component.restart_count += 1;
            component.restart_count
        };

        let _ = self.stop(component_id);

        self.start(component_id, capabilities)?;

        self.emit_event(OrchestratorEvent::ComponentRestarted {
            id: component_id.to_string(),
            attempt: restart_count,
        });

        self.logs.log(
            component_id,
            LogLevel::Warn,
            &format!("Component restarted (attempt {})", restart_count),
        );

        Ok(())
    }
    pub fn call(
        &self,
        target_component: &str,
        function: &str,
        args: Vec<ComponentValue>,
    ) -> Result<ExecutionResult> {
        let handle = self
            .router
            .get_target(target_component)
            .ok_or_else(|| Error::ComponentNotFound(target_component.to_string()))?;

        let start = Instant::now();

        let runtime = self.runtime.lock().unwrap();
        let result = runtime.call(&handle, function, args);

        let duration_ms = start.elapsed().as_millis() as u64;
        if self.config.metrics_enabled {
            let mut metrics = self.metrics.write().unwrap();
            if let Some(m) = metrics.get_mut(target_component) {
                m.call_count += 1;
                m.total_time_ms += duration_ms;
                m.avg_time_ms = m.total_time_ms as f64 / m.call_count as f64;
                if result.is_err() {
                    m.error_count += 1;
                }
            }
        }

        result
    }
    pub fn inter_component_call(
        &self,
        source_component: &str,
        target_component: &str,
        function: &str,
        args: Vec<ComponentValue>,
    ) -> Result<ExecutionResult> {
        let source_handle = self
            .router
            .get_target(source_component)
            .ok_or_else(|| Error::ComponentNotFound(source_component.to_string()))?;
        let runtime = self.runtime.lock().unwrap();
        let source_instance = runtime
            .get_instance(&source_handle)
            .ok_or_else(|| Error::ComponentNotFound(source_component.to_string()))?;
        let specific = Capability::ComponentCall {
            component: target_component.to_string(),
            function: function.to_string(),
        };
        let any = Capability::ComponentCallAny {
            component: target_component.to_string(),
        };
        if !(source_instance.has_capability(&specific) || source_instance.has_capability(&any)) {
            return Err(Error::CapabilityDenied {
                capability: format!("component_call {}::{}", target_component, function),
                component: source_component.to_string(),
            });
        }

        self.emit_event(OrchestratorEvent::ComponentCall {
            from: source_component.to_string(),
            to: target_component.to_string(),
            function: function.to_string(),
        });

        self.call(target_component, function, args)
            .map_err(|e| Error::InterComponentCallFailed {
                caller: source_component.to_string(),
                callee: target_component.to_string(),
                function: function.to_string(),
                reason: e.to_string(),
            })
    }
    pub fn check_health(&self) {
        let to_check: Vec<(String, InstanceHandle)> = {
            let components = self.components.read().unwrap();
            components
                .iter()
                .filter(|(_, c)| c.status == ComponentStatus::Running)
                .filter_map(|(id, c)| c.handle.clone().map(|h| (id.clone(), h)))
                .collect()
        };

        for (id, handle) in to_check {
            let result = {
                let runtime = self.runtime.lock().unwrap();
                runtime.call(&handle, "health", vec![])
            };

            let healthy = result.is_ok();

            {
                let mut components = self.components.write().unwrap();
                if let Some(component) = components.get_mut(&id) {
                    component.last_health_check = Some(Instant::now());
                    component.healthy = healthy;
                }
            }

            if healthy {
                self.emit_event(OrchestratorEvent::HealthCheckPassed { id: id.clone() });
            } else {
                self.emit_event(OrchestratorEvent::HealthCheckFailed {
                    id: id.clone(),
                    reason: "Health check failed".to_string(),
                });
            }
        }
    }
    pub fn status(&self, component_id: &str) -> Option<ComponentStatus> {
        let components = self.components.read().unwrap();
        components.get(component_id).map(|c| c.status)
    }
    pub fn all_statuses(&self) -> HashMap<String, ComponentStatus> {
        let components = self.components.read().unwrap();
        components
            .iter()
            .map(|(k, v)| (k.clone(), v.status))
            .collect()
    }
    pub fn component_metrics(&self, component_id: &str) -> Option<ComponentMetrics> {
        let metrics = self.metrics.read().unwrap();
        metrics.get(component_id).cloned()
    }
    pub fn orchestrator_metrics(&self) -> OrchestratorMetrics {
        let components = self.components.read().unwrap();
        let metrics = self.metrics.read().unwrap();

        let mut result = OrchestratorMetrics {
            uptime_ms: self.start_time.elapsed().as_millis() as u64,
            ..Default::default()
        };

        for component in components.values() {
            match component.status {
                ComponentStatus::Running => result.components_running += 1,
                ComponentStatus::Stopped | ComponentStatus::Pending => {
                    result.components_stopped += 1
                }
                ComponentStatus::Failed => result.components_failed += 1,
                _ => {}
            }
        }

        for m in metrics.values() {
            result.total_calls += m.call_count;
            result.total_errors += m.error_count;
        }

        result
    }
    pub fn start_all(&self, capabilities: CapabilitySet) -> Result<()> {
        let order = self.compute_start_order()?;

        for component_id in order {
            self.start(&component_id, capabilities.clone())?;
        }

        Ok(())
    }
    pub fn stop_all(&self) -> Result<()> {
        let order = self.compute_start_order()?;

        for component_id in order.into_iter().rev() {
            let _ = self.stop(&component_id);
        }

        Ok(())
    }
    fn compute_start_order(&self) -> Result<Vec<String>> {
        let components = self.components.read().unwrap();

        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut dependents: HashMap<String, Vec<String>> = HashMap::new();

        for (id, component) in components.iter() {
            in_degree.entry(id.clone()).or_insert(0);
            for dep in &component.dependencies {
                *in_degree.entry(id.clone()).or_insert(0) += 1;
                dependents.entry(dep.clone()).or_default().push(id.clone());
            }
        }

        let mut queue: Vec<String> = in_degree
            .iter()
            .filter(|&(_, deg)| *deg == 0)
            .map(|(id, _)| id.clone())
            .collect();

        let mut result = Vec::new();

        while let Some(current) = queue.pop() {
            result.push(current.clone());

            if let Some(deps) = dependents.get(&current) {
                for dep in deps {
                    let deg = in_degree.get_mut(dep).unwrap();
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push(dep.clone());
                    }
                }
            }
        }

        if result.len() != components.len() {
            return Err(Error::DependencyCycle {
                cycle: "Circular dependency detected".to_string(),
            });
        }

        Ok(result)
    }
    pub fn on_event(&self, callback: EventCallback) {
        let mut listeners = self.listeners.write().unwrap();
        listeners.push(callback);
    }
    fn emit_event(&self, event: OrchestratorEvent) {
        let listeners = self.listeners.read().unwrap();
        for listener in listeners.iter() {
            listener(&event);
        }
    }
    pub fn get_logs(&self, component_id: &str, limit: usize) -> Vec<LogEntry> {
        self.logs.get_logs(component_id, limit)
    }
    pub fn get_all_logs(&self, limit: usize) -> Vec<LogEntry> {
        self.logs.get_all_logs(limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_status_display() {
        assert_eq!(ComponentStatus::Running.to_string(), "running");
        assert_eq!(ComponentStatus::Failed.to_string(), "failed");
    }
}
