//! Plugin System
//!
//! Plugins are WASI components that expose hook functions (e.g. on_build).

use crate::v2::config::{PluginConfig, RunConfig};
use crate::v2::registry::{InstallOptions, Registry, RegistryConfig};
use crate::v2::runtime::{CapabilitySet, RuntimeConfig, RuntimeEngine};
use crate::v2::{Error, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluginHook {
    DevStart,
    Build,
    Install,
    Test,
    Deploy,
}

impl PluginHook {
    pub fn function_name(&self) -> &'static str {
        match self {
            PluginHook::DevStart => "on_dev_start",
            PluginHook::Build => "on_build",
            PluginHook::Install => "on_install",
            PluginHook::Test => "on_test",
            PluginHook::Deploy => "on_deploy",
        }
    }
}

pub struct PluginManager {
    runtime: Arc<Mutex<RuntimeEngine>>,
    plugins: HashMap<String, PluginInstance>,
}

struct PluginInstance {
    handle: crate::v2::runtime::InstanceHandle,
    hooks: HashSet<String>,
}

impl PluginManager {
    pub async fn load_all(config: &RunConfig, project_dir: &Path) -> Result<Self> {
        let runtime = Arc::new(Mutex::new(RuntimeEngine::new(RuntimeConfig::production())?));
        let mut plugins = HashMap::new();

        let mut registry = None;
        for (name, plugin) in &config.plugins {
            if !plugin.enabled {
                continue;
            }

            let wasm_path = resolve_plugin_path(config, project_dir, plugin, &mut registry).await?;
            if !wasm_path.exists() {
                return Err(Error::other(format!(
                    "Plugin '{}' not found at {}",
                    name,
                    wasm_path.display()
                )));
            }

            let mut runtime_lock = runtime.lock().unwrap();
            let component_id = runtime_lock.load_component(&wasm_path)?;
            let mut caps = CapabilitySet::deterministic();
            caps.grant(crate::v2::runtime::Capability::Stdout);
            caps.grant(crate::v2::runtime::Capability::Stderr);
            let handle = runtime_lock.instantiate(&component_id, caps)?;

            let hooks = plugin.hooks.iter().cloned().collect::<HashSet<_>>();
            plugins.insert(name.clone(), PluginInstance { handle, hooks });
        }

        Ok(Self { runtime, plugins })
    }

    pub fn run_hook(&self, hook: PluginHook) -> Result<()> {
        let fn_name = hook.function_name();
        for (_name, plugin) in &self.plugins {
            if !plugin.hooks.is_empty() && !plugin.hooks.contains(fn_name) {
                continue;
            }

            let runtime_lock = self.runtime.lock().unwrap();
            match runtime_lock.call(&plugin.handle, fn_name, vec![]) {
                Ok(_) => {}
                Err(err) => {
                    let message = err.to_string();
                    if !message.contains("Function") {
                        return Err(err);
                    }
                }
            }
        }
        Ok(())
    }
}

async fn resolve_plugin_path(
    config: &RunConfig,
    project_dir: &Path,
    plugin: &PluginConfig,
    registry: &mut Option<Registry>,
) -> Result<PathBuf> {
    if let Some(ref path) = plugin.path {
        return Ok(project_dir.join(path));
    }

    if let Some(ref package) = plugin.package {
        let mut registry_config = RegistryConfig::default();
        registry_config.registry_url = config.registry.url.clone();
        registry_config.mirrors = config.registry.mirrors.clone();
        registry_config.auth_token = config.registry.auth_token.clone();

        if registry.is_none() {
            *registry = Some(Registry::new(registry_config, project_dir)?);
        }

        let registry = registry.as_mut().unwrap();
        registry.load_lockfile()?;
        let install_path = registry
            .install(
                package,
                plugin.version.as_deref(),
                InstallOptions {
                    skip_lockfile: true,
                    verify: true,
                    ..Default::default()
                },
            )
            .await?;
        return Ok(install_path);
    }

    Err(Error::other("Plugin must specify path or package"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_names() {
        assert_eq!(PluginHook::Build.function_name(), "on_build");
        assert_eq!(PluginHook::Deploy.function_name(), "on_deploy");
    }
}
