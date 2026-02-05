use super::instance::ImportBinding;
use super::*;
use crate::v2::wit::extract_wit_from_bytes;
use crate::v2::{Error, Result};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[cfg(feature = "v2")]
use super::wasi_ctx::{WasiCtxBuilder, WasiHostState};
#[cfg(feature = "v2")]
use wasmtime::component::Component;
#[cfg(feature = "v2")]
use wasmtime::{Config, Engine, Store};

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub security: SecurityPolicy,
    pub enable_cache: bool,
    pub cache_dir: Option<std::path::PathBuf>,
    pub parallel_instantiation: bool,
    pub max_concurrent_components: usize,
    pub debug: bool,
    pub fuel_limit: Option<u64>,
    pub epoch_interruption: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            security: SecurityPolicy::default(),
            enable_cache: true,
            cache_dir: None,
            parallel_instantiation: true,
            max_concurrent_components: 100,
            debug: false,
            fuel_limit: None,
            epoch_interruption: true,
        }
    }
}

impl RuntimeConfig {
    pub fn development() -> Self {
        Self {
            security: SecurityPolicy::development(),
            debug: true,
            ..Default::default()
        }
    }

    pub fn production() -> Self {
        Self {
            security: SecurityPolicy::default(),
            debug: false,
            fuel_limit: Some(1_000_000_000),
            epoch_interruption: true,
            ..Default::default()
        }
    }

    #[cfg(feature = "v2")]
    fn to_wasmtime_config(&self) -> Config {
        let mut config = Config::new();
        config.wasm_component_model(true);
        config.async_support(true);

        if self.fuel_limit.is_some() {
            config.consume_fuel(true);
        }

        if self.epoch_interruption {
            config.epoch_interruption(true);
        }

        config.cranelift_opt_level(wasmtime::OptLevel::Speed);

        config
    }
}

pub struct RuntimeEngine {
    config: RuntimeConfig,
    components: HashMap<String, Arc<LoadedComponent>>,
    instances: Arc<Mutex<HashMap<String, Arc<ComponentInstance>>>>,
    stats: Arc<Mutex<RuntimeStats>>,
    linker: Mutex<ComponentLinker>,
    memory_pool: MemoryPool,
    #[cfg(feature = "v2")]
    wasmtime_engine: Engine,
}

impl RuntimeEngine {
    pub fn new(config: RuntimeConfig) -> Result<Self> {
        let memory_config = MemoryConfig {
            max_per_component: config.security.max_memory,
            pool_size: config.max_concurrent_components * config.security.max_memory,
        };

        #[cfg(feature = "v2")]
        let wasmtime_engine = Engine::new(&config.to_wasmtime_config())
            .map_err(|e| Error::RuntimeInit(e.to_string()))?;

        Ok(Self {
            config,
            components: HashMap::new(),
            instances: Arc::new(Mutex::new(HashMap::new())),
            stats: Arc::new(Mutex::new(RuntimeStats::default())),
            linker: Mutex::new(ComponentLinker::new()),
            memory_pool: MemoryPool::new(memory_config),
            #[cfg(feature = "v2")]
            wasmtime_engine,
        })
    }

    pub fn default_engine() -> Result<Self> {
        Self::new(RuntimeConfig::default())
    }

    pub fn load_component(&mut self, path: &Path) -> Result<String> {
        let start = Instant::now();

        let mut component = LoadedComponent::from_file(path)?;

        if component.wit.is_none() {
            let wit_from_file = path.with_extension("wit");
            if wit_from_file.exists() {
                match crate::v2::wit::WitPackage::from_file(&wit_from_file) {
                    Ok(wit) => {
                        component.wit = Some(wit);
                    }
                    Err(e) => {
                        if self.config.debug {
                            eprintln!("WIT parse failed for {}: {}", wit_from_file.display(), e);
                        }
                    }
                }
            }
        }

        if component.wit.is_none() {
            match extract_wit_from_bytes(component.bytes.as_ref()) {
                Ok(wit) => {
                    component.wit = Some(wit);
                }
                Err(e) => {
                    if self.config.debug {
                        eprintln!("WIT extraction failed for {}: {}", path.display(), e);
                    }
                }
            }
        }

        if let Some(ref wit_pkg) = component.wit {
            let mut linker = self.linker.lock().unwrap();
            let _ = linker.register_exports(&component.id, wit_pkg);
            let _ = linker.register_imports(&component.id, wit_pkg);
        }

        #[cfg(feature = "v2")]
        {
            let compiled = Component::from_binary(&self.wasmtime_engine, &component.bytes)
                .map_err(|e| Error::InvalidComponent {
                    path: path.to_path_buf(),
                    reason: e.to_string(),
                })?;
            component.compiled = Some(compiled);
        }

        let id = component.id.clone();
        self.components.insert(id.clone(), Arc::new(component));

        let mut stats = self.stats.lock().unwrap();
        stats.components_loaded += 1;
        stats.startup_ms += start.elapsed().as_millis() as u64;

        Ok(id)
    }

