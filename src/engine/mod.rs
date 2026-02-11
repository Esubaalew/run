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
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use crate::cli::InputSource;
use crate::language::{LanguageSpec, canonical_language_id};

// ---------------------------------------------------------------------------
// Compilation cache: hash source code -> reuse compiled binaries
// ---------------------------------------------------------------------------

static COMPILE_CACHE: LazyLock<Mutex<CompileCache>> =
    LazyLock::new(|| Mutex::new(CompileCache::new()));
static SCCACHE_INIT: OnceLock<()> = OnceLock::new();
static SCCACHE_READY: AtomicBool = AtomicBool::new(false);
static PERF_COUNTERS: LazyLock<Mutex<HashMap<String, u64>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

struct CompileCache {
    dir: PathBuf,
    entries: HashMap<String, PathBuf>,
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

    fn get(&self, cache_id: &str) -> Option<&PathBuf> {
        self.entries.get(cache_id).filter(|p| p.exists())
    }

    fn insert(&mut self, cache_id: String, path: PathBuf) {
        self.entries.insert(cache_id, path);
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


fn cache_id(namespace: &str, source_hash: u64) -> String {
    format!("{namespace}-{:016x}", source_hash)
}

fn cache_path(dir: &Path, namespace: &str, source_hash: u64) -> PathBuf {
    let suffix = std::env::consts::EXE_SUFFIX;
    let cached_name = format!("{}{}", cache_id(namespace, source_hash), suffix);
    dir.join(cached_name)
}

fn perf_file_path() -> PathBuf {
    std::env::temp_dir().join("run-perf-counters.csv")
}

fn read_perf_file() -> HashMap<String, u64> {
    let path = perf_file_path();
    let Ok(text) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    let mut map = HashMap::new();
    for line in text.lines() {
        if let Some((key, value)) = line.split_once(',')
            && let Ok(parsed) = value.parse::<u64>()
        {
            map.insert(key.to_string(), parsed);
        }
    }
    map
}

fn write_perf_file(map: &HashMap<String, u64>) {
    let mut rows = map.iter().collect::<Vec<_>>();
    rows.sort_by(|a, b| a.0.cmp(b.0));
    let mut buf = String::new();
    for (key, value) in rows {
        buf.push_str(key);
        buf.push(',');
        buf.push_str(&value.to_string());
        buf.push('\n');
    }
    let _ = std::fs::write(perf_file_path(), buf);
}

/// Look up a cached binary for the given language namespace + source hash.
/// Returns Some(path) if a valid cached binary exists.
pub fn cache_lookup(namespace: &str, source_hash: u64) -> Option<PathBuf> {
    let mut cache = COMPILE_CACHE.lock().ok()?;
    let id = cache_id(namespace, source_hash);

    if let Some(path) = cache.get(&id).cloned() {
        return Some(path);
    }

    let disk_path = cache_path(cache.cache_dir(), namespace, source_hash);
    if disk_path.exists() {
        cache.insert(id, disk_path.clone());
        return Some(disk_path);
    }

    None
}

/// Store a compiled binary in the cache. Copies the binary to the cache directory.
pub fn cache_store(namespace: &str, source_hash: u64, binary: &Path) -> Option<PathBuf> {
    let mut cache = COMPILE_CACHE.lock().ok()?;
    let cached_path = cache_path(cache.cache_dir(), namespace, source_hash);
    if std::fs::copy(binary, &cached_path).is_ok() {
        // Ensure executable permission on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&cached_path, std::fs::Permissions::from_mode(0o755));
        }
        cache.insert(cache_id(namespace, source_hash), cached_path.clone());
        Some(cached_path)
    } else {
        None
    }
}

/// Execute a cached binary, returning the Output. Returns None if no cache entry.
pub fn try_cached_execution(namespace: &str, source_hash: u64) -> Option<std::process::Output> {
    let cached = cache_lookup(namespace, source_hash)?;
    let mut cmd = std::process::Command::new(&cached);
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::inherit());
    cmd.output().ok()
}

/// Build a compiler command with optional daemon/cache wrappers.
///
/// Behavior controlled by RUN_COMPILER_DAEMON:
/// - off: use raw compiler
/// - ccache: force ccache wrapper
/// - sccache: force sccache wrapper
/// - auto (default): prefer sccache, then ccache, else raw compiler
pub fn compiler_command(compiler: &Path) -> Command {
    let mode = std::env::var("RUN_COMPILER_DAEMON")
        .unwrap_or_else(|_| "adaptive".to_string())
        .to_ascii_lowercase();
    if mode == "off" {
        perf_record("global", "compiler.raw");
        return Command::new(compiler);
    }

    let want_sccache = mode == "sccache" || mode == "auto" || mode == "adaptive";
    if want_sccache && let Ok(sccache) = which::which("sccache") {
        if mode == "adaptive" && !SCCACHE_READY.load(Ordering::Relaxed) {
            let _ = SCCACHE_INIT.get_or_init(|| {
                let sccache_clone = sccache.clone();
                std::thread::spawn(move || {
                    let ready = std::process::Command::new(&sccache_clone)
                        .arg("--start-server")
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status()
                        .is_ok_and(|s| s.success());
                    if ready {
                        SCCACHE_READY.store(true, Ordering::Relaxed);
                    }
                });
            });
            perf_record("global", "compiler.raw.adaptive_warmup");
            return Command::new(compiler);
        }
        let _ = SCCACHE_INIT.get_or_init(|| {
            let ready = std::process::Command::new(&sccache)
                .arg("--start-server")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            if ready {
                SCCACHE_READY.store(true, Ordering::Relaxed);
            }
        });
        let mut cmd = Command::new(sccache);
        cmd.arg(compiler);
        perf_record("global", "compiler.sccache");
        return cmd;
    }

    let want_ccache = mode == "ccache" || mode == "auto" || mode == "adaptive";
    if want_ccache && let Ok(ccache) = which::which("ccache") {
        let mut cmd = Command::new(ccache);
        cmd.arg(compiler);
        perf_record("global", "compiler.ccache");
        return cmd;
    }

    perf_record("global", "compiler.raw.fallback");
    Command::new(compiler)
}

