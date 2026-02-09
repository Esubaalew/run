use std::io::{self, Write};
use std::path::Path;
use std::time::SystemTime;

use anyhow::{Context, Result};

use crate::cli::{Command, ExecutionSpec};
use crate::engine::{
    ExecutionPayload, LanguageRegistry, build_install_command, default_language,
    detect_language_for_source, ensure_known_language,
};
use crate::language::LanguageSpec;
use crate::output;
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
        Command::CheckToolchains => check_toolchains(&registry),
        Command::ShowVersions { language } => show_versions(&registry, language),
        Command::Install { language, package } => {
            let lang = language.unwrap_or_else(|| LanguageSpec::new(default_language()));
            install_package(&lang, &package)
        }
        Command::Bench { spec, iterations } => bench_run(spec, &registry, iterations),
        Command::Watch { spec } => watch_run(spec, &registry),
    }
}

fn check_toolchains(registry: &LanguageRegistry) -> Result<i32> {
    println!("Checking language toolchains...\n");

    let mut available = 0u32;
    let mut missing = 0u32;

    let mut languages: Vec<_> = registry.known_languages();
    languages.sort();

    for lang_id in &languages {
        let spec = LanguageSpec::new(lang_id.to_string());
        if let Some(engine) = registry.resolve(&spec) {
            let status = match engine.validate() {
                Ok(()) => {
                    available += 1;
                    "\x1b[32m OK \x1b[0m"
                }
                Err(_) => {
                    missing += 1;
                    "\x1b[31mMISS\x1b[0m"
                }
            };
            println!("  [{status}] {:<14} {}", engine.display_name(), lang_id);
        }
    }

    println!();
    println!(
        "  {} available, {} missing, {} total",
        available,
        missing,
        available + missing
    );

    if missing > 0 {
        println!("\n  Tip: Install missing toolchains to enable those languages.");
    }

    Ok(0)
}

fn show_versions(
    registry: &LanguageRegistry,
    language: Option<LanguageSpec>,
) -> Result<i32> {
    println!("Language toolchain versions...\n");

    let mut available = 0u32;
    let mut missing = 0u32;

    let mut languages: Vec<String> = if let Some(lang) = language {
        vec![lang.canonical_id().to_string()]
    } else {
        registry
            .known_languages()
            .into_iter()
            .map(|value| value.to_string())
            .collect()
    };
    languages.sort();

    for lang_id in &languages {
        let spec = LanguageSpec::new(lang_id.to_string());
        if let Some(engine) = registry.resolve(&spec) {
            match engine.toolchain_version() {
                Ok(Some(version)) => {
                    available += 1;
                    println!(
                        "  [\x1b[32m OK \x1b[0m] {:<14} {} - {}",
                        engine.display_name(),
                        lang_id,
                        version
                    );
                }
                Ok(None) => {
                    available += 1;
                    println!(
                        "  [\x1b[33m ?? \x1b[0m] {:<14} {} - unknown",
                        engine.display_name(),
                        lang_id
                    );
                }
                Err(_) => {
                    missing += 1;
                    println!(
                        "  [\x1b[31mMISS\x1b[0m] {:<14} {}",
                        engine.display_name(),
                        lang_id
                    );
                }
            }
        }
    }

    println!();
    println!(
        "  {} available, {} missing, {} total",
        available,
        missing,
        available + missing
    );

    if missing > 0 {
        println!("\n  Tip: Install missing toolchains to enable those languages.");
    }

    Ok(0)
}

fn execute_once(spec: ExecutionSpec, registry: &LanguageRegistry) -> Result<i32> {
    let payload = ExecutionPayload::from_input_source(&spec.source, &spec.args)
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

    if let Err(e) = engine.validate() {
        let display = engine.display_name();
        let id = engine.id();
        eprintln!(
            "Warning: {display} ({id}) toolchain not found: {e:#}\n\
             Install the required toolchain and ensure it is on your PATH."
        );
        return Err(e.context(format!("{display} is not available")));
    }

    let outcome = engine.execute(&payload)?;

    if !outcome.stdout.is_empty() {
        print!("{}", outcome.stdout);
        io::stdout().flush().ok();
    }
    if !outcome.stderr.is_empty() {
        let formatted = output::format_stderr(engine.display_name(), &outcome.stderr, outcome.success());
        eprint!("{formatted}");
        io::stderr().flush().ok();
    }

    // Show timing on stderr if RUN_TIMING=1 or if execution was slow (>1s)
    let show_timing = std::env::var("RUN_TIMING").is_ok_and(|v| v == "1" || v == "true");
    if show_timing || outcome.duration.as_millis() > 1000 {
        eprintln!(
            "\x1b[2m[{} {}ms]\x1b[0m",
            engine.display_name(),
            outcome.duration.as_millis()
        );
    }

    Ok(outcome
        .exit_code
        .unwrap_or(if outcome.success() { 0 } else { 1 }))
}

