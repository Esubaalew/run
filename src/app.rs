use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::sync::mpsc;
use std::time::SystemTime;

use anyhow::{Context, Result};

use crate::cli::{CacheAction, Command, ExecutionSpec, InputSource};
use crate::engine::{
    ExecutionPayload, LanguageRegistry, build_install_command, default_language,
    detect_language_for_source, ensure_known_language, perf_reset, perf_snapshot,
};
use crate::language::LanguageSpec;
use crate::output;
use crate::repl;
use crate::version;

pub fn run(command: Command) -> Result<i32> {
    match command {
        Command::ShowVersion => {
            println!("{}", version::describe());
            Ok(0)
        }
        Command::PerfReport => {
            print_perf_report();
            Ok(0)
        }
        Command::PerfReset => {
            perf_reset();
            eprintln!("\x1b[2m[perf] counters reset\x1b[0m");
            Ok(0)
        }
        Command::Cache { action } => cache_command(action),
        other => run_with_registry(other),
    }
}

fn run_with_registry(command: Command) -> Result<i32> {
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
        Command::ShowVersion => unreachable!("handled before registry bootstrap"),
        Command::CheckToolchains => check_toolchains(&registry),
        Command::ShowVersions { language } => show_versions(&registry, language),
        Command::Install { language, package } => {
            let lang = language.unwrap_or_else(|| LanguageSpec::new(default_language()));
            install_package(&lang, &package)
        }
        Command::Bench { spec, iterations } => bench_run(spec, &registry, iterations),
        Command::Watch { spec } => watch_run(spec, &registry),
        Command::WatchFile {
            path,
            language,
            args,
        } => watch_run(
            ExecutionSpec {
                language,
                source: InputSource::File(path),
                detect_language: true,
                args,
                json: false,
            },
            &registry,
        ),
        Command::Format { path } => format_file(&path),
        Command::Snippet {
            language,
            name,
            list,
        } => snippet_command(language, name, list),
        Command::Doctor => doctor(&registry),
        Command::Cache { .. } => unreachable!("handled before registry bootstrap"),
        Command::Share { path, port } => share_file(&path, port, &registry),
        Command::PerfReport | Command::PerfReset => {
            unreachable!("handled before registry bootstrap")
        }
    }
}

