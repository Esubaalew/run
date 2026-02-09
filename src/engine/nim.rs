use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tempfile::{Builder, TempDir};

use super::{ExecutionOutcome, ExecutionPayload, LanguageEngine, LanguageSession, run_version_command};

pub struct NimEngine {
    executable: Option<PathBuf>,
}

impl Default for NimEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl NimEngine {
    pub fn new() -> Self {
        Self {
            executable: resolve_nim_binary(),
        }
    }

    fn ensure_executable(&self) -> Result<&Path> {
        self.executable.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "Nim support requires the `nim` executable. Install it from https://nim-lang.org/install.html and ensure it is on your PATH."
            )
        })
    }

    fn write_temp_source(&self, code: &str) -> Result<(TempDir, PathBuf)> {
        let dir = Builder::new()
            .prefix("run-nim")
            .tempdir()
            .context("failed to create temporary directory for Nim source")?;
        let path = dir.path().join("snippet.nim");
        let mut contents = code.to_string();
        if !contents.ends_with('\n') {
            contents.push('\n');
        }
        std::fs::write(&path, contents).with_context(|| {
            format!("failed to write temporary Nim source to {}", path.display())
        })?;
        Ok((dir, path))
    }

    fn run_source(&self, source: &Path, args: &[String]) -> Result<std::process::Output> {
        let executable = self.ensure_executable()?;
        let mut cmd = Command::new(executable);
        cmd.arg("r")
            .arg(source)
            .arg("--colors:off")
            .arg("--hints:off")
            .arg("--verbosity:0")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if !args.is_empty() {
            cmd.arg("--").args(args);
        }
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

impl LanguageEngine for NimEngine {
    fn id(&self) -> &'static str {
        "nim"
    }

    fn display_name(&self) -> &'static str {
        "Nim"
    }

    fn aliases(&self) -> &[&'static str] {
        &["nimlang"]
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
        let (temp_dir, source_path) = match payload {
            ExecutionPayload::Inline { code, .. } | ExecutionPayload::Stdin { code, .. } => {
                let (dir, path) = self.write_temp_source(code)?;
                (Some(dir), path)
            }
            ExecutionPayload::File { path, .. } => {
                if path.extension().and_then(|e| e.to_str()) != Some("nim") {
                    let code = std::fs::read_to_string(path)?;
                    let (dir, new_path) = self.write_temp_source(&code)?;
                    (Some(dir), new_path)
                } else {
                    (None, path.clone())
                }
            }
        };

        let output = self.run_source(&source_path, payload.args())?;
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
        Ok(Box::new(NimSession::new(executable)?))
    }
}

fn resolve_nim_binary() -> Option<PathBuf> {
    which::which("nim").ok()
}

struct NimSession {
    executable: PathBuf,
    workspace: TempDir,
    snippets: Vec<String>,
    last_stdout: String,
    last_stderr: String,
}

impl NimSession {
    fn new(executable: PathBuf) -> Result<Self> {
        let workspace = TempDir::new().context("failed to create Nim session workspace")?;
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
        self.workspace.path().join("session.nim")
    }

    fn persist_source(&self) -> Result<()> {
        let source = self.render_source();
        fs::write(self.source_path(), source)
            .with_context(|| "failed to write Nim session source".to_string())
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
        cmd.arg("r")
            .arg("session.nim")
            .arg("--colors:off")
            .arg("--hints:off")
            .arg("--verbosity:0")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(self.workspace.path());
        cmd.output().with_context(|| {
            format!(
                "failed to execute {} for Nim session",
                self.executable.display()
            )
        })
    }

