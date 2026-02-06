//! Registry Client
//!
//! HTTP client for remote registries.

use super::{ComponentPackage, Dependency, compute_sha256};
use crate::v2::{Error, Result};
use semver::{Version, VersionReq};
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct RegistryConfig {
    pub registry_url: String,

    pub mirrors: Vec<String>,

    pub timeout_secs: u64,

    pub max_retries: u32,

    pub auth_token: Option<String>,

    pub user_agent: String,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            registry_url: "https://registry.esubalew.dev".to_string(),
            mirrors: vec![],
            timeout_secs: 30,
            max_retries: 3,
            auth_token: None,
            user_agent: format!("run/{}", env!("CARGO_PKG_VERSION")),
        }
    }
}

impl RegistryConfig {
    pub fn with_url(url: &str) -> Self {
        Self {
            registry_url: url.to_string(),
            ..Default::default()
        }
    }

    pub fn with_token(mut self, token: &str) -> Self {
        self.auth_token = Some(token.to_string());
        self
    }
}

#[derive(Debug, Deserialize)]
struct VersionsResponse {
    versions: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PackageResponse {
    name: String,
    version: String,
    description: Option<String>,
    sha256: String,
    download_url: String,
    wit_url: Option<String>,
    #[serde(default)]
    dependencies: Vec<DependencyResponse>,
    #[serde(default)]
    targets: Vec<String>,
    license: Option<String>,
    repository: Option<String>,
    size: Option<usize>,
    published_at: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct DependencyResponse {
    name: String,
    version: String,
    #[serde(default)]
    optional: bool,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct SearchResponse {
    packages: Vec<PackageResponse>,
    total: usize,
}

pub struct RegistryClient {
    config: RegistryConfig,
    http: reqwest::Client,
}

impl RegistryClient {
    pub fn new(config: RegistryConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .user_agent(&config.user_agent)
            .build()
            .expect("Failed to create HTTP client");

        Self { config, http }
    }

    fn get_urls(&self) -> Vec<String> {
        std::iter::once(self.config.registry_url.clone())
            .chain(self.config.mirrors.iter().cloned())
            .collect()
    }

    pub async fn get_versions(&self, name: &str) -> Result<Vec<Version>> {
        let encoded_name = urlencoding::encode(name);

        for base_url in self.get_urls() {
            let url = format!("{}/api/v1/packages/{}/versions", base_url, encoded_name);

            for attempt in 0..self.config.max_retries {
                match self.try_get_versions(&url, name).await {
                    Ok(versions) => return Ok(versions),
                    Err(_) => {
                        if attempt < self.config.max_retries - 1 {
                            let delay = Duration::from_millis(100 * 2u64.pow(attempt));
                            tokio::time::sleep(delay).await;
                        }
                    }
                }
            }
        }

        Err(Error::RegistryUnavailable {
            url: self.config.registry_url.clone(),
        })
    }

    async fn try_get_versions(&self, url: &str, name: &str) -> Result<Vec<Version>> {
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|_| Error::RegistryUnavailable {
                url: url.to_string(),
            })?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(Error::PackageNotFound {
                name: name.to_string(),
                version: "*".to_string(),
            });
        }

        if !response.status().is_success() {
            return Err(Error::RegistryUnavailable {
                url: url.to_string(),
            });
        }

        let data: VersionsResponse = response
            .json()
            .await
            .map_err(|e| Error::other(format!("Invalid response: {}", e)))?;

        let versions: Vec<Version> = data
            .versions
            .iter()
            .filter_map(|v| Version::parse(v).ok())
            .collect();

        Ok(versions)
    }

    pub async fn get_version_info(
        &self,
        name: &str,
        version: &Version,
    ) -> Result<ComponentPackage> {
        let encoded_name = urlencoding::encode(name);

        for base_url in self.get_urls() {
            let url = format!("{}/api/v1/packages/{}/{}", base_url, encoded_name, version);

            for attempt in 0..self.config.max_retries {
                match self.try_get_version_info(&url, name, version).await {
                    Ok(pkg) => return Ok(pkg),
                    Err(_) => {
                        if attempt < self.config.max_retries - 1 {
                            let delay = Duration::from_millis(100 * 2u64.pow(attempt));
                            tokio::time::sleep(delay).await;
                        }
                    }
                }
            }
        }

        Err(Error::PackageNotFound {
            name: name.to_string(),
            version: version.to_string(),
        })
    }

    async fn try_get_version_info(
        &self,
        url: &str,
        name: &str,
        version: &Version,
    ) -> Result<ComponentPackage> {
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|_| Error::RegistryUnavailable {
                url: url.to_string(),
            })?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(Error::PackageNotFound {
                name: name.to_string(),
                version: version.to_string(),
            });
        }

        if !response.status().is_success() {
            return Err(Error::RegistryUnavailable {
                url: url.to_string(),
            });
        }

        let data: PackageResponse = response
            .json()
            .await
            .map_err(|e| Error::other(format!("Invalid response: {}", e)))?;

