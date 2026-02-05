use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tempfile::{Builder, TempDir};

use super::{ExecutionOutcome, ExecutionPayload, LanguageEngine, LanguageSession};

pub struct CrystalEngine {
    executable: Option<PathBuf>,
}

impl CrystalEngine {
    pub fn new() -> Self {
        Self {
            executable: resolve_crystal_binary(),
        }
    }

    fn ensure_executable(&self) -> Result<&Path> {
        self.executable.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "Crystal support requires the `crystal` executable. Install it from https://crystal-lang.org/install/ and ensure it is on your PATH."
            )
        })
    }

    fn write_temp_source(&self, code: &str) -> Result<(TempDir, PathBuf)> {
        let dir = Builder::new()
            .prefix("run-crystal")
            .tempdir()
            .context("failed to create temporary directory for Crystal source")?;
        let path = dir.path().join("snippet.cr");
        let mut contents = code.to_string();
        if !contents.ends_with('\n') {
            contents.push('\n');
        }
        std::fs::write(&path, contents).with_context(|| {
            format!(
                "failed to write temporary Crystal source to {}",
                path.display()
            )
        })?;
        Ok((dir, path))
    }

    fn run_source(&self, source: &Path) -> Result<std::process::Output> {
        let executable = self.ensure_executable()?;
        let mut cmd = Command::new(executable);
        cmd.arg("run")
            .arg(source)
            .arg("--no-color")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd.stdin(Stdio::inherit());
        if let Some(dir) = source.parent() {
            cmd.current_dir(dir);
        }
        cmd.output().with_context(|| {
            format!(
                "failed to execute {} with source {}",
                executable.display(),
                source.display()
            )
        })
    }
}

impl LanguageEngine for CrystalEngine {
    fn id(&self) -> &'static str {
        "crystal"
    }

    fn display_name(&self) -> &'static str {
        "Crystal"
    }

    fn aliases(&self) -> &[&'static str] {
        &["cr", "crystal-lang"]
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
        let (temp_dir, source_path) = match payload {
            ExecutionPayload::Inline { code } | ExecutionPayload::Stdin { code } => {
                let (dir, path) = self.write_temp_source(code)?;
                (Some(dir), path)
            }
            ExecutionPayload::File { path } => (None, path.clone()),
        };

        let output = self.run_source(&source_path)?;
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
        Ok(Box::new(CrystalSession::new(executable)?))
    }
}

fn resolve_crystal_binary() -> Option<PathBuf> {
    which::which("crystal").ok()
}

struct CrystalSession {
    executable: PathBuf,
    workspace: TempDir,
    snippets: Vec<String>,
    last_stdout: String,
    last_stderr: String,
}

impl CrystalSession {
    fn new(executable: PathBuf) -> Result<Self> {
        let workspace = TempDir::new().context("failed to create Crystal session workspace")?;
        let session = Self {
            executable,
            workspace,
            snippets: Vec::new(),
            last_stdout: String::new(),
            last_stderr: String::new(),
        };
        session.persist_source()?;
        Ok(session)
    }

    fn source_path(&self) -> PathBuf {
        self.workspace.path().join("session.cr")
    }

    fn persist_source(&self) -> Result<()> {
        let source = self.render_source();
        fs::write(self.source_path(), source)
            .with_context(|| "failed to write Crystal session source".to_string())
    }

    fn render_source(&self) -> String {
        if self.snippets.is_empty() {
            return String::from("# session body\n");
        }

        let mut source = String::new();
        for snippet in &self.snippets {
            source.push_str(snippet);
            if !snippet.ends_with('\n') {
                source.push('\n');
            }
            source.push('\n');
        }
        source
    }

    fn run_program(&self) -> Result<std::process::Output> {
        let mut cmd = Command::new(&self.executable);
        cmd.arg("run")
            .arg("session.cr")
            .arg("--no-color")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(self.workspace.path());
        cmd.output().with_context(|| {
            format!(
                "failed to execute {} for Crystal session",
                self.executable.display()
            )
        })
    }

