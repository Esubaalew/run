use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tempfile::{Builder, TempDir};

use super::{ExecutionOutcome, ExecutionPayload, LanguageEngine, LanguageSession};

pub struct PythonEngine {
    executable: PathBuf,
}

impl PythonEngine {
    pub fn new() -> Self {
        let executable = resolve_python_binary();
        Self { executable }
    }

    fn binary(&self) -> &Path {
        &self.executable
    }

    fn run_command(&self) -> Command {
        Command::new(self.binary())
    }
}

impl LanguageEngine for PythonEngine {
    fn id(&self) -> &'static str {
        "python"
    }

    fn display_name(&self) -> &'static str {
        "Python"
    }

    fn aliases(&self) -> &[&'static str] {
        &["py", "python3", "py3"]
    }

    fn supports_sessions(&self) -> bool {
        true
    }

    fn validate(&self) -> Result<()> {
        let mut cmd = self.run_command();
        cmd.arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cmd.status()
            .with_context(|| format!("failed to invoke {}", self.binary().display()))?
            .success()
            .then_some(())
            .ok_or_else(|| anyhow::anyhow!("{} is not executable", self.binary().display()))
    }

    fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionOutcome> {
        let start = Instant::now();
        let mut cmd = self.run_command();
        let output = match payload {
            ExecutionPayload::Inline { code } => {
                cmd.arg("-c").arg(code);
                cmd.stdin(Stdio::inherit());
                cmd.output()
            }
            ExecutionPayload::File { path } => {
                cmd.arg(path);
                cmd.stdin(Stdio::inherit());
                cmd.output()
            }
            ExecutionPayload::Stdin { code } => {
                cmd.arg("-")
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
                let mut child = cmd.spawn().with_context(|| {
                    format!(
                        "failed to start {} for stdin execution",
                        self.binary().display()
                    )
                })?;
                if let Some(mut stdin) = child.stdin.take() {
                    stdin.write_all(code.as_bytes())?;
                }
                child.wait_with_output()
            }
        }?;

        Ok(ExecutionOutcome {
            language: self.id().to_string(),
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            duration: start.elapsed(),
        })
    }

    fn start_session(&self) -> Result<Box<dyn LanguageSession>> {
        Ok(Box::new(PythonSession::new(self.executable.clone())?))
    }
}

struct PythonSession {
    executable: PathBuf,
    dir: TempDir,
    source_path: PathBuf,
    statements: Vec<String>,
    previous_stdout: String,
    previous_stderr: String,
}

impl PythonSession {
    fn new(executable: PathBuf) -> Result<Self> {
        let dir = Builder::new()
            .prefix("run-python-repl")
            .tempdir()
            .context("failed to create temporary directory for python repl")?;
        let source_path = dir.path().join("session.py");
        fs::write(&source_path, "# Python REPL session\n")
            .with_context(|| format!("failed to initialize {}", source_path.display()))?;

        Ok(Self {
            executable,
            dir,
            source_path,
            statements: Vec::new(),
            previous_stdout: String::new(),
            previous_stderr: String::new(),
        })
    }

    fn render_source(&self) -> String {
        let mut source = String::from("import sys\nfrom math import *\n\n");
        for snippet in &self.statements {
            source.push_str(snippet);
            if !snippet.ends_with('\n') {
                source.push('\n');
            }
        }
        source
    }

    fn write_source(&self, contents: &str) -> Result<()> {
        fs::write(&self.source_path, contents).with_context(|| {
            format!(
                "failed to write generated Python REPL source to {}",
                self.source_path.display()
            )
        })
    }

    fn run_current(&mut self, start: Instant) -> Result<(ExecutionOutcome, bool)> {
        let source = self.render_source();
        self.write_source(&source)?;

        let output = self.run_script()?;
        let stdout_full = normalize_output(&output.stdout);
        let stderr_full = normalize_output(&output.stderr);

        let stdout_delta = diff_output(&self.previous_stdout, &stdout_full);
        let stderr_delta = diff_output(&self.previous_stderr, &stderr_full);

        let success = output.status.success();
        if success {
            self.previous_stdout = stdout_full;
            self.previous_stderr = stderr_full;
        }

        let outcome = ExecutionOutcome {
            language: "python".to_string(),
            exit_code: output.status.code(),
            stdout: stdout_delta,
            stderr: stderr_delta,
            duration: start.elapsed(),
        };

        Ok((outcome, success))
    }

