use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tempfile::{Builder, TempDir};

use super::{ExecutionOutcome, ExecutionPayload, LanguageEngine, LanguageSession};

pub struct REngine {
    executable: Option<PathBuf>,
}

impl REngine {
    pub fn new() -> Self {
        Self {
            executable: resolve_r_binary(),
        }
    }

    fn ensure_executable(&self) -> Result<&Path> {
        self.executable.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "R support requires the `Rscript` executable. Install R from https://cran.r-project.org/ and ensure `Rscript` is on your PATH."
            )
        })
    }

    fn write_temp_source(&self, code: &str) -> Result<(TempDir, PathBuf)> {
        let dir = Builder::new()
            .prefix("run-r")
            .tempdir()
            .context("failed to create temporary directory for R source")?;
        let path = dir.path().join("snippet.R");
        let mut contents = code.to_string();
        if !contents.ends_with('\n') {
            contents.push('\n');
        }
        fs::write(&path, contents)
            .with_context(|| format!("failed to write temporary R source to {}", path.display()))?;
        Ok((dir, path))
    }

    fn execute_with_path(&self, source: &Path) -> Result<std::process::Output> {
        let executable = self.ensure_executable()?;
        let mut cmd = Command::new(executable);
        cmd.arg("--vanilla")
            .arg(source)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd.stdin(Stdio::inherit());
        cmd.output().with_context(|| {
            format!(
                "failed to invoke {} to run {}",
                executable.display(),
                source.display()
            )
        })
    }
}

impl LanguageEngine for REngine {
    fn id(&self) -> &'static str {
        "r"
    }

    fn display_name(&self) -> &'static str {
        "R"
    }

    fn aliases(&self) -> &[&'static str] {
        &["rscript"]
    }

    fn supports_sessions(&self) -> bool {
        self.executable.is_some()
    }

    fn validate(&self) -> Result<()> {
        let executable = self.ensure_executable()?;
        let mut cmd = Command::new(executable);
        cmd.arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cmd.status()
            .with_context(|| format!("failed to invoke {}", executable.display()))?
            .success()
            .then_some(())
            .ok_or_else(|| anyhow::anyhow!("{} is not executable", executable.display()))
    }

    fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionOutcome> {
        let start = Instant::now();
        let (temp_dir, path) = match payload {
            ExecutionPayload::Inline { code } => {
                let (dir, path) = self.write_temp_source(code)?;
                (Some(dir), path)
            }
            ExecutionPayload::Stdin { code } => {
                let (dir, path) = self.write_temp_source(code)?;
                (Some(dir), path)
            }
            ExecutionPayload::File { path } => (None, path.clone()),
        };

        let output = self.execute_with_path(&path)?;
        drop(temp_dir);

        Ok(ExecutionOutcome {
            language: self.id().to_string(),
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            duration: start.elapsed(),
        })
    }

    fn start_session(&self) -> Result<Box<dyn LanguageSession>> {
        let executable = self.ensure_executable()?.to_path_buf();
        Ok(Box::new(RSession::new(executable)?))
    }
}

fn resolve_r_binary() -> Option<PathBuf> {
    which::which("Rscript").ok()
}

struct RSession {
    executable: PathBuf,
    dir: TempDir,
    script_path: PathBuf,
    statements: Vec<String>,
    previous_stdout: String,
    previous_stderr: String,
}

impl RSession {
    fn new(executable: PathBuf) -> Result<Self> {
        let dir = Builder::new()
            .prefix("run-r-repl")
            .tempdir()
            .context("failed to create temporary directory for R repl")?;
        let script_path = dir.path().join("session.R");
        fs::write(&script_path, "options(warn=1)\n")
            .with_context(|| format!("failed to initialize {}", script_path.display()))?;

        Ok(Self {
            executable,
            dir,
            script_path,
            statements: Vec::new(),
            previous_stdout: String::new(),
            previous_stderr: String::new(),
        })
    }

    fn render_script(&self) -> String {
        let mut script = String::from("options(warn=1)\n");
        for stmt in &self.statements {
            script.push_str(stmt);
            if !stmt.ends_with('\n') {
                script.push('\n');
            }
        }
        script
    }

    fn write_script(&self, contents: &str) -> Result<()> {
        fs::write(&self.script_path, contents).with_context(|| {
            format!(
                "failed to write generated R REPL script to {}",
                self.script_path.display()
            )
        })
    }

    fn run_current(&mut self, start: Instant) -> Result<(ExecutionOutcome, bool)> {
        let script = self.render_script();
        self.write_script(&script)?;

        let mut cmd = Command::new(&self.executable);
        cmd.arg("--vanilla")
            .arg(&self.script_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(self.dir.path());
        let output = cmd.output().with_context(|| {
            format!(
                "failed to execute R session script {} with {}",
                self.script_path.display(),
                self.executable.display()
            )
        })?;

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
            language: "r".to_string(),
            exit_code: output.status.code(),
            stdout: stdout_delta,
            stderr: stderr_delta,
            duration: start.elapsed(),
        };

        Ok((outcome, success))
    }

    fn run_snippet(&mut self, snippet: String) -> Result<ExecutionOutcome> {
        self.statements.push(snippet);
        let start = Instant::now();
        let (outcome, success) = self.run_current(start)?;
        if !success {
            let _ = self.statements.pop();
            let script = self.render_script();
            self.write_script(&script)?;
        }
        Ok(outcome)
    }

    fn reset_state(&mut self) -> Result<()> {
        self.statements.clear();
        self.previous_stdout.clear();
        self.previous_stderr.clear();
        let script = self.render_script();
        self.write_script(&script)
    }
}

impl LanguageSession for RSession {
    fn language_id(&self) -> &str {
        "r"
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
                    "R commands:\n  :reset - clear session state\n  :help  - show this message\n"
                        .to_string(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        let snippet = if should_wrap_expression(trimmed) {
            wrap_expression(trimmed)
        } else {
            ensure_trailing_newline(code)
        };

        self.run_snippet(snippet)
    }

    fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}

fn should_wrap_expression(code: &str) -> bool {
    if code.contains('\n') {
        return false;
    }

    let lowered = code.trim_start().to_ascii_lowercase();
    const STATEMENT_PREFIXES: [&str; 12] = [
        "if ", "for ", "while ", "repeat", "function", "library", "require", "print", "cat",
        "source", "options", "setwd",
    ];
    if STATEMENT_PREFIXES
        .iter()
        .any(|prefix| lowered.starts_with(prefix))
    {
        return false;
    }

    if code.contains("<-") || code.contains("=") {
        return false;
    }

    true
}

fn wrap_expression(code: &str) -> String {
    format!("print(({}))\n", code)
}

fn ensure_trailing_newline(code: &str) -> String {
    let mut owned = code.to_string();
    if !owned.ends_with('\n') {
        owned.push('\n');
    }
    owned
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
