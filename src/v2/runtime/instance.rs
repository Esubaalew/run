use super::engine::CliContext;
use super::*;
use crate::v2::{Error, Result};
use std::sync::{
    atomic::{AtomicI32, AtomicU64, AtomicU8, Ordering},
    Arc, Weak,
};

#[cfg(feature = "v2")]
use super::wasi_ctx::WasiHostState;
#[cfg(feature = "v2")]
use anyhow::anyhow;
#[cfg(feature = "v2")]
use wasmtime::component::{Instance, Linker as WasmtimeLinker, Val};
#[cfg(feature = "v2")]
use wasmtime::Store;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceState {
    Created,
    Running,
    Paused,
    Completed,
    Error,
    Terminated,
}

impl std::fmt::Display for InstanceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstanceState::Created => write!(f, "created"),
            InstanceState::Running => write!(f, "running"),
            InstanceState::Paused => write!(f, "paused"),
            InstanceState::Completed => write!(f, "completed"),
            InstanceState::Error => write!(f, "error"),
            InstanceState::Terminated => write!(f, "terminated"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InstanceHandle {
    pub id: String,
    pub component_id: String,
}

impl InstanceHandle {
    pub fn new(component_id: &str) -> Self {
        Self {
            id: format!("{}_{}", component_id, generate_instance_id()),
            component_id: component_id.to_string(),
        }
    }
}

fn generate_instance_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    duration.as_nanos() as u64
}

pub struct ComponentInstance {
    handle: InstanceHandle,
    component: Arc<LoadedComponent>,
    capabilities: CapabilitySet,
    state: AtomicU8,
    memory: Option<AllocatedMemory>,
    stdout_buffer: std::sync::Mutex<Vec<u8>>,
    stderr_buffer: std::sync::Mutex<Vec<u8>>,
    exit_code: AtomicI32,
    exports: Vec<String>,
    fuel_remaining: Option<AtomicU64>,
    import_bindings: Vec<ImportBinding>,
    bridge_instances:
        Option<Weak<std::sync::Mutex<std::collections::HashMap<String, Arc<ComponentInstance>>>>>,
    #[cfg(feature = "v2")]
    store: std::sync::Mutex<Option<Store<WasiHostState>>>,
    #[cfg(feature = "v2")]
    wasmtime_instance: std::sync::Mutex<Option<Instance>>,
}

pub struct AllocatedMemory {
    pub id: u64,
    pub size: usize,
}

#[derive(Debug, Clone)]
pub struct ImportBinding {
    pub import_name: String,
    pub functions: Vec<String>,
    pub provider_component: String,
}

impl ComponentInstance {
    #[cfg(not(feature = "v2"))]
    pub fn new(
        component_id: String,
        component: Arc<LoadedComponent>,
        capabilities: CapabilitySet,
        memory: AllocatedMemory,
        fuel_limit: Option<u64>,
        import_bindings: Vec<ImportBinding>,
        bridge_instances: Option<
            Weak<std::sync::Mutex<std::collections::HashMap<String, Arc<ComponentInstance>>>>,
        >,
    ) -> Result<Self> {
        let handle = InstanceHandle::new(&component_id);

        let exports = if let Some(ref wit) = component.wit {
            wit.interfaces
                .values()
                .flat_map(|iface| iface.functions.keys().cloned())
                .collect()
        } else {
            vec![]
        };

        Ok(Self {
            handle,
            component,
            capabilities,
            state: AtomicU8::new(InstanceState::Created as u8),
            memory: Some(memory),
            stdout_buffer: std::sync::Mutex::new(Vec::new()),
            stderr_buffer: std::sync::Mutex::new(Vec::new()),
            exit_code: AtomicI32::new(0),
            exports,
            fuel_remaining: fuel_limit.map(AtomicU64::new),
            import_bindings,
            bridge_instances,
        })
    }

