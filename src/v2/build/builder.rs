//! Component Builder
//!
//! Handles the actual compilation of components.

use super::metadata::BuildMetadata;
use crate::v2::registry::compute_sha256;
use crate::v2::{Error, Result};
use serde_json;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Zig,
    Wasm,
}

impl Language {
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "rust" | "rs" => Ok(Language::Rust),
            "python" | "py" => Ok(Language::Python),
            "javascript" | "js" => Ok(Language::JavaScript),
            "typescript" | "ts" => Ok(Language::TypeScript),
            "go" | "golang" => Ok(Language::Go),
            "zig" => Ok(Language::Zig),
            "wasm" | "wasm32" => Ok(Language::Wasm),
            other => Err(Error::other(format!(
                "Unsupported language: {}. Supported: rust, python, javascript, typescript, go, zig, wasm",
                other
            ))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Language::Rust => "rust",
            Language::Python => "python",
            Language::JavaScript => "javascript",
            Language::TypeScript => "typescript",
            Language::Go => "go",
            Language::Zig => "zig",
            Language::Wasm => "wasm",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BuildConfig {
    pub output_dir: PathBuf,

    pub release: bool,

    pub wit_dir: Option<PathBuf>,

    pub reproducible: bool,

    pub source_date_epoch: Option<u64>,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            output_dir: PathBuf::from(".run/build"),
            release: true,
            wit_dir: None,
            reproducible: false,
            source_date_epoch: None,
        }
    }
}

impl BuildConfig {
    pub fn from_run_config(config: &crate::v2::config::RunConfig, base_dir: &Path) -> Result<Self> {
        Ok(Self {
            output_dir: base_dir.join(&config.build.output_dir),
            release: config.build.opt_level == "release",
            wit_dir: None,
            reproducible: config.build.reproducible,
            source_date_epoch: config.build.source_date_epoch,
        })
    }
}

#[derive(Debug, Clone)]
pub struct BuildResult {
    pub name: String,

    pub output_path: PathBuf,

    pub metadata: BuildMetadata,

    pub duration_ms: u64,

    pub warnings: Vec<String>,
}

pub struct ComponentBuilder {
    config: BuildConfig,
}

impl ComponentBuilder {
    pub fn new(config: BuildConfig) -> Self {
        Self { config }
    }

    pub fn build(
        &self,
        source_dir: &Path,
        name: &str,
        lang: Language,
        world: Option<&str>,
    ) -> Result<BuildResult> {
        let start = std::time::Instant::now();

        std::fs::create_dir_all(&self.config.output_dir)?;

        let output_path = match lang {
            Language::Rust => self.build_rust(source_dir, name)?,
            Language::Python => self.build_python(source_dir, name, world)?,
            Language::JavaScript => self.build_javascript(source_dir, name)?,
            Language::TypeScript => self.build_typescript(source_dir, name)?,
            Language::Go => self.build_go(source_dir, name)?,
            Language::Zig => self.build_zig(source_dir, name)?,
            Language::Wasm => self.build_wasm(source_dir, name)?,
        };

        let bytes = std::fs::read(&output_path)?;
        validate_wasm_header(&bytes, &output_path)?;
        let sha256 = compute_sha256(&bytes);
        let size = bytes.len();

        let (exports, imports) = extract_wit_info(&output_path)?;

        let built_at = if self.config.reproducible {
            self.config.source_date_epoch.unwrap_or(0)
        } else {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        };

        let metadata = BuildMetadata {
            name: name.to_string(),
            sha256,
            size,
            language: lang.as_str().to_string(),
            exports,
            imports,
            description: None,
            built_at,
        };

        let metadata_path = output_path.with_extension("meta.toml");
        let metadata_content = toml::to_string_pretty(&metadata)
            .map_err(|e| Error::other(format!("Failed to serialize metadata: {}", e)))?;
        std::fs::write(&metadata_path, metadata_content)?;

        Ok(BuildResult {
            name: name.to_string(),
            output_path,
            metadata,
            duration_ms: start.elapsed().as_millis() as u64,
            warnings: vec![],
        })
    }