    fn run_current(&mut self, start: Instant) -> Result<(ExecutionOutcome, bool)> {
        self.persist_source()?;
        let output = self.run_program()?;
        let stdout_full = Self::normalize_output(&output.stdout);
        let stderr_raw = Self::normalize_output(&output.stderr);
        let stderr_filtered = filter_nim_stderr(&stderr_raw);

        let success = output.status.success();
        let (stdout, stderr) = if success {
            let stdout_delta = Self::diff_outputs(&self.last_stdout, &stdout_full);
            let stderr_delta = Self::diff_outputs(&self.last_stderr, &stderr_filtered);
            self.last_stdout = stdout_full;
            self.last_stderr = stderr_filtered;
            (stdout_delta, stderr_delta)
        } else {
            (stdout_full, stderr_raw)
        };

        let outcome = ExecutionOutcome {
            language: "nim".to_string(),
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

impl LanguageSession for NimSession {
    fn language_id(&self) -> &str {
        "nim"
    }

    fn eval(&mut self, code: &str) -> Result<ExecutionOutcome> {
        let trimmed = code.trim();
        if trimmed.is_empty() {
            return Ok(ExecutionOutcome {
                language: "nim".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        if trimmed.eq_ignore_ascii_case(":reset") {
            self.reset()?;
            return Ok(ExecutionOutcome {
                language: "nim".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        if trimmed.eq_ignore_ascii_case(":help") {
            return Ok(ExecutionOutcome {
                language: "nim".to_string(),
                exit_code: None,
                stdout:
                    "Nim commands:\n  :reset - clear session state\n  :help  - show this message\n"
                        .to_string(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        let snippet = match classify_nim_snippet(trimmed) {
            NimSnippetKind::Statement => prepare_statement(code),
            NimSnippetKind::Expression => wrap_expression(trimmed),
        };

        let (outcome, _) = self.apply_snippet(snippet)?;
        Ok(outcome)
    }

    fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}

enum NimSnippetKind {
    Statement,
    Expression,
}

fn classify_nim_snippet(code: &str) -> NimSnippetKind {
    if looks_like_nim_statement(code) {
        NimSnippetKind::Statement
    } else {
        NimSnippetKind::Expression
    }
}

fn looks_like_nim_statement(code: &str) -> bool {
    let trimmed = code.trim_start();
    trimmed.contains('\n')
        || trimmed.ends_with(';')
        || trimmed.ends_with(':')
        || trimmed.starts_with("#")
        || trimmed.starts_with("import ")
        || trimmed.starts_with("from ")
        || trimmed.starts_with("include ")
        || trimmed.starts_with("let ")
        || trimmed.starts_with("var ")
        || trimmed.starts_with("const ")
        || trimmed.starts_with("type ")
        || trimmed.starts_with("proc ")
        || trimmed.starts_with("iterator ")
        || trimmed.starts_with("macro ")
        || trimmed.starts_with("template ")
        || trimmed.starts_with("when ")
        || trimmed.starts_with("block ")
        || trimmed.starts_with("if ")
        || trimmed.starts_with("for ")
        || trimmed.starts_with("while ")
        || trimmed.starts_with("case ")
}

fn ensure_trailing_newline(code: &str) -> String {
    let mut snippet = code.to_string();
    if !snippet.ends_with('\n') {
        snippet.push('\n');
    }
    snippet
}

fn prepare_statement(code: &str) -> String {
    let mut snippet = ensure_trailing_newline(code);
    let identifiers = collect_declared_identifiers(code);
    if identifiers.is_empty() {
        return snippet;
    }

    for name in identifiers {
        snippet.push_str("discard ");
        snippet.push_str(&name);
        snippet.push('\n');
    }

    snippet
}

fn wrap_expression(code: &str) -> String {
    format!("echo ({})\n", code)
}

fn collect_declared_identifiers(code: &str) -> Vec<String> {
    let mut identifiers = BTreeSet::new();

    for line in code.lines() {
        let trimmed = line.trim_start();
        let rest = if let Some(stripped) = trimmed.strip_prefix("let ") {
            stripped
        } else if let Some(stripped) = trimmed.strip_prefix("var ") {
            stripped
        } else if let Some(stripped) = trimmed.strip_prefix("const ") {
            stripped
        } else {
            continue;
        };

        let before_comment = rest.split('#').next().unwrap_or(rest);
        let declaration_part = before_comment.split('=').next().unwrap_or(before_comment);

        for segment in declaration_part.split(',') {
            let mut candidate = segment.trim();
            if candidate.is_empty() {
                continue;
            }

            candidate = candidate.trim_matches('`');
            candidate = candidate.trim_end_matches('*');
            candidate = candidate.trim();

            if candidate.is_empty() {
                continue;
            }

            let mut name = String::new();
            for ch in candidate.chars() {
                if is_nim_identifier_part(ch) {
                    name.push(ch);
                } else {
                    break;
                }
            }

            if name.is_empty() {
                continue;
            }

            if name
                .chars()
                .next()
                .is_none_or(|ch| !is_nim_identifier_start(ch))
            {
                continue;
            }

            identifiers.insert(name);
        }
    }

    identifiers.into_iter().collect()
}

fn is_nim_identifier_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_nim_identifier_part(ch: char) -> bool {
    is_nim_identifier_start(ch) || ch.is_ascii_digit()
}

fn filter_nim_stderr(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return false;
            }
            if trimmed.chars().all(|c| c == '.') {
                return false;
            }
            if trimmed.starts_with("Hint: used config file") {
                return false;
            }
            if trimmed.starts_with("Hint:  [Link]") {
                return false;
            }
            if trimmed.starts_with("Hint: mm: ") {
                return false;
            }
            if (trimmed.starts_with("Hint: ")
                || trimmed.chars().next().is_some_and(|c| c.is_ascii_digit()))
                && (trimmed.contains(" lines;")
                    || trimmed.contains(" proj:")
                    || trimmed.contains(" out:")
                    || trimmed.contains("Success")
                    || trimmed.contains("[Success"))
            {
                return false;
            }
            if trimmed.starts_with("Hint: /") && trimmed.contains("--colors:off") {
                return false;
            }
            if trimmed.starts_with("CC: ") {
                return false;
            }

            true
        })
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}