    #[cfg(feature = "v2")]
    pub fn new_with_store(
        component_id: String,
        component: Arc<LoadedComponent>,
        capabilities: CapabilitySet,
        memory: AllocatedMemory,
        fuel_limit: Option<u64>,
        store: Store<WasiHostState>,
        import_bindings: Vec<ImportBinding>,
        bridge_instances: Option<
            Weak<std::sync::Mutex<std::collections::HashMap<String, Arc<ComponentInstance>>>>,
        >,
    ) -> Result<Self> {
        let handle = InstanceHandle::new(&component_id);

        let exports = if let Some(ref wit) = component.wit {
            wit.interfaces
                .values()
                .flat_map(|iface| iface.functions.keys().cloned())
                .collect()
        } else {
            vec![]
        };

        Ok(Self {
            handle,
            component,
            capabilities,
            state: AtomicU8::new(InstanceState::Created as u8),
            memory: Some(memory),
            stdout_buffer: std::sync::Mutex::new(Vec::new()),
            stderr_buffer: std::sync::Mutex::new(Vec::new()),
            exit_code: AtomicI32::new(0),
            exports,
            fuel_remaining: fuel_limit.map(AtomicU64::new),
            import_bindings,
            bridge_instances,
            store: std::sync::Mutex::new(Some(store)),
            wasmtime_instance: std::sync::Mutex::new(None),
        })
    }

    #[cfg(feature = "v2")]
    pub fn new(
        component_id: String,
        component: Arc<LoadedComponent>,
        capabilities: CapabilitySet,
        memory: AllocatedMemory,
        fuel_limit: Option<u64>,
        import_bindings: Vec<ImportBinding>,
        bridge_instances: Option<
            Weak<std::sync::Mutex<std::collections::HashMap<String, Arc<ComponentInstance>>>>,
        >,
    ) -> Result<Self> {
        let handle = InstanceHandle::new(&component_id);

        let exports = if let Some(ref wit) = component.wit {
            wit.interfaces
                .values()
                .flat_map(|iface| iface.functions.keys().cloned())
                .collect()
        } else {
            vec![]
        };

        Ok(Self {
            handle,
            component,
            capabilities,
            state: AtomicU8::new(InstanceState::Created as u8),
            memory: Some(memory),
            stdout_buffer: std::sync::Mutex::new(Vec::new()),
            stderr_buffer: std::sync::Mutex::new(Vec::new()),
            exit_code: AtomicI32::new(0),
            exports,
            fuel_remaining: fuel_limit.map(AtomicU64::new),
            import_bindings,
            bridge_instances,
            store: std::sync::Mutex::new(None),
            wasmtime_instance: std::sync::Mutex::new(None),
        })
    }

    pub fn handle(&self) -> InstanceHandle {
        self.handle.clone()
    }

    pub fn component_id(&self) -> &str {
        &self.handle.component_id
    }

    pub fn state(&self) -> InstanceState {
        match self.state.load(Ordering::SeqCst) {
            0 => InstanceState::Created,
            1 => InstanceState::Running,
            2 => InstanceState::Paused,
            3 => InstanceState::Completed,
            4 => InstanceState::Error,
            _ => InstanceState::Terminated,
        }
    }

    fn set_state(&self, state: InstanceState) {
        self.state.store(state as u8, Ordering::SeqCst);
    }

    pub fn has_capability(&self, cap: &Capability) -> bool {
        self.capabilities.has(cap)
    }

    #[cfg(feature = "v2")]
    fn ensure_instance(&self, store: &mut Store<WasiHostState>) -> Result<()> {
        let mut instance_guard = self.wasmtime_instance.lock().unwrap();
        if instance_guard.is_some() {
            return Ok(());
        }

        let compiled = self
            .component
            .compiled
            .as_ref()
            .ok_or_else(|| Error::ExecutionFailed {
                component: self.component_id().to_string(),
                reason: "Component not compiled".to_string(),
            })?;

        let mut linker = WasmtimeLinker::<WasiHostState>::new(store.engine());
        wasmtime_wasi::add_to_linker_sync(&mut linker).map_err(|e| Error::ExecutionFailed {
            component: self.component_id().to_string(),
            reason: format!("Failed to add WASI to linker: {}", e),
        })?;

        self.add_import_bindings(&mut linker)?;

        let instance = linker
            .instantiate(store, compiled)
            .map_err(|e| Error::ExecutionFailed {
                component: self.component_id().to_string(),
                reason: format!("Instantiation failed: {}", e),
            })?;

        *instance_guard = Some(instance);
        Ok(())
    }