    fn build_rust(&self, source_dir: &Path, name: &str) -> Result<PathBuf> {
        let mut cmd = Command::new("cargo");
        cmd.args(["component", "build"]);

        if self.config.release {
            cmd.arg("--release");
        }

        cmd.current_dir(source_dir);
        self.apply_reproducible_env(&mut cmd);

        let output = cmd.output().map_err(|e| {
            Error::other(format!(
                "Failed to run cargo component. Is cargo-component installed? Error: {}",
                e
            ))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::other(format!("cargo component failed:\n{}", stderr)));
        }

        let profile = if self.config.release {
            "release"
        } else {
            "debug"
        };
        let target_dir = source_dir.join(format!("target/wasm32-wasip1/{}", profile));

        let wasm_name = get_rust_crate_name(source_dir)?.replace('-', "_");

        let source = target_dir.join(format!("{}.wasm", wasm_name));
        let dest = self.config.output_dir.join(format!("{}.wasm", name));

        std::fs::copy(&source, &dest).map_err(|e| {
            Error::other(format!(
                "Failed to copy {} to {}: {}",
                source.display(),
                dest.display(),
                e
            ))
        })?;

        Ok(dest)
    }

    fn build_python(&self, source_dir: &Path, name: &str, world: Option<&str>) -> Result<PathBuf> {
        let dest = self.config.output_dir.join(format!("{}.wasm", name));

        let wit_dir = self
            .config
            .wit_dir
            .clone()
            .or_else(|| find_wit_dir_internal(source_dir))
            .ok_or_else(|| {
                Error::other(format!(
                    "WIT directory not found near {}",
                    source_dir.display()
                ))
            })?;

        let mut cmd = Command::new("componentize-py");
        cmd.args(["-d", wit_dir.to_str().unwrap()]);

        if let Some(w) = world {
            cmd.args(["-w", w]);
        }

        let main_module = find_python_main(source_dir)?;

        cmd.arg("componentize");
        cmd.arg(&main_module);
        cmd.args(["-o", dest.to_str().unwrap()]);
        cmd.current_dir(source_dir);
        self.apply_reproducible_env(&mut cmd);

        let output = cmd.output().map_err(|e| {
            Error::other(format!(
                "Failed to run componentize-py. Is it installed? Error: {}",
                e
            ))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::other(format!("componentize-py failed:\n{}", stderr)));
        }

        Ok(dest)
    }

    fn build_javascript(&self, source_dir: &Path, name: &str) -> Result<PathBuf> {
        let dest = self.config.output_dir.join(format!("{}.wasm", name));
        let entry = find_js_entry(source_dir)?;

        let mut cmd = Command::new("jco");
        cmd.args([
            "componentize",
            entry.to_str().unwrap(),
            "-o",
            dest.to_str().unwrap(),
        ]);
        cmd.current_dir(source_dir);
        self.apply_reproducible_env(&mut cmd);

        let output = cmd.output().map_err(|e| {
            Error::other(format!(
                "Failed to run jco componentize. Is jco installed? Error: {}",
                e
            ))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::other(format!(
                "jco componentize failed:\n{}",
                stderr
            )));
        }

        Ok(dest)
    }

    fn build_typescript(&self, source_dir: &Path, name: &str) -> Result<PathBuf> {
        let dest = self.config.output_dir.join(format!("{}.wasm", name));
        let compiled_entry = ensure_typescript_compiled(source_dir)?;

        let mut cmd = Command::new("jco");
        cmd.args([
            "componentize",
            compiled_entry.to_str().unwrap(),
            "-o",
            dest.to_str().unwrap(),
        ]);
        cmd.current_dir(source_dir);
        self.apply_reproducible_env(&mut cmd);

        let output = cmd.output().map_err(|e| {
            Error::other(format!(
                "Failed to run jco componentize. Is jco installed? Error: {}",
                e
            ))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::other(format!(
                "jco componentize failed:\n{}",
                stderr
            )));
        }

        Ok(dest)
    }

    fn build_go(&self, source_dir: &Path, name: &str) -> Result<PathBuf> {
        let module_wasm = self.config.output_dir.join(format!("{}.module.wasm", name));
        let dest = self.config.output_dir.join(format!("{}.wasm", name));

        let mut cmd = Command::new("tinygo");
        cmd.args(["build", "-target=wasi", "-o", module_wasm.to_str().unwrap()]);
        cmd.current_dir(source_dir);
        self.apply_reproducible_env(&mut cmd);

        let output = cmd.output().map_err(|e| {
            Error::other(format!(
                "Failed to run tinygo. Is tinygo installed? Error: {}",
                e
            ))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::other(format!("tinygo build failed:\n{}", stderr)));
        }

        let mut componentize = Command::new("wasm-tools");
        componentize.args([
            "component",
            "new",
            module_wasm.to_str().unwrap(),
            "-o",
            dest.to_str().unwrap(),
        ]);
        componentize.current_dir(source_dir);
        self.apply_reproducible_env(&mut componentize);

        let output = componentize.output().map_err(|e| {
            Error::other(format!(
                "Failed to run wasm-tools component new. Is wasm-tools installed? Error: {}",
                e
            ))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::other(format!(
                "wasm-tools component new failed:\n{}",
                stderr
            )));
        }

        Ok(dest)
    }

