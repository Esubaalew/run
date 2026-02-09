use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tempfile::{Builder, TempDir};

use super::{ExecutionOutcome, ExecutionPayload, LanguageEngine, LanguageSession, run_version_command};

pub struct DartEngine {
    executable: Option<PathBuf>,
}

impl Default for DartEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl DartEngine {
    pub fn new() -> Self {
        Self {
            executable: resolve_dart_binary(),
        }
    }

    fn ensure_executable(&self) -> Result<&Path> {
        self.executable.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "Dart support requires the `dart` executable. Install the Dart SDK from https://dart.dev/get-dart and ensure `dart` is on your PATH."
            )
        })
    }

    fn prepare_inline_source(code: &str) -> String {
        if contains_main(code) {
            let mut snippet = code.to_string();
            if !snippet.ends_with('\n') {
                snippet.push('\n');
            }
            return snippet;
        }

        let mut wrapped = String::from("Future<void> main() async {\n");
        for line in code.lines() {
            if line.trim().is_empty() {
                wrapped.push_str("  \n");
            } else {
                wrapped.push_str("  ");
                wrapped.push_str(line);
                if !line.trim_end().ends_with(';') && !line.trim_end().ends_with('}') {
                    wrapped.push(';');
                }
                wrapped.push('\n');
            }
        }
        wrapped.push_str("}\n");
        wrapped
    }

    fn write_temp_source(&self, code: &str) -> Result<(TempDir, PathBuf)> {
        let dir = Builder::new()
            .prefix("run-dart")
            .tempdir()
            .context("failed to create temporary directory for Dart source")?;
        let path = dir.path().join("main.dart");
        fs::write(&path, Self::prepare_inline_source(code)).with_context(|| {
            format!(
                "failed to write temporary Dart source to {}",
                path.display()
            )
        })?;
        Ok((dir, path))
    }

    fn execute_path(&self, path: &Path, args: &[String]) -> Result<std::process::Output> {
        let executable = self.ensure_executable()?;
        let mut cmd = Command::new(executable);
        cmd.arg("run")
            .arg("--enable-asserts")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd.stdin(Stdio::inherit());

        if let Some(parent) = path.parent() {
            cmd.current_dir(parent);
            if let Some(file_name) = path.file_name() {
                cmd.arg(file_name);
            } else {
                cmd.arg(path);
            }
            cmd.args(args);
        } else {
            cmd.arg(path).args(args);
        }

        cmd.output().with_context(|| {
            format!(
                "failed to invoke {} to run {}",
                executable.display(),
                path.display()
            )
        })
    }
}

impl LanguageEngine for DartEngine {
    fn id(&self) -> &'static str {
        "dart"
    }

    fn display_name(&self) -> &'static str {
        "Dart"
    }

    fn aliases(&self) -> &[&'static str] {
        &["dartlang", "flutter"]
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
            ExecutionPayload::Inline { code, .. } => {
                let (dir, path) = self.write_temp_source(code)?;
                (Some(dir), path)
            }
            ExecutionPayload::Stdin { code, .. } => {
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
        Ok(Box::new(DartSession::new(executable)?))
    }
}

fn resolve_dart_binary() -> Option<PathBuf> {
    which::which("dart").ok()
}

fn contains_main(code: &str) -> bool {
    code.lines()
        .any(|line| line.contains("void main") || line.contains("Future<void> main"))
}

struct DartSession {
    executable: PathBuf,
    workspace: TempDir,
    imports: BTreeSet<String>,
    declarations: Vec<String>,
    statements: Vec<String>,
    previous_stdout: String,
    previous_stderr: String,
}

impl DartSession {
    fn new(executable: PathBuf) -> Result<Self> {
        let workspace = Builder::new()
            .prefix("run-dart-repl")
            .tempdir()
            .context("failed to create temporary directory for Dart repl")?;
        let session = Self {
            executable,
            workspace,
            imports: BTreeSet::new(),
            declarations: Vec::new(),
            statements: Vec::new(),
            previous_stdout: String::new(),
            previous_stderr: String::new(),
        };
        session.persist_source()?;
        Ok(session)
    }

    fn source_path(&self) -> PathBuf {
        self.workspace.path().join("session.dart")
    }

