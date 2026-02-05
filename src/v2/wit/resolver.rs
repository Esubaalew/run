//! WIT Interface Resolver
//!
//! Resolves WIT interface references and validates component compatibility.

use super::*;
use crate::v2::{Error, Result};
use std::collections::HashMap;

pub struct WitResolver {
    packages: HashMap<String, WitPackage>,
}

impl WitResolver {
    pub fn new() -> Self {
        Self {
            packages: HashMap::new(),
        }
    }

    pub fn register_package(&mut self, package: WitPackage) {
        let id = package.id.to_string();
        self.packages.insert(id, package);
    }

    pub fn resolve_interface(&self, reference: &WitInterfaceRef) -> Result<&WitInterface> {
        match reference {
            WitInterfaceRef::Local(name) => Err(Error::WitInterfaceNotFound {
                interface: format!("local:{}", name),
            }),
            WitInterfaceRef::External { package, interface } => {
                let pkg_id = package.to_string();
                let pkg =
                    self.packages
                        .get(&pkg_id)
                        .ok_or_else(|| Error::WitInterfaceNotFound {
                            interface: format!("{}/{}", pkg_id, interface),
                        })?;

                pkg.get_interface(interface)
                    .ok_or_else(|| Error::WitInterfaceNotFound {
                        interface: format!("{}/{}", pkg_id, interface),
                    })
            }
        }
    }

    pub fn can_satisfy(&self, exporter: &WitInterface, importer: &WitInterface) -> Result<()> {
        for (func_name, required_func) in &importer.functions {
            let provided_func =
                exporter
                    .functions
                    .get(func_name)
                    .ok_or_else(|| Error::WitIncompatible {
                        from: exporter.name.clone(),
                        to: format!("{}::{}", importer.name, func_name),
                    })?;

            if required_func.params.len() != provided_func.params.len() {
                return Err(Error::WitTypeMismatch {
                    expected: format!("{} params", required_func.params.len()),
                    actual: format!("{} params", provided_func.params.len()),
                });
            }

            for (req, prov) in required_func.params.iter().zip(&provided_func.params) {
                if !super::types::types_compatible(&req.ty, &prov.ty) {
                    return Err(Error::WitTypeMismatch {
                        expected: super::types::type_to_string(&req.ty),
                        actual: super::types::type_to_string(&prov.ty),
                    });
                }
            }

            match (&required_func.results, &provided_func.results) {
                (WitResults::None, WitResults::None) => {}
                (WitResults::Anon(req), WitResults::Anon(prov)) => {
                    if !super::types::types_compatible(req, prov) {
                        return Err(Error::WitTypeMismatch {
                            expected: super::types::type_to_string(req),
                            actual: super::types::type_to_string(prov),
                        });
                    }
                }
                (WitResults::Named(req), WitResults::Named(prov)) => {
                    if req.len() != prov.len() {
                        return Err(Error::WitTypeMismatch {
                            expected: format!("{} return values", req.len()),
                            actual: format!("{} return values", prov.len()),
                        });
                    }
                }
                _ => {
                    return Err(Error::WitTypeMismatch {
                        expected: "matching return types".to_string(),
                        actual: "mismatched return types".to_string(),
                    });
                }
            }
        }

        Ok(())
    }

    pub fn resolve_world_imports(&self, world: &WitWorld) -> Result<Vec<(String, &WitInterface)>> {
        let mut resolved = Vec::new();

        for import in &world.imports {
            if let WitWorldItem::Interface { name, interface } = import {
                let resolved_interface = self.resolve_interface(interface)?;
                resolved.push((name.clone(), resolved_interface));
            }
        }

        Ok(resolved)
    }

    pub fn build_dependency_graph(&self, components: &[WitPackage]) -> Result<DependencyGraph> {
        let mut graph = DependencyGraph::new();

        for component in components {
            let component_id = component.id.to_string();
            graph.add_node(component_id.clone());

            for world in component.worlds.values() {
                for import in &world.imports {
                    if let WitWorldItem::Interface { interface, .. } = import {
                        if let WitInterfaceRef::External { package, .. } = interface {
                            let dep_id = package.to_string();
                            graph.add_edge(&component_id, &dep_id);
                        }
                    }
                }
            }
        }

        if graph.has_cycle() {
            return Err(Error::DependencyCycle {
                cycle: graph.get_cycle().unwrap_or_default(),
            });
        }

        Ok(graph)
    }
}

impl Default for WitResolver {
    fn default() -> Self {
        Self::new()
    }
}

