#[cfg(feature = "v2")]
mod tests {
    use run::v2::runtime::{
        Capability, CapabilitySet, RuntimeConfig, RuntimeEngine, SecurityPolicy,
    };

    #[test]
    fn test_runtime_engine_creation() {
        let engine = RuntimeEngine::new(RuntimeConfig::default());
        assert!(engine.is_ok(), "Runtime engine should create successfully");
    }

    #[test]
    fn test_production_config_has_fuel() {
        let config = RuntimeConfig::production();
        assert!(
            config.fuel_limit.is_some(),
            "Production config should have fuel limit"
        );
        // epoch_interruption is disabled until we implement a safe
        // background epoch-increment thread (the naïve approach deadlocks).
        assert!(
            !config.epoch_interruption,
            "Epoch interruption disabled pending safe background thread"
        );
    }

    #[test]
    fn test_development_config() {
        let config = RuntimeConfig::development();
        assert!(config.debug, "Dev config should enable debug");
        assert!(
            config.security.allow_unrestricted,
            "Dev config should allow unrestricted"
        );
    }

    #[test]
    fn test_capability_set() {
        let mut caps = CapabilitySet::new();

        assert!(!caps.has(&Capability::Stdout));

        caps.grant(Capability::Stdout);
        assert!(caps.has(&Capability::Stdout));

        caps.revoke(&Capability::Stdout);
        assert!(!caps.has(&Capability::Stdout));
    }

    #[test]
    fn test_cli_default_capabilities() {
        let caps = CapabilitySet::cli_default();

        assert!(caps.has(&Capability::Stdout), "CLI should have stdout");
        assert!(caps.has(&Capability::Stderr), "CLI should have stderr");
        assert!(caps.has(&Capability::Stdin), "CLI should have stdin");
        assert!(caps.has(&Capability::Args), "CLI should have args");
        assert!(caps.has(&Capability::Cwd), "CLI should have cwd");
        assert!(caps.has(&Capability::Clock), "CLI should have clock");
        assert!(caps.has(&Capability::Exit), "CLI should have exit");

        assert!(!caps.has(&Capability::NetConnect {
            host: "localhost".to_string(),
            port: 80
        }));
    }

    #[test]
    fn test_unrestricted_capability() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::Unrestricted);

        assert!(caps.has(&Capability::Stdout));
        assert!(caps.has(&Capability::FileRead(std::path::PathBuf::from("/any/path"))));
        assert!(caps.has(&Capability::NetConnect {
            host: "example.com".to_string(),
            port: 443
        }));
    }

    #[test]
    fn test_security_policy_host_matching() {
        let mut policy = SecurityPolicy::default();

        assert!(policy.is_host_allowed("example.com"));
        assert!(policy.is_host_allowed("localhost"));

        policy.blocked_hosts.push("blocked.com".to_string());
        assert!(!policy.is_host_allowed("blocked.com"));

        policy.blocked_hosts.push("*.internal.corp".to_string());
        assert!(!policy.is_host_allowed("api.internal.corp"));
        assert!(policy.is_host_allowed("external.com"));
    }

    #[test]
    fn test_strict_security_policy() {
        let policy = SecurityPolicy::strict();

        assert!(
            !policy.allow_unrestricted,
            "Strict should not allow unrestricted"
        );
        assert!(
            policy.allowed_hosts.is_empty(),
            "Strict should block all hosts by default"
        );
        assert!(
            policy.max_memory < 128 * 1024 * 1024,
            "Strict should have low memory limit"
        );
        assert!(
            policy.max_execution_time_ms <= 5000,
            "Strict should have short timeout"
        );
    }

    #[test]
    fn test_capability_check_error() {
        let caps = CapabilitySet::new();

        let result = caps.check(&Capability::Stdout);
        assert!(result.is_err(), "Should fail capability check");
    }

    #[test]
    fn test_unrestricted_denied_in_production() {
        let engine = RuntimeEngine::new(RuntimeConfig::production()).unwrap();
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::Unrestricted);

        let wasm_bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        let mut engine = engine;
        let load_result = engine.load_component_bytes("test", wasm_bytes);

        if load_result.is_ok() {
            let result = engine.instantiate("test", caps);
            assert!(
                result.is_err(),
                "Unrestricted should be denied in production"
            );
        }
    }

    #[test]
    fn test_directory_capability_hierarchy() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::DirRead(std::path::PathBuf::from("/data")));

        assert!(caps.has(&Capability::FileRead(std::path::PathBuf::from(
            "/data/file.txt"
        ))));

        assert!(!caps.has(&Capability::FileRead(std::path::PathBuf::from(
            "/other/file.txt"
        ))));
    }

    #[test]
    fn test_component_call_capability() {
        let mut caps = CapabilitySet::new();

        caps.grant(Capability::ComponentCall {
            component: "logger".to_string(),
            function: "log".to_string(),
        });

        assert!(caps.has(&Capability::ComponentCall {
            component: "logger".to_string(),
            function: "log".to_string(),
        }));

        assert!(!caps.has(&Capability::ComponentCall {
            component: "logger".to_string(),
            function: "delete_all".to_string(),
        }));
    }

    #[test]
    fn test_component_call_any_capability() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::ComponentCallAny {
            component: "logger".to_string(),
        });

        assert!(caps.has(&Capability::ComponentCall {
            component: "logger".to_string(),
            function: "log".to_string(),
        }));

        assert!(caps.has(&Capability::ComponentCall {
            component: "logger".to_string(),
            function: "clear".to_string(),
        }));

        assert!(!caps.has(&Capability::ComponentCall {
            component: "database".to_string(),
            function: "query".to_string(),
        }));
    }

    #[test]
    fn test_capability_merge() {
        let mut caps1 = CapabilitySet::new();
        caps1.grant(Capability::Stdout);

        let mut caps2 = CapabilitySet::new();
        caps2.grant(Capability::Stderr);

        caps1.merge(&caps2);

        assert!(caps1.has(&Capability::Stdout));
        assert!(caps1.has(&Capability::Stderr));
    }

    #[test]
    fn test_capability_intersection() {
        let mut caps1 = CapabilitySet::new();
        caps1.grant(Capability::Stdout);
        caps1.grant(Capability::Stderr);

        let mut caps2 = CapabilitySet::new();
        caps2.grant(Capability::Stderr);
        caps2.grant(Capability::Stdin);

        let intersection = caps1.intersect(&caps2);

        assert!(!intersection.has(&Capability::Stdout));
        assert!(intersection.has(&Capability::Stderr));
        assert!(!intersection.has(&Capability::Stdin));
    }
}

#[cfg(not(feature = "v2"))]
mod basic_tests {
    #[test]
    fn test_placeholder() {
        // Placeholder so the test binary compiles without v2 feature
    }
}