    #[cfg(feature = "v2")]
    fn add_import_bindings(&self, linker: &mut WasmtimeLinker<WasiHostState>) -> Result<()> {
        if self.import_bindings.is_empty() {
            return Ok(());
        }

        for binding in &self.import_bindings {
            let mut instance =
                linker
                    .instance(&binding.import_name)
                    .map_err(|e| Error::ExecutionFailed {
                        component: self.component_id().to_string(),
                        reason: format!(
                            "Failed to create import instance '{}': {}",
                            binding.import_name, e
                        ),
                    })?;

            for func_name in &binding.functions {
                let provider_component = binding.provider_component.clone();
                let function = func_name.clone();
                let function_for_closure = function.clone();
                let bridge_instances = self.bridge_instances.clone();
                let capabilities = self.capabilities.clone();

                instance
                    .func_new(&function, move |_store, params, results| {
                        let required = Capability::ComponentCall {
                            component: provider_component.clone(),
                            function: function_for_closure.clone(),
                        };
                        if !capabilities.has(&required) {
                            return Err(anyhow!("Capability denied: {}", required.description()));
                        }

                        let provider =
                            resolve_provider_instance(&bridge_instances, &provider_component)?;
                        let args = params.iter().map(val_to_component_value).collect();
                        let call_result = provider
                            .call(&function_for_closure, args)
                            .map_err(|e| anyhow!(e.to_string()))?;
                        write_results_from_component_value(results, call_result.return_value)
                    })
                    .map_err(|e| Error::ExecutionFailed {
                        component: self.component_id().to_string(),
                        reason: format!(
                            "Failed to bind import {}.{}: {}",
                            binding.import_name, func_name, e
                        ),
                    })?;
            }
        }

        Ok(())
    }

    #[cfg(feature = "v2")]
    pub fn call(&self, function: &str, args: Vec<ComponentValue>) -> Result<CallResult> {
        self.set_state(InstanceState::Running);

        let mut store_guard = self.store.lock().unwrap();
        let store = store_guard.as_mut().ok_or_else(|| Error::ExecutionFailed {
            component: self.component_id().to_string(),
            reason: "Store not initialized".to_string(),
        })?;

        if let Some(ref _fuel) = self.fuel_remaining {
            let current = store.get_fuel().unwrap_or(0);
            if current == 0 {
                self.set_state(InstanceState::Error);
                return Err(Error::ExecutionFailed {
                    component: self.component_id().to_string(),
                    reason: "Fuel exhausted".to_string(),
                });
            }
        }

        self.ensure_instance(store)?;

        let instance_guard = self.wasmtime_instance.lock().unwrap();

        if let Some(instance) = instance_guard.as_ref() {
            let func =
                instance
                    .get_func(&mut *store, function)
                    .ok_or_else(|| Error::ExecutionFailed {
                        component: self.component_id().to_string(),
                        reason: format!("Function '{}' not found", function),
                    })?;

            let wasm_args: Vec<Val> = args.iter().map(component_value_to_val).collect();
            let results_len = func.results(&mut *store).len();
            let mut results: Vec<Val> = std::iter::repeat_with(|| Val::Bool(false))
                .take(results_len)
                .collect();

            func.call(&mut *store, &wasm_args, &mut results)
                .map_err(|e| {
                    self.set_state(InstanceState::Error);
                    Error::ExecutionFailed {
                        component: self.component_id().to_string(),
                        reason: e.to_string(),
                    }
                })?;

            func.post_return(&mut *store)
                .map_err(|e| Error::ExecutionFailed {
                    component: self.component_id().to_string(),
                    reason: format!("Post-return failed: {}", e),
                })?;

            self.set_state(InstanceState::Completed);

            let host = store.data();
            let return_value = if results.is_empty() {
                None
            } else if results.len() == 1 {
                Some(val_to_component_value(&results[0]))
            } else {
                Some(ComponentValue::Tuple(
                    results.iter().map(val_to_component_value).collect(),
                ))
            };

            Ok(CallResult {
                exit_code: 0,
                stdout: host.stdout_buffer.clone(),
                stderr: host.stderr_buffer.clone(),
                return_value,
            })
        } else {
            self.set_state(InstanceState::Error);
            Err(Error::ExecutionFailed {
                component: self.component_id().to_string(),
                reason: "Component not compiled or instance not created".to_string(),
            })
        }
    }

