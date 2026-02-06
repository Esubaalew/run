mod bash;
mod c;
mod cpp;
mod crystal;
mod csharp;
mod dart;
mod elixir;
mod go;
mod groovy;
mod haskell;
mod java;
mod javascript;
mod julia;
mod kotlin;
mod lua;
mod nim;
mod perl;
mod php;
mod python;
mod r;
mod ruby;
mod rust;
mod swift;
mod typescript;
mod zig;

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Result, bail};

use crate::cli::InputSource;
use crate::language::{LanguageSpec, canonical_language_id};

// ---------------------------------------------------------------------------
// Compilation cache: hash source code -> reuse compiled binaries
// ---------------------------------------------------------------------------

use std::sync::LazyLock;

static COMPILE_CACHE: LazyLock<Mutex<CompileCache>> =
    LazyLock::new(|| Mutex::new(CompileCache::new()));

struct CompileCache {
    dir: PathBuf,
    entries: HashMap<u64, PathBuf>,
}

impl CompileCache {
    fn new() -> Self {
        let dir = std::env::temp_dir().join("run-compile-cache");
        let _ = std::fs::create_dir_all(&dir);
        Self {
            dir,
            entries: HashMap::new(),
        }
    }

    fn get(&self, hash: u64) -> Option<&PathBuf> {
        self.entries.get(&hash).filter(|p| p.exists())
    }

    fn insert(&mut self, hash: u64, path: PathBuf) {
        self.entries.insert(hash, path);
    }

    fn cache_dir(&self) -> &Path {
        &self.dir
    }
}

/// Hash source code for cache lookup.
pub fn hash_source(source: &str) -> u64 {
    // Simple FNV-1a hash — fast and good enough for cache keys.
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in source.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Look up a cached binary for the given source hash.
/// Returns Some(path) if a valid cached binary exists.
pub fn cache_lookup(source_hash: u64) -> Option<PathBuf> {
    let cache = COMPILE_CACHE.lock().ok()?;
    cache.get(source_hash).cloned()
}

/// Store a compiled binary in the cache. Copies the binary to the cache directory.
pub fn cache_store(source_hash: u64, binary: &Path) -> Option<PathBuf> {
    let mut cache = COMPILE_CACHE.lock().ok()?;
    let suffix = std::env::consts::EXE_SUFFIX;
    let cached_name = format!("{:016x}{}", source_hash, suffix);
    let cached_path = cache.cache_dir().join(cached_name);
    if std::fs::copy(binary, &cached_path).is_ok() {
        // Ensure executable permission on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&cached_path, std::fs::Permissions::from_mode(0o755));
        }
        cache.insert(source_hash, cached_path.clone());
        Some(cached_path)
    } else {
        None
    }
}

/// Execute a cached binary, returning the Output. Returns None if no cache entry.
pub fn try_cached_execution(source_hash: u64) -> Option<std::process::Output> {
    let cached = cache_lookup(source_hash)?;
    let mut cmd = std::process::Command::new(&cached);
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::inherit());
    cmd.output().ok()
}

/// Default execution timeout: 60 seconds.
/// Override with RUN_TIMEOUT_SECS env var.
pub fn execution_timeout() -> Duration {
    let secs = std::env::var("RUN_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(60);
    Duration::from_secs(secs)
}

/// Wait for a child process with a timeout. Kills the process if it exceeds the limit.
/// Returns the Output on success, or an error on timeout.
pub fn wait_with_timeout(mut child: Child, timeout: Duration) -> Result<std::process::Output> {
    let start = Instant::now();
    let poll_interval = Duration::from_millis(50);

    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                // Process finished — collect output
                return child.wait_with_output().map_err(Into::into);
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait(); // reap
                    bail!(
                        "Execution timed out after {:.1}s (limit: {}s). \
                         Set RUN_TIMEOUT_SECS to increase.",
                        start.elapsed().as_secs_f64(),
                        timeout.as_secs()
                    );
                }
                std::thread::sleep(poll_interval);
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    }
}