    fn persist_source(&self) -> Result<()> {
        let source = self.render_source();
        fs::write(self.source_path(), source)
            .with_context(|| "failed to write Dart session source".to_string())
    }

    fn render_source(&self) -> String {
        let mut source = String::from("import 'dart:async';\n");
        for import in &self.imports {
            source.push_str(import);
            if !import.trim_end().ends_with(';') {
                source.push(';');
            }
            source.push('\n');
        }
        source.push('\n');
        for decl in &self.declarations {
            source.push_str(decl);
            if !decl.ends_with('\n') {
                source.push('\n');
            }
            source.push('\n');
        }
        source.push_str("Future<void> main() async {\n");
        if self.statements.is_empty() {
            source.push_str("  // session body\n");
        } else {
            for stmt in &self.statements {
                for line in stmt.lines() {
                    source.push_str("  ");
                    source.push_str(line);
                    source.push('\n');
                }
            }
        }
        source.push_str("}\n");
        source
    }

    fn run_program(&self) -> Result<std::process::Output> {
        let mut cmd = Command::new(&self.executable);
        cmd.arg("run")
            .arg("--enable-asserts")
            .arg("session.dart")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(self.workspace.path());
        cmd.output().with_context(|| {
            format!(
                "failed to execute {} for Dart session",
                self.executable.display()
            )
        })
    }

    fn run_standalone_program(&self, code: &str) -> Result<ExecutionOutcome> {
        let start = Instant::now();
        let path = self.workspace.path().join("standalone.dart");
        fs::write(&path, ensure_trailing_newline(code))
            .with_context(|| "failed to write Dart standalone source".to_string())?;

        let mut cmd = Command::new(&self.executable);
        cmd.arg("run")
            .arg("--enable-asserts")
            .arg("standalone.dart")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(self.workspace.path());
        let output = cmd.output().with_context(|| {
            format!(
                "failed to execute {} for Dart standalone program",
                self.executable.display()
            )
        })?;

        let outcome = ExecutionOutcome {
            language: self.language_id().to_string(),
            exit_code: output.status.code(),
            stdout: normalize_output(&output.stdout),
            stderr: normalize_output(&output.stderr),
            duration: start.elapsed(),
        };

        let _ = fs::remove_file(&path);

        Ok(outcome)
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
            language: "dart".to_string(),
            exit_code: output.status.code(),
            stdout: stdout_delta,
            stderr: stderr_delta,
            duration: start.elapsed(),
        };

        Ok((outcome, success))
    }

    fn apply_import(&mut self, code: &str) -> Result<(ExecutionOutcome, bool)> {
        let mut updated = false;
        for line in code.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let statement = if trimmed.ends_with(';') {
                trimmed.to_string()
            } else {
                format!("{};", trimmed)
            };
            if self.imports.insert(statement) {
                updated = true;
            }
        }
        if !updated {
            return Ok((
                ExecutionOutcome {
                    language: "dart".to_string(),
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
            for line in code.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let statement = if trimmed.ends_with(';') {
                    trimmed.to_string()
                } else {
                    format!("{};", trimmed)
                };
                self.imports.remove(&statement);
            }
            self.persist_source()?;
        }
        Ok((outcome, success))
    }

    fn apply_declaration(&mut self, code: &str) -> Result<(ExecutionOutcome, bool)> {
        let snippet = ensure_trailing_newline(code);
        self.declarations.push(snippet);
        let start = Instant::now();
        let (outcome, success) = self.run_current(start)?;
        if !success {
            let _ = self.declarations.pop();
            self.persist_source()?;
        }
        Ok((outcome, success))
    }

    fn apply_statement(&mut self, code: &str) -> Result<(ExecutionOutcome, bool)> {
        self.statements.push(ensure_trailing_semicolon(code));
        let start = Instant::now();
        let (outcome, success) = self.run_current(start)?;
        if !success {
            let _ = self.statements.pop();
            self.persist_source()?;
        }
        Ok((outcome, success))
    }

    fn apply_expression(&mut self, code: &str) -> Result<(ExecutionOutcome, bool)> {
        self.statements.push(wrap_expression(code));
        let start = Instant::now();
        let (outcome, success) = self.run_current(start)?;
        if !success {
            let _ = self.statements.pop();
            self.persist_source()?;
        }
        Ok((outcome, success))
    }

    fn reset(&mut self) -> Result<()> {
        self.imports.clear();
        self.declarations.clear();
        self.statements.clear();
        self.previous_stdout.clear();
        self.previous_stderr.clear();
        self.persist_source()
    }
}

