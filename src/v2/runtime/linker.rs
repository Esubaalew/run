use crate::v2::wit::{WitInterface, WitInterfaceRef, WitPackage, WitResults, WitType};
use crate::v2::{Error, Result};
use std::collections::HashMap;

#[cfg(feature = "v2")]
use super::wasi_ctx::WasiHostState;
#[cfg(feature = "v2")]
use wasmtime::component::Linker as WasmtimeLinker;

#[derive(Debug)]
pub enum LinkageError {
    UnsatisfiedImport {
        component: String,
        interface: String,
    },
    CircularDependency {
        cycle: Vec<String>,
    },
    TypeMismatch {
        component: String,
        interface: String,
        expected: String,
        actual: String,
    },
}

impl std::fmt::Display for LinkageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LinkageError::UnsatisfiedImport {
                component,
                interface,
            } => {
                write!(
                    f,
                    "Component '{}' has unsatisfied import: {}",
                    component, interface
                )
            }
            LinkageError::CircularDependency { cycle } => {
                write!(f, "Circular dependency detected: {}", cycle.join(" -> "))
            }
            LinkageError::TypeMismatch {
                component,
                interface,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "Type mismatch in '{}' for interface '{}': expected {}, got {}",
                    component, interface, expected, actual
                )
            }
        }
    }
}

impl std::error::Error for LinkageError {}

#[derive(Debug, Clone)]
struct ExportEntry {
    component_id: String,
    interface: WitInterface,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ImportEntry {
    component_id: String,
    import_name: String,
    interface_ref: WitInterfaceRef,
}

pub struct ComponentLinker {
    exports: HashMap<String, ExportEntry>,
    pending_imports: HashMap<String, Vec<ImportEntry>>,
    resolved_links: HashMap<(String, String), String>,
}

impl ComponentLinker {
    pub fn new() -> Self {
        Self {
            exports: HashMap::new(),
            pending_imports: HashMap::new(),
            resolved_links: HashMap::new(),
        }
    }

