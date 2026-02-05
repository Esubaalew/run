//! Orchestrator Integration Tests
//!
//! Tests for multi-component orchestration, lifecycle management,
//! and inter-component communication.

#[cfg(test)]
mod integration_tests {
    use crate::v2::orchestrator::*;
    use crate::v2::runtime::{CapabilitySet, RuntimeConfig, RuntimeEngine};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    fn create_test_orchestrator() -> Orchestrator {
        let runtime_config = RuntimeConfig::development();
        let runtime = RuntimeEngine::new(runtime_config).unwrap();
        let runtime = Arc::new(Mutex::new(runtime));

        let config = OrchestratorConfig {
            health_checks: false, // Disable for tests
            health_check_interval: Duration::from_secs(60),
            restart_policy: RestartPolicy::OnFailure,
            max_restart_attempts: 3,
            log_buffer_size: 1000,
            metrics_enabled: true,
        };

        Orchestrator::new(runtime, config)
    }

    #[test]
    fn test_orchestrator_creation() {
        let orch = create_test_orchestrator();
        let metrics = orch.orchestrator_metrics();

        assert_eq!(metrics.components_running, 0);
        assert_eq!(metrics.components_stopped, 0);
        assert_eq!(metrics.total_calls, 0);
    }

    #[test]
    fn test_component_registration() {
        let orch = create_test_orchestrator();

        let result = orch.register("test-component", vec![]);
        assert!(result.is_ok());

        let status = orch.status("test-component");
        assert!(status.is_some());
    }

    #[test]
    fn test_multiple_component_registration() {
        let orch = create_test_orchestrator();

        orch.register("api", vec![]).unwrap();
        orch.register("worker", vec![]).unwrap();
        orch.register("cache", vec![]).unwrap();

        let statuses = orch.all_statuses();
        assert_eq!(statuses.len(), 3);
        assert!(statuses.contains_key("api"));
        assert!(statuses.contains_key("worker"));
        assert!(statuses.contains_key("cache"));
    }

    #[test]
    fn test_component_with_dependencies() {
        let orch = create_test_orchestrator();

        orch.register("database", vec![]).unwrap();
        orch.register("cache", vec![]).unwrap();
        orch.register("api", vec!["database".to_string(), "cache".to_string()])
            .unwrap();

        let statuses = orch.all_statuses();
        assert_eq!(statuses.len(), 3);
    }

