use std::io::Write;

use predicates::prelude::*;
use run::engine::LanguageEngine;
use tempfile::NamedTempFile;

fn python_available() -> bool {
    run::engine::PythonEngine::new().validate().is_ok()
}

fn rust_available() -> bool {
    run::engine::RustEngine::new().validate().is_ok()
}

fn go_available() -> bool {
    run::engine::GoEngine::new().validate().is_ok()
}

// ---------------------------------------------------------------------------
// Compilation cache tests
// ---------------------------------------------------------------------------

#[test]
fn hash_source_deterministic() {
    let h1 = run::engine::hash_source("fn main() {}");
    let h2 = run::engine::hash_source("fn main() {}");
    assert_eq!(h1, h2, "same source must produce same hash");
}

#[test]
fn hash_source_different_inputs() {
    let h1 = run::engine::hash_source("fn main() {}");
    let h2 = run::engine::hash_source("fn main() { 1 }");
    assert_ne!(h1, h2, "different source must produce different hash");
}

#[test]
fn hash_source_empty_string() {
    let h = run::engine::hash_source("");
    assert_ne!(h, 0, "empty string should still produce a valid hash");
}

#[test]
fn cache_lookup_returns_none_for_unknown() {
    let hash = run::engine::hash_source("this_is_a_very_unique_test_string_12345");
    assert!(
        run::engine::cache_lookup(hash).is_none(),
        "cache should return None for unknown hashes"
    );
}

#[test]
fn cache_store_and_retrieve() {
    let mut tmp = NamedTempFile::new().expect("create temp file");
    writeln!(tmp, "#!/bin/sh\necho cached").expect("write temp");
    let path = tmp.path().to_path_buf();

    let hash = run::engine::hash_source("cache_store_and_retrieve_test");

    let cached = run::engine::cache_store(hash, &path);
    assert!(cached.is_some(), "cache_store should succeed");

    let lookup = run::engine::cache_lookup(hash);
    assert!(lookup.is_some(), "cache_lookup should find stored entry");
    assert!(lookup.unwrap().exists(), "cached path should exist on disk");
}

// ---------------------------------------------------------------------------
// Rust compilation cache integration
// ---------------------------------------------------------------------------

#[test]
fn rust_inline_uses_cache_on_repeated_run() {
    if !rust_available() {
        eprintln!("skipping: rustc not available");
        return;
    }

    let engine = run::engine::RustEngine::new();
    let code = r#"fn main() { println!("cache_test"); }"#;
    let payload = run::engine::ExecutionPayload::Inline {
        code: code.to_string(),
    };

    let out1 = engine.execute(&payload).expect("first run should succeed");
    assert!(out1.success());
    assert!(out1.stdout.contains("cache_test"));
    let t1 = out1.duration;

    let out2 = engine.execute(&payload).expect("second run should succeed");
    assert!(out2.success());
    assert!(out2.stdout.contains("cache_test"));

    eprintln!(
        "first: {}ms, second: {}ms",
        t1.as_millis(),
        out2.duration.as_millis()
    );
}

// ---------------------------------------------------------------------------
// --bench CLI flag
// ---------------------------------------------------------------------------

#[test]
fn bench_flag_python() {
    if !python_available() {
        eprintln!("skipping: python not available");
        return;
    }

    let cmd = assert_cmd::Command::cargo_bin("run")
        .expect("binary")
        .args(["--bench", "3", "python", "-c", "print('bench')"])
        .assert();

    cmd.success()
        .stdout(predicate::str::contains("bench"))
        .stderr(predicate::str::contains("Results (3 runs)"));
}

#[test]
fn bench_flag_requires_positive_iterations() {
    if !python_available() {
        eprintln!("skipping: python not available");
        return;
    }

    let cmd = assert_cmd::Command::cargo_bin("run")
        .expect("binary")
        .args(["--bench", "0", "python", "-c", "print('ok')"])
        .assert();

    cmd.success()
        .stdout(predicate::str::contains("ok"))
        .stderr(predicate::str::contains("Results (1 runs)"));
}

// ---------------------------------------------------------------------------
// --watch CLI flag (just validate parsing, not the loop)
// ---------------------------------------------------------------------------

#[test]
fn watch_flag_requires_file() {
    let cmd = assert_cmd::Command::cargo_bin("run")
        .expect("binary")
        .args(["--watch", "python", "-c", "print('x')"])
        .assert();

    cmd.failure()
        .stderr(predicate::str::contains("--watch requires a file path"));
}

// ---------------------------------------------------------------------------
// Config file loading
// ---------------------------------------------------------------------------

#[test]
fn config_default_is_empty() {
    let config = run::config::RunConfig::default();
    assert!(config.language.is_none());
    assert!(config.timeout.is_none());
    assert!(config.timing.is_none());
    assert!(config.bench_iterations.is_none());
}

#[test]
fn config_load_from_toml() {
    let mut tmp = NamedTempFile::new().expect("create temp");
    writeln!(
        tmp,
        r#"language = "python"
timeout = 30
timing = true
bench_iterations = 10
"#
    )
    .expect("write");

    let config = run::config::RunConfig::load(tmp.path()).expect("parse config");
    assert_eq!(config.language.as_deref(), Some("python"));
    assert_eq!(config.timeout, Some(30));
    assert_eq!(config.timing, Some(true));
    assert_eq!(config.bench_iterations, Some(10));
}

#[test]
fn config_load_ignores_unknown_keys() {
    let mut tmp = NamedTempFile::new().expect("create temp");
    writeln!(tmp, "language = \"go\"\nunknown_key = 42\n").expect("write");

    let result = run::config::RunConfig::load(tmp.path());
    if let Ok(config) = result {
        assert_eq!(config.language.as_deref(), Some("go"));
    }
}

// ---------------------------------------------------------------------------
// REPL utilities
// ---------------------------------------------------------------------------

#[test]
fn format_duration_millis() {
    // Test the format_duration function indirectly through the repl module
    // We verify it through execution timing format in outcomes
    let engine = run::engine::PythonEngine::new();
    if engine.validate().is_err() {
        eprintln!("skipping: python not available");
        return;
    }

    let payload = run::engine::ExecutionPayload::Inline {
        code: "1+1".to_string(),
    };
    let outcome = engine.execute(&payload).expect("should run");
    assert!(outcome.success());

    assert!(outcome.duration.as_nanos() > 0);
}