impl LanguageSession for DartSession {
    fn language_id(&self) -> &str {
        "dart"
    }

    fn eval(&mut self, code: &str) -> Result<ExecutionOutcome> {
        let trimmed = code.trim();
        if trimmed.is_empty() {
            return Ok(ExecutionOutcome {
                language: "dart".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        if trimmed.eq_ignore_ascii_case(":reset") {
            self.reset()?;
            return Ok(ExecutionOutcome {
                language: "dart".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        if trimmed.eq_ignore_ascii_case(":help") {
            return Ok(ExecutionOutcome {
                language: "dart".to_string(),
                exit_code: None,
                stdout:
                    "Dart commands:\n  :reset - clear session state\n  :help  - show this message\n"
                        .to_string(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        if contains_main(code) {
            return self.run_standalone_program(code);
        }

        match classify_snippet(trimmed) {
            DartSnippet::Import => {
                let (outcome, success) = self.apply_import(code)?;
                if !success {
                    return Ok(outcome);
                }
                Ok(outcome)
            }
            DartSnippet::Declaration => {
                let (outcome, _) = self.apply_declaration(code)?;
                Ok(outcome)
            }
            DartSnippet::Expression => {
                let (outcome, _) = self.apply_expression(trimmed)?;
                Ok(outcome)
            }
            DartSnippet::Statement => {
                let (outcome, _) = self.apply_statement(code)?;
                Ok(outcome)
            }
        }
    }

    fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}

enum DartSnippet {
    Import,
    Declaration,
    Statement,
    Expression,
}

fn classify_snippet(code: &str) -> DartSnippet {
    if is_import(code) {
        return DartSnippet::Import;
    }

    if is_declaration(code) {
        return DartSnippet::Declaration;
    }

    if should_wrap_expression(code) {
        return DartSnippet::Expression;
    }

    DartSnippet::Statement
}

fn is_import(code: &str) -> bool {
    code.lines().all(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("import ")
            || trimmed.starts_with("export ")
            || trimmed.starts_with("part ")
            || trimmed.starts_with("part of ")
    })
}

fn is_declaration(code: &str) -> bool {
    let lowered = code.trim_start().to_ascii_lowercase();
    const PREFIXES: [&str; 9] = [
        "class ",
        "enum ",
        "typedef ",
        "extension ",
        "mixin ",
        "void ",
        "Future<",
        "Future<void> ",
        "@",
    ];
    PREFIXES.iter().any(|prefix| lowered.starts_with(prefix)) && !contains_main(code)
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
    const STATEMENT_PREFIXES: [&str; 12] = [
        "var ", "final ", "const ", "if ", "for ", "while ", "do ", "switch ", "return ", "throw ",
        "await ", "yield ",
    ];
    if STATEMENT_PREFIXES
        .iter()
        .any(|prefix| lowered.starts_with(prefix))
    {
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

fn ensure_trailing_semicolon(code: &str) -> String {
    let lines: Vec<&str> = code.lines().collect();
    if lines.is_empty() {
        return ensure_trailing_newline(code);
    }

    let mut result = String::new();
    for (idx, line) in lines.iter().enumerate() {
        let trimmed_end = line.trim_end();
        if trimmed_end.is_empty() {
            result.push_str(line);
        } else if trimmed_end.ends_with(';')
            || trimmed_end.ends_with('}')
            || trimmed_end.ends_with('{')
            || trimmed_end.trim_start().starts_with("//")
        {
            result.push_str(trimmed_end);
        } else {
            result.push_str(trimmed_end);
            result.push(';');
        }

        if idx + 1 < lines.len() {
            result.push('\n');
        }
    }

    ensure_trailing_newline(&result)
}

fn wrap_expression(code: &str) -> String {
    format!("print(({}));\n", code)
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
