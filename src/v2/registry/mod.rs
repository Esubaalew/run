//! Registry

mod cache;
mod client;
mod local;
mod lockfile;
mod resolver;

pub use cache::{CacheConfig, ComponentCache};
pub use client::{RegistryClient, RegistryConfig};
pub use local::{LocalRegistry, LocalRegistryConfig, PackageMetadata};
pub use lockfile::{LockedComponent, Lockfile, compute_sha256};
pub use resolver::{DependencyResolver, ResolvedDependency};

use crate::v2::{Error, Result};
use semver::{Version, VersionReq};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
#[derive(Debug, Clone)]
pub struct ComponentPackage {
    pub name: String,

    pub version: Version,

    pub description: String,

    pub sha256: String,

    pub download_url: String,

    pub wit_url: Option<String>,

    pub dependencies: Vec<Dependency>,

    pub targets: Vec<String>,

    pub license: Option<String>,

    pub repository: Option<String>,

    pub size: usize,

    pub published_at: u64,
}
#[derive(Debug, Clone)]
pub struct Dependency {
    pub name: String,

    pub version_req: VersionReq,

    pub optional: bool,

    pub features: Vec<String>,
}

impl Dependency {
    pub fn new(name: &str, version_req: &str) -> Result<Self> {
        let version_req = VersionReq::parse(version_req)
            .map_err(|e| Error::other(format!("Invalid version requirement: {}", e)))?;
        Ok(Self {
            name: name.to_string(),
            version_req,
            optional: false,
            features: vec![],
        })
    }

    pub fn optional(mut self) -> Self {
        self.optional = true;
        self
    }

    pub fn with_features(mut self, features: Vec<String>) -> Self {
        self.features = features;
        self
    }
}
#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub install_dir: Option<PathBuf>,

    pub skip_lockfile: bool,

    pub force: bool,

    pub offline: bool,

    pub verify: bool,

    pub dev: bool,
}

impl Default for InstallOptions {
    fn default() -> Self {
        Self {
            install_dir: None,
            skip_lockfile: false,
            force: false,
            offline: false,
            verify: false,
            dev: false,
        }
    }
}
pub struct Registry {
    client: RegistryClient,

    local: Option<LocalRegistry>,

    cache: ComponentCache,

    resolver: DependencyResolver,

    lockfile: Option<Lockfile>,

    base_dir: PathBuf,

    always_verify: bool,
}