pub use bash::BashEngine;
pub use c::CEngine;
pub use cpp::CppEngine;
pub use crystal::CrystalEngine;
pub use csharp::CSharpEngine;
pub use dart::DartEngine;
pub use elixir::ElixirEngine;
pub use go::GoEngine;
pub use groovy::GroovyEngine;
pub use haskell::HaskellEngine;
pub use java::JavaEngine;
pub use javascript::JavascriptEngine;
pub use julia::JuliaEngine;
pub use kotlin::KotlinEngine;
pub use lua::LuaEngine;
pub use nim::NimEngine;
pub use perl::PerlEngine;
pub use php::PhpEngine;
pub use python::PythonEngine;
pub use r::REngine;
pub use ruby::RubyEngine;
pub use rust::RustEngine;
pub use swift::SwiftEngine;
pub use typescript::TypeScriptEngine;
pub use zig::ZigEngine;

pub trait LanguageSession {
    fn language_id(&self) -> &str;
    fn eval(&mut self, code: &str) -> Result<ExecutionOutcome>;
    fn shutdown(&mut self) -> Result<()>;
}

pub trait LanguageEngine {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str {
        self.id()
    }
    fn aliases(&self) -> &[&'static str] {
        &[]
    }
    fn supports_sessions(&self) -> bool {
        false
    }
    fn validate(&self) -> Result<()> {
        Ok(())
    }
    fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionOutcome>;
    fn start_session(&self) -> Result<Box<dyn LanguageSession>> {
        bail!("{} does not support interactive sessions yet", self.id())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionPayload {
    Inline { code: String },
    File { path: std::path::PathBuf },
    Stdin { code: String },
}

impl ExecutionPayload {
    pub fn from_input_source(source: &InputSource) -> Result<Self> {
        match source {
            InputSource::Inline(code) => Ok(Self::Inline {
                code: normalize_inline_code(code).into_owned(),
            }),
            InputSource::File(path) => Ok(Self::File { path: path.clone() }),
            InputSource::Stdin => {
                use std::io::Read;
                let mut buffer = String::new();
                std::io::stdin().read_to_string(&mut buffer)?;
                Ok(Self::Stdin { code: buffer })
            }
        }
    }

    pub fn as_inline(&self) -> Option<&str> {
        match self {
            ExecutionPayload::Inline { code } => Some(code.as_str()),
            ExecutionPayload::Stdin { code } => Some(code.as_str()),
            ExecutionPayload::File { .. } => None,
        }
    }

    pub fn as_file_path(&self) -> Option<&Path> {
        match self {
            ExecutionPayload::File { path } => Some(path.as_path()),
            _ => None,
        }
    }
}

fn normalize_inline_code(code: &str) -> Cow<'_, str> {
    if !code.contains('\\') {
        return Cow::Borrowed(code);
    }

    let mut result = String::with_capacity(code.len());
    let mut chars = code.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;
    let mut escape_in_quote = false;

    while let Some(ch) = chars.next() {
        if in_single {
            result.push(ch);
            if escape_in_quote {
                escape_in_quote = false;
            } else if ch == '\\' {
                escape_in_quote = true;
            } else if ch == '\'' {
                in_single = false;
            }
            continue;
        }

        if in_double {
            result.push(ch);
            if escape_in_quote {
                escape_in_quote = false;
            } else if ch == '\\' {
                escape_in_quote = true;
            } else if ch == '"' {
                in_double = false;
            }
            continue;
        }

        match ch {
            '\'' => {
                in_single = true;
                result.push(ch);
            }
            '"' => {
                in_double = true;
                result.push(ch);
            }
            '\\' => match chars.next() {
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('\\') => result.push('\\'),
                Some(other) => {
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            },
            _ => result.push(ch),
        }
    }

    Cow::Owned(result)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionOutcome {
    pub language: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration: Duration,
}

impl ExecutionOutcome {
    pub fn success(&self) -> bool {
        match self.exit_code {
            Some(code) => code == 0,
            None => self.stderr.trim().is_empty(),
        }
    }
}

pub struct LanguageRegistry {
    engines: HashMap<String, Box<dyn LanguageEngine + Send + Sync>>, // keyed by canonical id
    alias_lookup: HashMap<String, String>,
}

impl LanguageRegistry {
    pub fn bootstrap() -> Self {
        let mut registry = Self {
            engines: HashMap::new(),
            alias_lookup: HashMap::new(),
        };

        registry.register_language(PythonEngine::new());
        registry.register_language(BashEngine::new());
        registry.register_language(JavascriptEngine::new());
        registry.register_language(RubyEngine::new());
        registry.register_language(RustEngine::new());
        registry.register_language(GoEngine::new());
        registry.register_language(CSharpEngine::new());
        registry.register_language(TypeScriptEngine::new());
        registry.register_language(LuaEngine::new());
        registry.register_language(JavaEngine::new());
        registry.register_language(GroovyEngine::new());
        registry.register_language(PhpEngine::new());
        registry.register_language(KotlinEngine::new());
        registry.register_language(CEngine::new());
        registry.register_language(CppEngine::new());
        registry.register_language(REngine::new());
        registry.register_language(DartEngine::new());
        registry.register_language(SwiftEngine::new());
        registry.register_language(PerlEngine::new());
        registry.register_language(JuliaEngine::new());
        registry.register_language(HaskellEngine::new());
        registry.register_language(ElixirEngine::new());
        registry.register_language(CrystalEngine::new());
        registry.register_language(ZigEngine::new());
        registry.register_language(NimEngine::new());

        registry
    }

    pub fn register_language<E>(&mut self, engine: E)
    where
        E: LanguageEngine + Send + Sync + 'static,
    {
        let id = engine.id().to_string();
        for alias in engine.aliases() {
            self.alias_lookup
                .insert(canonical_language_id(alias), id.clone());
        }
        self.alias_lookup
            .insert(canonical_language_id(&id), id.clone());
        self.engines.insert(id, Box::new(engine));
    }

    pub fn resolve(&self, spec: &LanguageSpec) -> Option<&(dyn LanguageEngine + Send + Sync)> {
        let canonical = canonical_language_id(spec.canonical_id());
        let target_id = self
            .alias_lookup
            .get(&canonical)
            .cloned()
            .unwrap_or_else(|| canonical);
        self.engines
            .get(&target_id)
            .map(|engine| engine.as_ref() as _)
    }

    pub fn resolve_by_id(&self, id: &str) -> Option<&(dyn LanguageEngine + Send + Sync)> {
        let canonical = canonical_language_id(id);
        let target_id = self
            .alias_lookup
            .get(&canonical)
            .cloned()
            .unwrap_or_else(|| canonical);
        self.engines
            .get(&target_id)
            .map(|engine| engine.as_ref() as _)
    }

    pub fn engines(&self) -> impl Iterator<Item = &(dyn LanguageEngine + Send + Sync)> {
        self.engines.values().map(|engine| engine.as_ref() as _)
    }

    pub fn known_languages(&self) -> Vec<String> {
        let mut ids: Vec<_> = self.engines.keys().cloned().collect();
        ids.sort();
        ids
    }
}

/// Returns the package install command for a language, if one exists.
/// Returns (binary, args_before_package) so the caller can append the package name.
pub fn package_install_command(
    language_id: &str,
) -> Option<(&'static str, &'static [&'static str])> {
    match language_id {
        "python" => Some(("pip", &["install"])),
        "javascript" | "typescript" => Some(("npm", &["install"])),
        "rust" => Some(("cargo", &["add"])),
        "go" => Some(("go", &["get"])),
        "ruby" => Some(("gem", &["install"])),
        "php" => Some(("composer", &["require"])),
        "lua" => Some(("luarocks", &["install"])),
        "dart" => Some(("dart", &["pub", "add"])),
        "perl" => Some(("cpanm", &[])),
        "julia" => Some(("julia", &["-e"])), // special: wraps in Pkg.add()
        "haskell" => Some(("cabal", &["install"])),
        "nim" => Some(("nimble", &["install"])),
        "r" => Some(("Rscript", &["-e"])), // special: wraps in install.packages()
        "kotlin" => None,                  // no standard CLI package manager
        "java" => None,                    // maven/gradle are project-based
        "c" | "cpp" => None,               // system packages
        "bash" => None,
        "swift" => None,
        "crystal" => Some(("shards", &["install"])),
        "elixir" => None, // mix deps.get is project-based
        "groovy" => None,
        "csharp" => Some(("dotnet", &["add", "package"])),
        "zig" => None,
        _ => None,
    }
}

/// Build a full install command for a package in the given language.
/// Returns None if the language has no package manager.
pub fn build_install_command(language_id: &str, package: &str) -> Option<std::process::Command> {
    let (binary, base_args) = package_install_command(language_id)?;

    let mut cmd = std::process::Command::new(binary);

    match language_id {
        "julia" => {
            // julia -e 'using Pkg; Pkg.add("package")'
            cmd.arg("-e")
                .arg(format!("using Pkg; Pkg.add(\"{package}\")"));
        }
        "r" => {
            // Rscript -e 'install.packages("package", repos="https://cran.r-project.org")'
            cmd.arg("-e").arg(format!(
                "install.packages(\"{package}\", repos=\"https://cran.r-project.org\")"
            ));
        }
        _ => {
            for arg in base_args {
                cmd.arg(arg);
            }
            cmd.arg(package);
        }
    }

    Some(cmd)
}

pub fn default_language() -> &'static str {
    "python"
}

pub fn ensure_known_language(spec: &LanguageSpec, registry: &LanguageRegistry) -> Result<()> {
    if registry.resolve(spec).is_some() {
        return Ok(());
    }

    let available = registry.known_languages();
    bail!(
        "Unknown language '{}'. Available languages: {}",
        spec.canonical_id(),
        available.join(", ")
    )
}

pub fn detect_language_for_source(
    source: &ExecutionPayload,
    registry: &LanguageRegistry,
) -> Option<LanguageSpec> {
    if let Some(path) = source.as_file_path() {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext_lower = ext.to_ascii_lowercase();
            if let Some(lang) = extension_to_language(&ext_lower) {
                let spec = LanguageSpec::new(lang);
                if registry.resolve(&spec).is_some() {
                    return Some(spec);
                }
            }
        }
    }

    if let Some(code) = source.as_inline() {
        if let Some(lang) = crate::detect::detect_language_from_snippet(code) {
            let spec = LanguageSpec::new(lang);
            if registry.resolve(&spec).is_some() {
                return Some(spec);
            }
        }
    }

    None
}

fn extension_to_language(ext: &str) -> Option<&'static str> {
    match ext {
        "py" | "pyw" => Some("python"),
        "rs" => Some("rust"),
        "go" => Some("go"),
        "cs" => Some("csharp"),
        "ts" | "tsx" => Some("typescript"),
        "js" | "mjs" | "cjs" | "jsx" => Some("javascript"),
        "rb" => Some("ruby"),
        "lua" => Some("lua"),
        "java" => Some("java"),
        "groovy" => Some("groovy"),
        "php" => Some("php"),
        "kt" | "kts" => Some("kotlin"),
        "c" => Some("c"),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Some("cpp"),
        "sh" | "bash" | "zsh" => Some("bash"),
        "r" => Some("r"),
        "dart" => Some("dart"),
        "swift" => Some("swift"),
        "perl" | "pl" | "pm" => Some("perl"),
        "julia" | "jl" => Some("julia"),
        "hs" => Some("haskell"),
        "ex" | "exs" => Some("elixir"),
        "cr" => Some("crystal"),
        "zig" => Some("zig"),
        "nim" => Some("nim"),
        _ => None,
    }
}
