use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tempfile::{Builder, TempDir};

use super::{ExecutionOutcome, ExecutionPayload, LanguageEngine, LanguageSession};

pub struct PerlEngine {
    executable: Option<PathBuf>,
}

impl Default for PerlEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PerlEngine {
    pub fn new() -> Self {
        Self {
            executable: resolve_perl_binary(),
        }
    }

    fn ensure_executable(&self) -> Result<&Path> {
        self.executable.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "Perl support requires the `perl` executable. Install Perl from https://www.perl.org/get.html and ensure `perl` is on your PATH."
            )
        })
    }

    fn write_temp_script(&self, code: &str) -> Result<(TempDir, PathBuf)> {
        let dir = Builder::new()
            .prefix("run-perl")
            .tempdir()
            .context("failed to create temporary directory for Perl source")?;
        let path = dir.path().join("snippet.pl");
        let mut contents = code.to_string();
        if !contents.ends_with('\n') {
            contents.push('\n');
        }
        fs::write(&path, contents).with_context(|| {
            format!(
                "failed to write temporary Perl source to {}",
                path.display()
            )
        })?;
        Ok((dir, path))
    }

    fn execute_path(&self, path: &Path) -> Result<std::process::Output> {
        let executable = self.ensure_executable()?;
        let mut cmd = Command::new(executable);
        cmd.arg(path).stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd.stdin(Stdio::inherit());
        if let Some(parent) = path.parent() {
            cmd.current_dir(parent);
        }
        cmd.output().with_context(|| {
            format!(
                "failed to execute {} with script {}",
                executable.display(),
                path.display()
            )
        })
    }
}

impl LanguageEngine for PerlEngine {
    fn id(&self) -> &'static str {
        "perl"
    }

    fn display_name(&self) -> &'static str {
        "Perl"
    }

    fn aliases(&self) -> &[&'static str] {
        &["pl"]
    }

    fn supports_sessions(&self) -> bool {
        self.executable.is_some()
    }

    fn validate(&self) -> Result<()> {
        let executable = self.ensure_executable()?;
        let mut cmd = Command::new(executable);
        cmd.arg("-v").stdout(Stdio::null()).stderr(Stdio::null());
        cmd.status()
            .with_context(|| format!("failed to invoke {}", executable.display()))?
            .success()
            .then_some(())
            .ok_or_else(|| anyhow::anyhow!("{} is not executable", executable.display()))
    }

    fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionOutcome> {
        let start = Instant::now();
        let (temp_dir, path) = match payload {
            ExecutionPayload::Inline { code } | ExecutionPayload::Stdin { code } => {
                let (dir, path) = self.write_temp_script(code)?;
                (Some(dir), path)
            }
            ExecutionPayload::File { path } => (None, path.clone()),
        };

        let output = self.execute_path(&path)?;
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
        Ok(Box::new(PerlSession::new(executable)?))
    }
}

fn resolve_perl_binary() -> Option<PathBuf> {
    which::which("perl").ok()
}

#[derive(Default)]
struct PerlSessionState {
    pragmas: BTreeSet<String>,
    declarations: Vec<String>,
    statements: Vec<String>,
}

struct PerlSession {
    executable: PathBuf,
    workspace: TempDir,
    state: PerlSessionState,
    previous_stdout: String,
    previous_stderr: String,
}

impl PerlSession {
    fn new(executable: PathBuf) -> Result<Self> {
        let workspace = Builder::new()
            .prefix("run-perl-repl")
            .tempdir()
            .context("failed to create temporary directory for Perl repl")?;
        let mut state = PerlSessionState::default();
        state.pragmas.insert("use strict;".to_string());
        state.pragmas.insert("use warnings;".to_string());
        state.pragmas.insert("use feature 'say';".to_string());
        let session = Self {
            executable,
            workspace,
            state,
            previous_stdout: String::new(),
            previous_stderr: String::new(),
        };
        session.persist_source()?;
        Ok(session)
    }

    fn source_path(&self) -> PathBuf {
        self.workspace.path().join("session.pl")
    }

    fn persist_source(&self) -> Result<()> {
        let source = self.render_source();
        fs::write(self.source_path(), source)
            .with_context(|| "failed to write Perl session source".to_string())
    }

    fn render_source(&self) -> String {
        let mut source = String::new();
        for pragma in &self.state.pragmas {
            source.push_str(pragma);
            if !pragma.ends_with('\n') {
                source.push('\n');
            }
        }
        source.push('\n');

        for decl in &self.state.declarations {
            source.push_str(decl);
            if !decl.ends_with('\n') {
                source.push('\n');
            }
            source.push('\n');
        }

        if self.state.statements.is_empty() {
            source.push_str("# session body\n");
        } else {
            for stmt in &self.state.statements {
                source.push_str(stmt);
                if !stmt.ends_with('\n') {
                    source.push('\n');
                }
            }
        }

        source
    }

    fn run_program(&self) -> Result<std::process::Output> {
        let mut cmd = Command::new(&self.executable);
        cmd.arg("session.pl")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(self.workspace.path());
        cmd.output().with_context(|| {
            format!(
                "failed to execute {} for Perl session",
                self.executable.display()
            )
        })
    }

    fn run_current(&mut self, start: Instant) -> Result<(ExecutionOutcome, bool)> {
        self.persist_source()?;
        let output = self.run_program()?;
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
            language: "perl".to_string(),
            exit_code: output.status.code(),
            stdout: stdout_delta,
            stderr: stderr_delta,
            duration: start.elapsed(),
        };