    pub fn register_exports(&mut self, component_id: &str, wit: &WitPackage) -> Result<()> {
        let package_id = wit.id.to_string();

        for (name, interface) in &wit.interfaces {
            let export_key = format!("{}/{}", package_id, name);

            self.exports.insert(
                export_key,
                ExportEntry {
                    component_id: component_id.to_string(),
                    interface: interface.clone(),
                },
            );
        }

        for world in wit.worlds.values() {
            for export in &world.exports {
                if let crate::v2::wit::WitWorldItem::Interface { name, interface } = export {
                    match interface {
                        WitInterfaceRef::Local(local_name) => {
                            if let Some(iface) = wit.interfaces.get(local_name) {
                                let export_key = format!("{}/{}", package_id, name);
                                self.exports.insert(
                                    export_key,
                                    ExportEntry {
                                        component_id: component_id.to_string(),
                                        interface: iface.clone(),
                                    },
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(())
    }

    pub fn register_imports(&mut self, component_id: &str, wit: &WitPackage) -> Result<()> {
        let mut imports = Vec::new();

        for world in wit.worlds.values() {
            for import in &world.imports {
                if let crate::v2::wit::WitWorldItem::Interface { name, interface } = import {
                    imports.push(ImportEntry {
                        component_id: component_id.to_string(),
                        import_name: name.clone(),
                        interface_ref: interface.clone(),
                    });
                }
            }
        }

        if !imports.is_empty() {
            self.pending_imports
                .insert(component_id.to_string(), imports);
        }

        Ok(())
    }

    pub fn resolve_imports(&mut self, component_id: &str, wit: &WitPackage) -> Result<()> {
        self.register_imports(component_id, wit)?;

        if let Some(imports) = self.pending_imports.get(component_id) {
            for import in imports.clone() {
                let export_key = match &import.interface_ref {
                    WitInterfaceRef::Local(name) => {
                        format!("{}/{}", wit.id.to_string(), name)
                    }
                    WitInterfaceRef::External { package, interface } => {
                        format!("{}/{}", package.to_string(), interface)
                    }
                };

                if let Some(export) = self.exports.get(&export_key) {
                    if let WitInterfaceRef::Local(local_name) = &import.interface_ref {
                        if let Some(import_iface) = wit.interfaces.get(local_name) {
                            self.check_interface_compatibility(&export.interface, import_iface)
                                .map_err(|e| Error::other(e.to_string()))?;
                        }
                    }
                    if let Some(import_entries) = self.pending_imports.get(component_id) {
                        for import_entry in import_entries {
                            if import_entry.import_name == import.import_name {}
                        }
                    }

                    self.resolved_links.insert(
                        (component_id.to_string(), import.import_name.clone()),
                        export.component_id.clone(),
                    );
                }
            }
        }

        Ok(())
    }

    pub fn check_satisfied(&self, component_id: &str) -> std::result::Result<(), LinkageError> {
        if let Some(imports) = self.pending_imports.get(component_id) {
            for import in imports {
                let key = (component_id.to_string(), import.import_name.clone());
                if !self.resolved_links.contains_key(&key) {
                    return Err(LinkageError::UnsatisfiedImport {
                        component: component_id.to_string(),
                        interface: import.import_name.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    pub fn get_provider(&self, component_id: &str, import_name: &str) -> Option<&str> {
        self.resolved_links
            .get(&(component_id.to_string(), import_name.to_string()))
            .map(|s| s.as_str())
    }

    pub fn instantiation_order(&self) -> std::result::Result<Vec<String>, LinkageError> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut dependents: HashMap<String, Vec<String>> = HashMap::new();

        for component_id in self.pending_imports.keys() {
            in_degree.entry(component_id.clone()).or_insert(0);
        }
        for export in self.exports.values() {
            in_degree.entry(export.component_id.clone()).or_insert(0);
        }

        for ((importer, _), provider) in &self.resolved_links {
            if importer != provider {
                *in_degree.entry(importer.clone()).or_insert(0) += 1;
                dependents
                    .entry(provider.clone())
                    .or_default()
                    .push(importer.clone());
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

        if result.len() != in_degree.len() {
            let remaining: Vec<_> = in_degree
                .iter()
                .filter(|&(_, deg)| *deg > 0)
                .map(|(id, _)| id.clone())
                .collect();
            return Err(LinkageError::CircularDependency { cycle: remaining });
        }

        Ok(result)
    }

    pub fn list_exports(&self) -> Vec<(&str, &str)> {
        self.exports
            .iter()
            .map(|(key, entry)| (key.as_str(), entry.component_id.as_str()))
            .collect()
    }

    pub fn list_links(&self) -> Vec<(&str, &str, &str)> {
        self.resolved_links
            .iter()
            .map(|((importer, import), provider)| {
                (importer.as_str(), import.as_str(), provider.as_str())
            })
            .collect()
    }

    pub fn clear(&mut self) {
        self.exports.clear();
        self.pending_imports.clear();
        self.resolved_links.clear();
    }
}

impl Default for ComponentLinker {
    fn default() -> Self {
        Self::new()
    }
}

impl ComponentLinker {
    pub fn check_interface_compatibility(
        &self,
        exporter: &WitInterface,
        importer: &WitInterface,
    ) -> std::result::Result<(), LinkageError> {
        for (func_name, required_func) in &importer.functions {
            let provided_func =
                exporter
                    .functions
                    .get(func_name)
                    .ok_or_else(|| LinkageError::TypeMismatch {
                        component: "linker".to_string(),
                        interface: importer.name.clone(),
                        expected: format!("function {}", func_name),
                        actual: "missing".to_string(),
                    })?;

            if required_func.params.len() != provided_func.params.len() {
                return Err(LinkageError::TypeMismatch {
                    component: "linker".to_string(),
                    interface: importer.name.clone(),
                    expected: format!("{} params for {}", required_func.params.len(), func_name),
                    actual: format!("{} params", provided_func.params.len()),
                });
            }

            for (i, (req_param, prov_param)) in required_func
                .params
                .iter()
                .zip(provided_func.params.iter())
                .enumerate()
            {
                if !self.types_compatible(&req_param.ty, &prov_param.ty) {
                    return Err(LinkageError::TypeMismatch {
                        component: "linker".to_string(),
                        interface: importer.name.clone(),
                        expected: format!("param {} type {:?}", i, req_param.ty),
                        actual: format!("{:?}", prov_param.ty),
                    });
                }
            }

            if !self.results_compatible(&required_func.results, &provided_func.results) {
                return Err(LinkageError::TypeMismatch {
                    component: "linker".to_string(),
                    interface: importer.name.clone(),
                    expected: format!("{} return type", func_name),
                    actual: "incompatible return type".to_string(),
                });
            }
        }

        Ok(())
    }

    fn types_compatible(&self, expected: &WitType, actual: &WitType) -> bool {
        expected == actual
    }

    fn results_compatible(&self, expected: &WitResults, actual: &WitResults) -> bool {
        match (expected, actual) {
            (WitResults::None, WitResults::None) => true,
            (WitResults::Anon(e), WitResults::Anon(a)) => self.types_compatible(e, a),
            (WitResults::Named(e), WitResults::Named(a)) => {
                if e.len() != a.len() {
                    return false;
                }
                e.iter()
                    .zip(a.iter())
                    .all(|(ep, ap)| ep.name == ap.name && self.types_compatible(&ep.ty, &ap.ty))
            }
            _ => false,
        }
    }

    pub fn validate_all_links(&self) -> Vec<LinkageError> {
        let errors = Vec::new();

        for ((importer, import_name), _provider) in &self.resolved_links {
            if let Some(import_entries) = self.pending_imports.get(importer) {
                for import in import_entries {
                    if &import.import_name == import_name {
                        let export_key = match &import.interface_ref {
                            WitInterfaceRef::Local(name) => name.clone(),
                            WitInterfaceRef::External { package, interface } => {
                                format!("{}/{}", package.to_string(), interface)
                            }
                        };

                        if let Some(_export) = self.exports.get(&export_key) {}
                    }
                }
            }
        }

        errors
    }

    #[cfg(feature = "v2")]
    pub fn create_wasmtime_linker(
        &self,
        engine: &wasmtime::Engine,
    ) -> wasmtime::Result<WasmtimeLinker<WasiHostState>> {
        let mut linker = WasmtimeLinker::<WasiHostState>::new(engine);

        wasmtime_wasi::add_to_linker_sync(&mut linker)?;

        Ok(linker)
    }

    pub fn get_resolved_imports(&self, component_id: &str) -> Vec<(String, String)> {
        self.resolved_links
            .iter()
            .filter(|((importer, _), _)| importer == component_id)
            .map(|((_, import_name), provider)| (import_name.clone(), provider.clone()))
            .collect()
    }

    pub fn get_import_provider(&self, component_id: &str, import_name: &str) -> Option<String> {
        self.resolved_links
            .get(&(component_id.to_string(), import_name.to_string()))
            .cloned()
    }

    #[cfg(feature = "v2")]
    pub fn validate_wasmtime_linkage(
        &self,
        engine: &wasmtime::Engine,
        components: &std::collections::HashMap<String, wasmtime::component::Component>,
    ) -> std::result::Result<(), LinkageError> {
        let linker =
            self.create_wasmtime_linker(engine)
                .map_err(|e| LinkageError::UnsatisfiedImport {
                    component: "wasmtime".to_string(),
                    interface: format!("WASI setup failed: {}", e),
                })?;

        let order = self.instantiation_order()?;

        for component_id in &order {
            if let Some(_component) = components.get(component_id) {}
        }

        let _ = linker;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linker_creation() {
        let linker = ComponentLinker::new();
        assert!(linker.exports.is_empty());
        assert!(linker.pending_imports.is_empty());
    }
}
