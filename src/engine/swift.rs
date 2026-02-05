use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tempfile::{Builder, TempDir};

use super::{ExecutionOutcome, ExecutionPayload, LanguageEngine, LanguageSession};

pub struct SwiftEngine {
    executable: Option<PathBuf>,
}

impl SwiftEngine {
    pub fn new() -> Self {
        Self {
            executable: resolve_swift_binary(),
        }
    }

    fn ensure_executable(&self) -> Result<&Path> {
        self.executable.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "Swift support requires the `swift` executable. Install Xcode command-line tools or the Swift toolchain from https://www.swift.org/download/ and ensure `swift` is on your PATH."
            )
        })
    }

    fn write_temp_source(&self, code: &str) -> Result<(TempDir, PathBuf)> {
        let dir = Builder::new()
            .prefix("run-swift")
            .tempdir()
            .context("failed to create temporary directory for Swift source")?;
        let path = dir.path().join("snippet.swift");
        let mut contents = code.to_string();
        if !contents.ends_with('\n') {
            contents.push('\n');
        }
        fs::write(&path, contents).with_context(|| {
            format!(
                "failed to write temporary Swift source to {}",
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

impl LanguageEngine for SwiftEngine {
    fn id(&self) -> &'static str {
        "swift"
    }

    fn display_name(&self) -> &'static str {
        "Swift"
    }

    fn aliases(&self) -> &[&'static str] {
        &["swiftlang"]
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
        Ok(Box::new(SwiftSession::new(executable)?))
    }
}

fn resolve_swift_binary() -> Option<PathBuf> {
    which::which("swift").ok()
}

#[derive(Default)]
struct SwiftSessionState {
    imports: BTreeSet<String>,
    declarations: Vec<String>,
    statements: Vec<String>,
}

struct SwiftSession {
    executable: PathBuf,
    workspace: TempDir,
    state: SwiftSessionState,
    previous_stdout: String,
    previous_stderr: String,
}

impl SwiftSession {
    fn new(executable: PathBuf) -> Result<Self> {
        let workspace = Builder::new()
            .prefix("run-swift-repl")
            .tempdir()
            .context("failed to create temporary directory for Swift repl")?;
        let session = Self {
            executable,
            workspace,
            state: SwiftSessionState::default(),
            previous_stdout: String::new(),
            previous_stderr: String::new(),
        };
        session.persist_source()?;
        Ok(session)
    }

    fn source_path(&self) -> PathBuf {
        self.workspace.path().join("session.swift")
    }

    fn persist_source(&self) -> Result<()> {
        let source = self.render_source();
        fs::write(self.source_path(), source)
            .with_context(|| "failed to write Swift session source".to_string())
    }

    fn render_source(&self) -> String {
        let mut source = String::from("import Foundation\n");

        for import in &self.state.imports {
            let trimmed = import.trim();
            if trimmed.eq("import Foundation") {
                continue;
            }
            source.push_str(trimmed);
            if !trimmed.ends_with('\n') {
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
            source.push_str("// session body\n");
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
        cmd.arg("session.swift")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(self.workspace.path());
        cmd.output().with_context(|| {
            format!(
                "failed to execute {} for Swift session",
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
            language: "swift".to_string(),
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
            let stmt = if trimmed.ends_with(';') {
                trimmed.trim_end_matches(';').to_string()
            } else {
                trimmed.to_string()
            };
            if self.state.imports.insert(stmt.clone()) {
                inserted.push(stmt);
            }
        }

        if inserted.is_empty() {
            return Ok((
                ExecutionOutcome {
                    language: "swift".to_string(),
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
            for stmt in inserted {
                self.state.imports.remove(&stmt);
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
        self.state.statements.push(ensure_trailing_newline(code));
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
        self.state.imports.clear();
        self.state.declarations.clear();
        self.state.statements.clear();
        self.previous_stdout.clear();
        self.previous_stderr.clear();
        self.persist_source()
    }
}

impl LanguageSession for SwiftSession {
    fn language_id(&self) -> &str {
        "swift"
    }

    fn eval(&mut self, code: &str) -> Result<ExecutionOutcome> {
        let trimmed = code.trim();
        if trimmed.is_empty() {
            return Ok(ExecutionOutcome {
                language: "swift".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        if trimmed.eq_ignore_ascii_case(":reset") {
            self.reset()?;
            return Ok(ExecutionOutcome {
                language: "swift".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        if trimmed.eq_ignore_ascii_case(":help") {
            return Ok(ExecutionOutcome {
                language: "swift".to_string(),
                exit_code: None,
                stdout:
                    "Swift commands:\n  :reset - clear session state\n  :help  - show this message\n"
                        .to_string(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        match classify_snippet(trimmed) {
            SwiftSnippet::Import => {
                let (outcome, _) = self.apply_import(code)?;
                Ok(outcome)
            }
            SwiftSnippet::Declaration => {
                let (outcome, _) = self.apply_declaration(code)?;
                Ok(outcome)
            }
            SwiftSnippet::Expression => {
                let (outcome, _) = self.apply_expression(trimmed)?;
                Ok(outcome)
            }
            SwiftSnippet::Statement => {
                let (outcome, _) = self.apply_statement(code)?;
                Ok(outcome)
            }
        }
    }

    fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}

enum SwiftSnippet {
    Import,
    Declaration,
    Statement,
    Expression,
}

fn classify_snippet(code: &str) -> SwiftSnippet {
    if is_import(code) {
        return SwiftSnippet::Import;
    }

    if is_declaration(code) {
        return SwiftSnippet::Declaration;
    }

    if should_wrap_expression(code) {
        return SwiftSnippet::Expression;
    }

    SwiftSnippet::Statement
}

fn is_import(code: &str) -> bool {
    code.lines()
        .all(|line| line.trim_start().starts_with("import "))
}

fn is_declaration(code: &str) -> bool {
    let lowered = code.trim_start().to_ascii_lowercase();
    const PREFIXES: [&str; 8] = [
        "func ",
        "class ",
        "struct ",
        "enum ",
        "protocol ",
        "extension ",
        "actor ",
        "typealias ",
    ];
    PREFIXES.iter().any(|prefix| lowered.starts_with(prefix))
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
    const STATEMENT_PREFIXES: [&str; 10] = [
        "let ", "var ", "if ", "for ", "while ", "repeat ", "guard ", "switch ", "return ",
        "throw ",
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

fn wrap_expression(code: &str) -> String {
    format!("print(({}))\n", code.trim())
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