fn install_package(language: &LanguageSpec, package: &str) -> Result<i32> {
    let lang_id = language.canonical_id();
    let override_key = format!("RUN_INSTALL_COMMAND_{}", lang_id.to_ascii_uppercase());
    let override_value = std::env::var(&override_key).ok();

    let Some(mut cmd) = build_install_command(lang_id, package) else {
        if override_value.is_some() {
            eprintln!(
                "\x1b[31mError:\x1b[0m {override_key} is set but could not be parsed.\n\
                 Provide a valid command, e.g. {override_key}=\"uv pip install {{package}}\""
            );
            return Ok(1);
        }
        eprintln!(
            "\x1b[31mError:\x1b[0m No package manager available for '{lang_id}'.\n\
             This language doesn't have a standard CLI package manager.\n\
             Tip: You can override with {override_key}=\"<cmd> {{package}}\"",
        );
        return Ok(1);
    };

    eprintln!("\x1b[36m[run]\x1b[0m Installing '{package}' for {lang_id}...");

    let result = cmd
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status();

    match result {
        Ok(status) if status.success() => {
            eprintln!("\x1b[32m[run]\x1b[0m Successfully installed '{package}' for {lang_id}");
            Ok(0)
        }
        Ok(status) => {
            eprintln!("\x1b[31m[run]\x1b[0m Failed to install '{package}' for {lang_id}");
            Ok(status.code().unwrap_or(1))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            let program = cmd.get_program().to_string_lossy();
            eprintln!(
                "\x1b[31m[run]\x1b[0m Package manager not found: {program}"
            );
            eprintln!("Tip: install it or set {override_key}=\"<cmd> {{package}}\"");
            Ok(1)
        }
        Err(err) => {
            return Err(err).with_context(|| format!("failed to run package manager for {lang_id}"));
        }
    }
}

fn bench_run(spec: ExecutionSpec, registry: &LanguageRegistry, iterations: u32) -> Result<i32> {
    let payload = ExecutionPayload::from_input_source(&spec.source, &spec.args)
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

    engine
        .validate()
        .with_context(|| format!("{} is not available", engine.display_name()))?;

    eprintln!(
        "\x1b[1mBenchmark:\x1b[0m {} — {} iteration{}",
        engine.display_name(),
        iterations,
        if iterations == 1 { "" } else { "s" }
    );

    // Warmup run (not counted)
    let warmup = engine.execute(&payload)?;
    if !warmup.success() {
        eprintln!("\x1b[31mError:\x1b[0m Code failed during warmup run");
        if !warmup.stderr.is_empty() {
            eprint!("{}", warmup.stderr);
        }
        return Ok(1);
    }
    eprintln!("\x1b[2m  warmup: {}ms\x1b[0m", warmup.duration.as_millis());

    let mut times: Vec<f64> = Vec::with_capacity(iterations as usize);

    for i in 0..iterations {
        let outcome = engine.execute(&payload)?;
        let ms = outcome.duration.as_secs_f64() * 1000.0;
        times.push(ms);

        if i < 3 || i == iterations - 1 || (i + 1) % 10 == 0 {
            eprintln!("\x1b[2m  run {}: {:.2}ms\x1b[0m", i + 1, ms);
        }
    }

    times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let total: f64 = times.iter().sum();
    let avg = total / times.len() as f64;
    let min = times.first().copied().unwrap_or(0.0);
    let max = times.last().copied().unwrap_or(0.0);
    let median = if times.len().is_multiple_of(2) && times.len() >= 2 {
        (times[times.len() / 2 - 1] + times[times.len() / 2]) / 2.0
    } else {
        times[times.len() / 2]
    };

    // Standard deviation
    let variance: f64 = times.iter().map(|t| (t - avg).powi(2)).sum::<f64>() / times.len() as f64;
    let stddev = variance.sqrt();

    eprintln!();
    eprintln!("\x1b[1mResults ({} runs):\x1b[0m", iterations);
    eprintln!("  min:    \x1b[32m{:.2}ms\x1b[0m", min);
    eprintln!("  max:    \x1b[33m{:.2}ms\x1b[0m", max);
    eprintln!("  avg:    \x1b[36m{:.2}ms\x1b[0m", avg);
    eprintln!("  median: \x1b[36m{:.2}ms\x1b[0m", median);
    eprintln!("  stddev: {:.2}ms", stddev);

    if !warmup.stdout.is_empty() {
        print!("{}", warmup.stdout);
        io::stdout().flush().ok();
    }

    Ok(0)
}

