use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tempfile::{Builder, TempDir};

use super::{ExecutionOutcome, ExecutionPayload, LanguageEngine, LanguageSession, run_version_command};

pub struct ElixirEngine {
    executable: Option<PathBuf>,
}

impl Default for ElixirEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ElixirEngine {
    pub fn new() -> Self {
        Self {
            executable: resolve_elixir_binary(),
        }
    }

    fn ensure_executable(&self) -> Result<&Path> {
        self.executable.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "Elixir support requires the `elixir` executable. Install Elixir from https://elixir-lang.org/install.html and ensure `elixir` is on your PATH."
            )
        })
    }

    fn write_temp_source(&self, code: &str) -> Result<(TempDir, PathBuf)> {
        let dir = Builder::new()
            .prefix("run-elixir")
            .tempdir()
            .context("failed to create temporary directory for Elixir source")?;
        let path = dir.path().join("snippet.exs");
        let mut contents = code.to_string();
        if !contents.ends_with('\n') {
            contents.push('\n');
        }
        fs::write(&path, contents).with_context(|| {
            format!(
                "failed to write temporary Elixir source to {}",
                path.display()
            )
        })?;
        Ok((dir, path))
    }

    fn execute_path(&self, path: &Path, args: &[String]) -> Result<std::process::Output> {
        let executable = self.ensure_executable()?;
        let mut cmd = Command::new(executable);
        cmd.arg("--no-color")
            .arg(path)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
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

impl LanguageEngine for ElixirEngine {
    fn id(&self) -> &'static str {
        "elixir"
    }

    fn display_name(&self) -> &'static str {
        "Elixir"
    }

    fn aliases(&self) -> &[&'static str] {
        &["ex", "exs", "iex"]
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

    fn toolchain_version(&self) -> Result<Option<String>> {
        let executable = self.ensure_executable()?;
        let mut cmd = Command::new(executable);
        cmd.arg("--version");
        let context = format!("{}", executable.display());
        run_version_command(cmd, &context)
    }

    fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionOutcome> {
        let start = Instant::now();
        let (temp_dir, path) = match payload {
            ExecutionPayload::Inline { code, .. } | ExecutionPayload::Stdin { code, .. } => {
                let (dir, path) = self.write_temp_source(code)?;
                (Some(dir), path)
            }
            ExecutionPayload::File { path, .. } => (None, path.clone()),
        };

        let output = self.execute_path(&path, payload.args())?;
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
        Ok(Box::new(ElixirSession::new(executable)?))
    }
}

fn resolve_elixir_binary() -> Option<PathBuf> {
    which::which("elixir").ok()
}

#[derive(Default)]
struct ElixirSessionState {
    directives: BTreeSet<String>,
    declarations: Vec<String>,
    statements: Vec<String>,
}

struct ElixirSession {
    executable: PathBuf,
    workspace: TempDir,
    state: ElixirSessionState,
    previous_stdout: String,
    previous_stderr: String,
}

impl ElixirSession {
    fn new(executable: PathBuf) -> Result<Self> {
        let workspace = Builder::new()
            .prefix("run-elixir-repl")
            .tempdir()
            .context("failed to create temporary directory for Elixir repl")?;
        let session = Self {
            executable,
            workspace,
            state: ElixirSessionState::default(),
            previous_stdout: String::new(),
            previous_stderr: String::new(),
        };
        session.persist_source()?;
        Ok(session)
    }

    fn source_path(&self) -> PathBuf {
        self.workspace.path().join("session.exs")
    }

    fn persist_source(&self) -> Result<()> {
        let source = self.render_source();
        fs::write(self.source_path(), source)
            .with_context(|| "failed to write Elixir session source".to_string())
    }

    fn render_source(&self) -> String {
        let mut source = String::new();

        for directive in &self.state.directives {
            source.push_str(directive);
            if !directive.ends_with('\n') {
                source.push('\n');
            }
        }
        if !self.state.directives.is_empty() {
            source.push('\n');
        }

        for decl in &self.state.declarations {
            source.push_str(decl);
            if !decl.ends_with('\n') {
                source.push('\n');
            }
            source.push('\n');
        }

        for stmt in &self.state.statements {
            source.push_str(stmt);
            if !stmt.ends_with('\n') {
                source.push('\n');
            }
        }

        source
    }