    fn build_zig(&self, source_dir: &Path, name: &str) -> Result<PathBuf> {
        let dest = self.config.output_dir.join(format!("{}.wasm", name));
        let optimize = if self.config.release {
            "ReleaseFast"
        } else {
            "Debug"
        };

        let mut cmd = Command::new("zig");
        cmd.args([
            "build",
            "-Dtarget=wasm32-wasip1",
            &format!("-Doptimize={}", optimize),
        ]);
        cmd.current_dir(source_dir);
        self.apply_reproducible_env(&mut cmd);

        let output = cmd.output().map_err(|e| {
            Error::other(format!(
                "Failed to run zig build. Is Zig installed? Error: {}",
                e
            ))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::other(format!("zig build failed:\n{}", stderr)));
        }

        let wasm_path = find_first_wasm(&source_dir.join("zig-out"))
            .ok_or_else(|| Error::other("Zig build did not produce a wasm output".to_string()))?;

        std::fs::copy(&wasm_path, &dest).map_err(|e| {
            Error::other(format!(
                "Failed to copy {} to {}: {}",
                wasm_path.display(),
                dest.display(),
                e
            ))
        })?;

        Ok(dest)
    }

    fn build_wasm(&self, source_dir: &Path, name: &str) -> Result<PathBuf> {
        let dest = self.config.output_dir.join(format!("{}.wasm", name));
        let source = if source_dir.is_file() {
            source_dir.to_path_buf()
        } else {
            source_dir.join(format!("{}.wasm", name))
        };

        if !source.exists() {
            return Err(Error::other(format!(
                "WASM source not found at {}",
                source.display()
            )));
        }

        std::fs::copy(&source, &dest).map_err(|e| {
            Error::other(format!(
                "Failed to copy {} to {}: {}",
                source.display(),
                dest.display(),
                e
            ))
        })?;

        Ok(dest)
    }
}

impl ComponentBuilder {
    fn apply_reproducible_env(&self, cmd: &mut Command) {
        if !self.config.reproducible {
            return;
        }

        let epoch = self.config.source_date_epoch.unwrap_or(0).to_string();
        cmd.env("SOURCE_DATE_EPOCH", epoch);
        cmd.env("TZ", "UTC");
        cmd.env("LC_ALL", "C");

        if cmd.get_program() == std::ffi::OsStr::new("cargo") {
            let flags = std::env::var("RUSTFLAGS").unwrap_or_default();
            let extra = "-C debuginfo=0 -C link-arg=-s";
            if flags.contains(extra) {
                cmd.env("RUSTFLAGS", flags);
            } else if flags.is_empty() {
                cmd.env("RUSTFLAGS", extra);
            } else {
                cmd.env("RUSTFLAGS", format!("{} {}", flags, extra));
            }
        }
    }
}

fn get_rust_crate_name(path: &Path) -> Result<String> {
    let cargo_toml = path.join("Cargo.toml");
    let content = std::fs::read_to_string(&cargo_toml)?;

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("name") && line.contains('=') {
            if let Some(name) = line.split('=').nth(1) {
                return Ok(name.trim().trim_matches('"').to_string());
            }
        }
    }

    Ok(path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("component")
        .to_string())
}

fn find_wit_dir_internal(path: &Path) -> Option<PathBuf> {
    let candidates = [
        path.join("wit"),
        path.join("../wit"),
        path.join("../../wit"),
    ];

    for candidate in candidates {
        if candidate.exists() {
            return candidate.canonicalize().ok();
        }
    }

    None
}

fn find_js_entry(path: &Path) -> Result<PathBuf> {
    if path.is_file() {
        return Ok(path.to_path_buf());
    }

    let package_json = path.join("package.json");
    if package_json.exists() {
        if let Ok(content) = std::fs::read_to_string(&package_json) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(main) = json.get("main").and_then(|v| v.as_str()) {
                    let candidate = path.join(main);
                    if candidate.exists() {
                        return Ok(candidate);
                    }
                }
                if let Some(module) = json.get("module").and_then(|v| v.as_str()) {
                    let candidate = path.join(module);
                    if candidate.exists() {
                        return Ok(candidate);
                    }
                }
            }
        }
    }

    let candidates = [
        path.join("index.js"),
        path.join("src/index.js"),
        path.join("dist/index.js"),
    ];

    for candidate in candidates {
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    Err(Error::other(format!(
        "No JavaScript entry found in {}",
        path.display()
    )))
}