    #[cfg(not(feature = "v2"))]
    pub fn call(&self, function: &str, args: Vec<ComponentValue>) -> Result<CallResult> {
        if !self.exports.contains(&function.to_string()) {
            return Err(Error::ExecutionFailed {
                component: self.component_id().to_string(),
                reason: format!("Function '{}' not found", function),
            });
        }

        let fuel_cost = 1 + args.len() as u64;
        self.consume_fuel(fuel_cost)?;

        self.set_state(InstanceState::Running);

        let result = CallResult {
            exit_code: 0,
            stdout: self.stdout_buffer.lock().unwrap().clone(),
            stderr: self.stderr_buffer.lock().unwrap().clone(),
            return_value: Some(ComponentValue::Unit),
        };

        self.set_state(InstanceState::Completed);

        Ok(result)
    }

    #[cfg(feature = "v2")]
    pub fn run_cli(&self, _ctx: CliContext) -> Result<CallResult> {
        self.set_state(InstanceState::Running);

        self.capabilities
            .check(&Capability::Args)
            .map_err(|_| Error::CapabilityDenied {
                capability: "args".to_string(),
                component: self.component_id().to_string(),
            })?;

        let mut store_guard = self.store.lock().unwrap();
        let store = store_guard.as_mut().ok_or_else(|| Error::ExecutionFailed {
            component: self.component_id().to_string(),
            reason: "Store not initialized".to_string(),
        })?;

        self.ensure_instance(store)?;

        let instance_guard = self.wasmtime_instance.lock().unwrap();
        let instance = instance_guard.as_ref().unwrap();

        if let Some(func) = instance.get_func(&mut *store, "_start") {
            func.call(&mut *store, &[], &mut []).map_err(|e| {
                self.set_state(InstanceState::Error);
                Error::ExecutionFailed {
                    component: self.component_id().to_string(),
                    reason: e.to_string(),
                }
            })?;
        }

        self.set_state(InstanceState::Completed);

        let host = store.data();

        Ok(CallResult {
            exit_code: 0,
            stdout: host.stdout_buffer.clone(),
            stderr: host.stderr_buffer.clone(),
            return_value: None,
        })
    }

    #[cfg(not(feature = "v2"))]
    pub fn run_cli(&self, ctx: CliContext) -> Result<CallResult> {
        self.set_state(InstanceState::Running);

        self.capabilities
            .check(&Capability::Args)
            .map_err(|_| Error::CapabilityDenied {
                capability: "args".to_string(),
                component: self.component_id().to_string(),
            })?;

        let fuel_cost = 1 + ctx.args.len() as u64;
        self.consume_fuel(fuel_cost)?;

        let stdout = format!(
            "Component {} executed with args: {:?}\n",
            self.component_id(),
            ctx.args
        );

        self.set_state(InstanceState::Completed);

        Ok(CallResult {
            exit_code: 0,
            stdout: stdout.into_bytes(),
            stderr: vec![],
            return_value: None,
        })
    }

    pub fn write_stdout(&self, data: &[u8]) {
        if self.capabilities.has(&Capability::Stdout) {
            let mut buffer = self.stdout_buffer.lock().unwrap();
            buffer.extend_from_slice(data);
        }
    }

    pub fn write_stderr(&self, data: &[u8]) {
        if self.capabilities.has(&Capability::Stderr) {
            let mut buffer = self.stderr_buffer.lock().unwrap();
            buffer.extend_from_slice(data);
        }
    }

    pub fn read_stdout(&self) -> Vec<u8> {
        self.stdout_buffer.lock().unwrap().clone()
    }

    pub fn read_stderr(&self) -> Vec<u8> {
        self.stderr_buffer.lock().unwrap().clone()
    }

    pub fn pause(&self) -> Result<()> {
        if self.state() != InstanceState::Running {
            return Err(Error::LifecycleError {
                component: self.component_id().to_string(),
                reason: "Can only pause running instances".to_string(),
            });
        }
        self.set_state(InstanceState::Paused);
        Ok(())
    }

    pub fn resume(&self) -> Result<()> {
        if self.state() != InstanceState::Paused {
            return Err(Error::LifecycleError {
                component: self.component_id().to_string(),
                reason: "Can only resume paused instances".to_string(),
            });
        }
        self.set_state(InstanceState::Running);
        Ok(())
    }