        Ok((outcome, success))
    }

    fn apply_pragma(&mut self, code: &str) -> Result<(ExecutionOutcome, bool)> {
        let mut inserted = Vec::new();
        for line in code.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let normalized = if trimmed.ends_with(';') {
                trimmed.trim_end_matches(';').to_string() + ";"
            } else {
                format!("{};", trimmed)
            };
            if self.state.pragmas.insert(normalized.clone()) {
                inserted.push(normalized);
            }
        }

        if inserted.is_empty() {
            return Ok((
                ExecutionOutcome {
                    language: "perl".to_string(),
                    exit_code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    duration: Duration::default(),
                },
                true,
            ));
        }

        let start = Instant::now();
        let (outcome, success) = self.run_current(start)?;
        if !success {
            for pragma in inserted {
                self.state.pragmas.remove(&pragma);
            }
            self.persist_source()?;
        }
        Ok((outcome, success))
    }

    fn apply_declaration(&mut self, code: &str) -> Result<(ExecutionOutcome, bool)> {
        let snippet = ensure_trailing_newline(code);
        self.state.declarations.push(snippet);
        let start = Instant::now();
        let (outcome, success) = self.run_current(start)?;
        if !success {
            let _ = self.state.declarations.pop();
            self.persist_source()?;
        }
        Ok((outcome, success))
    }

    fn apply_statement(&mut self, code: &str) -> Result<(ExecutionOutcome, bool)> {
        self.state.statements.push(ensure_statement(code));
        let start = Instant::now();
        let (outcome, success) = self.run_current(start)?;
        if !success {
            let _ = self.state.statements.pop();
            self.persist_source()?;
        }
        Ok((outcome, success))
    }

    fn apply_expression(&mut self, code: &str) -> Result<(ExecutionOutcome, bool)> {
        self.state.statements.push(wrap_expression(code));
        let start = Instant::now();
        let (outcome, success) = self.run_current(start)?;
        if !success {
            let _ = self.state.statements.pop();
            self.persist_source()?;
        }
        Ok((outcome, success))
    }

    fn reset(&mut self) -> Result<()> {
        self.state.pragmas.clear();
        self.state.declarations.clear();
        self.state.statements.clear();
        self.previous_stdout.clear();
        self.previous_stderr.clear();
        self.state.pragmas.insert("use strict;".to_string());
        self.state.pragmas.insert("use warnings;".to_string());
        self.state.pragmas.insert("use feature 'say';".to_string());
        self.persist_source()
    }
}

impl LanguageSession for PerlSession {
    fn language_id(&self) -> &str {
        "perl"
    }

    fn eval(&mut self, code: &str) -> Result<ExecutionOutcome> {
        let trimmed = code.trim();
        if trimmed.is_empty() {
            return Ok(ExecutionOutcome {
                language: "perl".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        if trimmed.eq_ignore_ascii_case(":reset") {
            self.reset()?;
            return Ok(ExecutionOutcome {
                language: "perl".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        if trimmed.eq_ignore_ascii_case(":help") {
            return Ok(ExecutionOutcome {
                language: "perl".to_string(),
                exit_code: None,
                stdout:
                    "Perl commands:\n  :reset - clear session state\n  :help  - show this message\n"
                        .to_string(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        match classify_snippet(trimmed) {
            PerlSnippet::Pragma => {
                let (outcome, _) = self.apply_pragma(code)?;
                Ok(outcome)
            }
            PerlSnippet::Declaration => {
                let (outcome, _) = self.apply_declaration(code)?;
                Ok(outcome)
            }
            PerlSnippet::Expression => {
                let (outcome, _) = self.apply_expression(trimmed)?;
                Ok(outcome)
            }
            PerlSnippet::Statement => {
                let (outcome, _) = self.apply_statement(code)?;
                Ok(outcome)
            }
        }
    }

    fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}

enum PerlSnippet {
    Pragma,
    Declaration,
    Statement,
    Expression,
}

fn classify_snippet(code: &str) -> PerlSnippet {
    if is_pragma(code) {
        return PerlSnippet::Pragma;
    }

    if is_declaration(code) {
        return PerlSnippet::Declaration;
    }

    if should_wrap_expression(code) {
        return PerlSnippet::Expression;
    }

    PerlSnippet::Statement
}

fn is_pragma(code: &str) -> bool {
    code.lines().all(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("use ") || trimmed.starts_with("no ")
    })
}

fn is_declaration(code: &str) -> bool {
    let trimmed = code.trim_start();
    trimmed.starts_with("sub ")
}

fn should_wrap_expression(code: &str) -> bool {
    if code.contains('\n') {
        return false;
    }

    let trimmed = code.trim();
    if trimmed.is_empty() {
        return false;
    }

    if trimmed.ends_with(';') {
        return false;
    }

    let lowered = trimmed.to_ascii_lowercase();
    const STATEMENT_PREFIXES: [&str; 9] = [
        "my ", "our ", "state ", "if ", "for ", "while ", "foreach ", "given ", "when ",
    ];

    if STATEMENT_PREFIXES
        .iter()
        .any(|prefix| lowered.starts_with(prefix))
    {
        return false;
    }

    if trimmed.contains('=') {
        return false;
    }

    true
}

fn ensure_trailing_newline(code: &str) -> String {
    let mut owned = code.to_string();
    if !owned.ends_with('\n') {
        owned.push('\n');
    }
    owned
}

fn ensure_statement(code: &str) -> String {
    if code.trim().is_empty() {
        return String::new();
    }

    let mut owned = code.to_string();
    if !code.contains('\n') {
        let trimmed = owned.trim_end();
        if !trimmed.ends_with(';') && !trimmed.ends_with('}') {
            owned.push(';');
        }
    }
    if !owned.ends_with('\n') {
        owned.push('\n');
    }
    owned
}

fn wrap_expression(code: &str) -> String {
    format!("say({});\n", code.trim())
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