fn print_perf_report() {
    let rows = perf_snapshot();
    if rows.is_empty() {
        println!("perf_counter,count");
        return;
    }
    println!("perf_counter,count");
    for (key, value) in rows {
        println!("{key},{value}");
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

fn show_versions(registry: &LanguageRegistry, language: Option<LanguageSpec>) -> Result<i32> {
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

    let outcome = match engine.execute(&payload) {
        Ok(outcome) => outcome,
        Err(err) if err.to_string().contains("Execution timed out") => {
            eprintln!(
                "[run] Execution timed out after {}s",
                crate::runtime::timeout_secs()
            );
            return Ok(124);
        }
        Err(err) => return Err(err),
    };

    if spec.json {
        let exit_code = outcome
            .exit_code
            .unwrap_or(if outcome.success() { 0 } else { 1 });
        let version = engine
            .toolchain_version()
            .ok()
            .flatten()
            .unwrap_or_default();
        let envelope = serde_json::json!({
            "language": engine.id(),
            "stdout": outcome.stdout,
            "stderr": outcome.stderr,
            "exit_code": exit_code,
            "duration_ms": outcome.duration.as_millis(),
            "toolchain_version": version,
        });
        println!("{}", serde_json::to_string(&envelope)?);
        return Ok(exit_code);
    }

    if !outcome.stdout.is_empty() {
        print!("{}", outcome.stdout);
        io::stdout().flush().ok();
    }
    if !outcome.stderr.is_empty() {
        let formatted =
            output::format_stderr(engine.display_name(), &outcome.stderr, outcome.success());
        eprint!("{formatted}");
        io::stderr().flush().ok();
    }

    // Show timing on stderr if requested or if execution was slow (>1s)
    let show_timing = crate::runtime::timing_enabled();
    if show_timing || outcome.duration.as_millis() > 1000 {
        eprintln!(
            "\x1b[2m[{} {}ms]\x1b[0m",
            engine.display_name(),
            outcome.duration.as_millis()
        );
    }

    if std::env::var("RUN_PERF_REPORT").is_ok_and(|v| v == "1" || v == "true") {
        eprintln!("\x1b[2m[perf]\x1b[0m");
        for (key, value) in perf_snapshot() {
            eprintln!("\x1b[2m  {key}={value}\x1b[0m");
        }
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
            eprintln!("\x1b[31m[run]\x1b[0m Package manager not found: {program}");
            eprintln!("Tip: install it or set {override_key}=\"<cmd> {{package}}\"");
            Ok(1)
        }
        Err(err) => {
            Err(err).with_context(|| format!("failed to run package manager for {lang_id}"))
        }
    }
}

fn cache_command(action: CacheAction) -> Result<i32> {
    match action {
        CacheAction::Stats => {
            let stats = crate::cache::stats()?;
            println!("Cache: {}", crate::cache::root_dir().display());
            println!("entries: {}", stats.entries);
            println!("bytes: {}", stats.total_bytes);
            for (lang, count) in stats.by_language {
                println!("{lang}: {count}");
            }
            Ok(0)
        }
        CacheAction::Clear => {
            crate::cache::clear()?;
            println!("[run] cache cleared");
            Ok(0)
        }
        CacheAction::ClearLang(lang) => {
            crate::cache::clear_lang(&lang)?;
            println!("[run] cache cleared for {lang}");
            Ok(0)
        }
    }
}

fn snippet_command(language: LanguageSpec, name: Option<String>, list: bool) -> Result<i32> {
    let language = language.canonical_id().to_string();
    let names = crate::templates::names_for_language(&language);
    if list {
        if names.is_empty() {
            eprintln!("[run] No snippets available for {language}");
            return Ok(2);
        }
        for name in names {
            println!("{name}");
        }
        return Ok(0);
    }

    let Some(name) = name else {
        eprintln!(
            "[run] snippet requires a template name. Available: {}",
            names.join(", ")
        );
        return Ok(2);
    };

    if let Some(template) = crate::templates::find(&language, &name) {
        print!("{}", template.source);
        io::stdout().flush().ok();
        Ok(0)
    } else {
        eprintln!(
            "[run] Unknown snippet '{name}' for {language}. Available: {}",
            names.join(", ")
        );
        Ok(2)
    }
}

fn format_file(path: &Path) -> Result<i32> {
    if !path.is_file() {
        eprintln!("[run] File not found: {}", path.display());
        return Ok(1);
    }

    let Some(lang) = language_from_path(path) else {
        eprintln!("[run] No formatter available for unknown");
        return Ok(2);
    };
    let candidates: &[(&str, &[&str])] = match lang {
        "python" => &[("black", &[]), ("autopep8", &["-i"])],
        "javascript" | "typescript" => &[("prettier", &["--write"])],
        "rust" => &[("rustfmt", &[])],
        "go" => &[("gofmt", &["-w"])],
        "c" | "cpp" => &[("clang-format", &["-i"])],
        "java" => &[("google-java-format", &["-i"])],
        _ => &[],
    };
    if candidates.is_empty() {
        eprintln!("[run] No formatter available for {lang}");
        return Ok(2);
    }

    for (program, args) in candidates {
        let Ok(binary) = which::which(program) else {
            continue;
        };
        let status = std::process::Command::new(binary)
            .args(*args)
            .arg(path)
            .status()
            .with_context(|| format!("failed to run formatter {program}"))?;
        return Ok(if status.success() {
            0
        } else {
            eprintln!("[run] formatter {program} failed");
            1
        });
    }

    eprintln!("[run] Formatter not found for {lang}");
    Ok(2)
}

fn doctor(registry: &LanguageRegistry) -> Result<i32> {
    println!(
        "{:<12} {:<16} {:<24} Status",
        "Language", "Toolchain", "Version"
    );
    println!("────────────────────────────────────────────────────────────");
    let mut missing = 0;
    let mut languages = registry.known_languages();
    languages.sort();
    for lang in languages {
        let spec = LanguageSpec::new(lang.clone());
        if let Some(engine) = registry.resolve(&spec) {
            let toolchain = toolchain_name(engine.id());
            match engine.validate() {
                Ok(()) => {
                    let version = engine
                        .toolchain_version()
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| "unknown".to_string());
                    let status = if version == "unknown" {
                        "⚠ Unknown"
                    } else {
                        "✓ OK"
                    };
                    println!(
                        "{:<12} {:<16} {:<24} {}",
                        engine.display_name(),
                        toolchain,
                        version.lines().next().unwrap_or("unknown"),
                        status
                    );
                }
                Err(_) => {
                    missing += 1;
                    println!(
                        "{:<12} {:<16} {:<24} ✗ MISSING",
                        engine.display_name(),
                        toolchain,
                        "✗ Not found"
                    );
                }
            }
        }
    }
    Ok(if missing == 0 { 0 } else { 1 })
}