    pub fn terminate(&self) {
        self.set_state(InstanceState::Terminated);
    }

    pub fn mark_error(&self) {
        self.set_state(InstanceState::Error);
    }

    pub fn exit_code(&self) -> i32 {
        self.exit_code.load(Ordering::SeqCst)
    }

    pub fn take_memory(&mut self) -> Option<AllocatedMemory> {
        self.memory.take()
    }

    pub fn exports(&self) -> &[String] {
        &self.exports
    }

    #[cfg(feature = "v2")]
    pub fn remaining_fuel(&self) -> Option<u64> {
        let store_guard = self.store.lock().unwrap();
        if let Some(ref store) = *store_guard {
            store.get_fuel().ok()
        } else {
            self.fuel_remaining
                .as_ref()
                .map(|f| f.load(Ordering::SeqCst))
        }
    }

    #[cfg(not(feature = "v2"))]
    pub fn remaining_fuel(&self) -> Option<u64> {
        self.fuel_remaining
            .as_ref()
            .map(|f| f.load(Ordering::SeqCst))
    }

    #[cfg(not(feature = "v2"))]
    fn consume_fuel(&self, cost: u64) -> Result<()> {
        let Some(fuel) = self.fuel_remaining.as_ref() else {
            return Ok(());
        };

        loop {
            let current = fuel.load(Ordering::SeqCst);
            if current < cost {
                self.set_state(InstanceState::Error);
                return Err(Error::ExecutionFailed {
                    component: self.component_id().to_string(),
                    reason: "Fuel exhausted".to_string(),
                });
            }
            if fuel
                .compare_exchange(current, current - cost, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return Ok(());
            }
        }
    }
}

#[derive(Debug)]
pub struct CallResult {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub return_value: Option<ComponentValue>,
}

#[cfg(feature = "v2")]
fn resolve_provider_instance(
    bridge_instances: &Option<
        Weak<std::sync::Mutex<std::collections::HashMap<String, Arc<ComponentInstance>>>>,
    >,
    provider_component: &str,
) -> anyhow::Result<Arc<ComponentInstance>> {
    let instances = bridge_instances
        .as_ref()
        .and_then(|weak| weak.upgrade())
        .ok_or_else(|| anyhow!("Runtime bridge not available"))?;

    let instance = {
        let map = instances.lock().unwrap();
        map.values()
            .find(|inst| inst.component_id() == provider_component)
            .cloned()
    };

    instance.ok_or_else(|| {
        anyhow!(
            "Provider component '{}' not instantiated",
            provider_component
        )
    })
}

#[cfg(feature = "v2")]
fn write_results_from_component_value(
    results: &mut [Val],
    return_value: Option<ComponentValue>,
) -> anyhow::Result<()> {
    if results.is_empty() {
        return Ok(());
    }

    if results.len() == 1 {
        let value = return_value.unwrap_or(ComponentValue::Unit);
        results[0] = component_value_to_val(&value);
        return Ok(());
    }

    let values = match return_value {
        Some(ComponentValue::Tuple(values)) => values,
        Some(ComponentValue::Record(fields)) => fields.into_iter().map(|(_, v)| v).collect(),
        Some(value) => vec![value],
        None => Vec::new(),
    };

    if values.len() != results.len() {
        return Err(anyhow!(
            "expected {} results, got {}",
            results.len(),
            values.len()
        ));
    }

    for (slot, value) in results.iter_mut().zip(values.iter()) {
        *slot = component_value_to_val(value);
    }

    Ok(())
}