impl Registry {
    pub fn new(config: RegistryConfig, base_dir: &Path) -> Result<Self> {
        let cache_dir = base_dir.join(".run").join("cache");
        let cache = ComponentCache::new(CacheConfig {
            cache_dir: cache_dir.clone(),
            max_size: 1024 * 1024 * 1024,
        })?;

        let local = LocalRegistry::open_default().ok();

        Ok(Self {
            client: RegistryClient::new(config),
            local,
            cache,
            resolver: DependencyResolver::new(),
            lockfile: None,
            base_dir: base_dir.to_path_buf(),
            always_verify: true,
        })
    }
    pub fn new_local_only(base_dir: &Path) -> Result<Self> {
        let cache_dir = base_dir.join(".run").join("cache");
        let cache = ComponentCache::new(CacheConfig {
            cache_dir: cache_dir.clone(),
            max_size: 1024 * 1024 * 1024,
        })?;

        let local = LocalRegistry::open_default()?;

        Ok(Self {
            client: RegistryClient::new(RegistryConfig::default()),
            local: Some(local),
            cache,
            resolver: DependencyResolver::new(),
            lockfile: None,
            base_dir: base_dir.to_path_buf(),
            always_verify: true,
        })
    }
    pub fn local_registry_mut(&mut self) -> Option<&mut LocalRegistry> {
        self.local.as_mut()
    }
    pub fn local_registry(&self) -> Option<&LocalRegistry> {
        self.local.as_ref()
    }
    pub fn load_lockfile(&mut self) -> Result<()> {
        let lockfile_path = self.base_dir.join("run.lock");
        if lockfile_path.exists() {
            let lockfile = Lockfile::load(&lockfile_path)?;
            if !lockfile.verify() {
                return Err(Error::LockfileConflict {
                    reason: "Lockfile checksum mismatch".to_string(),
                });
            }
            self.lockfile = Some(lockfile);
        }
        Ok(())
    }
    pub fn save_lockfile(&self) -> Result<()> {
        if let Some(ref lockfile) = self.lockfile {
            let lockfile_path = self.base_dir.join("run.lock");
            lockfile.save(&lockfile_path)?;
        }
        Ok(())
    }
    pub async fn install(
        &mut self,
        name: &str,
        version: Option<&str>,
        options: InstallOptions,
    ) -> Result<PathBuf> {
        let version_req = match version {
            Some(v) => {
                VersionReq::parse(v).map_err(|e| Error::other(format!("Invalid version: {}", e)))?
            }
            None => VersionReq::STAR,
        };

        let install_dir = options
            .install_dir
            .unwrap_or_else(|| self.base_dir.join(".run").join("components"));
        std::fs::create_dir_all(&install_dir)?;

        let locked_version = self
            .lockfile
            .as_ref()
            .and_then(|l| l.get(name))
            .filter(|locked| version_req.matches(&locked.version))
            .map(|locked| (locked.version.clone(), locked.sha256.clone()));

        if let Some((ref ver, ref expected_hash)) = locked_version {
            let exact_req = VersionReq::exact(ver);
            if let Some(cached_path) = self.cache.get(name, &exact_req)? {
                if self.always_verify || options.verify {
                    let bytes = std::fs::read(&cached_path)?;
                    let actual_hash = compute_sha256(&bytes);
                    if &actual_hash != expected_hash {
                        self.cache.remove_version(name, ver)?;
                    } else if !options.force {
                        return copy_to_install_dir(&install_dir, name, ver, &cached_path);
                    }
                } else if !options.force {
                    return copy_to_install_dir(&install_dir, name, ver, &cached_path);
                }
            }
        }

        if let Some(ref local) = self.local {
            if let Ok(versions) = local.get_versions(name) {
                for ver in versions.iter().rev() {
                    if version_req.matches(ver) {
                        if let Ok(bytes) = local.get_component_verified(name, ver) {
                            let hash = compute_sha256(&bytes);

                            let _cached_path = self.cache.store(name, ver, &bytes)?;
                            let installed_path =
                                write_to_install_dir(&install_dir, name, ver, &bytes)?;

                            if !options.skip_lockfile {
                                let mut lockfile =
                                    self.lockfile.take().unwrap_or_else(Lockfile::new);
                                lockfile.add(LockedComponent {
                                    name: name.to_string(),
                                    version: ver.clone(),
                                    sha256: hash,
                                    dependencies: vec![],
                                });
                                self.lockfile = Some(lockfile);
                                self.save_lockfile()?;
                            }

                            return Ok(installed_path);
                        }
                    }
                }
            }
        }

        if options.offline {
            if let Some(cached) = self.cache.get(name, &version_req)? {
                if let Some(version) = parse_version_from_cached(&cached) {
                    return copy_to_install_dir(&install_dir, name, &version, &cached);
                }
                return Ok(cached);
            }
            return Err(Error::other(
                "Component not found in cache or local registry (offline mode)",
            ));
        }

        let resolved = self
            .resolver
            .resolve(
                &self.client,
                &[Dependency::new(name, &version_req.to_string())?],
            )
            .await?;

        let mut installed_version = Version::new(0, 0, 0);
        let mut installed_path: Option<PathBuf> = None;

        for dep in &resolved {
            if !options.force {
                let version_req = VersionReq::exact(&dep.version);
                if self.cache.get(&dep.name, &version_req)?.is_some() {
                    if dep.name == name {
                        installed_version = dep.version.clone();
                        let cached = self.cache.get(&dep.name, &version_req)?.unwrap();
                        installed_path = Some(copy_to_install_dir(
                            &install_dir,
                            name,
                            &dep.version,
                            &cached,
                        )?);
                    }
                    continue;
                }
            }

            let component_bytes = self.client.download(&dep.name, &dep.version).await?;

            let actual_hash = compute_sha256(&component_bytes);
            if self.always_verify || options.verify {
                if actual_hash != dep.sha256 {
                    return Err(Error::HashMismatch {
                        package: dep.name.clone(),
                        expected: dep.sha256.clone(),
                        actual: actual_hash,
                    });
                }
            }

            self.cache
                .store(&dep.name, &dep.version, &component_bytes)?;

            if dep.name == name {
                installed_version = dep.version.clone();
                installed_path = Some(write_to_install_dir(
                    &install_dir,
                    name,
                    &dep.version,
                    &component_bytes,
                )?);
            }
        }

        if !options.skip_lockfile {
            let mut lockfile = self.lockfile.take().unwrap_or_else(Lockfile::new);
            for dep in resolved {
                lockfile.add(LockedComponent {
                    name: dep.name,
                    version: dep.version,
                    sha256: dep.sha256,
                    dependencies: dep.dependencies,
                });
            }
            self.lockfile = Some(lockfile);
            self.save_lockfile()?;
        }

        if let Some(path) = installed_path {
            Ok(path)
        } else {
            Ok(install_dir.join(format!(
                "{}@{}.wasm",
                safe_component_name(name),
                installed_version
            )))
        }
    }
    pub async fn install_all(&mut self, options: InstallOptions) -> Result<()> {
        let config_path = self.base_dir.join("run.toml");
        let config = crate::v2::config::RunConfig::load(&config_path)?;

        for (name, dep_config) in &config.dependencies {
            self.install(name, Some(&dep_config.version), options.clone())
                .await?;
        }

        if options.dev {
            for (name, dep_config) in &config.dev_dependencies {
                self.install(name, Some(&dep_config.version), options.clone())
                    .await?;
            }
        }

        Ok(())
    }
    pub async fn update(&mut self, name: &str) -> Result<Version> {
        let current_version = self
            .lockfile
            .as_ref()
            .and_then(|l| l.get(name))
            .map(|c| c.version.clone());

        let latest = self.client.get_latest_version(name).await?;

        if current_version.map(|v| v < latest).unwrap_or(true) {
            self.install(name, Some(&latest.to_string()), InstallOptions::default())
                .await?;
        }

        Ok(latest)
    }
    pub async fn update_all(&mut self) -> Result<HashMap<String, Version>> {
        let mut updates = HashMap::new();

        let to_update: Vec<(String, Version)> = self
            .lockfile
            .as_ref()
            .map(|l| {
                l.components()
                    .map(|c| (c.name.clone(), c.version.clone()))
                    .collect()
            })
            .unwrap_or_default();

        for (name, old_version) in to_update {
            let new_version = self.update(&name).await?;
            if new_version > old_version {
                updates.insert(name, new_version);
            }
        }

        Ok(updates)
    }
    pub fn remove(&mut self, name: &str) -> Result<()> {
        self.cache.remove(name)?;

        if let Some(ref mut lockfile) = self.lockfile {
            lockfile.remove(name);
            self.save_lockfile()?;
        }

        Ok(())
    }
    pub fn list_installed(&self) -> Result<Vec<(String, Version)>> {
        self.cache.list_all()
    }
    pub async fn search(&self, query: &str) -> Result<Vec<ComponentPackage>> {
        self.client.search(query).await
    }
    pub async fn info(&self, name: &str) -> Result<ComponentPackage> {
        self.client.get_info(name).await
    }
    pub fn verify_all(&self) -> Result<Vec<String>> {
        let mut invalid = Vec::new();

        if let Some(ref lockfile) = self.lockfile {
            for locked in lockfile.components() {
                let version_req = VersionReq::exact(&locked.version);
                if let Some(path) = self.cache.get(&locked.name, &version_req)? {
                    let bytes = std::fs::read(&path)?;
                    let hash = compute_sha256(&bytes);
                    if hash != locked.sha256 {
                        invalid.push(locked.name.clone());
                    }
                }
            }
        }

        Ok(invalid)
    }
    pub fn clean(&self) -> Result<usize> {
        self.cache.clean_unused(self.lockfile.as_ref())
    }
}

