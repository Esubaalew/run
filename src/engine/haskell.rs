use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tempfile::{Builder, TempDir};

use super::{ExecutionOutcome, ExecutionPayload, LanguageEngine, LanguageSession};

pub struct HaskellEngine {
    executable: Option<PathBuf>,
}

impl HaskellEngine {
    pub fn new() -> Self {
        Self {
            executable: resolve_runghc_binary(),
        }
    }

    fn ensure_executable(&self) -> Result<&Path> {
        self.executable.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "Haskell support requires the `runghc` executable. Install the GHC toolchain from https://www.haskell.org/ghc/ (or via ghcup) and ensure `runghc` is on your PATH."
            )
        })
    }

    fn write_temp_source(&self, code: &str) -> Result<(TempDir, PathBuf)> {
        let dir = Builder::new()
            .prefix("run-haskell")
            .tempdir()
            .context("failed to create temporary directory for Haskell source")?;
        let path = dir.path().join("snippet.hs");
        let mut contents = code.to_string();
        if !contents.ends_with('\n') {
            contents.push('\n');
        }
        fs::write(&path, contents).with_context(|| {
            format!(
                "failed to write temporary Haskell source to {}",
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

impl LanguageEngine for HaskellEngine {
    fn id(&self) -> &'static str {
        "haskell"
    }

    fn display_name(&self) -> &'static str {
        "Haskell"
    }

    fn aliases(&self) -> &[&'static str] {
        &["hs", "ghci"]
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
            ExecutionPayload::Inline { code } | ExecutionPayload::Stdin { code } => {
                let (dir, path) = self.write_temp_source(code)?;
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
        Ok(Box::new(HaskellSession::new(executable)?))
    }
}

fn resolve_runghc_binary() -> Option<PathBuf> {
    which::which("runghc").ok()
}

#[derive(Default)]
struct HaskellSessionState {
    imports: BTreeSet<String>,
    declarations: Vec<String>,
    statements: Vec<String>,
}

struct HaskellSession {
    executable: PathBuf,
    workspace: TempDir,
    state: HaskellSessionState,
    previous_stdout: String,
    previous_stderr: String,
}

impl HaskellSession {
    fn new(executable: PathBuf) -> Result<Self> {
        let workspace = Builder::new()
            .prefix("run-haskell-repl")
            .tempdir()
            .context("failed to create temporary directory for Haskell repl")?;
        let session = Self {
            executable,
            workspace,
            state: HaskellSessionState::default(),
            previous_stdout: String::new(),
            previous_stderr: String::new(),
        };
        session.persist_source()?;
        Ok(session)
    }

    fn source_path(&self) -> PathBuf {
        self.workspace.path().join("session.hs")
    }

    fn persist_source(&self) -> Result<()> {
        let source = self.render_source();
        fs::write(self.source_path(), source)
            .with_context(|| "failed to write Haskell session source".to_string())
    }

    fn render_source(&self) -> String {
        let mut source = String::new();
        source.push_str("import Prelude\n");
        for import in &self.state.imports {
            source.push_str(import);
            if !import.ends_with('\n') {
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

        source.push_str("main :: IO ()\n");
        source.push_str("main = do\n");
        if self.state.statements.is_empty() {
            source.push_str("    return ()\n");
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
        cmd.arg("session.hs")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(self.workspace.path());
        cmd.output().with_context(|| {
            format!(
                "failed to execute {} for Haskell session",
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
            language: "haskell".to_string(),
            exit_code: output.status.code(),
            stdout: stdout_delta,
            stderr: stderr_delta,
            duration: start.elapsed(),
        };

        Ok((outcome, success))
    }

    fn apply_import(&mut self, code: &str) -> Result<(ExecutionOutcome, bool)> {
        let mut inserted = Vec::new();
        for line in code.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let normalized = trimmed.to_string();
            if self.state.imports.insert(normalized.clone()) {
                inserted.push(normalized);
            }
        }

        if inserted.is_empty() {
            return Ok((
                ExecutionOutcome {
                    language: "haskell".to_string(),
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
            for item in inserted {
                self.state.imports.remove(&item);
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
        let snippet = indent_block(code);
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
        self.state.imports.clear();
        self.state.declarations.clear();
        self.state.statements.clear();
        self.previous_stdout.clear();
        self.previous_stderr.clear();
        self.persist_source()
    }
}

impl LanguageSession for HaskellSession {
    fn language_id(&self) -> &str {
        "haskell"
    }

    fn eval(&mut self, code: &str) -> Result<ExecutionOutcome> {
        let trimmed = code.trim();
        if trimmed.is_empty() {
            return Ok(ExecutionOutcome {
                language: "haskell".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        if trimmed.eq_ignore_ascii_case(":reset") {
            self.reset()?;
            return Ok(ExecutionOutcome {
                language: "haskell".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        if trimmed.eq_ignore_ascii_case(":help") {
            return Ok(ExecutionOutcome {
                language: "haskell".to_string(),
                exit_code: None,
                stdout: "Haskell commands:\n  :reset — clear session state\n  :help  — show this message\n"
                    .to_string(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        match classify_snippet(trimmed) {
            HaskellSnippet::Import => {
                let (outcome, _) = self.apply_import(code)?;
                Ok(outcome)
            }
            HaskellSnippet::Declaration => {
                let (outcome, _) = self.apply_declaration(code)?;
                Ok(outcome)
            }
            HaskellSnippet::Expression => {
                let (outcome, _) = self.apply_expression(trimmed)?;
                Ok(outcome)
            }
            HaskellSnippet::Statement => {
                let (outcome, _) = self.apply_statement(code)?;
                Ok(outcome)
            }
        }
    }

    fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}

enum HaskellSnippet {
    Import,
    Declaration,
    Statement,
    Expression,
}

fn classify_snippet(code: &str) -> HaskellSnippet {
    if is_import(code) {
        return HaskellSnippet::Import;
    }

    if is_declaration(code) {
        return HaskellSnippet::Declaration;
    }

    if should_wrap_expression(code) {
        return HaskellSnippet::Expression;
    }

    HaskellSnippet::Statement
}

fn is_import(code: &str) -> bool {
    code.lines()
        .all(|line| line.trim_start().starts_with("import "))
}

fn is_declaration(code: &str) -> bool {
    let trimmed = code.trim_start();
    if trimmed.starts_with("let ") {
        return false;
    }
    let lowered = trimmed.to_ascii_lowercase();
    const PREFIXES: [&str; 8] = [
        "module ",
        "data ",
        "type ",
        "newtype ",
        "class ",
        "instance ",
        "foreign ",
        "default ",
    ];
    if PREFIXES.iter().any(|prefix| lowered.starts_with(prefix)) {
        return true;
    }

    if trimmed.contains("::") {
        return true;
    }

    // simple function definition detection: name args =
    if let Some(lhs) = trimmed.split('=').next() {
        let lhs = lhs.trim();
        if lhs.is_empty() {
            return false;
        }
        let first_token = lhs.split_whitespace().next().unwrap_or("");
        if first_token.eq_ignore_ascii_case("let") {
            return false;
        }
        first_token
            .chars()
            .next()
            .map(|c| c.is_alphabetic())
            .unwrap_or(false)
    } else {
        false
    }
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
    const STATEMENT_PREFIXES: [&str; 11] = [
        "let ",
        "case ",
        "if ",
        "do ",
        "import ",
        "module ",
        "data ",
        "type ",
        "newtype ",
        "class ",
        "instance ",
    ];

    if STATEMENT_PREFIXES
        .iter()
        .any(|prefix| lowered.starts_with(prefix))
    {
        return false;
    }

    if trimmed.contains('=') || trimmed.contains("->") || trimmed.contains("<-") {
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

fn indent_block(code: &str) -> String {
    let mut result = String::new();
    for line in code.split_inclusive('\n') {
        if line.ends_with('\n') {
            result.push_str("    ");
            result.push_str(line);
        } else {
            result.push_str("    ");
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}

fn wrap_expression(code: &str) -> String {
    indent_block(&format!("print (({}))\n", code.trim()))
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
