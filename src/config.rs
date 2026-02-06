use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Project-level configuration loaded from `run.toml` or `.runrc`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RunConfig {
    /// Default language when none is specified.
    pub language: Option<String>,
    /// Execution timeout in seconds.
    pub timeout: Option<u64>,
    /// Always show execution timing.
    pub timing: Option<bool>,
    /// Default benchmark iterations.
    pub bench_iterations: Option<u32>,
}

impl RunConfig {
    /// Search for a config file in the current directory and ancestors.
    /// Checks `run.toml`, then `.runrc` (TOML format).
    pub fn discover() -> Self {
        let cwd = std::env::current_dir().ok();
        let cwd = match cwd {
            Some(ref p) => p.as_path(),
            None => return Self::default(),
        };

        for dir in cwd.ancestors() {
            for name in &["run.toml", ".runrc"] {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    if let Ok(config) = Self::load(&candidate) {
                        return config;
                    }
                }
            }
        }

        Self::default()
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        toml::from_str(&content)
            .map_err(|e| format!("invalid config in {}: {e}", path.display()))
    }

    pub fn apply_env(&self) {
        if let Some(secs) = self.timeout {
            if std::env::var("RUN_TIMEOUT_SECS").is_err() {
                // SAFETY: called once at startup before any threads are spawned.
                unsafe { std::env::set_var("RUN_TIMEOUT_SECS", secs.to_string()); }
            }
        }
        if let Some(true) = self.timing {
            if std::env::var("RUN_TIMING").is_err() {
                // SAFETY: called once at startup before any threads are spawned.
                unsafe { std::env::set_var("RUN_TIMING", "1"); }
            }
        }
    }

    pub fn find_config_path() -> Option<PathBuf> {
        let cwd = std::env::current_dir().ok()?;
        for dir in cwd.ancestors() {
            for name in &["run.toml", ".runrc"] {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        None
    }
}