    #[test]
    fn test_duplicate_registration_fails() {
        let orch = create_test_orchestrator();

        orch.register("unique-component", vec![]).unwrap();

        let result = orch.register("unique-component", vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn test_component_status_tracking() {
        let orch = create_test_orchestrator();

        orch.register("status-test", vec![]).unwrap();

        let status = orch.status("status-test");
        assert!(status.is_some());
        assert_eq!(status.unwrap(), ComponentStatus::Pending);
    }

    #[test]
    fn test_event_listener() {
        let orch = create_test_orchestrator();

        let events_received = Arc::new(Mutex::new(Vec::new()));
        let events_clone = Arc::clone(&events_received);

        orch.on_event(Box::new(move |event| {
            let mut events = events_clone.lock().unwrap();
            events.push(format!("{:?}", event));
        }));

        orch.register("event-test", vec![]).unwrap();
    }

    #[test]
    fn test_restart_policy_configuration() {
        let runtime = RuntimeEngine::new(RuntimeConfig::development()).unwrap();
        let runtime = Arc::new(Mutex::new(runtime));

        let config = OrchestratorConfig {
            restart_policy: RestartPolicy::Never,
            ..Default::default()
        };

        let _orch = Orchestrator::new(runtime, config);
    }

    #[test]
    fn test_metrics_collection() {
        let orch = create_test_orchestrator();

        orch.register("metrics-test-1", vec![]).unwrap();
        orch.register("metrics-test-2", vec![]).unwrap();

        let metrics = orch.orchestrator_metrics();

        assert_eq!(metrics.components_running, 0);
        assert_eq!(metrics.components_stopped, 2);
    }

    #[test]
    fn test_component_metrics_retrieval() {
        let orch = create_test_orchestrator();

        orch.register("metrics-comp", vec![]).unwrap();

        let metrics = orch.component_metrics("metrics-comp");
        assert!(metrics.is_some());

        let m = metrics.unwrap();
        assert_eq!(m.call_count, 0);
        assert_eq!(m.error_count, 0);
    }

    #[test]
    fn test_dependency_order() {
        let orch = create_test_orchestrator();

        orch.register("A", vec![]).unwrap();
        orch.register("B", vec!["A".to_string()]).unwrap();
        orch.register("C", vec!["B".to_string()]).unwrap();

        let statuses = orch.all_statuses();
        assert_eq!(statuses.len(), 3);
    }

    #[test]
    fn test_circular_dependency_detection() {
        let orch = create_test_orchestrator();

        orch.register("X", vec!["Z".to_string()]).unwrap();
        orch.register("Y", vec!["X".to_string()]).unwrap();
        orch.register("Z", vec!["Y".to_string()]).unwrap();

        let result = orch.start_all(CapabilitySet::cli_default());
        assert!(result.is_err());
    }

    #[test]
    fn test_health_check_config() {
        let health_check = HealthCheck::default();

        assert_eq!(health_check.function, "health");
        assert_eq!(health_check.timeout, Duration::from_secs(5));
        assert_eq!(health_check.interval, Duration::from_secs(30));
    }

    #[test]
    fn test_component_status_display() {
        assert_eq!(format!("{}", ComponentStatus::Pending), "pending");
        assert_eq!(format!("{}", ComponentStatus::Running), "running");
        assert_eq!(format!("{}", ComponentStatus::Stopped), "stopped");
        assert_eq!(format!("{}", ComponentStatus::Failed), "failed");
    }

    #[test]
    fn test_orchestrator_event_variants() {
        let events = vec![
            OrchestratorEvent::ComponentStarted {
                id: "test".to_string(),
            },
            OrchestratorEvent::ComponentStopped {
                id: "test".to_string(),
                exit_code: 0,
            },
            OrchestratorEvent::ComponentFailed {
                id: "test".to_string(),
                error: "error".to_string(),
            },
            OrchestratorEvent::ComponentRestarted {
                id: "test".to_string(),
                attempt: 1,
            },
            OrchestratorEvent::HealthCheckPassed {
                id: "test".to_string(),
            },
            OrchestratorEvent::HealthCheckFailed {
                id: "test".to_string(),
                reason: "timeout".to_string(),
            },
            OrchestratorEvent::ComponentCall {
                from: "a".to_string(),
                to: "b".to_string(),
                function: "func".to_string(),
            },
        ];

        assert_eq!(events.len(), 7);
    }

    #[test]
    fn test_component_metrics_default() {
        let metrics = ComponentMetrics::default();

        assert_eq!(metrics.call_count, 0);
        assert_eq!(metrics.error_count, 0);
        assert_eq!(metrics.total_time_ms, 0);
        assert_eq!(metrics.restart_count, 0);
    }

    #[test]
    fn test_orchestrator_metrics_default() {
        let metrics = OrchestratorMetrics::default();

        assert_eq!(metrics.components_running, 0);
        assert_eq!(metrics.components_stopped, 0);
        assert_eq!(metrics.total_calls, 0);
    }

    #[test]
    fn test_restart_policy_variants() {
        assert_eq!(RestartPolicy::Never, RestartPolicy::Never);
        assert_eq!(RestartPolicy::OnFailure, RestartPolicy::OnFailure);
        assert_eq!(RestartPolicy::Always, RestartPolicy::Always);
        assert_ne!(RestartPolicy::Never, RestartPolicy::Always);
    }

    #[test]
    fn test_stop_all_empty() {
        let orch = create_test_orchestrator();

        let result = orch.stop_all();
        assert!(result.is_ok());
    }

    #[test]
    fn test_status_nonexistent() {
        let orch = create_test_orchestrator();

        let status = orch.status("nonexistent");
        assert!(status.is_none());
    }

    #[test]
    fn test_concurrent_registration() {
        use std::thread;

        let orch = Arc::new(create_test_orchestrator());
        let mut handles = vec![];

        for i in 0..10 {
            let orch_clone = Arc::clone(&orch);
            let handle =
                thread::spawn(move || orch_clone.register(&format!("concurrent-{}", i), vec![]));
            handles.push(handle);
        }

        for handle in handles {
            let result = handle.join().unwrap();
            assert!(result.is_ok());
        }

        assert_eq!(orch.all_statuses().len(), 10);
    }

    #[test]
    fn test_all_statuses_empty() {
        let orch = create_test_orchestrator();

        let statuses = orch.all_statuses();
        assert!(statuses.is_empty());
    }

    #[test]
    fn test_get_logs_for_component() {
        let orch = create_test_orchestrator();

        orch.register("log-test", vec![]).unwrap();

        let logs = orch.get_logs("log-test", 10);
        assert!(logs.len() <= 10);
    }

    #[test]
    fn test_get_all_logs() {
        let orch = create_test_orchestrator();

        orch.register("log-all-1", vec![]).unwrap();
        orch.register("log-all-2", vec![]).unwrap();

        let logs = orch.get_all_logs(100);
        assert!(logs.len() <= 100);
    }

    #[test]
    fn test_component_metrics_none_for_unregistered() {
        let orch = create_test_orchestrator();

        let metrics = orch.component_metrics("not-registered");
        assert!(metrics.is_none());
    }

    #[test]
    fn test_orchestrator_config_defaults() {
        let config = OrchestratorConfig::default();

        assert!(config.health_checks);
        assert_eq!(config.health_check_interval, Duration::from_secs(30));
        assert_eq!(config.restart_policy, RestartPolicy::OnFailure);
        assert_eq!(config.max_restart_attempts, 3);
        assert_eq!(config.log_buffer_size, 10_000);
        assert!(config.metrics_enabled);
    }

    #[test]
    fn test_call_router_stats() {
        let router = CallRouter::new();
        let stats = router.stats();

        assert_eq!(stats.component_count, 0);
    }

    #[test]
    fn test_log_aggregator_creation() {
        let logs = LogAggregator::new(1000);

        logs.log("test-comp", LogLevel::Info, "Test message");

        let entries = logs.get_logs("test-comp", 10);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].message.contains("Test message"));
    }

    #[test]
    fn test_log_levels() {
        let logs = LogAggregator::new(100);

        logs.log("comp", LogLevel::Debug, "debug");
        logs.log("comp", LogLevel::Info, "info");
        logs.log("comp", LogLevel::Warn, "warn");
        logs.log("comp", LogLevel::Error, "error");

        let entries = logs.get_logs("comp", 10);
        assert_eq!(entries.len(), 4);
    }

    #[test]
    fn test_managed_component_creation() {
        let managed = ManagedComponent {
            id: "test".to_string(),
            handle: None,
            status: ComponentStatus::Pending,
            restart_count: 0,
            last_health_check: None,
            healthy: true,
            dependencies: vec!["dep1".to_string()],
            dependents: vec![],
        };

        assert_eq!(managed.id, "test");
        assert_eq!(managed.dependencies.len(), 1);
        assert!(managed.healthy);
    }
}