    pub fn load_component_bytes(&mut self, id: &str, bytes: Vec<u8>) -> Result<String> {
        let mut component = LoadedComponent::from_bytes(id, bytes);

        if component.wit.is_none() {
            match extract_wit_from_bytes(component.bytes.as_ref()) {
                Ok(wit) => {
                    component.wit = Some(wit);
                }
                Err(e) => {
                    if self.config.debug {
                        eprintln!("WIT extraction failed for {}: {}", id, e);
                    }
                }
            }
        }

        if let Some(ref wit_pkg) = component.wit {
            let mut linker = self.linker.lock().unwrap();
            let _ = linker.register_exports(&component.id, wit_pkg);
            let _ = linker.register_imports(&component.id, wit_pkg);
        }

        #[cfg(feature = "v2")]
        {
            let compiled = Component::from_binary(&self.wasmtime_engine, &component.bytes)
                .map_err(|e| Error::InvalidComponent {
                    path: std::path::PathBuf::from(id),
                    reason: e.to_string(),
                })?;
            component.compiled = Some(compiled);
        }

        let id = component.id.clone();
        self.components.insert(id.clone(), Arc::new(component));

        let mut stats = self.stats.lock().unwrap();
        stats.components_loaded += 1;

        Ok(id)
    }

    pub fn instantiate(
        &self,
        component_id: &str,
        capabilities: CapabilitySet,
    ) -> Result<InstanceHandle> {
        let start = Instant::now();

        let component = self
            .components
            .get(component_id)
            .ok_or_else(|| Error::ComponentNotFound(component_id.to_string()))?;

        let import_bindings = if let Some(ref wit) = component.wit {
            let mut linker = self.linker.lock().unwrap();
            linker.register_exports(component_id, wit)?;
            linker.resolve_imports(component_id, wit)?;
            if let Err(e) = linker.check_satisfied(component_id) {
                return Err(Error::ComponentInstantiation {
                    component: component_id.to_string(),
                    reason: e.to_string(),
                });
            }
            let errors = linker.validate_all_links();
            if !errors.is_empty() {
                let details = errors
                    .into_iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join("; ");
                return Err(Error::ComponentInstantiation {
                    component: component_id.to_string(),
                    reason: details,
                });
            }

            self.build_import_bindings(component_id, wit)?
        } else {
            Vec::new()
        };

        self.validate_capabilities(&capabilities)?;

        let memory = self.memory_pool.allocate()?;

        #[cfg(feature = "v2")]
        let instance = {
            let wasi_ctx = WasiCtxBuilder::from_capabilities(&capabilities)
                .build()
                .map_err(|e| Error::ComponentInstantiation {
                    component: component_id.to_string(),
                    reason: format!("Failed to build WASI context: {}", e),
                })?;

            let host_state = WasiHostState::new(wasi_ctx, self.config.fuel_limit);
            let mut store = Store::new(&self.wasmtime_engine, host_state);

            if let Some(fuel) = self.config.fuel_limit {
                store
                    .set_fuel(fuel)
                    .map_err(|e| Error::ComponentInstantiation {
                        component: component_id.to_string(),
                        reason: format!("Failed to set fuel: {}", e),
                    })?;
            }

            if self.config.epoch_interruption {
                store.epoch_deadline_trap();
            }

            ComponentInstance::new_with_store(
                component_id.to_string(),
                Arc::clone(component),
                capabilities,
                memory,
                self.config.fuel_limit,
                store,
                import_bindings,
                Some(Arc::downgrade(&self.instances)),
            )?
        };

        #[cfg(not(feature = "v2"))]
        let instance = ComponentInstance::new(
            component_id.to_string(),
            Arc::clone(component),
            capabilities,
            memory,
            self.config.fuel_limit,
            import_bindings,
            Some(Arc::downgrade(&self.instances)),
        )?;

        let handle = instance.handle();

        let mut instances = self.instances.lock().unwrap();
        instances.insert(handle.id.clone(), Arc::new(instance));

        let mut stats = self.stats.lock().unwrap();
        stats.instantiations += 1;
        stats.startup_ms += start.elapsed().as_millis() as u64;

        Ok(handle)
    }