fn watch_run(spec: ExecutionSpec, registry: &LanguageRegistry) -> Result<i32> {
    use crate::cli::InputSource;

    let file_path = match &spec.source {
        InputSource::File(p) => p.clone(),
        _ => anyhow::bail!("--watch requires a file path (use -f or pass a file as argument)"),
    };

    if !file_path.exists() {
        anyhow::bail!("File not found: {}", file_path.display());
    }

    let payload = ExecutionPayload::from_input_source(&spec.source, &spec.args)
        .context("failed to materialize execution payload")?;
    let language = resolve_language(
        spec.language.clone(),
        spec.detect_language,
        Some(&payload),
        registry,
    )?;

    let engine = registry
        .resolve(&language)
        .context("failed to resolve language engine")?;

    engine
        .validate()
        .with_context(|| format!("{} is not available", engine.display_name()))?;

    eprintln!(
        "\x1b[1m[watch]\x1b[0m Watching \x1b[36m{}\x1b[0m ({}). Press Ctrl+C to stop.",
        file_path.display(),
        engine.display_name()
    );

    fn get_mtime(path: &Path) -> Option<SystemTime> {
        std::fs::metadata(path).ok()?.modified().ok()
    }

    let mut last_mtime = get_mtime(&file_path);
    let mut run_count = 0u32;

    // Initial run
    run_count += 1;
    eprintln!("\n\x1b[2m--- run #{run_count} ---\x1b[0m");
    run_file_once(&file_path, engine, &spec.args);

    loop {
        std::thread::sleep(std::time::Duration::from_millis(300));

        let current_mtime = get_mtime(&file_path);
        if current_mtime != last_mtime {
            last_mtime = current_mtime;
            run_count += 1;

            eprintln!("\n\x1b[2m--- run #{run_count} ---\x1b[0m");

            run_file_once(&file_path, engine, &spec.args);
        }
    }
}

fn run_file_once(
    file_path: &Path,
    engine: &dyn crate::engine::LanguageEngine,
    args: &[String],
) {
    let payload = ExecutionPayload::File {
        path: file_path.to_path_buf(),
        args: args.to_vec(),
    };
    match engine.execute(&payload) {
        Ok(outcome) => {
            if !outcome.stdout.is_empty() {
                print!("{}", outcome.stdout);
                io::stdout().flush().ok();
            }
            if !outcome.stderr.is_empty() {
                eprint!("\x1b[31m{}\x1b[0m", outcome.stderr);
                io::stderr().flush().ok();
            }
            let ms = outcome.duration.as_millis();
            let status = if outcome.success() {
                "\x1b[32mOK\x1b[0m"
            } else {
                "\x1b[31mFAIL\x1b[0m"
            };
            eprintln!("\x1b[2m[{status} {ms}ms]\x1b[0m");
        }
        Err(e) => {
            eprintln!("\x1b[31mError:\x1b[0m {e:#}");
        }
    }
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

    if allow_detect
        && let Some(payload) = payload
        && let Some(detected) = detect_language_for_source(payload, registry)
    {
        return Ok(detected);
    }

    let default = LanguageSpec::new(default_language());
    ensure_known_language(&default, registry)?;
    Ok(default)
}
