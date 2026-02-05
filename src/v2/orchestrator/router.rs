//! Call Router
//!
//! Routes function calls between components.
//! This is the "nervous system" of cross-component communication.

use crate::v2::runtime::InstanceHandle;
use std::collections::HashMap;
use std::sync::RwLock;
#[derive(Debug, Clone)]
pub struct RouteTarget {
    pub component_id: String,

    pub handle: InstanceHandle,

    pub priority: u32,

    pub weight: u32,
}
pub struct CallRouter {
    routes: RwLock<HashMap<String, Vec<RouteTarget>>>,

    interface_routes: RwLock<HashMap<(String, String), String>>,
}

impl CallRouter {
    pub fn new() -> Self {
        Self {
            routes: RwLock::new(HashMap::new()),
            interface_routes: RwLock::new(HashMap::new()),
        }
    }
    pub fn register(&self, component_id: &str, handle: InstanceHandle) {
        let target = RouteTarget {
            component_id: component_id.to_string(),
            handle,
            priority: 0,
            weight: 1,
        };

        let mut routes = self.routes.write().unwrap();
        routes
            .entry(component_id.to_string())
            .or_default()
            .push(target);
    }
    pub fn register_with_priority(
        &self,
        component_id: &str,
        handle: InstanceHandle,
        priority: u32,
    ) {
        let target = RouteTarget {
            component_id: component_id.to_string(),
            handle,
            priority,
            weight: 1,
        };

        let mut routes = self.routes.write().unwrap();
        let targets = routes.entry(component_id.to_string()).or_default();
        targets.push(target);
        targets.sort_by(|a, b| b.priority.cmp(&a.priority));
    }
    pub fn unregister(&self, component_id: &str) {
        let mut routes = self.routes.write().unwrap();
        routes.remove(component_id);

        let mut interface_routes = self.interface_routes.write().unwrap();
        interface_routes.retain(|_, v| v != component_id);
    }
    pub fn unregister_handle(&self, component_id: &str, handle: &InstanceHandle) {
        let mut routes = self.routes.write().unwrap();
        if let Some(targets) = routes.get_mut(component_id) {
            targets.retain(|t| t.handle.id != handle.id);
            if targets.is_empty() {
                routes.remove(component_id);
            }
        }
    }
    pub fn get_target(&self, component_id: &str) -> Option<InstanceHandle> {
        let routes = self.routes.read().unwrap();
        routes
            .get(component_id)
            .and_then(|targets| targets.first())
            .map(|t| t.handle.clone())
    }
    pub fn get_all_targets(&self, component_id: &str) -> Vec<InstanceHandle> {
        let routes = self.routes.read().unwrap();
        routes
            .get(component_id)
            .map(|targets| targets.iter().map(|t| t.handle.clone()).collect())
            .unwrap_or_default()
    }
    pub fn register_interface(
        &self,
        interface_name: &str,
        function_name: &str,
        component_id: &str,
    ) {
        let mut interface_routes = self.interface_routes.write().unwrap();
        interface_routes.insert(
            (interface_name.to_string(), function_name.to_string()),
            component_id.to_string(),
        );
    }
    pub fn resolve_interface(
        &self,
        interface_name: &str,
        function_name: &str,
    ) -> Option<InstanceHandle> {
        let interface_routes = self.interface_routes.read().unwrap();
        let component_id =
            interface_routes.get(&(interface_name.to_string(), function_name.to_string()))?;
        self.get_target(component_id)
    }
    pub fn is_registered(&self, component_id: &str) -> bool {
        let routes = self.routes.read().unwrap();
        routes.contains_key(component_id)
    }
    pub fn target_count(&self, component_id: &str) -> usize {
        let routes = self.routes.read().unwrap();
        routes.get(component_id).map(|t| t.len()).unwrap_or(0)
    }
    pub fn list_components(&self) -> Vec<String> {
        let routes = self.routes.read().unwrap();
        routes.keys().cloned().collect()
    }
    pub fn clear(&self) {
        let mut routes = self.routes.write().unwrap();
        routes.clear();

        let mut interface_routes = self.interface_routes.write().unwrap();
        interface_routes.clear();
    }
    pub fn stats(&self) -> RouterStats {
        let routes = self.routes.read().unwrap();
        let interface_routes = self.interface_routes.read().unwrap();

        RouterStats {
            component_count: routes.len(),
            total_targets: routes.values().map(|v| v.len()).sum(),
            interface_routes: interface_routes.len(),
        }
    }
}

impl Default for CallRouter {
    fn default() -> Self {
        Self::new()
    }
}
#[derive(Debug, Clone)]
pub struct RouterStats {
    pub component_count: usize,

    pub total_targets: usize,

    pub interface_routes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_handle(id: &str) -> InstanceHandle {
        InstanceHandle {
            id: id.to_string(),
            component_id: id.to_string(),
        }
    }

    #[test]
    fn test_router_basic() {
        let router = CallRouter::new();

        router.register("comp1", make_handle("inst1"));
        assert!(router.is_registered("comp1"));
        assert!(!router.is_registered("comp2"));

        let target = router.get_target("comp1");
        assert!(target.is_some());
        assert_eq!(target.unwrap().id, "inst1");
    }

    #[test]
    fn test_router_unregister() {
        let router = CallRouter::new();

        router.register("comp1", make_handle("inst1"));
        router.unregister("comp1");

        assert!(!router.is_registered("comp1"));
        assert!(router.get_target("comp1").is_none());
    }

    #[test]
    fn test_router_interface() {
        let router = CallRouter::new();

        router.register("calculator", make_handle("calc_inst"));
        router.register_interface("math", "add", "calculator");

        let target = router.resolve_interface("math", "add");
        assert!(target.is_some());
    }
}