    pub fn call(
        &self,
        handle: &InstanceHandle,
        function: &str,
        args: Vec<ComponentValue>,
    ) -> Result<ExecutionResult> {
        let start = Instant::now();

        let instances = self.instances.lock().unwrap();
        let instance = instances
            .get(&handle.id)
            .ok_or_else(|| Error::ComponentNotFound(handle.id.clone()))?;

        #[cfg(feature = "v2")]
        {
            let _deadline_ms = self.config.security.max_execution_time_ms;
        }

        let result = instance.call(function, args);
        let elapsed_ms = start.elapsed().as_millis() as u64;

        if elapsed_ms > self.config.security.max_execution_time_ms {
            instance.mark_error();
            return Err(Error::ExecutionFailed {
                component: handle.component_id.clone(),
                reason: format!(
                    "Call exceeded max execution time ({} ms)",
                    self.config.security.max_execution_time_ms
                ),
            });
        }
        let result = result?;

        drop(instances);
        let mut stats = self.stats.lock().unwrap();
        stats.function_calls += 1;

        Ok(ExecutionResult {
            exit_code: result.exit_code,
            stdout: result.stdout,
            stderr: result.stderr,
            duration_ms: elapsed_ms,
            return_value: result.return_value,
        })
    }

    pub fn run_cli(
        &mut self,
        path: &Path,
        args: Vec<String>,
        env: Vec<(String, String)>,
    ) -> Result<ExecutionResult> {
        let start = Instant::now();

        let component_id = self.load_component(path)?;

        let mut caps = self.config.security.cli_default.clone();

        if let Ok(cwd) = std::env::current_dir() {
            caps.grant(Capability::DirRead(cwd.clone()));
            caps.grant(Capability::FileRead(cwd.clone()));
        }

        let handle = self.instantiate(&component_id, caps)?;

        let ctx = CliContext {
            args,
            env,
            stdin: None,
            cwd: std::env::current_dir().ok(),
        };

        let instances = self.instances.lock().unwrap();
        let instance = instances.get(&handle.id).unwrap();
        let result = instance.run_cli(ctx)?;

        Ok(ExecutionResult {
            exit_code: result.exit_code,
            stdout: result.stdout,
            stderr: result.stderr,
            duration_ms: start.elapsed().as_millis() as u64,
            return_value: None,
        })
    }

    pub fn get_instance(&self, handle: &InstanceHandle) -> Option<Arc<ComponentInstance>> {
        let instances = self.instances.lock().unwrap();
        instances.get(&handle.id).cloned()
    }

    pub fn terminate(&self, handle: &InstanceHandle) -> Result<()> {
        let mut instances = self.instances.lock().unwrap();
        if let Some(instance) = instances.remove(&handle.id) {
            if let Ok(mut mem_guard) = Arc::try_unwrap(instance) {
                self.memory_pool.release(mem_guard.take_memory());
            }
        }
        Ok(())
    }

    pub fn stats(&self) -> RuntimeStats {
        self.stats.lock().unwrap().clone()
    }

    pub fn reset_stats(&self) {
        let mut stats = self.stats.lock().unwrap();
        *stats = RuntimeStats::default();
    }

    pub fn active_instances(&self) -> usize {
        self.instances.lock().unwrap().len()
    }

    pub fn is_loaded(&self, component_id: &str) -> bool {
        self.components.contains_key(component_id)
    }

    pub fn get_component_info(&self, component_id: &str) -> Option<ComponentInfo> {
        self.components.get(component_id).map(|c| ComponentInfo {
            id: c.id.clone(),
            hash: c.hash.clone(),
            size_bytes: c.bytes.len(),
            source_path: c.source_path.clone(),
        })
    }

    pub fn list_components(&self) -> Vec<String> {
        self.components.keys().cloned().collect()
    }

    pub fn unload(&mut self, component_id: &str) -> Result<()> {
        let mut instances = self.instances.lock().unwrap();
        instances.retain(|_, inst| inst.component_id() != component_id);
        drop(instances);

        self.components.remove(component_id);
        Ok(())
    }

