mod bash;
mod c;
mod cpp;
mod crystal;
mod csharp;
mod dart;
mod elixir;
mod go;
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
use std::path::Path;
use std::time::Duration;

use anyhow::{Result, bail};

use crate::cli::InputSource;
use crate::language::{LanguageSpec, canonical_language_id};

pub use bash::BashEngine;
pub use c::CEngine;
pub use cpp::CppEngine;
pub use crystal::CrystalEngine;
pub use csharp::CSharpEngine;
pub use dart::DartEngine;
pub use elixir::ElixirEngine;
pub use go::GoEngine;
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
