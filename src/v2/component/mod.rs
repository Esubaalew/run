//! High-Level Component API
//!
//! Provides a simple, ergonomic API for working with WASI components.

use crate::v2::runtime::{
    Capability, CapabilitySet, ComponentValue, ExecutionResult, InstanceHandle, LoadedComponent,
    RuntimeEngine,
};
use crate::v2::wit::WitPackage;
use crate::v2::Result;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub struct Component {
    id: String,

    loaded: Arc<LoadedComponent>,

    wit: Option<WitPackage>,

    runtime: Arc<Mutex<RuntimeEngine>>,

    instance: Option<InstanceHandle>,

    capabilities: CapabilitySet,
}

impl Component {
    pub fn from_file(runtime: Arc<Mutex<RuntimeEngine>>, path: &Path) -> Result<Self> {
        let loaded = LoadedComponent::from_file(path)?;
        let id = loaded.id.clone();

        let wit_path = path.with_extension("wit");
        let wit = if wit_path.exists() {
            WitPackage::from_file(&wit_path).ok()
        } else {
            None
        };

        {
            let mut rt = runtime.lock().unwrap();
            rt.load_component_bytes(&id, loaded.bytes.to_vec())?;
        }

        Ok(Self {
            id,
            loaded: Arc::new(loaded),
            wit,
            runtime,
            instance: None,
            capabilities: CapabilitySet::cli_default(),
        })
    }

    pub fn from_bytes(
        runtime: Arc<Mutex<RuntimeEngine>>,
        id: &str,
        bytes: Vec<u8>,
    ) -> Result<Self> {
        let loaded = LoadedComponent::from_bytes(id, bytes.clone());

        {
            let mut rt = runtime.lock().unwrap();
            rt.load_component_bytes(id, bytes)?;
        }

        Ok(Self {
            id: id.to_string(),
            loaded: Arc::new(loaded),
            wit: None,
            runtime,
            instance: None,
            capabilities: CapabilitySet::cli_default(),
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn hash(&self) -> &str {
        &self.loaded.hash
    }

    pub fn with_capabilities(mut self, capabilities: CapabilitySet) -> Self {
        self.capabilities = capabilities;
        self
    }

    pub fn grant(&mut self, capability: Capability) {
        self.capabilities.grant(capability);
    }

    pub fn instantiate(&mut self) -> Result<()> {
        if self.instance.is_some() {
            return Ok(()); // Already instantiated
        }

        let runtime = self.runtime.lock().unwrap();
        let handle = runtime.instantiate(&self.id, self.capabilities.clone())?;
        self.instance = Some(handle);
        Ok(())
    }

    pub fn call(&mut self, function: &str, args: Vec<ComponentValue>) -> Result<ExecutionResult> {
        if self.instance.is_none() {
            self.instantiate()?;
        }

        let handle = self.instance.as_ref().unwrap();
        let runtime = self.runtime.lock().unwrap();
        runtime.call(handle, function, args)
    }

    pub fn run(&mut self, _args: Vec<String>) -> Result<ExecutionResult> {
        self.call("_start", vec![])
            .or_else(|_| self.call("main", vec![]))
    }

    pub fn wit(&self) -> Option<&WitPackage> {
        self.wit.as_ref()
    }

    pub fn exports(&self) -> Vec<String> {
        self.wit
            .as_ref()
            .map(|w| {
                w.interfaces
                    .values()
                    .flat_map(|i| i.functions.keys().cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn terminate(&mut self) -> Result<()> {
        if let Some(handle) = self.instance.take() {
            let runtime = self.runtime.lock().unwrap();
            runtime.terminate(&handle)?;
        }
        Ok(())
    }
}

impl Drop for Component {
    fn drop(&mut self) {
        let _ = self.terminate();
    }
}

pub struct ComponentBuilder {
    runtime: Arc<Mutex<RuntimeEngine>>,
    capabilities: CapabilitySet,
}

impl ComponentBuilder {
    pub fn new(runtime: Arc<Mutex<RuntimeEngine>>) -> Self {
        Self {
            runtime,
            capabilities: CapabilitySet::cli_default(),
        }
    }

    pub fn with_capability(mut self, cap: Capability) -> Self {
        self.capabilities.grant(cap);
        self
    }

    pub fn with_file_access(mut self, path: &Path) -> Self {
        self.capabilities
            .grant(Capability::FileRead(path.to_path_buf()));
        self.capabilities
            .grant(Capability::FileWrite(path.to_path_buf()));
        self
    }

    pub fn with_network(mut self, host: &str, port: u16) -> Self {
        self.capabilities.grant(Capability::NetConnect {
            host: host.to_string(),
            port,
        });
        self
    }

    pub fn from_file(self, path: &Path) -> Result<Component> {
        let mut component = Component::from_file(self.runtime, path)?;
        component.capabilities = self.capabilities;
        Ok(component)
    }

    pub fn from_bytes(self, id: &str, bytes: Vec<u8>) -> Result<Component> {
        let mut component = Component::from_bytes(self.runtime, id, bytes)?;
        component.capabilities = self.capabilities;
        Ok(component)
    }
}

pub fn load(path: &Path) -> Result<Component> {
    let runtime = Arc::new(Mutex::new(RuntimeEngine::default_engine()?));
    Component::from_file(runtime, path)
}

pub fn run(path: &Path, args: Vec<String>) -> Result<i32> {
    let mut component = load(path)?;
    let result = component.run(args)?;
    Ok(result.exit_code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_builder() {
        let runtime = Arc::new(Mutex::new(RuntimeEngine::default_engine().unwrap()));
        let _builder = ComponentBuilder::new(runtime)
            .with_capability(Capability::Stdout)
            .with_capability(Capability::Clock);

        assert!(true);
    }
}