fn share_file(path: &Path, port: Option<u16>, registry: &LanguageRegistry) -> Result<i32> {
    if !path.is_file() {
        eprintln!("[run] File not found: {}", path.display());
        return Ok(1);
    }
    let address = format!("127.0.0.1:{}", port.unwrap_or(0));
    let server = tiny_http::Server::http(&address)
        .map_err(|err| anyhow::anyhow!("failed to start share server: {err}"))?;
    let url = format!("http://{}", server.server_addr());
    println!("Sharing at {url} (Ctrl-C to stop)");

    let lang = language_from_path(path).unwrap_or("text");
    let spec = ExecutionSpec {
        language: (lang != "text").then(|| LanguageSpec::new(lang.to_string())),
        source: InputSource::File(path.to_path_buf()),
        detect_language: true,
        args: Vec::new(),
        json: false,
    };
    let output = execute_capture(spec, registry).unwrap_or_default();
    for request in server.incoming_requests() {
        let route = request.url().to_string();
        if route == "/raw" {
            let text = fs::read_to_string(path).unwrap_or_default();
            let _ = request.respond(tiny_http::Response::from_string(text));
            continue;
        }
        let body = render_share_html(path, lang, &output);
        let response = tiny_http::Response::from_string(body).with_header(
            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..])
                .unwrap(),
        );
        let _ = request.respond(response);
    }
    Ok(0)
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
    use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};

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

    println!(
        "[run watch] watching {} ({}) — Ctrl-C to stop",
        file_path.display(),
        engine.display_name()
    );

    let mut run_count = 0u32;

    run_count += 1;
    print!("\x1b[2J\x1b[H");
    println!("[run watch] run #{run_count}");
    run_file_once(&file_path, engine, &spec.args);

    let (tx, rx) = mpsc::channel();
    let mut watcher = RecommendedWatcher::new(
        move |res| {
            let _ = tx.send(res);
        },
        Config::default(),
    )?;
    watcher.watch(&file_path, RecursiveMode::NonRecursive)?;

    loop {
        match rx.recv() {
            Ok(Ok(_event)) => {
                while rx
                    .recv_timeout(std::time::Duration::from_millis(150))
                    .is_ok()
                {}
                run_count += 1;
                print!("\x1b[2J\x1b[H");
                let now = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|duration| duration.as_secs())
                    .unwrap_or(0);
                println!("[run watch] run #{run_count} at {now}");
                run_file_once(&file_path, engine, &spec.args);
            }
            Ok(Err(err)) => eprintln!("[run] watch error: {err}"),
            Err(err) => anyhow::bail!("[run] watch channel closed: {err}"),
        }
    }
}

fn run_file_once(file_path: &Path, engine: &dyn crate::engine::LanguageEngine, args: &[String]) {
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

fn execute_capture(spec: ExecutionSpec, registry: &LanguageRegistry) -> Result<String> {
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
    let outcome = engine.execute(&payload)?;
    let mut output = outcome.stdout;
    output.push_str(&outcome.stderr);
    Ok(output)
}

fn render_share_html(path: &Path, language: &str, output: &str) -> String {
    let code = fs::read_to_string(path).unwrap_or_default();
    let syntax_set = syntect::parsing::SyntaxSet::load_defaults_newlines();
    let theme_set = syntect::highlighting::ThemeSet::load_defaults();
    let syntax = syntax_set
        .find_syntax_by_extension(path.extension().and_then(|ext| ext.to_str()).unwrap_or(""))
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());
    let rendered_code = theme_set
        .themes
        .get("base16-ocean.dark")
        .and_then(|theme| {
            syntect::html::highlighted_html_for_string(&code, &syntax_set, syntax, theme).ok()
        })
        .unwrap_or_else(|| format!("<pre>{}</pre>", html_escape(&code)));
    format!(
        "<!doctype html><meta charset=\"utf-8\"><title>{}</title>\
         <style>body{{font-family:system-ui;margin:2rem;background:#111;color:#eee}}pre{{padding:1rem;overflow:auto;background:#1b1b1b}}.out{{white-space:pre-wrap}}</style>\
         <h1>{}</h1><p>Language: {}</p>{}<h2>Last output</h2><pre class=\"out\">{}</pre>",
        html_escape(&path.display().to_string()),
        html_escape(&path.display().to_string()),
        html_escape(language),
        rendered_code,
        html_escape(output)
    )
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn language_from_path(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "py" | "pyw" => Some("python"),
        "js" | "jsx" | "mjs" | "cjs" => Some("javascript"),
        "ts" | "tsx" => Some("typescript"),
        "rs" => Some("rust"),
        "go" => Some("go"),
        "c" | "h" => Some("c"),
        "cc" | "cpp" | "cxx" | "hpp" | "hxx" => Some("cpp"),
        "java" => Some("java"),
        "rb" => Some("ruby"),
        "sh" | "bash" | "zsh" => Some("bash"),
        _ => None,
    }
}

fn toolchain_name(language: &str) -> &'static str {
    match language {
        "python" => "python3",
        "javascript" => "node",
        "typescript" => "deno",
        "rust" => "rustc",
        "go" => "go",
        "c" => "cc",
        "cpp" => "c++",
        "java" => "javac/java",
        "kotlin" => "kotlinc",
        "csharp" => "dotnet",
        "bash" => "bash",
        "ruby" => "ruby",
        "lua" => "lua",
        "php" => "php",
        "r" => "Rscript",
        "dart" => "dart",
        "swift" => "swift",
        "perl" => "perl",
        "julia" => "julia",
        "haskell" => "runghc",
        "elixir" => "elixir",
        "crystal" => "crystal",
        "zig" => "zig",
        "nim" => "nim",
        "groovy" => "groovy",
        _ => "unknown",
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