    fn run_script(&self) -> Result<std::process::Output> {
        let mut cmd = Command::new(&self.executable);
        cmd.arg(&self.source_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(self.dir.path());
        cmd.output().with_context(|| {
            format!(
                "failed to run python session script {} with {}",
                self.source_path.display(),
                self.executable.display()
            )
        })
    }

    fn run_snippet(&mut self, snippet: String) -> Result<ExecutionOutcome> {
        self.statements.push(snippet);
        let start = Instant::now();
        let (outcome, success) = self.run_current(start)?;
        if !success {
            let _ = self.statements.pop();
            let source = self.render_source();
            self.write_source(&source)?;
        }
        Ok(outcome)
    }

    fn reset_state(&mut self) -> Result<()> {
        self.statements.clear();
        self.previous_stdout.clear();
        self.previous_stderr.clear();
        let source = self.render_source();
        self.write_source(&source)
    }
}

impl LanguageSession for PythonSession {
    fn language_id(&self) -> &str {
        "python"
    }

    fn eval(&mut self, code: &str) -> Result<ExecutionOutcome> {
        let trimmed = code.trim();
        if trimmed.is_empty() {
            return Ok(ExecutionOutcome {
                language: self.language_id().to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        if trimmed.eq_ignore_ascii_case(":reset") {
            self.reset_state()?;
            return Ok(ExecutionOutcome {
                language: self.language_id().to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        if trimmed.eq_ignore_ascii_case(":help") {
            return Ok(ExecutionOutcome {
                language: self.language_id().to_string(),
                exit_code: None,
                stdout:
                    "Python commands:\n  :reset — clear session state\n  :help  — show this message\n"
                        .to_string(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        if should_treat_as_expression(trimmed) {
            let snippet = wrap_expression(trimmed, self.statements.len());
            let outcome = self.run_snippet(snippet)?;
            if outcome.exit_code.unwrap_or(0) == 0 {
                return Ok(outcome);
            }
        }

        let snippet = ensure_trailing_newline(code);
        self.run_snippet(snippet)
    }

    fn shutdown(&mut self) -> Result<()> {
        // TempDir cleanup handled automatically.
        Ok(())
    }
}

fn resolve_python_binary() -> PathBuf {
    let candidates = ["python3", "python", "py"]; // windows py launcher
    for name in candidates {
        if let Ok(path) = which::which(name) {
            return path;
        }
    }
    PathBuf::from("python3")
}

fn ensure_trailing_newline(code: &str) -> String {
    let mut owned = code.to_string();
    if !owned.ends_with('\n') {
        owned.push('\n');
    }
    owned
}

fn wrap_expression(code: &str, index: usize) -> String {
    format!("__run_value_{index} = ({code})\nprint(repr(__run_value_{index}), flush=True)\n")
}

fn diff_output(previous: &str, current: &str) -> String {
    if let Some(stripped) = current.strip_prefix(previous) {
        stripped.to_string()
    } else {
        current.to_string()
    }
}

fn normalize_output(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .replace("\r\n", "\n")
        .replace('\r', "")
}

fn should_treat_as_expression(code: &str) -> bool {
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.contains('\n') {
        return false;
    }
    if trimmed.ends_with(':') {
        return false;
    }

    let lowered = trimmed.to_ascii_lowercase();
    const STATEMENT_PREFIXES: [&str; 21] = [
        "import ",
        "from ",
        "def ",
        "class ",
        "if ",
        "for ",
        "while ",
        "try",
        "except",
        "finally",
        "with ",
        "return ",
        "raise ",
        "yield",
        "async ",
        "await ",
        "assert ",
        "del ",
        "global ",
        "nonlocal ",
        "pass",
    ];
    if STATEMENT_PREFIXES
        .iter()
        .any(|prefix| lowered.starts_with(prefix))
    {
        return false;
    }

    if lowered.starts_with("print(") || lowered.starts_with("print ") {
        return false;
    }

    if trimmed.starts_with("#") {
        return false;
    }

    if trimmed.contains('=')
        && !trimmed.contains("==")
        && !trimmed.contains("!=")
        && !trimmed.contains(">=")
        && !trimmed.contains("<=")
        && !trimmed.contains("=>")
    {
        return false;
    }

    true
}