fn ensure_typescript_compiled(path: &Path) -> Result<PathBuf> {
    if path.is_file() && path.extension().map(|e| e == "js").unwrap_or(false) {
        return Ok(path.to_path_buf());
    }

    if path.join("tsconfig.json").exists() {
        let mut cmd = Command::new("tsc");
        cmd.args(["-p", path.to_str().unwrap()]);
        cmd.current_dir(path);
        let output = cmd.output().map_err(|e| {
            Error::other(format!(
                "Failed to run tsc. Is TypeScript installed? Error: {}",
                e
            ))
        })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::other(format!("tsc failed:\n{}", stderr)));
        }
    }

    let candidates = [
        path.join("dist/index.js"),
        path.join("build/index.js"),
        path.join("lib/index.js"),
        path.join("index.js"),
    ];

    for candidate in candidates {
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    Err(Error::other(format!(
        "TypeScript build output not found in {}",
        path.display()
    )))
}

fn find_first_wasm(path: &Path) -> Option<PathBuf> {
    if !path.exists() {
        return None;
    }

    if path.is_file() && path.extension().map(|e| e == "wasm").unwrap_or(false) {
        return Some(path.to_path_buf());
    }

    let entries = std::fs::read_dir(path).ok()?;
    for entry in entries.flatten() {
        let entry_path = entry.path();
        if entry_path.is_dir() {
            if let Some(found) = find_first_wasm(&entry_path) {
                return Some(found);
            }
        } else if entry_path.extension().map(|e| e == "wasm").unwrap_or(false) {
            return Some(entry_path);
        }
    }

    None
}

fn find_python_main(path: &Path) -> Result<String> {
    let candidates = ["app", "main", "__init__"];

    for name in candidates {
        if path.join(format!("{}.py", name)).exists() {
            return Ok(name.to_string());
        }
    }

    Err(Error::other(format!(
        "No Python module (app.py, main.py) found in {}",
        path.display()
    )))
}

fn extract_wit_info(path: &Path) -> Result<(Vec<String>, Vec<String>)> {
    let output = Command::new("wasm-tools")
        .args(["component", "wit", path.to_str().unwrap()])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let wit = String::from_utf8_lossy(&output.stdout);
            let exports = extract_exports(&wit);
            let imports = extract_imports(&wit);
            Ok((exports, imports))
        }
        _ => Ok((vec![], vec![])),
    }
}

fn extract_exports(wit: &str) -> Vec<String> {
    let mut exports = Vec::new();

    for line in wit.lines() {
        let line = line.trim();
        if line.starts_with("export ") {
            if let Some(name) = line.strip_prefix("export ") {
                let name = name.trim_end_matches(';').trim();
                exports.push(name.to_string());
            }
        }
    }

    exports
}

fn extract_imports(wit: &str) -> Vec<String> {
    let mut imports = Vec::new();

    for line in wit.lines() {
        let line = line.trim();
        if line.starts_with("import ") {
            if let Some(name) = line.strip_prefix("import ") {
                let name = name.trim_end_matches(';').trim();
                imports.push(name.to_string());
            }
        }
    }

    imports
}

fn validate_wasm_header(bytes: &[u8], path: &Path) -> Result<()> {
    let expected = [0x00, 0x61, 0x73, 0x6d];
    if bytes.len() < expected.len() || bytes[..4] != expected {
        return Err(Error::InvalidComponent {
            path: path.to_path_buf(),
            reason: "Output is not a valid WASM binary".to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_from_str() {
        assert_eq!(Language::from_str("rust").unwrap(), Language::Rust);
        assert_eq!(Language::from_str("Python").unwrap(), Language::Python);
        assert_eq!(Language::from_str("py").unwrap(), Language::Python);
        assert_eq!(Language::from_str("js").unwrap(), Language::JavaScript);
        assert_eq!(Language::from_str("ts").unwrap(), Language::TypeScript);
        assert_eq!(Language::from_str("go").unwrap(), Language::Go);
        assert_eq!(Language::from_str("zig").unwrap(), Language::Zig);
        assert_eq!(Language::from_str("wasm").unwrap(), Language::Wasm);
        assert!(Language::from_str("unknown").is_err());
    }

    #[test]
    fn test_build_config_default() {
        let config = BuildConfig::default();
        assert!(config.release);
    }
}