    fn run_current(&mut self, start: Instant) -> Result<(ExecutionOutcome, bool)> {
        self.persist_source()?;
        let output = self.run_program()?;
        let stdout_full = Self::normalize_output(&output.stdout);
        let stderr_full = Self::normalize_output(&output.stderr);

        let success = output.status.success();
        let (stdout, stderr) = if success {
            let stdout_delta = Self::diff_outputs(&self.last_stdout, &stdout_full);
            let stderr_delta = Self::diff_outputs(&self.last_stderr, &stderr_full);
            self.last_stdout = stdout_full;
            self.last_stderr = stderr_full;
            (stdout_delta, stderr_delta)
        } else {
            (stdout_full, stderr_full)
        };

        let outcome = ExecutionOutcome {
            language: "crystal".to_string(),
            exit_code: output.status.code(),
            stdout,
            stderr,
            duration: start.elapsed(),
        };

        Ok((outcome, success))
    }

    fn apply_snippet(&mut self, snippet: String) -> Result<(ExecutionOutcome, bool)> {
        self.snippets.push(snippet);
        let start = Instant::now();
        let (outcome, success) = self.run_current(start)?;
        if !success {
            let _ = self.snippets.pop();
            self.persist_source()?;
        }
        Ok((outcome, success))
    }

    fn reset(&mut self) -> Result<()> {
        self.snippets.clear();
        self.last_stdout.clear();
        self.last_stderr.clear();
        self.persist_source()
    }

    fn normalize_output(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes)
            .replace("\r\n", "\n")
            .replace('\r', "")
    }

    fn diff_outputs(previous: &str, current: &str) -> String {
        current
            .strip_prefix(previous)
            .map(|s| s.to_string())
            .unwrap_or_else(|| current.to_string())
    }
}

impl LanguageSession for CrystalSession {
    fn language_id(&self) -> &str {
        "crystal"
    }

    fn eval(&mut self, code: &str) -> Result<ExecutionOutcome> {
        let trimmed = code.trim();
        if trimmed.is_empty() {
            return Ok(ExecutionOutcome {
                language: "crystal".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        if trimmed.eq_ignore_ascii_case(":reset") {
            self.reset()?;
            return Ok(ExecutionOutcome {
                language: "crystal".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        if trimmed.eq_ignore_ascii_case(":help") {
            return Ok(ExecutionOutcome {
                language: "crystal".to_string(),
                exit_code: None,
                stdout: "Crystal commands:\n  :reset - clear session state\n  :help  - show this message\n"
                    .to_string(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        let snippet = match classify_crystal_snippet(trimmed) {
            CrystalSnippetKind::Statement => ensure_trailing_newline(code),
            CrystalSnippetKind::Expression => wrap_expression(trimmed),
        };

        let (outcome, _) = self.apply_snippet(snippet)?;
        Ok(outcome)
    }

    fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}

enum CrystalSnippetKind {
    Statement,
    Expression,
}

fn classify_crystal_snippet(code: &str) -> CrystalSnippetKind {
    if looks_like_crystal_statement(code) {
        CrystalSnippetKind::Statement
    } else {
        CrystalSnippetKind::Expression
    }
}

fn looks_like_crystal_statement(code: &str) -> bool {
    let trimmed = code.trim_start();
    trimmed.contains('\n')
        || trimmed.ends_with(';')
        || trimmed.ends_with('}')
        || trimmed.ends_with("end")
        || trimmed.ends_with("do")
        || trimmed.starts_with("require ")
        || trimmed.starts_with("def ")
        || trimmed.starts_with("class ")
        || trimmed.starts_with("module ")
        || trimmed.starts_with("struct ")
        || trimmed.starts_with("record ")
        || trimmed.starts_with("enum ")
        || trimmed.starts_with("macro ")
        || trimmed.starts_with("alias ")
        || trimmed.starts_with("include ")
        || trimmed.starts_with("extend ")
        || trimmed.starts_with("@[")
}

fn ensure_trailing_newline(code: &str) -> String {
    let mut snippet = code.to_string();
    if !snippet.ends_with('\n') {
        snippet.push('\n');
    }
    snippet
}

fn wrap_expression(code: &str) -> String {
    format!("p({})\n", code)
}