    pub fn link_components(&mut self, components: &[&str]) -> Result<()> {
        let mut linker = self.linker.lock().unwrap();
        for component_id in components {
            let component = self
                .components
                .get(*component_id)
                .ok_or_else(|| Error::ComponentNotFound(component_id.to_string()))?;

            if let Some(ref wit) = component.wit {
                linker.register_exports(component_id, wit)?;
            }
        }

        for component_id in components {
            let component = self.components.get(*component_id).unwrap();
            if let Some(ref wit) = component.wit {
                linker.resolve_imports(component_id, wit)?;
            }
        }

        Ok(())
    }

    #[cfg(feature = "v2")]
    pub fn increment_epoch(&self) {
        self.wasmtime_engine.increment_epoch();
    }

    #[cfg(feature = "v2")]
    pub fn wasmtime_engine(&self) -> &Engine {
        &self.wasmtime_engine
    }

    fn build_import_bindings(
        &self,
        component_id: &str,
        wit: &crate::v2::wit::WitPackage,
    ) -> Result<Vec<ImportBinding>> {
        let resolved = {
            let linker = self.linker.lock().unwrap();
            linker.get_resolved_imports(component_id)
        };

        if resolved.is_empty() {
            return Ok(Vec::new());
        }

        let mut bindings = Vec::new();

        for (import_name, provider_id) in resolved {
            let interface_ref = wit
                .worlds
                .values()
                .flat_map(|world| world.imports.iter())
                .find_map(|item| match item {
                    crate::v2::wit::WitWorldItem::Interface { name, interface }
                        if name == &import_name =>
                    {
                        Some(interface.clone())
                    }
                    _ => None,
                })
                .ok_or_else(|| Error::ComponentInstantiation {
                    component: component_id.to_string(),
                    reason: format!("Missing interface for import '{}'", import_name),
                })?;

            let interface_name = match &interface_ref {
                crate::v2::wit::WitInterfaceRef::Local(name) => name.clone(),
                crate::v2::wit::WitInterfaceRef::External { interface, .. } => interface.clone(),
            };

            let mut functions: Vec<String> = Vec::new();

            if let Some(interface) = wit.interfaces.get(&interface_name) {
                functions.extend(interface.functions.keys().cloned());
            } else if let Some(provider_wit) = self
                .components
                .get(&provider_id)
                .and_then(|c| c.wit.as_ref())
            {
                if let Some(interface) = provider_wit.interfaces.get(&interface_name) {
                    functions.extend(interface.functions.keys().cloned());
                }
            }

            if functions.is_empty() {
                return Err(Error::ComponentInstantiation {
                    component: component_id.to_string(),
                    reason: format!(
                        "No functions found for import '{}' ({})",
                        import_name, interface_name
                    ),
                });
            }

            bindings.push(ImportBinding {
                import_name,
                functions,
                provider_component: provider_id,
            });
        }

        Ok(bindings)
    }

    fn validate_capabilities(&self, caps: &CapabilitySet) -> Result<()> {
        let policy = &self.config.security;
        for cap in caps.iter() {
            match cap {
                Capability::Unrestricted if !policy.allow_unrestricted => {
                    return Err(Error::InvalidCapability(
                        "Unrestricted capability is not allowed by policy".to_string(),
                    ));
                }
                Capability::NetConnect { host, .. } => {
                    if !policy.is_host_allowed(host) {
                        return Err(Error::InvalidCapability(format!(
                            "Network access to '{}' is blocked by policy",
                            host
                        )));
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct CliContext {
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub stdin: Option<Vec<u8>>,
    pub cwd: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ComponentInfo {
    pub id: String,
    pub hash: String,
    pub size_bytes: usize,
    pub source_path: Option<std::path::PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_config_defaults() {
        let config = RuntimeConfig::default();
        assert!(config.enable_cache);
        assert!(config.parallel_instantiation);
        assert_eq!(config.max_concurrent_components, 100);
    }

    #[test]
    fn test_runtime_engine_creation() {
        let engine = RuntimeEngine::new(RuntimeConfig::default());
        assert!(engine.is_ok());
    }

    #[test]
    fn test_unrestricted_capability_denied() {
        // Test that production config denies unrestricted capabilities
        let config = RuntimeConfig::production();
        assert!(!config.security.allow_unrestricted);

        // Create engine and test that validate_capabilities would reject unrestricted
        let engine = RuntimeEngine::new(config).unwrap();
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::Unrestricted);

        // Directly test the validation logic
        let result = engine.validate_capabilities(&caps);
        assert!(matches!(result, Err(Error::InvalidCapability(_))));
    }
}
