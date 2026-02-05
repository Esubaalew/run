use std::io::{self, Write};

use anyhow::{Context, Result};

use crate::cli::{Command, ExecutionSpec};
use crate::engine::{
    ExecutionPayload, LanguageRegistry, default_language, detect_language_for_source,
    ensure_known_language,
};
use crate::language::LanguageSpec;
use crate::repl;
use crate::version;

pub fn run(command: Command) -> Result<i32> {
    let registry = LanguageRegistry::bootstrap();

    match command {
        Command::Execute(spec) => execute_once(spec, &registry),
        Command::Repl {
            initial_language,
            detect_language,
        } => {
            let language = resolve_language(initial_language, detect_language, None, &registry)?;
            repl::run_repl(language, registry, detect_language)
        }
        Command::ShowVersion => {
            println!("{}", version::describe());
            Ok(0)
        }
    }
}

fn execute_once(spec: ExecutionSpec, registry: &LanguageRegistry) -> Result<i32> {
    let payload = ExecutionPayload::from_input_source(&spec.source)
        .context("failed to materialize execution payload")?;
    let language = resolve_language(
        spec.language,
        spec.detect_language,
        Some(&payload),
        registry,
    )?;

    let engine = registry
        .resolve(&language)
        .context("failed to resolve language engine")?;

    engine.validate().ok();
    let outcome = engine.execute(&payload)?;

    if !outcome.stdout.is_empty() {
        print!("{}", outcome.stdout);
        io::stdout().flush().ok();
    }
    if !outcome.stderr.is_empty() {
        eprint!("{}", outcome.stderr);
        io::stderr().flush().ok();
    }

    Ok(outcome
        .exit_code
        .unwrap_or(if outcome.success() { 0 } else { 1 }))
}

fn resolve_language(
    explicit: Option<LanguageSpec>,
    allow_detect: bool,
    payload: Option<&ExecutionPayload>,
    registry: &LanguageRegistry,
) -> Result<LanguageSpec> {
    if let Some(spec) = explicit {
        ensure_known_language(&spec, registry)?;
        return Ok(spec);
    }

    if allow_detect {
        if let Some(payload) = payload {
            if let Some(detected) = detect_language_for_source(payload, registry) {
                return Ok(detected);
            }
        }
    }

    let default = LanguageSpec::new(default_language());
    ensure_known_language(&default, registry)?;
    Ok(default)
}