pub fn perf_record(language: &str, event: &str) {
    let key = format!("{language}.{event}");
    if let Ok(mut counters) = PERF_COUNTERS.lock() {
        let entry = counters.entry(key).or_insert(0);
        *entry += 1;
        let mut disk = read_perf_file();
        let disk_entry = disk.entry(language.to_string() + "." + event).or_insert(0);
        *disk_entry += 1;
        write_perf_file(&disk);
    }
}

pub fn perf_snapshot() -> Vec<(String, u64)> {
    let disk = read_perf_file();
    let mut rows = if !disk.is_empty() {
        disk.into_iter().collect::<Vec<_>>()
    } else if let Ok(counters) = PERF_COUNTERS.lock() {
        counters
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows
}

pub fn perf_reset() {
    if let Ok(mut counters) = PERF_COUNTERS.lock() {
        counters.clear();
    }
    let _ = std::fs::remove_file(perf_file_path());
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
    fn toolchain_version(&self) -> Result<Option<String>> {
        Ok(None)
    }
    fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionOutcome>;
    fn start_session(&self) -> Result<Box<dyn LanguageSession>> {
        bail!("{} does not support interactive sessions yet", self.id())
    }
}

pub(crate) fn version_line_from_output(output: &Output) -> Option<String> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    for line in stderr.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

pub(crate) fn run_version_command(mut cmd: Command, context: &str) -> Result<Option<String>> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let output = cmd
        .output()
        .with_context(|| format!("failed to invoke {context}"))?;
    let version = version_line_from_output(&output);
    if output.status.success() || version.is_some() {
        Ok(version)
    } else {
        bail!("{context} exited with status {}", output.status);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionPayload {
    Inline {
        code: String,
        args: Vec<String>,
    },
    File {
        path: std::path::PathBuf,
        args: Vec<String>,
    },
    Stdin {
        code: String,
        args: Vec<String>,
    },
}

impl ExecutionPayload {
    pub fn from_input_source(source: &InputSource, args: &[String]) -> Result<Self> {
        let args = args.to_vec();
        match source {
            InputSource::Inline(code) => Ok(Self::Inline {
                code: normalize_inline_code(code).into_owned(),
                args,
            }),
            InputSource::File(path) => Ok(Self::File {
                path: path.clone(),
                args,
            }),
            InputSource::Stdin => {
                use std::io::Read;
                let mut buffer = String::new();
                std::io::stdin().read_to_string(&mut buffer)?;
                Ok(Self::Stdin { code: buffer, args })
            }
        }
    }

    pub fn as_inline(&self) -> Option<&str> {
        match self {
            ExecutionPayload::Inline { code, .. } => Some(code.as_str()),
            ExecutionPayload::Stdin { code, .. } => Some(code.as_str()),
            ExecutionPayload::File { .. } => None,
        }
    }

    pub fn as_file_path(&self) -> Option<&Path> {
        match self {
            ExecutionPayload::File { path, .. } => Some(path.as_path()),
            _ => None,
        }
    }

    pub fn args(&self) -> &[String] {
        match self {
            ExecutionPayload::Inline { args, .. } => args.as_slice(),
            ExecutionPayload::File { args, .. } => args.as_slice(),
            ExecutionPayload::Stdin { args, .. } => args.as_slice(),
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
            .unwrap_or(canonical);
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
            .unwrap_or(canonical);
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

fn install_override_command(language_id: &str, package: &str) -> Option<std::process::Command> {
    let key = format!("RUN_INSTALL_COMMAND_{}", language_id.to_ascii_uppercase());
    let template = std::env::var(&key).ok()?;
    let expanded = if template.contains("{package}") {
        template.replace("{package}", package)
    } else {
        format!("{template} {package}")
    };
    let parts = shell_words::split(&expanded).ok()?;
    if parts.is_empty() {
        return None;
    }
    let mut cmd = std::process::Command::new(&parts[0]);
    for arg in &parts[1..] {
        cmd.arg(arg);
    }
    Some(cmd)
}

/// Build a full install command for a package in the given language.
/// Returns None if the language has no package manager.
pub fn build_install_command(language_id: &str, package: &str) -> Option<std::process::Command> {
    if let Some(cmd) = install_override_command(language_id, package) {
        return Some(cmd);
    }

    if language_id == "python" {
        let python = python::resolve_python_binary();
        let mut cmd = std::process::Command::new(python);
        cmd.arg("-m").arg("pip").arg("install").arg(package);
        return Some(cmd);
    }

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
    if let Some(path) = source.as_file_path()
        && let Some(ext) = path.extension().and_then(|e| e.to_str())
    {
        let ext_lower = ext.to_ascii_lowercase();
        if let Some(lang) = extension_to_language(&ext_lower) {
            let spec = LanguageSpec::new(lang);
            if registry.resolve(&spec).is_some() {
                return Some(spec);
            }
        }
    }

    if let Some(code) = source.as_inline()
        && let Some(lang) = crate::detect::detect_language_from_snippet(code)
    {
        let spec = LanguageSpec::new(lang);
        if registry.resolve(&spec).is_some() {
            return Some(spec);
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