fn safe_component_name(name: &str) -> String {
    name.replace(':', "__").replace('/', "_").replace('\\', "_")
}

fn install_filename(name: &str, version: &Version) -> String {
    format!("{}@{}.wasm", safe_component_name(name), version)
}

fn write_to_install_dir(
    install_dir: &Path,
    name: &str,
    version: &Version,
    bytes: &[u8],
) -> Result<PathBuf> {
    let dest = install_dir.join(install_filename(name, version));
    std::fs::write(&dest, bytes)?;
    Ok(dest)
}

fn copy_to_install_dir(
    install_dir: &Path,
    name: &str,
    version: &Version,
    cached_path: &Path,
) -> Result<PathBuf> {
    let dest = install_dir.join(install_filename(name, version));
    std::fs::copy(cached_path, &dest)?;
    Ok(dest)
}

fn parse_version_from_cached(path: &Path) -> Option<Version> {
    let stem = path.file_stem()?.to_string_lossy();
    let (_name, ver) = stem.rsplit_once('@')?;
    Version::parse(ver).ok()
}

#[allow(dead_code)]
trait VersionReqExt {
    fn exact(version: &Version) -> Self;
    fn matches_any(&self) -> Option<&Version>;
}

impl VersionReqExt for VersionReq {
    fn exact(version: &Version) -> Self {
        VersionReq::parse(&format!("={}", version)).unwrap()
    }

    fn matches_any(&self) -> Option<&Version> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dependency_new() {
        let dep = Dependency::new("wasi:http", "^0.2.0").unwrap();
        assert_eq!(dep.name, "wasi:http");
        assert!(!dep.optional);
    }

    #[test]
    fn test_dependency_optional() {
        let dep = Dependency::new("wasi:http", "^0.2.0").unwrap().optional();
        assert!(dep.optional);
    }
}
