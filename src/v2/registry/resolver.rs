//! Dependency Resolver
//!
//! Resolves component dependencies using a SAT-solver-like approach.

use super::*;
use crate::v2::{Error, Result};
use semver::{Version, VersionReq};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone)]
pub struct ResolvedDependency {
    pub name: String,

    pub version: Version,

    pub sha256: String,

    pub dependencies: Vec<String>,
}

pub struct DependencyResolver {
    version_cache: HashMap<String, Vec<Version>>,
}

impl DependencyResolver {
    pub fn new() -> Self {
        Self {
            version_cache: HashMap::new(),
        }
    }

    pub async fn resolve(
        &mut self,
        client: &RegistryClient,
        dependencies: &[Dependency],
    ) -> Result<Vec<ResolvedDependency>> {
        let mut constraints: HashMap<String, VersionReq> = HashMap::new();
        let mut to_resolve: VecDeque<String> = VecDeque::new();

        for dep in dependencies {
            constraints.insert(dep.name.clone(), dep.version_req.clone());
            to_resolve.push_back(dep.name.clone());
        }

        let mut resolved: HashMap<String, ResolvedDependency> = HashMap::new();
        let mut visited: HashSet<String> = HashSet::new();

        while let Some(name) = to_resolve.pop_front() {
            if visited.contains(&name) {
                continue;
            }
            visited.insert(name.clone());

            let constraint = constraints.get(&name).cloned().unwrap_or(VersionReq::STAR);

            let versions = self.get_versions(client, &name).await?;

            let version = versions
                .iter()
                .filter(|v| constraint.matches(v))
                .max()
                .cloned()
                .ok_or_else(|| Error::VersionResolutionFailed {
                    package: name.clone(),
                    reason: format!("No version satisfies {}", constraint),
                })?;

            let package = client.get_version_info(&name, &version).await?;

            for dep in &package.dependencies {
                if !visited.contains(&dep.name) {
                    let existing = constraints.get(&dep.name);
                    let merged = match existing {
                        Some(existing_req) => {
                            if is_more_restrictive(&dep.version_req, existing_req) {
                                dep.version_req.clone()
                            } else {
                                existing_req.clone()
                            }
                        }
                        None => dep.version_req.clone(),
                    };
                    constraints.insert(dep.name.clone(), merged);
                    to_resolve.push_back(dep.name.clone());
                }
            }

            resolved.insert(
                name.clone(),
                ResolvedDependency {
                    name: name.clone(),
                    version,
                    sha256: package.sha256.clone(),
                    dependencies: package
                        .dependencies
                        .iter()
                        .map(|d| d.name.clone())
                        .collect(),
                },
            );
        }

        Ok(self.topological_sort(resolved))
    }

    async fn get_versions(&mut self, client: &RegistryClient, name: &str) -> Result<Vec<Version>> {
        if let Some(versions) = self.version_cache.get(name) {
            return Ok(versions.clone());
        }

        let versions = client.get_versions(name).await?;
        self.version_cache
            .insert(name.to_string(), versions.clone());
        Ok(versions)
    }

    fn topological_sort(
        &self,
        resolved: HashMap<String, ResolvedDependency>,
    ) -> Vec<ResolvedDependency> {
        let mut result = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut temp_visited: HashSet<String> = HashSet::new();

        fn visit(
            name: &str,
            resolved: &HashMap<String, ResolvedDependency>,
            visited: &mut HashSet<String>,
            temp_visited: &mut HashSet<String>,
            result: &mut Vec<ResolvedDependency>,
        ) {
            if visited.contains(name) {
                return;
            }
            if temp_visited.contains(name) {
                return; // Cycle - skip for now
            }

            temp_visited.insert(name.to_string());

            if let Some(dep) = resolved.get(name) {
                for child in &dep.dependencies {
                    visit(child, resolved, visited, temp_visited, result);
                }
                visited.insert(name.to_string());
                result.push(dep.clone());
            }

            temp_visited.remove(name);
        }

        for name in resolved.keys() {
            visit(
                name,
                &resolved,
                &mut visited,
                &mut temp_visited,
                &mut result,
            );
        }

        result
    }

    pub fn clear_cache(&mut self) {
        self.version_cache.clear();
    }
}

impl Default for DependencyResolver {
    fn default() -> Self {
        Self::new()
    }
}

fn is_more_restrictive(a: &VersionReq, b: &VersionReq) -> bool {
    a.to_string().len() > b.to_string().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolver_creation() {
        let resolver = DependencyResolver::new();
        assert!(resolver.version_cache.is_empty());
    }
}
