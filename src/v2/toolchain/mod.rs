//! Toolchain Manager
//!
//! Manages build toolchains for hermetic, reproducible builds.

use crate::v2::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolchainLockfile {
    pub version: String,
    pub toolchains: HashMap<String, ToolchainEntry>,
    pub build: BuildEnv,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolchainEntry {
    pub version: String,
    pub sha256: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildEnv {
    #[serde(rename = "SOURCE_DATE_EPOCH")]
    pub source_date_epoch: u64,
    #[serde(rename = "TZ")]
    pub tz: String,
    #[serde(rename = "LC_ALL")]
    pub lc_all: String,
    #[serde(rename = "RUSTFLAGS")]
    pub rustflags: String,
}

impl ToolchainLockfile {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        toml::from_str(&content)
            .map_err(|e| Error::other(format!("Invalid toolchain lockfile: {}", e)))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| Error::other(format!("Failed to serialize lockfile: {}", e)))?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn verify_toolchain(&self, name: &str) -> Result<bool> {
        let entry = self
            .toolchains
            .get(name)
            .ok_or_else(|| Error::other(format!("Toolchain '{}' not in lockfile", name)))?;

        let version = get_toolchain_version(name)?;
        Ok(version == entry.version)
    }

    pub fn apply_build_env(&self, cmd: &mut Command) {
        cmd.env(
            "SOURCE_DATE_EPOCH",
            self.build.source_date_epoch.to_string(),
        );
        cmd.env("TZ", &self.build.tz);
        cmd.env("LC_ALL", &self.build.lc_all);
        if !self.build.rustflags.is_empty() {
            cmd.env("RUSTFLAGS", &self.build.rustflags);
        }
    }
}

pub struct ToolchainManager {
    lockfile: Option<ToolchainLockfile>,
    install_dir: PathBuf,
    lockfile_path: PathBuf,
}

impl ToolchainManager {
    pub fn new(project_dir: &Path) -> Result<Self> {
        let lockfile_path = project_dir.join("run.lock.toml");
        let lockfile = if lockfile_path.exists() {
            Some(ToolchainLockfile::load(&lockfile_path)?)
        } else {
            None
        };

        let install_dir = project_dir.join(".run").join("toolchains");
        std::fs::create_dir_all(&install_dir)?;

        Ok(Self {
            lockfile,
            install_dir,
            lockfile_path,
        })
    }

    pub fn ensure_toolchain(&self, name: &str) -> Result<()> {
        if let Some(ref lockfile) = self.lockfile {
            if !lockfile.verify_toolchain(name)? {
                return Err(Error::other(format!(
                    "Toolchain '{}' version mismatch. Expected {}, run `run toolchain sync`",
                    name,
                    lockfile
                        .toolchains
                        .get(name)
                        .map(|e| e.version.as_str())
                        .unwrap_or("unknown")
                )));
            }
        }
        Ok(())
    }

    pub fn install_toolchain(&self, name: &str, entry: &ToolchainEntry) -> Result<PathBuf> {
        let dest_dir = self.install_dir.join(name).join(&entry.version);
        if dest_dir.exists() {
            return Ok(dest_dir);
        }

        std::fs::create_dir_all(&dest_dir)?;

        println!("Installing {} {}...", name, entry.version);

        // Download and verify would go here
        // For now, we expect the tool to be in PATH

        Ok(dest_dir)
    }

    pub fn get_toolchain_path(&self, name: &str) -> Option<PathBuf> {
        if let Some(ref lockfile) = self.lockfile {
            if let Some(entry) = lockfile.toolchains.get(name) {
                let path = self.install_dir.join(name).join(&entry.version);
                if path.exists() {
                    return Some(path);
                }
            }
        }
        None
    }

    pub fn sync(&mut self) -> Result<ToolchainLockfile> {
        let tools = [
            "cargo-component",
            "componentize-py",
            "jco",
            "tinygo",
            "wasm-tools",
            "wasmtime",
        ];

        let mut toolchains = HashMap::new();

        for tool in tools {
            let version = get_toolchain_version(tool)?;
            let binary_path = resolve_toolchain_binary(tool)?;
            let sha256 = sha256_file(&binary_path)?;
            let source = format!("local:{}", binary_path.display());

            toolchains.insert(
                tool.to_string(),
                ToolchainEntry {
                    version,
                    sha256,
                    source,
                },
            );
        }

        let build = self
            .lockfile
            .as_ref()
            .map(|lock| lock.build.clone())
            .unwrap_or_else(default_build_env);

        let lockfile = ToolchainLockfile {
            version: "1".to_string(),
            toolchains,
            build,
        };

        lockfile.save(&self.lockfile_path)?;
        self.lockfile = Some(lockfile.clone());
        Ok(lockfile)
    }
}

fn get_toolchain_version(name: &str) -> Result<String> {
    let output = match name {
        "cargo-component" => Command::new("cargo")
            .args(["component", "--version"])
            .output(),
        "componentize-py" => Command::new("componentize-py").arg("--version").output(),
        "jco" => Command::new("jco").arg("--version").output(),
        "tinygo" => Command::new("tinygo").arg("version").output(),
        "wasm-tools" => Command::new("wasm-tools").arg("--version").output(),
        "wasmtime" => Command::new("wasmtime").arg("--version").output(),
        _ => return Err(Error::other(format!("Unknown toolchain '{}'", name))),
    };

    match output {
        Ok(out) if out.status.success() => {
            let version_str = String::from_utf8_lossy(&out.stdout);
            let version = parse_version_from_output(&version_str)
                .ok_or_else(|| Error::other(format!("Could not parse version for {}", name)))?;
            Ok(version)
        }
        _ => Err(Error::other(format!("Toolchain '{}' not found", name))),
    }
}

fn parse_version_from_output(output: &str) -> Option<String> {
    for word in output.split_whitespace() {
        if word.chars().next()?.is_ascii_digit() {
            if word.contains('.') {
                return Some(
                    word.trim_end_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.')
                        .to_string(),
                );
            }
        }
    }
    None
}

fn resolve_toolchain_binary(name: &str) -> Result<PathBuf> {
    let candidates: Vec<&str> = match name {
        "cargo-component" => vec!["cargo-component", "cargo"],
        _ => vec![name],
    };

    for candidate in candidates {
        if let Some(path) = find_in_path(candidate) {
            return Ok(path);
        }
    }

    Err(Error::other(format!(
        "Toolchain '{}' not found in PATH",
        name
    )))
}

fn find_in_path(cmd: &str) -> Option<PathBuf> {
    if cmd.contains(std::path::MAIN_SEPARATOR) {
        let path = PathBuf::from(cmd);
        if path.exists() && is_executable(&path) {
            return Some(path);
        }
    }

    let path_env = env::var_os("PATH")?;
    for dir in env::split_paths(&path_env) {
        let candidate = dir.join(cmd);
        if candidate.exists() && is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn is_executable(path: &Path) -> bool {
    if let Ok(metadata) = std::fs::metadata(path) {
        if !metadata.is_file() {
            return false;
        }
        #[cfg(unix)]
        {
            return metadata.permissions().mode() & 0o111 != 0;
        }
        #[cfg(not(unix))]
        {
            return true;
        }
    }
    false
}

fn sha256_file(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn default_build_env() -> BuildEnv {
    BuildEnv {
        source_date_epoch: 0,
        tz: "UTC".to_string(),
        lc_all: "C".to_string(),
        rustflags: "-C debuginfo=0 -C link-arg=-s".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version() {
        assert_eq!(
            parse_version_from_output("cargo-component 0.13.2"),
            Some("0.13.2".to_string())
        );
        assert_eq!(
            parse_version_from_output("version 1.4.0"),
            Some("1.4.0".to_string())
        );
    }
}