    fn run_program(&self) -> Result<std::process::Output> {
        let mut cmd = Command::new(&self.executable);
        cmd.arg("--no-color")
            .arg("session.exs")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(self.workspace.path());
        cmd.output().with_context(|| {
            format!(
                "failed to execute {} for Elixir session",
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
            language: "elixir".to_string(),
            exit_code: output.status.code(),
            stdout: stdout_delta,
            stderr: stderr_delta,
            duration: start.elapsed(),
        };

        Ok((outcome, success))
    }

    fn apply_directive(&mut self, code: &str) -> Result<(ExecutionOutcome, bool)> {
        let mut inserted = Vec::new();
        for line in code.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let normalized = trimmed.to_string();
            if self.state.directives.insert(normalized.clone()) {
                inserted.push(normalized);
            }
        }

        if inserted.is_empty() {
            return Ok((
                ExecutionOutcome {
                    language: "elixir".to_string(),
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
            for directive in inserted {
                self.state.directives.remove(&directive);
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
        let snippet = prepare_statement(code);
        self.state.statements.push(snippet);
        let start = Instant::now();
        let (outcome, success) = self.run_current(start)?;
        if !success {
            let _ = self.state.statements.pop();
            self.persist_source()?;
        }
        Ok((outcome, success))
    }

    fn apply_expression(&mut self, code: &str) -> Result<(ExecutionOutcome, bool)> {
        let wrapped = wrap_expression(code);
        self.state.statements.push(wrapped);
        let start = Instant::now();
        let (outcome, success) = self.run_current(start)?;
        if !success {
            let _ = self.state.statements.pop();
            self.persist_source()?;
        }
        Ok((outcome, success))
    }

    fn reset(&mut self) -> Result<()> {
        self.state.directives.clear();
        self.state.declarations.clear();
        self.state.statements.clear();
        self.previous_stdout.clear();
        self.previous_stderr.clear();
        self.persist_source()
    }
}

impl LanguageSession for ElixirSession {
    fn language_id(&self) -> &str {
        "elixir"
    }

    fn eval(&mut self, code: &str) -> Result<ExecutionOutcome> {
        let trimmed = code.trim();
        if trimmed.is_empty() {
            return Ok(ExecutionOutcome {
                language: "elixir".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        if trimmed.eq_ignore_ascii_case(":reset") {
            self.reset()?;
            return Ok(ExecutionOutcome {
                language: "elixir".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        if trimmed.eq_ignore_ascii_case(":help") {
            return Ok(ExecutionOutcome {
                language: "elixir".to_string(),
                exit_code: None,
                stdout:
                    "Elixir commands:\n  :reset - clear session state\n  :help  - show this message\n"
                        .to_string(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        match classify_snippet(trimmed) {
            ElixirSnippet::Directive => {
                let (outcome, _) = self.apply_directive(code)?;
                Ok(outcome)
            }
            ElixirSnippet::Declaration => {
                let (outcome, _) = self.apply_declaration(code)?;
                Ok(outcome)
            }
            ElixirSnippet::Expression => {
                let (outcome, _) = self.apply_expression(trimmed)?;
                Ok(outcome)
            }
            ElixirSnippet::Statement => {
                let (outcome, _) = self.apply_statement(code)?;
                Ok(outcome)
            }
        }
    }

    fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}

enum ElixirSnippet {
    Directive,
    Declaration,
    Statement,
    Expression,
}

fn classify_snippet(code: &str) -> ElixirSnippet {
    if is_directive(code) {
        return ElixirSnippet::Directive;
    }

    if is_declaration(code) {
        return ElixirSnippet::Declaration;
    }

    if should_wrap_expression(code) {
        return ElixirSnippet::Expression;
    }

    ElixirSnippet::Statement
}

fn is_directive(code: &str) -> bool {
    code.lines().all(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("import ")
            || trimmed.starts_with("alias ")
            || trimmed.starts_with("require ")
            || trimmed.starts_with("use ")
    })
}

fn is_declaration(code: &str) -> bool {
    let trimmed = code.trim_start();
    trimmed.starts_with("defmodule ")
        || trimmed.starts_with("defprotocol ")
        || trimmed.starts_with("defimpl ")
}

fn should_wrap_expression(code: &str) -> bool {
    if code.contains('\n') {
        return false;
    }

    let trimmed = code.trim();
    if trimmed.is_empty() {
        return false;
    }

    let lowered = trimmed.to_ascii_lowercase();
    const STATEMENT_PREFIXES: [&str; 13] = [
        "import ",
        "alias ",
        "require ",
        "use ",
        "def ",
        "defp ",
        "defmodule ",
        "defprotocol ",
        "defimpl ",
        "case ",
        "try ",
        "receive ",
        "with ",
    ];

    if STATEMENT_PREFIXES
        .iter()
        .any(|prefix| lowered.starts_with(prefix))
    {
        return false;
    }

    if trimmed.contains('=') && !trimmed.starts_with(&[':', '?'][..]) {
        return false;
    }

    if trimmed.ends_with("do") || trimmed.contains(" fn ") {
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

fn prepare_statement(code: &str) -> String {
    let mut snippet = ensure_trailing_newline(code);
    let targets = collect_assignment_targets(code);
    if targets.is_empty() {
        return snippet;
    }

    for target in targets {
        snippet.push_str("_ = ");
        snippet.push_str(&target);
        snippet.push('\n');
    }

    snippet
}

fn collect_assignment_targets(code: &str) -> Vec<String> {
    let mut targets = BTreeSet::new();
    for line in code.lines() {
        if let Some(target) = parse_assignment_target(line) {
            targets.insert(target);
        }
    }
    targets.into_iter().collect()
}

fn parse_assignment_target(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }

    let (lhs_part, rhs_part) = trimmed.split_once('=')?;
    let lhs = lhs_part.trim();
    let rhs = rhs_part.trim();
    if lhs.is_empty() || rhs.is_empty() {
        return None;
    }

    let eq_index = trimmed.find('=')?;
    let before_char = trimmed[..eq_index]
        .chars()
        .rev()
        .find(|c| !c.is_whitespace());
    if matches!(
        before_char,
        Some('=') | Some('!') | Some('<') | Some('>') | Some('~') | Some(':')
    ) {
        return None;
    }

    let after_char = trimmed[eq_index + 1..].chars().find(|c| !c.is_whitespace());
    if matches!(after_char, Some('=') | Some('>') | Some('<') | Some('~')) {
        return None;
    }

    if !is_elixir_identifier(lhs) {
        return None;
    }

    Some(lhs.to_string())
}

fn is_elixir_identifier(candidate: &str) -> bool {
    let mut chars = candidate.chars();
    let first = match chars.next() {
        Some(ch) => ch,
        None => return false,
    };

    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }

    for ch in chars {
        if !(ch == '_' || ch.is_ascii_alphanumeric()) {
            return false;
        }
    }

    true
}

fn wrap_expression(code: &str) -> String {
    format!("IO.inspect(({}))\n", code.trim())
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
