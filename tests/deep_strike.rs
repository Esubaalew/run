use assert_cmd::Command;
use predicates::prelude::*;
use run::engine::LanguageEngine;

fn run_binary() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("run"))
}

fn python_available() -> bool {
    run::engine::PythonEngine::new().validate().is_ok()
}

#[test]
fn help_lists_workflow_subcommands() {
    run_binary()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Workflow commands:"))
        .stdout(predicate::str::contains("run doctor"))
        .stdout(predicate::str::contains("run cache --stats"))
        .stdout(predicate::str::contains("run alias list"))
        .stdout(predicate::str::contains("run share <file>"));
}

#[test]
fn alias_command_lists_and_supports_custom_aliases() {
    let temp = tempfile::tempdir().expect("tempdir");
    let aliases_file = temp.path().join("aliases.toml");
    let aliases_file = aliases_file.to_str().expect("utf8 path");

    run_binary()
        .env("RUN_ALIASES_FILE", aliases_file)
        .args(["alias", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Alias"))
        .stdout(predicate::str::contains("py"))
        .stdout(predicate::str::contains("python"));

    run_binary()
        .env("RUN_ALIASES_FILE", aliases_file)
        .args(["alias", "add", "p", "python"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Added alias 'p'"));

    run_binary()
        .env("RUN_ALIASES_FILE", aliases_file)
        .args(["alias", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Custom aliases"))
        .stdout(predicate::str::contains("p"));

    run_binary()
        .env("RUN_ALIASES_FILE", aliases_file)
        .args(["p", "-c", "print('alias-ok')"])
        .assert()
        .success()
        .stdout(predicate::str::contains("alias-ok"));

    run_binary()
        .env("RUN_ALIASES_FILE", aliases_file)
        .args(["alias", "remove", "p"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed alias 'p'"));

    run_binary()
        .env("RUN_ALIASES_FILE", aliases_file)
        .args(["alias", "add", "py", "python"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("reserved"));
}

#[test]
fn snippet_lists_and_emits_template() {
    run_binary()
        .args(["snippet", "python", "--list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("http-server"));

    run_binary()
        .args(["snippet", "python", "http-server"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ThreadingHTTPServer"));
}

#[test]
fn cache_stats_and_clear_work() {
    run_binary()
        .args(["cache", "--stats"])
        .assert()
        .success()
        .stdout(predicate::str::contains("entries:"));

    run_binary()
        .args(["cache", "--clear"])
        .assert()
        .success()
        .stdout(predicate::str::contains("cache cleared"));
}

#[test]
fn fmt_unsupported_file_exits_two() {
    let file = tempfile::NamedTempFile::new().expect("temp file");
    run_binary()
        .args(["fmt", file.path().to_str().expect("utf8 path")])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("No formatter available"));
}

#[test]
fn fmt_recognizes_kotlin_extension() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("main.kt");
    std::fs::write(&path, "fun main() {}").expect("write kotlin file");
    run_binary()
        .args(["fmt", path.to_str().expect("utf8 path")])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("kotlin"))
        .stderr(predicate::str::contains("No formatter available"));
}

#[test]
fn doctor_returns_documented_status_code() {
    run_binary()
        .arg("doctor")
        .assert()
        .code(predicate::in_iter([0, 1]))
        .stdout(predicate::str::contains("Language"));
}

#[test]
fn json_output_wraps_execution_result() {
    if !python_available() {
        return;
    }

    let output = run_binary()
        .args(["--json", "python", "-c", "print('json-ok')"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).expect("valid json");
    assert_eq!(value["language"], "python");
    assert_eq!(
        value["stdout"].as_str().unwrap_or("").replace("\r\n", "\n"),
        "json-ok\n"
    );
    assert_eq!(value["exit_code"], 0);
}

#[test]
fn timeout_exits_with_124() {
    if !python_available() {
        return;
    }

    run_binary()
        .args([
            "--timeout",
            "1",
            "python",
            "-c",
            "import time; time.sleep(5)",
        ])
        .assert()
        .code(124)
        .stderr(predicate::str::contains("Execution timed out after 1s"));
}

#[test]
fn default_timeout_is_unlimited() {
    if !python_available() {
        return;
    }

    run_binary()
        .args(["python", "-c", "print('no-timeout')"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no-timeout"));
}

#[test]
fn execution_handles_large_stdout_without_pipe_deadlock() {
    if !python_available() {
        return;
    }

    // Emit 100KB of stdout, exceeding standard 64KB OS pipe buffers.
    let output = run_binary()
        .args([
            "--timeout",
            "5",
            "python",
            "-c",
            "import sys; sys.stdout.write('A' * 100000)",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    assert_eq!(output.len(), 100000);
    assert!(output.iter().all(|&b| b == b'A'));
}

#[test]
fn execution_handles_large_stderr_without_pipe_deadlock() {
    if !python_available() {
        return;
    }

    // Emit 100KB of stderr, exceeding standard 64KB OS pipe buffers.
    let output = run_binary()
        .args([
            "--timeout",
            "5",
            "python",
            "-c",
            "import sys; sys.stderr.write('B' * 100000)",
        ])
        .assert()
        .success()
        .get_output()
        .stderr
        .clone();

    // Verify all 100,000 'B' bytes were captured without truncation
    let b_count = output.iter().filter(|&&b| b == b'B').count();
    assert_eq!(b_count, 100000);
}

#[test]
fn execution_handles_simultaneous_large_stdout_and_stderr() {
    if !python_available() {
        return;
    }

    let output = run_binary()
        .args([
            "--timeout",
            "5",
            "python",
            "-c",
            "import sys; sys.stdout.write('X' * 100000); sys.stderr.write('Y' * 100000)",
        ])
        .assert()
        .success()
        .get_output()
        .clone();

    assert_eq!(output.stdout.len(), 100000);
    assert!(output.stdout.iter().all(|&b| b == b'X'));

    let y_count = output.stderr.iter().filter(|&&b| b == b'Y').count();
    assert_eq!(y_count, 100000);
}

#[test]
fn execution_near_deadline_preserves_complete_output() {
    if !python_available() {
        return;
    }

    // A process writing output right before completion preserves full output without truncation.
    let output = run_binary()
        .args([
            "--timeout",
            "5",
            "python",
            "-c",
            "import sys; sys.stdout.write('Z' * 100000)",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    assert_eq!(output.len(), 100000);
    assert!(output.iter().all(|&b| b == b'Z'));
}