        convert_package_response(data)
    }

    pub async fn get_latest_version(&self, name: &str) -> Result<Version> {
        let versions = self.get_versions(name).await?;
        versions
            .into_iter()
            .max()
            .ok_or_else(|| Error::PackageNotFound {
                name: name.to_string(),
                version: "*".to_string(),
            })
    }

    pub async fn get_info(&self, name: &str) -> Result<ComponentPackage> {
        let version = self.get_latest_version(name).await?;
        self.get_version_info(name, &version).await
    }

    /// Rewrite download URLs that point to localhost so they use
    /// the client's configured registry URL instead.  This guards
    /// against a server whose REGISTRY_URL env var was not set.
    fn rewrite_download_url(&self, raw: &str) -> String {
        if raw.starts_with("http://localhost") || raw.starts_with("http://127.0.0.1") {
            if let Some(path_start) = raw.find("/packages/") {
                let base = self.config.registry_url.trim_end_matches('/');
                return format!("{}{}", base, &raw[path_start..]);
            }
        }
        raw.to_string()
    }

    pub async fn download(&self, name: &str, version: &Version) -> Result<Vec<u8>> {
        let info = self.get_version_info(name, version).await?;
        let download_url = self.rewrite_download_url(&info.download_url);

        let response = self
            .http
            .get(&download_url)
            .send()
            .await
            .map_err(|_| Error::RegistryUnavailable {
                url: download_url.clone(),
            })?;

        if !response.status().is_success() {
            return Err(Error::RegistryUnavailable {
                url: download_url.clone(),
            });
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| Error::other(format!("Download failed: {}", e)))?;

        let actual_hash = compute_sha256(&bytes);
        if actual_hash != info.sha256 {
            return Err(Error::HashMismatch {
                package: name.to_string(),
                expected: info.sha256,
                actual: actual_hash,
            });
        }

        Ok(bytes.to_vec())
    }

    pub async fn download_raw(&self, url: &str) -> Result<Vec<u8>> {
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|_| Error::RegistryUnavailable {
                url: url.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(Error::RegistryUnavailable {
                url: url.to_string(),
            });
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| Error::other(format!("Download failed: {}", e)))?;

        Ok(bytes.to_vec())
    }

    pub async fn search(&self, query: &str) -> Result<Vec<ComponentPackage>> {
        let encoded_query = urlencoding::encode(query);
        let url = format!(
            "{}/api/v1/search?q={}",
            self.config.registry_url, encoded_query
        );

        let response = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|_| Error::RegistryUnavailable { url: url.clone() })?;

        if !response.status().is_success() {
            return Err(Error::RegistryUnavailable { url });
        }

        let data: SearchResponse = response
            .json()
            .await
            .map_err(|e| Error::other(format!("Invalid response: {}", e)))?;

        data.packages
            .into_iter()
            .map(convert_package_response)
            .collect()
    }

    pub async fn publish(&self, package: &ComponentPackage, bytes: &[u8]) -> Result<()> {
        let token = self
            .config
            .auth_token
            .as_ref()
            .ok_or_else(|| Error::other("Authentication required for publishing"))?;

        let url = format!("{}/api/v1/packages", self.config.registry_url);

        let form = reqwest::multipart::Form::new()
            .text("name", package.name.clone())
            .text("version", package.version.to_string())
            .text("description", package.description.clone())
            .text("sha256", compute_sha256(bytes))
            .part(
                "component",
                reqwest::multipart::Part::bytes(bytes.to_vec())
                    .file_name(format!("{}.wasm", package.name))
                    .mime_str("application/wasm")
                    .map_err(|e| Error::other(format!("Invalid MIME type: {}", e)))?,
            );

        let response = self
            .http
            .post(&url)
            .bearer_auth(token)
            .multipart(form)
            .send()
            .await
            .map_err(|_| Error::RegistryUnavailable { url: url.clone() })?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(Error::other("Invalid authentication token"));
        }

        if response.status() == reqwest::StatusCode::CONFLICT {
            return Err(Error::other(format!(
                "Package {}@{} already exists",
                package.name, package.version
            )));
        }

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(Error::other(format!("Publish failed: {}", error_text)));
        }

        Ok(())
    }

    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/health", self.config.registry_url);

        match self.http.get(&url).send().await {
            Ok(response) => Ok(response.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    pub async fn stats(&self) -> Result<RegistryStats> {
        let url = format!("{}/api/v1/stats", self.config.registry_url);

        let response = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|_| Error::RegistryUnavailable { url: url.clone() })?;

        if !response.status().is_success() {
            return Err(Error::RegistryUnavailable { url });
        }

        response
            .json()
            .await
            .map_err(|e| Error::other(format!("Invalid response: {}", e)))
    }
}

fn convert_package_response(data: PackageResponse) -> Result<ComponentPackage> {
    let version = Version::parse(&data.version)
        .map_err(|e| Error::other(format!("Invalid version: {}", e)))?;

    let dependencies: Vec<Dependency> = data
        .dependencies
        .into_iter()
        .filter_map(|d| {
            VersionReq::parse(&d.version)
                .ok()
                .map(|version_req| Dependency {
                    name: d.name,
                    version_req,
                    optional: d.optional,
                    features: vec![],
                })
        })
        .collect();

    Ok(ComponentPackage {
        name: data.name,
        version,
        description: data.description.unwrap_or_default(),
        sha256: data.sha256,
        download_url: data.download_url,
        wit_url: data.wit_url,
        dependencies,
        targets: if data.targets.is_empty() {
            vec!["wasm32-wasip2".to_string()]
        } else {
            data.targets
        },
        license: data.license,
        repository: data.repository,
        size: data.size.unwrap_or(0),
        published_at: data.published_at.unwrap_or(0),
    })
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegistryStats {
    pub package_count: usize,

    pub version_count: usize,

    pub download_count: u64,

    pub uptime_percent: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_config_default() {
        let config = RegistryConfig::default();
        assert_eq!(config.registry_url, "https://registry.esubalew.dev");
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_registry_config_with_url() {
        let config = RegistryConfig::with_url("https://custom.registry.com");
        assert_eq!(config.registry_url, "https://custom.registry.com");
    }

    #[test]
    fn test_registry_config_with_token() {
        let config = RegistryConfig::default().with_token("secret123");
        assert_eq!(config.auth_token, Some("secret123".to_string()));
    }
}