#[cfg(feature = "v2")]
fn component_value_to_val(value: &ComponentValue) -> Val {
    match value {
        ComponentValue::Bool(b) => Val::Bool(*b),
        ComponentValue::U8(v) => Val::U8(*v),
        ComponentValue::U16(v) => Val::U16(*v),
        ComponentValue::U32(v) => Val::U32(*v),
        ComponentValue::U64(v) => Val::U64(*v),
        ComponentValue::S8(v) => Val::S8(*v),
        ComponentValue::S16(v) => Val::S16(*v),
        ComponentValue::S32(v) => Val::S32(*v),
        ComponentValue::S64(v) => Val::S64(*v),
        ComponentValue::F32(v) => Val::Float32(*v),
        ComponentValue::F64(v) => Val::Float64(*v),
        ComponentValue::Char(c) => Val::Char(*c),
        ComponentValue::String(s) => Val::String(s.clone()),
        ComponentValue::List(items) => {
            Val::List(items.iter().map(component_value_to_val).collect())
        }
        ComponentValue::Record(fields) => Val::Record(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), component_value_to_val(v)))
                .collect(),
        ),
        ComponentValue::Tuple(items) => {
            Val::Tuple(items.iter().map(component_value_to_val).collect())
        }
        ComponentValue::Variant { tag, value } => {
            let inner = value.as_ref().map(|v| Box::new(component_value_to_val(v)));
            Val::Variant(tag.clone(), inner)
        }
        ComponentValue::Option(inner) => {
            Val::Option(inner.as_ref().map(|v| Box::new(component_value_to_val(v))))
        }
        ComponentValue::Result { ok, err } => {
            if let Some(ok_val) = ok.as_ref() {
                Val::Result(Ok(Some(Box::new(component_value_to_val(ok_val)))))
            } else if let Some(err_val) = err.as_ref() {
                Val::Result(Err(Some(Box::new(component_value_to_val(err_val)))))
            } else {
                Val::Result(Ok(None))
            }
        }
        ComponentValue::Enum(tag) => Val::Enum(tag.clone()),
        ComponentValue::Flags(flags) => Val::Flags(flags.clone()),
        ComponentValue::Unit => Val::Tuple(Vec::new()),
        _ => Val::Bool(false),
    }
}

#[cfg(feature = "v2")]
fn val_to_component_value(val: &Val) -> ComponentValue {
    match val {
        Val::Bool(b) => ComponentValue::Bool(*b),
        Val::U8(v) => ComponentValue::U8(*v),
        Val::U16(v) => ComponentValue::U16(*v),
        Val::U32(v) => ComponentValue::U32(*v),
        Val::U64(v) => ComponentValue::U64(*v),
        Val::S8(v) => ComponentValue::S8(*v),
        Val::S16(v) => ComponentValue::S16(*v),
        Val::S32(v) => ComponentValue::S32(*v),
        Val::S64(v) => ComponentValue::S64(*v),
        Val::Float32(v) => ComponentValue::F32(*v),
        Val::Float64(v) => ComponentValue::F64(*v),
        Val::Char(c) => ComponentValue::Char(*c),
        Val::String(s) => ComponentValue::String(s.clone()),
        Val::List(items) => {
            ComponentValue::List(items.iter().map(val_to_component_value).collect())
        }
        Val::Record(fields) => ComponentValue::Record(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), val_to_component_value(v)))
                .collect(),
        ),
        Val::Tuple(items) => {
            if items.is_empty() {
                ComponentValue::Unit
            } else {
                ComponentValue::Tuple(items.iter().map(val_to_component_value).collect())
            }
        }
        Val::Variant(tag, val) => ComponentValue::Variant {
            tag: tag.clone(),
            value: val.as_ref().map(|v| Box::new(val_to_component_value(v))),
        },
        Val::Option(val) => {
            ComponentValue::Option(val.as_ref().map(|v| Box::new(val_to_component_value(v))))
        }
        Val::Result(res) => match res {
            Ok(ok) => ComponentValue::Result {
                ok: ok.as_ref().map(|v| Box::new(val_to_component_value(v))),
                err: None,
            },
            Err(err) => ComponentValue::Result {
                ok: None,
                err: err.as_ref().map(|v| Box::new(val_to_component_value(v))),
            },
        },
        Val::Enum(tag) => ComponentValue::Enum(tag.clone()),
        Val::Flags(flags) => ComponentValue::Flags(flags.clone()),
        _ => ComponentValue::Unit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instance_state_display() {
        assert_eq!(InstanceState::Running.to_string(), "running");
        assert_eq!(InstanceState::Completed.to_string(), "completed");
    }

    #[test]
    fn test_instance_handle_generation() {
        let h1 = InstanceHandle::new("test");
        let h2 = InstanceHandle::new("test");
        assert_ne!(h1.id, h2.id);
        assert_eq!(h1.component_id, "test");
    }
}