pub struct DependencyGraph {
    nodes: Vec<String>,
    edges: Vec<(usize, usize)>, // (from, to)
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    fn add_node(&mut self, id: String) {
        if !self.nodes.contains(&id) {
            self.nodes.push(id);
        }
    }

    fn add_edge(&mut self, from: &str, to: &str) {
        let from_idx = self.nodes.iter().position(|n| n == from);
        let to_idx = self.nodes.iter().position(|n| n == to);

        if let (Some(from_idx), Some(to_idx)) = (from_idx, to_idx) {
            self.edges.push((from_idx, to_idx));
        }
    }

    pub fn has_cycle(&self) -> bool {
        let n = self.nodes.len();
        let mut visited = vec![false; n];
        let mut rec_stack = vec![false; n];

        for i in 0..n {
            if self.detect_cycle(i, &mut visited, &mut rec_stack) {
                return true;
            }
        }
        false
    }

    fn detect_cycle(&self, node: usize, visited: &mut [bool], rec_stack: &mut [bool]) -> bool {
        if rec_stack[node] {
            return true;
        }
        if visited[node] {
            return false;
        }

        visited[node] = true;
        rec_stack[node] = true;

        for &(from, to) in &self.edges {
            if from == node && self.detect_cycle(to, visited, rec_stack) {
                return true;
            }
        }

        rec_stack[node] = false;
        false
    }

    pub fn get_cycle(&self) -> Option<String> {
        let n = self.nodes.len();
        let mut visited = vec![false; n];
        let mut rec_stack = vec![false; n];
        let mut path = Vec::new();

        for i in 0..n {
            if self.find_cycle_path(i, &mut visited, &mut rec_stack, &mut path) {
                return Some(
                    path.iter()
                        .map(|&idx| self.nodes[idx].as_str())
                        .collect::<Vec<_>>()
                        .join(" -> "),
                );
            }
        }
        None
    }

    fn find_cycle_path(
        &self,
        node: usize,
        visited: &mut [bool],
        rec_stack: &mut [bool],
        path: &mut Vec<usize>,
    ) -> bool {
        if rec_stack[node] {
            path.push(node);
            return true;
        }
        if visited[node] {
            return false;
        }

        visited[node] = true;
        rec_stack[node] = true;
        path.push(node);

        for &(from, to) in &self.edges {
            if from == node && self.find_cycle_path(to, visited, rec_stack, path) {
                return true;
            }
        }

        rec_stack[node] = false;
        path.pop();
        false
    }

    pub fn topological_order(&self) -> Vec<&str> {
        let n = self.nodes.len();
        let mut in_degree = vec![0usize; n];
        let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); n];

        // Edges are (component -> dependency), so dependency must come first.
        for &(from, to) in &self.edges {
            in_degree[from] += 1;
            dependents[to].push(from);
        }

        let mut queue: Vec<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
        let mut result = Vec::new();

        while let Some(node) = queue.pop() {
            result.push(self.nodes[node].as_str());
            for &dep in &dependents[node] {
                in_degree[dep] -= 1;
                if in_degree[dep] == 0 {
                    queue.push(dep);
                }
            }
        }
        result
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dependency_graph_no_cycle() {
        let mut graph = DependencyGraph::new();
        graph.add_node("a".to_string());
        graph.add_node("b".to_string());
        graph.add_node("c".to_string());
        graph.add_edge("a", "b");
        graph.add_edge("b", "c");

        assert!(!graph.has_cycle());
    }

    #[test]
    fn test_dependency_graph_with_cycle() {
        let mut graph = DependencyGraph::new();
        graph.add_node("a".to_string());
        graph.add_node("b".to_string());
        graph.add_node("c".to_string());
        graph.add_edge("a", "b");
        graph.add_edge("b", "c");
        graph.add_edge("c", "a");

        assert!(graph.has_cycle());
    }

    #[test]
    fn test_topological_order() {
        let mut graph = DependencyGraph::new();
        graph.add_node("app".to_string());
        graph.add_node("lib".to_string());
        graph.add_node("core".to_string());
        graph.add_edge("app", "lib");
        graph.add_edge("lib", "core");

        let order = graph.topological_order();
        let core_pos = order.iter().position(|&n| n == "core").unwrap();
        let lib_pos = order.iter().position(|&n| n == "lib").unwrap();
        let app_pos = order.iter().position(|&n| n == "app").unwrap();

        assert!(core_pos < lib_pos);
        assert!(lib_pos < app_pos);
    }
}
